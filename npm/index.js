// Validates .docx bytes as Microsoft's Open XML SDK validator would, in WebAssembly.
let module;
let instance;
let lastMemoryBytes = 0;

/** @internal The validator's own input limit (src/package.rs), checked before any copy. */
export const MAX_DOCX_BYTES = 100 << 20;
/** @internal */
export const tooLarge = () => ({ ok: false, openError: "DOCX exceeds the 100 MB validation limit.", errors: [] });

/** @internal The WebAssembly memory the last call grew to (the worker pool reads it). */
export const lastMemory = () => lastMemoryBytes;

async function load() {
  if (!module) {
    const url = new URL("./ooxml_validator.wasm", import.meta.url);
    const bytes = url.protocol === "file:"
      ? await (await import("node:fs/promises")).readFile(url)
      : new Uint8Array(await (await fetch(url)).arrayBuffer());
    module = await WebAssembly.compile(bytes);
  }
  return (await WebAssembly.instantiate(module, {})).exports;
}

/**
 * @param {Uint8Array} docx the document's bytes
 * @param {{ maxErrors?: number }} [options] cap the list (default 1000, the SDK's; 0: all)
 */
export async function validate(docx, { maxErrors = 1000 } = {}) {
  if (docx.length > MAX_DOCX_BYTES) return tooLarge();
  instance ??= await load();
  const { memory, alloc, dealloc, validate: run } = instance;
  const input = alloc(docx.length);
  new Uint8Array(memory.buffer, input, docx.length).set(docx);
  const packed = run(input, docx.length, maxErrors);
  dealloc(input, docx.length);
  const ptr = Number(packed >> 32n);
  const len = Number(packed & 0xffffffffn);
  const json = new TextDecoder().decode(new Uint8Array(memory.buffer, ptr, len));
  dealloc(ptr, len);
  // WebAssembly memory only grows. Typical documents peak at 8-15 MB; after a
  // larger one, start the next call with a fresh instance (from the compiled
  // module, so it costs well under a millisecond) instead of keeping the peak.
  lastMemoryBytes = memory.buffer.byteLength;
  if (lastMemoryBytes > 32 * 1024 * 1024) instance = undefined;
  return JSON.parse(json);
}
