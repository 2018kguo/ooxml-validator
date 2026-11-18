"""Mutation fuzzer: writes .docx, .xlsx and .pptx variants with a few random
edits each.

usage: python3 fuzz/mutate.py LIST OUT_DIR COUNT [SEED]
  LIST: one source package per line. Each mutant edits one XML part of a
  source (document, styles, sheets, slides, charts, drawings...) with 1-3 of:
  delete / duplicate / swap / move an element, rename it to a sibling's
  name, drop / add / perturb an attribute, wrap it in mc:AlternateContent,
  add an unknown or ignorable child. Feed OUT_DIR's list.txt to diff.py.
"""
import os, random, sys, zipfile
from xml.dom import minidom

VALUES = ["", "0", "1", "-1", "2147483648", "-0", "1.5", "1e3", " 7", "abc", "true", "on", "off", "t",
          "auto", "none", "single", "FF00AA", "zz", "x" * 300, "rId999", "999999999999", "INF", "2026-11-17T00:00:00Z"]
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"


def elements(node):
    out = []
    stack = [node]
    while stack:
        n = stack.pop()
        for c in n.childNodes:
            if c.nodeType == c.ELEMENT_NODE:
                out.append(c)
                stack.append(c)
    return out


def mutate(doc, rng):
    root = doc.documentElement
    els = [e for e in elements(root)]
    if not els:
        return "none"
    e = rng.choice(els)
    parent = e.parentNode
    op = rng.choice(["delete", "duplicate", "swap", "move", "rename", "drop_attr", "add_attr", "perturb", "perturb", "perturb",
                     "alternate", "unknown_child", "ignorable_child"])
    if op == "delete":
        parent.removeChild(e)
    elif op == "duplicate":
        parent.insertBefore(e.cloneNode(True), e)
    elif op == "swap":
        sib = e.nextSibling
        while sib is not None and sib.nodeType != sib.ELEMENT_NODE:
            sib = sib.nextSibling
        if sib is None:
            return "noop"
        parent.removeChild(sib)
        parent.insertBefore(sib, e)
    elif op == "move":
        target = rng.choice(els)
        if target is e or e in elements(target) or target in elements(e):
            return "noop"
        parent.removeChild(e)
        target.appendChild(e)
    elif op == "rename":
        names = sorted({c.tagName for c in els})
        new = doc.createElement(rng.choice(names))
        for k, v in e.attributes.items():
            new.setAttribute(k, v)
        while e.firstChild:
            new.appendChild(e.firstChild)
        parent.replaceChild(new, e)
    elif op == "drop_attr":
        keys = [k for k in e.attributes.keys() if not k.startswith("xmlns")]
        if not keys:
            return "noop"
        e.removeAttribute(rng.choice(keys))
    elif op == "add_attr":
        prefix = e.tagName.split(":")[0] if ":" in e.tagName else None
        local = rng.choice(["val", "id", "w", "type", "color", "space", "ascii", "fill", "name", "bogus", "r", "s", "t", "sheetId", "xfId", "ref", "sqref", "spid"])
        e.setAttribute(f"{prefix}:{local}" if prefix else local, rng.choice(VALUES))
    elif op == "perturb":
        keys = [k for k in e.attributes.keys() if not k.startswith("xmlns")]
        if not keys:
            return "noop"
        e.setAttribute(rng.choice(keys), rng.choice(VALUES))
    elif op == "alternate":
        root.setAttribute("xmlns:mc", MC)
        ac = doc.createElement("mc:AlternateContent")
        choice = doc.createElement("mc:Choice")
        req = rng.choice(["w14", "wps", "w15", "zz", "w", "x14", "p14", "a14"])
        choice.setAttribute("Requires", req)
        fallback = doc.createElement("mc:Fallback")
        parent.replaceChild(ac, e)
        choice.appendChild(e)
        ac.appendChild(choice)
        if rng.random() < 0.7:
            fallback.appendChild(e.cloneNode(True))
            ac.appendChild(fallback)
    elif op == "unknown_child":
        e.appendChild(doc.createElement(rng.choice(["w:bogus", "zz:thing", "w14:bogus", "unprefixed", "x:bogus", "a:bogus"])))
        if "zz:" in e.lastChild.tagName:
            root.setAttribute("xmlns:zz", "urn:example:zz")
    elif op == "ignorable_child":
        root.setAttribute("xmlns:mc", MC)
        root.setAttribute("xmlns:zz", "urn:example:zz")
        root.setAttribute("mc:Ignorable", (root.getAttribute("mc:Ignorable") + " zz").strip())
        if rng.random() < 0.5:
            root.setAttribute("mc:ProcessContent", "zz:wrap")
        w = doc.createElement(rng.choice(["zz:wrap", "zz:other"]))
        parent.replaceChild(w, e)
        w.appendChild(e)
    return op


def main():
    sources = [l.strip() for l in open(sys.argv[1]) if l.strip()]
    out = sys.argv[2]
    count = int(sys.argv[3])
    rng = random.Random(int(sys.argv[4]) if len(sys.argv) > 4 else 1)
    os.makedirs(out, exist_ok=True)
    made = []
    attempts = 0
    while len(made) < count and attempts < count * 5:
        attempts += 1
        src = rng.choice(sources)
        try:
            z = zipfile.ZipFile(src)
            # Test corpora hold zip bombs: skip a source that inflates past 64 MB.
            if sum(i.file_size for i in z.infolist()) > 64 << 20:
                continue
            files = {n: z.read(n) for n in z.namelist()}
        except Exception:
            continue
        parts = [n for n in files if n.startswith(("word/", "xl/", "ppt/")) and n.endswith(".xml") and len(files[n]) < 400_000]
        if not parts:
            continue
        part = rng.choice(parts)
        try:
            doc = minidom.parseString(files[part])
        except Exception:
            continue
        ops = [mutate(doc, rng) for _ in range(rng.randint(1, 3))]
        files[part] = doc.toxml(encoding="UTF-8")
        name = os.path.join(out, f"m{len(made):05d}{os.path.splitext(src)[1].lower()}")
        with zipfile.ZipFile(name, "w", zipfile.ZIP_DEFLATED) as o:
            for n, b in files.items():
                o.writestr(n, b)
        made.append(f"{name}\t{src}\t{part}\t{'+'.join(ops)}")
    with open(os.path.join(out, "list.txt"), "w") as f:
        f.write("\n".join(m.split("\t")[0] for m in made) + "\n")
    with open(os.path.join(out, "manifest.tsv"), "w") as f:
        f.write("\n".join(made) + "\n")
    print(len(made), "mutants")


main()
