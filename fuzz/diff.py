"""Differential check: obstat vs the Open XML SDK on a list of .docx files.

usage: python3 fuzz/diff.py LIST OUT_DIR [--jobs N] [--schema]
  LIST: one .docx path per line. OBSTAT is the obstat binary (default
  target/release/obstat); ORACLE the built SDK oracle (default
  fuzz/oracle/bin/out/Oracle.dll: dotnet build -c Release -o bin/out there).

Compares error ids over all parts; --schema leaves out the SDK's semantic
(Sem_*) and package (Pkg_*) errors, which obstat does not check yet:
  danger  obstat accepts what the SDK rejects   (must be zero)
  extra   obstat rejects what the SDK accepts
  ids     both reject, with different error id multisets
"""
import collections, json, os, subprocess, sys, threading

def sdk(paths, dll, jobs):
    out = {}
    chunks = [paths[i::jobs] for i in range(jobs)]
    def run(chunk):
        p = subprocess.Popen(["dotnet", dll], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        res, _ = p.communicate("\n".join(chunk) + "\n")
        for line in res.splitlines():
            try:
                d = json.loads(line)
            except ValueError:
                continue
            out[d["Path"]] = d
    ts = [threading.Thread(target=run, args=(c,)) for c in chunks if c]
    [t.start() for t in ts]; [t.join() for t in ts]
    return out

def obstat(paths, binary):
    out = {}
    for i in range(0, len(paths), 500):
        res = subprocess.run([binary] + paths[i:i+500], capture_output=True, text=True).stdout
        for line in res.splitlines():
            d = json.loads(line); out[d["path"]] = d
    return out

def main():
    paths = [l.strip() for l in open(sys.argv[1]) if l.strip()]
    outdir = sys.argv[2]; os.makedirs(outdir, exist_ok=True)
    global schema_only; schema_only = "--schema" in sys.argv
    jobs = int(sys.argv[sys.argv.index("--jobs") + 1]) if "--jobs" in sys.argv else 6
    here = os.path.dirname(os.path.abspath(__file__))
    s = sdk(paths, os.environ.get("ORACLE", os.path.join(here, "oracle/bin/out/Oracle.dll")), jobs)
    p = obstat(paths, os.environ.get("OBSTAT", os.path.join(here, "../target/release/obstat")))
    counts = collections.Counter(); rows = []
    for path in paths:
        a, b = s.get(path), p.get(path)
        if a is None or b is None:
            counts["missing"] += 1; continue
        if a.get("OpenError") and not a.get("Errors"):
            counts["sdk-open-error"] += 1; continue
        sdk_ids = sorted(e["Id"] for e in a["Errors"] if not (schema_only and e["Id"].startswith(("Sem_", "Pkg_"))))
        mine = sorted(b["ids"])
        if not sdk_ids and not mine: kind = "agree-ok"
        elif sdk_ids and not mine: kind = "danger"
        elif mine and not sdk_ids: kind = "extra"
        elif set(sdk_ids) == set(mine): kind = "agree-bad"
        else: kind = "ids"
        counts[kind] += 1
        if kind in ("danger", "extra", "ids"):
            first = next((e for e in a["Errors"] if e["Id"] in set(sdk_ids) - set(mine)), a["Errors"][0] if a["Errors"] else None)
            rows.append({"kind": kind, "path": path, "sdk": sorted(set(sdk_ids)), "obstat": sorted(set(mine)),
                         "sdk_first": first and (first.get("Part"), first.get("Node"), first.get("Description", "")[:200], first.get("XPath")), "proof_first": b.get("first", "")[:200]})
    json.dump(rows, open(os.path.join(outdir, "diffs.json"), "w"), indent=1)
    print(dict(counts))

main()
