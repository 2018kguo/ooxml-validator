import type { Report } from "./index.js";

export interface ValidatorPool {
  /** As `validate`, on a worker thread. */
  validate(docx: Uint8Array, options?: { maxErrors?: number }): Promise<Report>;
  close(): Promise<unknown>;
  [Symbol.dispose](): void;
}

/** Worker threads (default 1), each with its own WebAssembly instance. Node only. */
export function createValidatorPool(options?: { threads?: number }): ValidatorPool;
