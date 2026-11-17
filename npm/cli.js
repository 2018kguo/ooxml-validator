#!/usr/bin/env node
// ooxml-validator [--json] [--max-errors N] FILE.docx...: the Rust CLI's interface.
import { readFile } from "node:fs/promises";
import { validate } from "./index.js";

const args = process.argv.slice(2);
let json = false, maxErrors = 1000;
const paths = [];
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--json") json = true;
  else if (args[i] === "--max-errors") maxErrors = Number(args[++i]);
  else paths.push(args[i]);
}
if (!paths.length || Number.isNaN(maxErrors)) {
  console.error("usage: ooxml-validator [--json] [--max-errors N] FILE.docx...");
  process.exit(2);
}
let ok = true;
for (const path of paths) {
  let r;
  try {
    r = await validate(await readFile(path), { maxErrors });
  } catch (e) {
    r = { ok: false, openError: e.message, errors: [] };
  }
  ok &&= r.ok;
  if (json) console.log(JSON.stringify({ path, ...r }));
  else if (r.openError) console.log(`${path}: cannot be opened: ${r.openError}`);
  else if (r.ok) console.log(`${path}: ok`);
  else {
    console.log(`${path}: ${r.errors.length} error${r.errors.length === 1 ? "" : "s"}`);
    for (const e of r.errors) console.log(`  ${e.part} ${e.node} ${e.id}: ${e.description}`);
  }
}
process.exit(ok ? 0 : 1);
