// Validates .docx bytes as Microsoft's Open XML SDK validator would, in WebAssembly.
let instance;

async function load() {
  const url = new URL("./ooxml_validator.wasm", import.meta.url);
  const bytes = url.protocol === "file:"
    ? await (await import("node:fs/promises")).readFile(url)
    : new Uint8Array(await (await fetch(url)).arrayBuffer());
  return (await WebAssembly.instantiate(bytes, {})).instance.exports;
}

/**
 * @param {Uint8Array} docx the document's bytes
 * @param {{ maxErrors?: number }} [options] cap the list (default 1000, the SDK's; 0: all)
 */
export async function validate(docx, { maxErrors = 1000 } = {}) {
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
  // WebAssembly memory only grows: after a huge document, start the next
  // call with a fresh instance rather than keep the peak for good.
  if (memory.buffer.byteLength > 256 * 1024 * 1024) instance = undefined;
  return JSON.parse(json);
}
