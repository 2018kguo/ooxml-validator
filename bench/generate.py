"""Writes a large, valid .docx: a report of PAGES pages of headings, formatted
runs, tables, bookmarks and footnote-style small print. Deterministic.

usage: python3 bench/generate.py OUT.docx [PAGES]
"""
import random
import sys
import zipfile

W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
WORDS = ("volume corridor renewal quarter network tractor terminal dwell appointment maintenance budget claims "
         "freight region north south east west export central plan forecast capacity yard crane season").split()

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"""
RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"""
DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"""
STYLES = f"""<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:sz w:val="20"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="240" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:b/><w:sz w:val="32"/></w:rPr></w:style><w:style w:type="table" w:styleId="Grid"><w:name w:val="Table Grid"/><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders></w:tblPr></w:style></w:styles>"""


def run(rng, text):
    props = []
    if rng.random() < 0.15:
        props.append("<w:b/>")
    if rng.random() < 0.1:
        props.append("<w:i/>")
    if rng.random() < 0.2:
        props.append(f'<w:color w:val="{rng.randrange(1 << 24):06X}"/>')
    if rng.random() < 0.2:
        props.append(f'<w:sz w:val="{rng.choice((18, 20, 22, 24))}"/>')
    rpr = f"<w:rPr>{''.join(props)}</w:rPr>" if props else ""
    return f'<w:r w:rsidR="{rng.randrange(1 << 32):08X}">{rpr}<w:t xml:space="preserve">{text}</w:t></w:r>'


def sentence(rng):
    return " ".join(rng.choice(WORDS) for _ in range(rng.randint(8, 18))).capitalize() + ". "


def paragraph(rng):
    runs = "".join(run(rng, sentence(rng)) for _ in range(rng.randint(3, 8)))
    return f'<w:p w:rsidR="{rng.randrange(1 << 32):08X}"><w:pPr><w:spacing w:after="120"/><w:jc w:val="both"/></w:pPr>{runs}</w:p>'


def table(rng, rows):
    grid = "".join('<w:gridCol w:w="1800"/>' for _ in range(5))
    cell = lambda text: f'<w:tc><w:tcPr><w:tcW w:w="1800" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:spacing w:after="0"/></w:pPr>{run(rng, text)}</w:p></w:tc>'
    body = "".join("<w:tr>" + "".join(cell(f"{rng.uniform(0, 999):.1f}") for _ in range(5)) + "</w:tr>" for _ in range(rows))
    return f'<w:tbl><w:tblPr><w:tblStyle w:val="Grid"/><w:tblW w:w="9000" w:type="dxa"/><w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{body}</w:tbl>'


def main():
    out = sys.argv[1]
    pages = int(sys.argv[2]) if len(sys.argv) > 2 else 200
    rng = random.Random(7)
    parts = []
    for page in range(pages):
        parts.append(f'<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:bookmarkStart w:id="{page}" w:name="s{page}"/>{run(rng, f"{page + 1}. Section {page + 1}")}<w:bookmarkEnd w:id="{page}"/></w:p>')
        parts.extend(paragraph(rng) for _ in range(6))
        if page % 2 == 0:
            parts.append(table(rng, 8))
        parts.append('<w:p><w:r><w:br w:type="page"/></w:r></w:p>')
    sect = '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr>'
    document = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:document xmlns:w="{W}" xmlns:r="{R}"><w:body>{"".join(parts)}{sect}</w:body></w:document>'
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", CONTENT_TYPES)
        z.writestr("_rels/.rels", RELS)
        z.writestr("word/_rels/document.xml.rels", DOC_RELS)
        z.writestr("word/styles.xml", STYLES)
        z.writestr("word/document.xml", document)
    print(f"{out}: {pages} pages, document.xml {len(document) / 1e6:.1f} MB")


main()
