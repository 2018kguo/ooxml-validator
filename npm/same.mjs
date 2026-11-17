// node npm/same.mjs NATIVE_BIN LIST: the WebAssembly build reports exactly what
// the native build does (ok, open error, error ids) on every file in LIST.
import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { validate } from "./index.js";

const [bin, list] = process.argv.slice(2);
const paths = (await readFile(list, "utf8")).split("\n").filter(Boolean);
let differ = 0;
for (let i = 0; i < paths.length; i += 200) {
  const chunk = paths.slice(i, i + 200);
  let out;
  try {
    out = execFileSync(bin, ["--json", "--max-errors", "0", ...chunk], { maxBuffer: 1 << 30 });
  } catch (e) {
    out = e.stdout; // exit 1: some file is invalid
  }
  const native = out.toString().split("\n").filter(Boolean).map((l) => JSON.parse(l));
  for (const n of native) {
    const w = await validate(await readFile(n.path), { maxErrors: 0 });
    const key = (r) => JSON.stringify([r.ok, !!r.openError, r.errors.map((e) => e.id)]);
    if (key(w) !== key(n)) {
      differ++;
      console.log("differs:", n.path);
    }
  }
}
console.log(`${paths.length} files, ${differ} differ`);
process.exit(differ ? 1 : 0);
