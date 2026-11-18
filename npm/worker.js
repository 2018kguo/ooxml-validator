// Validation off the main thread (Node): a small pool of worker threads, each
// with its own WebAssembly instance.
import { Worker, isMainThread, parentPort } from "node:worker_threads";
import { MAX_DOCX_BYTES, lastMemory, tooLarge, validate } from "./index.js";

// A worker whose WebAssembly memory grew past this is replaced once idle, so a
// large document's memory goes back to the system rather than staying held.
const RECYCLE_BYTES = 64 * 1024 * 1024;

if (!isMainThread && parentPort) {
  parentPort.on("message", async ({ id, docx, options }) => {
    try {
      const report = await validate(docx, options);
      parentPort.postMessage({ id, report, recycle: lastMemory() > RECYCLE_BYTES });
    } catch (error) {
      parentPort.postMessage({ id, error: String(error?.stack ?? error) });
    }
  });
}

/**
 * @param {{ threads?: number }} [options]
 */
export function createValidatorPool({ threads = 1 } = {}) {
  let closed = false;
  const workers = [];

  function start(slot) {
    // The parent's flags (e.g. --input-type) need not suit a worker.
    const worker = new Worker(new URL(import.meta.url), { execArgv: [] });
    const w = { worker, pending: new Map(), retiring: false };
    worker.on("message", ({ id, report, error, recycle }) => {
      const p = w.pending.get(id);
      if (!p) return;
      w.pending.delete(id);
      // No new work goes to a retiring worker; once idle, a new one replaces it.
      w.retiring ||= Boolean(recycle);
      if (w.retiring && w.pending.size === 0 && workers[slot] === w) {
        start(slot);
        void worker.terminate();
      }
      // An idle pool never keeps the process alive; a busy worker does, so a
      // caller awaiting a result gets it.
      if (w.pending.size === 0) worker.unref();
      if (error) p.reject(new Error(error));
      else p.resolve(report);
    });
    // Crashed or out of memory: fail what it still held, start another.
    const gone = (error) => {
      if (workers[slot] !== w) return;
      for (const p of w.pending.values()) p.reject(error ?? new Error("ooxml-validator worker exited"));
      w.pending.clear();
      if (!closed) start(slot);
    };
    worker.on("error", gone);
    worker.on("exit", () => gone());
    // After the listeners: adding a "message" listener refs the worker again.
    worker.unref();
    workers[slot] = w;
  }
  for (let i = 0; i < threads; i++) start(i);

  let next = 0;
  return {
    /** @param {Uint8Array} docx @param {{ maxErrors?: number }} [options] */
    validate(docx, options) {
      if (closed) return Promise.reject(new Error("the validator pool is closed"));
      // The validator refuses these anyway; refuse before copying them to a thread.
      if (docx.length > MAX_DOCX_BYTES) return Promise.resolve(tooLarge());
      const load = (x) => (x.retiring ? Infinity : x.pending.size);
      const w = workers.reduce((a, b) => (load(b) < load(a) ? b : a));
      const id = next++;
      if (w.pending.size === 0) w.worker.ref();
      return new Promise((resolve, reject) => {
        w.pending.set(id, { resolve, reject });
        // One exact-size copy, moved rather than cloned: a view's whole
        // backing buffer would otherwise be copied, and the caller keeps theirs.
        const bytes = new Uint8Array(docx);
        w.worker.postMessage({ id, docx: bytes, options }, [bytes.buffer]);
      });
    },
    close() {
      closed = true;
      return Promise.all(workers.map((w) => w.worker.terminate()));
    },
    [Symbol.dispose]() {
      this.close();
    },
  };
}
