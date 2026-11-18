import type { Report } from "./index.js";

export interface ValidatorPool {
  /**
   * As `validate`, on a worker thread. With `timeoutMs`, a call still unanswered
   * then rejects with a `TimeoutError`; if it was running, its worker is
   * replaced and the calls queued behind it move to the new one.
   */
  validate(docx: Uint8Array, options?: { maxErrors?: number; timeoutMs?: number }): Promise<Report>;
  close(): Promise<unknown>;
  [Symbol.dispose](): void;
}

export class TimeoutError extends Error {
  name: "TimeoutError";
}

/** Worker threads (default 1), each with its own WebAssembly instance. Node only. */
export function createValidatorPool(options?: { threads?: number }): ValidatorPool;
