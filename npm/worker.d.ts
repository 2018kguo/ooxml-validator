import type { Format, Report } from "./index.js";

export interface ValidatorPool {
  /**
   * As `validate`, on a worker thread. With `timeoutMs`, a call still unanswered
   * then rejects with a `TimeoutError`; if it was running, its worker is
   * replaced and the calls queued behind it move to the new one.
   */
  validate(docx: Uint8Array, options?: { maxErrors?: number; format?: Format; timeoutMs?: number }): Promise<Report>;
  close(): Promise<unknown>;
  [Symbol.dispose](): void;
}

export class TimeoutError extends Error {
  name: "TimeoutError";
}

/**
 * Worker threads (default 1), each with its own WebAssembly instance. Node only.
 * `onReplace` hears each worker replaced: a call ran past its timeout, the
 * worker exited or crashed, or it grew past 64 MB and was retired once idle.
 */
export function createValidatorPool(options?: {
  threads?: number;
  onReplace?: (reason: "timeout" | "exit" | "memory") => void;
}): ValidatorPool;
