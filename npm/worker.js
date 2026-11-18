// Validation off the main thread (Node): a small pool of worker threads, each
// with its own WebAssembly instance.
import { Worker, isMainThread, parentPort } from "node:worker_threads";
import { validate } from "./index.js";

if (!isMainThread && parentPort) {
  parentPort.on("message", async ({ id, docx, options }) => {
    try {
      parentPort.postMessage({ id, report: await validate(docx, options) });
    } catch (error) {
      parentPort.postMessage({ id, error: String(error?.stack ?? error) });
    }
  });
}

/**
 * @param {{ threads?: number }} [options]
 */
export function createValidatorPool({ threads = 1 } = {}) {
  const workers = Array.from({ length: threads }, () => {
    // The parent's flags (e.g. --input-type) need not suit a worker.
    const worker = new Worker(new URL(import.meta.url), { execArgv: [] });
    return { worker, busy: 0 };
  });
  const pending = new Map();
  let next = 0;
  for (const w of workers) {
    w.worker.on("message", ({ id, report, error }) => {
      const p = pending.get(id);
      pending.delete(id);
      // An idle pool never keeps the process alive; a busy worker does, so a
      // caller awaiting a result gets it.
      if (--w.busy === 0) w.worker.unref();
      if (error) p.reject(new Error(error));
      else p.resolve(report);
    });
    w.worker.on("error", (error) => {
      for (const [id, p] of pending) if (p.worker === w) (pending.delete(id), p.reject(error));
      w.busy = 0;
      w.worker.unref();
    });
    // After the listeners: adding a "message" listener refs the worker again.
    w.worker.unref();
  }
  return {
    /** @param {Uint8Array} docx @param {{ maxErrors?: number }} [options] */
    validate(docx, options) {
      const w = workers.reduce((a, b) => (b.busy < a.busy ? b : a));
      const id = next++;
      if (w.busy++ === 0) w.worker.ref();
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject, worker: w });
        w.worker.postMessage({ id, docx, options });
      });
    },
    close() {
      return Promise.all(workers.map((w) => w.worker.terminate()));
    },
    [Symbol.dispose]() {
      this.close();
    },
  };
}
