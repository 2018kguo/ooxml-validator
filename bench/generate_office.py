"""Writes a large, valid .xlsx or .pptx. Deterministic.

usage: python3 bench/generate_office.py OUT.xlsx [ROWS]    a ledger: ROWS rows of
           dates, shared strings, numbers, formulas and styles (default 100000)
       python3 bench/generate_office.py OUT.pptx [SLIDES]  a deck: SLIDES slides of
           titles, bullets, tables and shapes (default 300)
"""
import random
import sys
import zipfile

XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
WORDS = ("volume corridor renewal quarter network tractor terminal dwell appointment maintenance budget claims "
         "freight region north south east west export central plan forecast capacity yard crane season").split()


def rels(items):
    return XML + f'<Relationships xmlns="{PKG}">' + "".join(f'<Relationship Id="{i}" Type="{REL}{t}" Target="{target}"/>' for i, t, target in items) + "</Relationships>"


def types(overrides):
    o = "".join(f'<Override PartName="{p}" ContentType="{c}"/>' for p, c in overrides)
    return (XML + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            f'<Default Extension="xml" ContentType="application/xml"/>{o}</Types>')


def column(i):
    s = ""
    i += 1
    while i:
        i, r = divmod(i - 1, 26)
        s = chr(65 + r) + s
    return s


