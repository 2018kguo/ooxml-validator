"""Draws docs/benchmarks-{light,dark}.svg from bench/results.json."""
import json, os

ROOT = os.path.join(os.path.dirname(__file__), "..")
FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", "Helvetica Neue", Arial, sans-serif'
THEMES = {
    "light": {"text": "#0b0b0b", "secondary": "#52514e", "ours": "#2a78d6", "sdk": "#eb6834"},
    "dark": {"text": "#f0efec", "secondary": "#c3c2b7", "ours": "#3987e5", "sdk": "#d95926"},
}
r = json.load(open(os.path.join(os.path.dirname(__file__), "results.json")))["runs"]
large = next(k for k in r if k != "corpus")


def ms(s):
    return f"{s:.2f} s" if s >= 1 else f"{s * 1000:.0f} ms"


def mb(b):
    return f"{b / 2**20:.0f} MB"


groups = [
    ("Validate one large document", f"{large}, cold start included", r[large], "seconds", ms),
    ("Validate a corpus", f"{r['corpus']['files']} documents in one process", r["corpus"], "seconds", ms),
    ("Peak memory", f"{large}", r[large], "peak_bytes", mb),
]


def svg(theme):
    t = THEMES[theme]
    width, label, bar_max = 880, 150, 560
    out = [f'<style>text{{font-family:{FONT}}}.t{{fill:{t["text"]}}}.s{{fill:{t["secondary"]}}}</style>']
    out.append(f'<rect x="0" y="10" width="14" height="14" rx="4" fill="{t["ours"]}"/><text x="20" y="22" class="t" font-size="15" font-weight="600">ooxml-validator</text>')
    out.append(f'<rect x="160" y="10" width="14" height="14" rx="4" fill="{t["sdk"]}"/><text x="180" y="22" class="t" font-size="15" font-weight="600">Open XML SDK</text>')
    y = 68
    alt = []
    for title, sub, run, key, fmt in groups:
        out.append(f'<text x="0" y="{y}" class="t" font-size="17" font-weight="650">{title}</text><text x="0" y="{y + 19}" class="s" font-size="13">{sub}</text>')
        top = max(run["ours"][key], run["sdk"][key])
        by = y + 32
        for side, name in (("ours", "ooxml-validator"), ("sdk", "Open XML SDK")):
            v = run[side][key]
            w = max(4, bar_max * v / top)
            out.append(f'<text x="0" y="{by + 19}" class="s" font-size="14">{name}</text>')
            out.append(f'<path d="M{label} {by} h{w - 4:.2f} a4 4 0 0 1 4 4 v20 a4 4 0 0 1 -4 4 h-{w - 4:.2f} z" fill="{t[side]}"/>')
            out.append(f'<text x="{label + w + 8:.2f}" y="{by + 19}" class="t" font-size="14" font-weight="650">{fmt(v)}</text>')
            alt.append(f"{title}: {name} {fmt(v)}")
            by += 36
        y = by + 34
    height = y - 30
    aria = "; ".join(alt)
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}" role="img" aria-label="{aria}">\n' + "".join(out) + "</svg>\n"


for theme in THEMES:
    with open(os.path.join(ROOT, f"docs/benchmarks-{theme}.svg"), "w") as f:
        f.write(svg(theme))
print("docs/benchmarks-light.svg, docs/benchmarks-dark.svg")
