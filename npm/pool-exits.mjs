// The pool never keeps a process alive when idle, and a pending result does: CI runs this under a timeout.
import { createValidatorPool } from "./worker.js";
const pool = createValidatorPool({ threads: 2 });
const rs = await Promise.all([1, 2, 3].map(() => pool.validate(new TextEncoder().encode("x"))));
console.log(rs.map((r) => r.ok).join(","));
pool.validate(new TextEncoder().encode("y")).then((r) => console.log("unawaited still delivered:", r.ok));