def xlsx(out, rows):
    rng = random.Random(7)
    S = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    CT = "application/vnd.openxmlformats-officedocument.spreadsheetml."
    strings = [" ".join(rng.choice(WORDS) for _ in range(rng.randint(1, 4))).capitalize() for _ in range(2000)]
    body = []
    for r in range(1, rows + 1):
        cells = [f'<c r="A{r}" s="1"><v>{45000 + r % 900}</v></c>', f'<c r="B{r}" t="s"><v>{rng.randrange(len(strings))}</v></c>']
        for c in range(2, 9):
            ref = f"{column(c)}{r}"
            k = rng.random()
            if k < 0.6:
                cells.append(f'<c r="{ref}" s="2"><v>{rng.uniform(-1e5, 1e5):.2f}</v></c>')
            elif k < 0.8:
                cells.append(f'<c r="{ref}"><v>{rng.randrange(1000)}</v></c>')
            elif k < 0.9:
                cells.append(f'<c r="{ref}" s="3"><f>SUM(C{r}:D{r})</f><v>{rng.uniform(0, 1e5):.2f}</v></c>')
            else:
                cells.append(f'<c r="{ref}" t="b"><v>{rng.randrange(2)}</v></c>')
        body.append(f'<row r="{r}" spans="1:9">{"".join(cells)}</row>')
    sheet = (XML + f'<worksheet xmlns="{S}" xmlns:r="{REL[:-1]}"><dimension ref="A1:I{rows}"/><sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>'
             f'<sheetFormatPr defaultRowHeight="15"/><cols><col min="1" max="1" width="12" customWidth="1"/><col min="2" max="2" width="30" customWidth="1"/></cols><sheetData>{"".join(body)}</sheetData>'
             f'<autoFilter ref="A1:I{rows}"/><conditionalFormatting sqref="C2:C{rows}"><cfRule type="cellIs" dxfId="0" priority="1" operator="lessThan"><formula>0</formula></cfRule></conditionalFormatting>'
             '<dataValidations count="1"><dataValidation type="list" allowBlank="1" showInputMessage="1" promptTitle="Region" prompt="Pick a region" sqref="B2:B10"><formula1>"North,South"</formula1></dataValidation></dataValidations>'
             '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/></worksheet>')
    styles = (XML + f'<styleSheet xmlns="{S}"><numFmts count="1"><numFmt numFmtId="164" formatCode="#,##0.00"/></numFmts><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts>'
              '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
              '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="4"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>'
              '<xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/><xf numFmtId="164" fontId="1" fillId="0" borderId="0" xfId="0" applyNumberFormat="1" applyFont="1"/></cellXfs>'
              '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles><dxfs count="1"><dxf><font><color rgb="FF9C0006"/></font></dxf></dxfs></styleSheet>')
    sst = XML + f'<sst xmlns="{S}" count="{rows}" uniqueCount="{len(strings)}">' + "".join(f"<si><t>{s}</t></si>" for s in strings) + "</sst>"
    workbook = (XML + f'<workbook xmlns="{S}" xmlns:r="{REL[:-1]}"><bookViews><workbookView/></bookViews><sheets><sheet name="Ledger" sheetId="1" r:id="rId1"/></sheets>'
                f'<definedNames><definedName name="_xlnm._FilterDatabase" localSheetId="0" hidden="1">Ledger!$A$1:$I${rows}</definedName></definedNames><calcPr calcId="191029"/></workbook>')
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", types([("/xl/workbook.xml", CT + "sheet.main+xml"), ("/xl/worksheets/sheet1.xml", CT + "worksheet+xml"), ("/xl/styles.xml", CT + "styles+xml"), ("/xl/sharedStrings.xml", CT + "sharedStrings+xml")]))
        z.writestr("_rels/.rels", rels([("rId1", "officeDocument", "xl/workbook.xml")]))
        z.writestr("xl/workbook.xml", workbook)
        z.writestr("xl/_rels/workbook.xml.rels", rels([("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "styles", "styles.xml"), ("rId3", "sharedStrings", "sharedStrings.xml")]))
        z.writestr("xl/worksheets/sheet1.xml", sheet)
        z.writestr("xl/styles.xml", styles)
        z.writestr("xl/sharedStrings.xml", sst)
    return len(sheet)


def pptx(out, slides):
    rng = random.Random(7)
    A = "http://schemas.openxmlformats.org/drawingml/2006/main"
    NS = f'xmlns:a="{A}" xmlns:r="{REL[:-1]}" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"'
    CT = "application/vnd.openxmlformats-officedocument.presentationml."
    GRP = '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'

    def sp(i, name, x, y, cx, cy, paras, ph=""):
        ps = "".join(f'<a:p><a:pPr lvl="{lvl}"/><a:r><a:rPr lang="en-US" sz="{rng.choice((1400, 1800, 2000))}" dirty="0"/><a:t>{t}</a:t></a:r></a:p>' for lvl, t in paras)
        return (f'<p:sp><p:nvSpPr><p:cNvPr id="{i}" name="{name}"/><p:cNvSpPr/><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm>'
                f'<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{rng.randrange(1 << 24):06X}"/></a:solidFill></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{ps}</p:txBody></p:sp>')

    grid = '<a:gridCol w="2103120"/>' * 5
    line = ('<a:ln w="6350">' + '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>' + "</a:ln>") * 3
    effects = "<a:effectStyle><a:effectLst/></a:effectStyle>" * 3

    def table(i):
        rows = "".join('<a:tr h="370840">' + "".join(f'<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>{rng.choice(WORDS)} {rng.randrange(1000)}</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>' for _ in range(5)) + "</a:tr>" for _ in range(8))
        return (f'<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{i}" name="Table {i}"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr>'
                f'<p:xfrm><a:off x="838200" y="3000000"/><a:ext cx="10515600" cy="2966720"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1" bandRow="1"/>'
                f'<a:tblGrid>{grid}</a:tblGrid>{rows}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>')

    def sentence():
        return " ".join(rng.choice(WORDS) for _ in range(rng.randint(5, 12))).capitalize()

    files = {}
    for n in range(1, slides + 1):
        shapes = [sp(2, "Title 1", 838200, 365125, 10515600, 1325563, [(0, sentence())], '<p:ph type="title"/>'),
                  sp(3, "Content 2", 838200, 1825625, 10515600, 1100000, [(rng.randrange(2), sentence()) for _ in range(rng.randint(3, 7))], '<p:ph idx="1"/>')]
        shapes += [sp(10 + k, f"Shape {k}", rng.randrange(9000000), rng.randrange(5000000), 1200000, 600000, [(0, rng.choice(WORDS))]) for k in range(rng.randint(2, 8))]
        if n % 3 == 0:
            shapes.append(table(40))
        files[f"ppt/slides/slide{n}.xml"] = XML + f'<p:sld {NS}><p:cSld><p:spTree>{GRP}{"".join(shapes)}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>'
        files[f"ppt/slides/_rels/slide{n}.xml.rels"] = rels([("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")])
    theme_colors = "".join(f'<a:{k}><a:srgbClr val="{v}"/></a:{k}>' for k, v in [("dk1", "000000"), ("lt1", "FFFFFF"), ("dk2", "44546A"), ("lt2", "E7E6E6"), ("accent1", "4472C4"), ("accent2", "ED7D31"), ("accent3", "A5A5A5"), ("accent4", "FFC000"), ("accent5", "5B9BD5"), ("accent6", "70AD47"), ("hlink", "0563C1"), ("folHlink", "954F72")])
    fill = '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>'
    files["ppt/theme/theme1.xml"] = (XML + f'<a:theme xmlns:a="{A}" name="T"><a:themeElements><a:clrScheme name="C">{theme_colors}</a:clrScheme><a:fontScheme name="F"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>'
                                     f'<a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="M"><a:fillStyleLst>{fill * 3}</a:fillStyleLst><a:lnStyleLst>{line}</a:lnStyleLst>'
                                     f'<a:effectStyleLst>{effects}</a:effectStyleLst><a:bgFillStyleLst>{fill * 3}</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>')
    files["ppt/slideMasters/slideMaster1.xml"] = (XML + f'<p:sldMaster {NS}><p:cSld><p:spTree>{GRP}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>'
                                                  '<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>')
    files["ppt/slideMasters/_rels/slideMaster1.xml.rels"] = rels([("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"), ("rId2", "theme", "../theme/theme1.xml")])
    files["ppt/slideLayouts/slideLayout1.xml"] = XML + f'<p:sldLayout {NS} type="obj" preserve="1"><p:cSld name="Title and Content"><p:spTree>{GRP}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>'
    files["ppt/slideLayouts/_rels/slideLayout1.xml.rels"] = rels([("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")])
    ids = "".join(f'<p:sldId id="{255 + n}" r:id="rId{n + 2}"/>' for n in range(1, slides + 1))
    files["ppt/presentation.xml"] = (XML + f'<p:presentation {NS}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst>'
                                     '<p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>')
    files["ppt/_rels/presentation.xml.rels"] = rels([("rId1", "slideMaster", "slideMasters/slideMaster1.xml"), ("rId2", "theme", "theme/theme1.xml")] + [(f"rId{n + 2}", "slide", f"slides/slide{n}.xml") for n in range(1, slides + 1)])
    overrides = [("/ppt/presentation.xml", CT + "presentation.main+xml"), ("/ppt/slideMasters/slideMaster1.xml", CT + "slideMaster+xml"), ("/ppt/slideLayouts/slideLayout1.xml", CT + "slideLayout+xml"),
                 ("/ppt/theme/theme1.xml", "application/vnd.openxmlformats-officedocument.theme+xml")] + [(f"/ppt/slides/slide{n}.xml", CT + "slide+xml") for n in range(1, slides + 1)]
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", types(overrides))
        z.writestr("_rels/.rels", rels([("rId1", "officeDocument", "ppt/presentation.xml")]))
        for k, v in files.items():
            z.writestr(k, v)
    return sum(len(v) for v in files.values())


out = sys.argv[1]
if out.endswith(".xlsx"):
    print(f"{out}: sheet XML {xlsx(out, int(sys.argv[2]) if len(sys.argv) > 2 else 100000) / 2**20:.1f} MB")
else:
    print(f"{out}: XML {pptx(out, int(sys.argv[2]) if len(sys.argv) > 2 else 300) / 2**20:.1f} MB")
