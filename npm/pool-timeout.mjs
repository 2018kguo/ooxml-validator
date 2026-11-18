// A call that times out while running stops only its own worker: the call
// queued behind it on that worker and the one on the other worker both
// complete, and a moved call keeps its own deadline. CI runs it on bench/generate.py's report: node npm/pool-timeout.mjs report.docx
import { readFileSync } from "node:fs";
import { TimeoutError, createValidatorPool } from "./worker.js";

const big = readFileSync(process.argv[2]);
const pool = createValidatorPool({ threads: 2 });
const settled = (p) => p.then((r) => `ok ${r.ok}`, (e) => `${e.name}: ${e.message}`);
// Worker 0: the slow call (1 ms budget, it also compiles the module), then a
// queued one. Worker 1: an independent one.
const slow = settled(pool.validate(big, { timeoutMs: 1 }));
const other = settled(pool.validate(big));
const queued = settled(pool.validate(big));
const results = { slow: await slow, other: await other, queued: await queued };
console.log(JSON.stringify(results));
const ok = results.slow.startsWith("TimeoutError") && results.other === "ok true" && results.queued === "ok true";
// One worker: a call moved to the new worker keeps its deadline and can time
// out there too, freeing that worker for the next call.
const single = createValidatorPool({ threads: 1 });
const first = settled(single.validate(big, { timeoutMs: 1 }));
const moved = settled(single.validate(big, { timeoutMs: 1 }));
const moves = { first: await first, moved: await moved, next: await settled(single.validate(big)) };
console.log(JSON.stringify(moves));
if (!moves.first.startsWith("TimeoutError") || !moves.moved.startsWith("TimeoutError") || moves.next !== "ok true") {
  console.error("FAIL: a moved call lost its deadline");
  process.exit(1);
}
const after = await settled(pool.validate(big, { timeoutMs: 60_000 }));
if (!ok || after !== "ok true" || !(new TimeoutError("x") instanceof Error)) {
  console.error("FAIL", after);
  process.exit(1);
}
console.log("timeout recovery ok");
