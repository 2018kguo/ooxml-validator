"""Draws docs/social.png (1280x640, the repository's social preview) from
bench/results.json with headless Chrome: name, pitch, one measured number, and
a page with the kind of error it reports."""
import json, os, subprocess, sys, tempfile

ROOT = os.path.join(os.path.dirname(__file__), "..")
CHROME = os.environ.get("CHROME", "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")
runs = json.load(open(os.path.join(os.path.dirname(__file__), "results.json")))["runs"]
large = runs[next(k for k in runs if k != "corpus")]
ratio = large["sdk"]["seconds"] / large["ours"]["seconds"]

FONT = '-apple-system, BlinkMacSystemFont, "Segoe UI", "Helvetica Neue", Arial, sans-serif'
MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
lines = [96, 100, 92, 98, 60, 0, 94, 99, 88, 97, 72, 0, 100, 95, 90, 64]
page = "".join(
    '<div class="gap"></div>' if w == 0 else
    f'<div class="l{" bad" if i == 8 else ""}" style="width:{w}%"></div>' for i, w in enumerate(lines))
html = f"""<!doctype html><meta charset=utf-8><style>
*{{box-sizing:border-box}}body{{margin:0;width:1280px;height:640px;background:#1a1a19;color:#f0efec;font-family:{FONT};overflow:hidden;position:relative}}
.left{{position:absolute;left:80px;top:92px;width:680px}}
h1{{font-size:88px;font-weight:700;letter-spacing:-.03em;margin:0 0 22px}}
p{{font-size:30px;line-height:1.3;color:#c3c2b7;margin:0}}
.n{{margin-top:50px;font-size:40px;font-weight:650;letter-spacing:-.01em}}.n i{{font-style:normal;color:#3987e5}}
.s{{margin-top:8px;font-size:22px;color:#c3c2b7}}
.page{{position:absolute;right:76px;top:70px;width:400px;height:500px;background:#fcfcfb;border-radius:10px;box-shadow:0 12px 50px #0009;transform:rotate(1.5deg);padding:46px 40px}}
.h{{height:16px;width:58%;background:#2b2b2a;border-radius:3px;margin-bottom:26px}}
.l{{height:9px;background:#9a9994;border-radius:2px;margin:0 0 13px}}.gap{{height:14px}}
.bad{{background:#9a9994;position:relative}}
.bad:after{{content:"";position:absolute;left:0;right:0;bottom:-7px;height:6px;background:radial-gradient(circle at 3px -1px,transparent 3px,#e5383b 3.5px,#e5383b 4.6px,transparent 5px) 0 0/8px 6px repeat-x}}
.tip{{position:absolute;left:30px;top:300px;background:#2b2b2a;color:#f0efec;font:500 14px/1.45 {MONO};padding:10px 13px;border-radius:7px;box-shadow:0 6px 20px #0006}}
.tip b{{color:#ff8a8c;font-weight:600}}
</style><div class="left"><h1>ooxml-validator</h1><p>Microsoft's Open XML SDK validator for Word documents, in Rust. The same verdicts, without .NET.</p>
<div class="n"><i>{ratio:.0f}×</i> faster than the Open XML SDK</div><div class="s">The same error ids on every one of 9,000+ test documents</div></div>
<div class="page"><div class="h"></div>{page}<div class="tip"><b>Sch_UnexpectedElementContentExpectingComplex</b><br>/word/document.xml w:rPr</div></div>"""
with tempfile.NamedTemporaryFile("w", suffix=".html", delete=False) as f:
    f.write(html)
out = os.path.abspath(os.path.join(ROOT, "docs/social.png"))
subprocess.run([CHROME, "--headless", "--disable-gpu", "--hide-scrollbars", "--force-device-scale-factor=1",
                "--window-size=1280,640", f"--screenshot={out}", "file://" + f.name], check=True, capture_output=True)
print(out)
