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

export class TimeoutError extends Error {
  name = "TimeoutError";
}

/**
 * @param {{ threads?: number }} [options]
 */
export function createValidatorPool({ threads = 1 } = {}) {
  let closed = false;
  let next = 0;
  const workers = [];

  // A worker answers in the order it was asked, so its first pending call is
  // the one running; the rest are queued behind it.
  const running = (w) => w.pending.keys().next().value;

  function send(w, id, call) {
    if (w.pending.size === 0) w.worker.ref();
    w.pending.set(id, call);
    // One exact-size copy, moved rather than cloned: a view's whole backing
    // buffer would otherwise be copied, and the caller keeps theirs.
    const bytes = new Uint8Array(call.docx);
    w.worker.postMessage({ id, docx: bytes, options: call.options }, [bytes.buffer]);
  }

  function settle(w, id) {
    const call = w.pending.get(id);
    if (!call) return undefined;
    w.pending.delete(id);
    clearTimeout(call.timer);
    // An idle pool never keeps the process alive; a busy worker does, so a
    // caller awaiting a result gets it.
    if (w.pending.size === 0) w.worker.unref();
    return call;
  }

  // Stop a worker and start another in its slot. The call it was running fails
  // with `error`; calls queued behind it move to the new worker untouched.
  function replace(w, error) {
    const slot = workers.indexOf(w);
    if (slot < 0) return;
    const runningId = running(w);
    const queued = [...w.pending].filter(([id]) => id !== runningId);
    const failed = runningId === undefined ? undefined : settle(w, runningId);
    for (const [id] of queued) settle(w, id);
    w.pending.clear();
    void w.worker.terminate();
    if (closed) {
      workers.splice(slot, 1);
      failed?.reject(error);
      for (const [, call] of queued) call.reject(error);
      return;
    }
    const fresh = start();
    workers[slot] = fresh;
    failed?.reject(error);
    for (const [id, call] of queued) send(fresh, id, call);
  }

  function start() {
    // The parent's flags (e.g. --input-type) need not suit a worker.
    const worker = new Worker(new URL(import.meta.url), { execArgv: [] });
    const w = { worker, pending: new Map(), retiring: false };
    worker.on("message", ({ id, report, error, recycle }) => {
      const call = settle(w, id);
      // No new work goes to a retiring worker; once idle, a new one replaces it.
      w.retiring ||= Boolean(recycle);
      if (w.retiring && w.pending.size === 0) replace(w);
      if (!call) return;
      if (error) call.reject(new Error(error));
      else call.resolve(report);
    });
    // Crashed or out of memory: the call it was running fails, the rest move on.
    const gone = (error) => {
      if (workers.includes(w)) replace(w, error ?? new Error("ooxml-validator worker exited"));
    };
    worker.on("error", gone);
    worker.on("exit", () => gone());
    // After the listeners: adding a "message" listener refs the worker again.
    worker.unref();
    return w;
  }
  for (let i = 0; i < threads; i++) workers.push(start());

  return {
    /**
     * @param {Uint8Array} docx
     * @param {{ maxErrors?: number, timeoutMs?: number }} [options]
     */
    validate(docx, { timeoutMs, ...options } = {}) {
      if (closed) return Promise.reject(new Error("the validator pool is closed"));
      // The validator refuses these anyway; refuse before copying them to a thread.
      if (docx.length > MAX_DOCX_BYTES) return Promise.resolve(tooLarge());
      const load = (x) => (x.retiring ? Infinity : x.pending.size);
      const w = workers.reduce((a, b) => (load(b) < load(a) ? b : a));
      const id = next++;
      return new Promise((resolve, reject) => {
        const call = { docx, options, resolve, reject, timer: undefined };
        if (timeoutMs !== undefined) {
          call.timer = setTimeout(() => {
            const owner = workers.find((x) => x.pending.has(id));
            if (!owner) return;
            const error = new TimeoutError(`validation took longer than ${timeoutMs} ms`);
            // Running: stop that worker so the abandoned work ends there.
            // Still queued behind another call: just give up on it.
            if (running(owner) === id) replace(owner, error);
            else settle(owner, id)?.reject(error);
          }, timeoutMs);
        }
        send(w, id, call);
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
