"""Times ooxml-validator against Microsoft's Open XML SDK on the same files.

usage: python3 bench/compare.py --sdk CMD --ours BIN [--rounds N] --large FILE... --corpus LIST

  --sdk     the SDK oracle as a command (fuzz/oracle, ideally published
            ReadyToRun: `dotnet publish -c Release -p:PublishReadyToRun=true`)
  --ours    target/release/ooxml-validator
  --large   documents timed one process each (cold start included)
  --corpus  a file listing many documents, validated in one process each side

Each round runs both sides alternately in fresh processes, so a noisy moment
lands on both. Reports medians of wall time and of peak resident memory, and
writes bench/results.json for bench/chart.py.
"""
import argparse, json, os, platform, statistics, subprocess, sys, time

ap = argparse.ArgumentParser()
ap.add_argument("--sdk", required=True)
ap.add_argument("--ours", required=True)
ap.add_argument("--wasm", help="the npm package's CLI, e.g. 'node npm/cli.js'")
ap.add_argument("--rounds", type=int, default=7)
ap.add_argument("--large", nargs="*", default=[])
ap.add_argument("--corpus")
args = ap.parse_args()


def run(cmd, stdin=None):
    t = time.perf_counter()
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    if stdin is not None:
        p.stdin.write(stdin)
    p.stdin.close()
    out = p.stdout.read()
    _, status, usage = os.wait4(p.pid, 0)
    wall = time.perf_counter() - t
    assert os.waitstatus_to_exitcode(status) in (0, 1), cmd
    rss = usage.ru_maxrss if sys.platform == "darwin" else usage.ru_maxrss * 1024
    return wall, rss, out


def sdk(paths):
    return run(args.sdk.split(), ("\n".join(paths) + "\n").encode())


def ours(paths):
    return run([args.ours, "--json", *paths])


def wasm(paths):
    return run([*args.wasm.split(), "--json", *paths])


def measure(name, paths):
    sides = (("sdk", sdk), ("ours", ours)) + ((("wasm", wasm),) if args.wasm else ())
    rows = {side: [] for side, _ in sides}
    for _ in range(args.rounds):
        for side, f in sides:
            wall, rss, out = f(paths)
            rows[side].append((wall, rss))
    # Same verdicts, or the timing means nothing.
    a = [json.loads(l)["Ok"] for l in sdk(paths)[2].splitlines()]
    b = [json.loads(l)["ok"] for l in ours(paths)[2].splitlines()]
    assert a == b, f"{name}: verdicts differ"
    res = {side: {"seconds": statistics.median(w for w, _ in r), "peak_bytes": statistics.median(m for _, m in r)} for side, r in rows.items()}
    res["files"] = len(paths)
    print(f"{name}: SDK {res['sdk']['seconds']*1e3:.0f} ms {res['sdk']['peak_bytes']/2**20:.0f} MB | "
          f"ooxml-validator {res['ours']['seconds']*1e3:.0f} ms {res['ours']['peak_bytes']/2**20:.0f} MB | "
          f"{res['sdk']['seconds']/res['ours']['seconds']:.1f}x faster"
          + (f" | wasm {res['wasm']['seconds']*1e3:.0f} ms {res['wasm']['peak_bytes']/2**20:.0f} MB" if args.wasm else ""), flush=True)
    return res


results = {"machine": platform.machine(), "cpu": subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True).stdout.strip() if sys.platform == "darwin" else platform.processor(), "rounds": args.rounds, "runs": {}}
for f in args.large:
    results["runs"][os.path.basename(f)] = measure(os.path.basename(f), [f])
if args.corpus:
    paths = [l.strip() for l in open(args.corpus) if l.strip()]
    results["runs"]["corpus"] = measure(f"corpus ({len(paths)} files)", paths)
json.dump(results, open(os.path.join(os.path.dirname(__file__), "results.json"), "w"), indent=2)
