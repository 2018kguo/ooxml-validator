// Validates .docx, .xlsx and .pptx bytes as Microsoft's Open XML SDK validator
// would, in WebAssembly.
let module;
let instance;
let lastMemoryBytes = 0;

/** @internal The validator's own input limit (src/package.rs), checked before any copy. */
export const MAX_DOCX_BYTES = 100 << 20;
const FORMATS = { word: 1, excel: 2, powerpoint: 3 };
const NOUNS = { word: "DOCX", excel: "XLSX", powerpoint: "PPTX" };

/** @internal */
export const tooLarge = (format) => ({ ok: false, openError: `${NOUNS[format] ?? "The package"} exceeds the 100 MB validation limit.`, errors: [] });

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
 * @param {Uint8Array} docx the package's bytes (.docx, .xlsx or .pptx)
 * @param {{ maxErrors?: number, format?: "word" | "excel" | "powerpoint" }} [options]
 *   cap the list (default 1000, the SDK's; 0: all); open it as this kind of
 *   package (default: the kind its main part's content type names)
 */
export async function validate(docx, { maxErrors = 1000, format } = {}) {
  if (format !== undefined && !(format in FORMATS)) throw new TypeError(`unknown format ${format}`);
  if (docx.length > MAX_DOCX_BYTES) return tooLarge(format);
  instance ??= await load();
  const { memory, alloc, dealloc, validate_as: run } = instance;
  const input = alloc(docx.length);
  new Uint8Array(memory.buffer, input, docx.length).set(docx);
  const packed = run(input, docx.length, maxErrors, FORMATS[format] ?? 0);
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
