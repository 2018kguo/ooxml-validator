"""Small workbooks and decks with the problems real producers write, plus the
near misses that must stay valid: one .xlsx or .pptx per shape. agree.sh
checks the SDK and ooxml-validator agree on every one.

usage: python3 fuzz/cases_office.py OUT_DIR   (writes OUT_DIR/*.xlsx, *.pptx and OUT_DIR/list.txt)"""
import os, sys, zipfile

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
MC = "http://schemas.openxmlformats.org/markup-compatibility/2006"


def write(name, files):
    with zipfile.ZipFile(os.path.join(OUT, name), "w", zipfile.ZIP_DEFLATED) as z:
        for k, v in files.items():
            if v is not None:
                z.writestr(k, v if isinstance(v, bytes) else v.encode())


def rels(*items):
    body = "".join(f'<Relationship Id="{i}" Type="{t if t.startswith("http") else REL + t}" Target="{target}"{extra}/>' for i, t, target, *rest in items for extra in [rest[0] if rest else ""])
    return f'{XML}<Relationships xmlns="{PKG}">{body}</Relationships>'


def types(defaults, overrides):
    d = "".join(f'<Default Extension="{e}" ContentType="{c}"/>' for e, c in defaults)
    o = "".join(f'<Override PartName="{p}" ContentType="{c}"/>' for p, c in dict(overrides).items())
    return f'{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{d}{o}</Types>'


DEFAULTS = [("rels", "application/vnd.openxmlformats-package.relationships+xml"), ("xml", "application/xml"), ("png", "image/png"), ("bin", "application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings"), ("mp4", "video/mp4")]

# ---------------------------------------------------------------- workbooks
S = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
SNS = (f'xmlns="{S}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
       f'xmlns:mc="{MC}" xmlns:x14ac="http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac" '
       'xmlns:xr="http://schemas.microsoft.com/office/spreadsheetml/2014/revision" mc:Ignorable="x14ac xr"')
SCT = "application/vnd.openxmlformats-officedocument.spreadsheetml."
SHEETS = '<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets>'
STYLES = (f'{XML}<styleSheet {SNS}><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts>'
          '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>'
          '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
          '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
          '<cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs>'
          '<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>')
ROWS = '<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c><c r="C1" s="1"><v>46000</v></c></row>'


def sheet(rows=ROWS, after="", before=""):
    return f'{XML}<worksheet {SNS}>{before}<sheetData>{rows}</sheetData>{after}</worksheet>'


def workbook(inner=SHEETS, attrs=""):
    return f'{XML}<workbook {SNS}{attrs}>{inner}</workbook>'


def xlsx(name, workbook_xml=None, sheet_xml=None, extra=None, wb_rels=(), sheet_rels=(), ct=(), drop=(), defaults=DEFAULTS):
    files = {
        "[Content_Types].xml": types(defaults, [("/xl/workbook.xml", SCT + "sheet.main+xml"), ("/xl/worksheets/sheet1.xml", SCT + "worksheet+xml"),
                                                ("/xl/styles.xml", SCT + "styles+xml"), ("/xl/sharedStrings.xml", SCT + "sharedStrings+xml"), *ct]),
        "_rels/.rels": rels(("rId1", "officeDocument", "xl/workbook.xml")),
        "xl/workbook.xml": workbook_xml or workbook(),
        "xl/_rels/workbook.xml.rels": rels(("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "styles", "styles.xml"), ("rId3", "sharedStrings", "sharedStrings.xml"), *wb_rels),
        "xl/worksheets/sheet1.xml": sheet_xml or sheet(),
        "xl/styles.xml": STYLES,
        "xl/sharedStrings.xml": f'{XML}<sst xmlns="{S}" count="1" uniqueCount="1"><si><t>Total</t></si></sst>',
    }
    if sheet_rels:
        files["xl/worksheets/_rels/sheet1.xml.rels"] = rels(*sheet_rels)
    files.update(extra or {})
    for d in drop:
        files.pop(d, None)
    write(name + ".xlsx", files)
    return name + ".xlsx"


DV = lambda title, prompt="Pick one": f'<dataValidations count="1"><dataValidation type="list" allowBlank="1" showInputMessage="1" promptTitle="{title}" prompt="{prompt}" sqref="A2"><formula1>"Yes,No"</formula1></dataValidation></dataValidations>'
NAMES = '<definedNames><definedName name="Total">Sheet1!$B$1</definedName></definedNames>'
# The compatibility markup Excel writes at the head of a workbook.
WB_MC = ('<fileVersion appName="xl" lastEdited="7" lowestEdited="7" rupBuild="27328"/><workbookPr defaultThemeVersion="166925"/>'
         '<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:x15="http://schemas.microsoft.com/office/spreadsheetml/2010/11/main" Requires="x15">'
         '<x15ac:absPath xmlns:x15ac="http://schemas.microsoft.com/office/spreadsheetml/2010/11/ac" url="C:\\Books\\"/></mc:Choice></mc:AlternateContent>'
         '<xr:revisionPtr revIDLastSave="0" documentId="8_{00000000-0000-0000-0000-000000000000}" xr6:coauthVersionLast="47" xr6:coauthVersionMax="47" xr10:uidLastSave="{00000000-0000-0000-0000-000000000000}" '
         'xmlns:xr6="http://schemas.microsoft.com/office/spreadsheetml/2016/revision6" xmlns:xr10="http://schemas.microsoft.com/office/spreadsheetml/2016/revision10"/>'
         '<bookViews><workbookView xWindow="0" yWindow="0" windowWidth="28800" windowHeight="12300"/></bookViews>')
CALC = '<calcPr calcId="191029"/>'
TALL = "x" * 32

made = []
xl = [
    ("x_ok_base", {}),
    ("x_ok_compat_markup", dict(workbook_xml=workbook(WB_MC + SHEETS + NAMES + CALC))),
    # The incident: named ranges written before the sheet list, with the compatibility markup present.
    ("x_names_before_sheets_compat", dict(workbook_xml=workbook(WB_MC + NAMES + SHEETS + CALC))),
    ("x_names_before_sheets", dict(workbook_xml=workbook(NAMES + SHEETS))),
    ("x_names_after_calc", dict(workbook_xml=workbook(SHEETS + CALC + NAMES))),
    ("x_names_twice", dict(workbook_xml=workbook(SHEETS + NAMES + NAMES))),
    ("x_defined_name_dup", dict(workbook_xml=workbook(SHEETS + '<definedNames><definedName name="A">Sheet1!$A$1</definedName><definedName name="a">Sheet1!$A$2</definedName></definedNames>'))),
    ("x_defined_name_local_sheet", dict(workbook_xml=workbook(SHEETS + '<definedNames><definedName name="A" localSheetId="3">Sheet1!$A$1</definedName></definedNames>'))),
    ("x_no_sheets", dict(workbook_xml=workbook(""))),
    ("x_empty_sheets", dict(workbook_xml=workbook("<sheets/>"))),
    ("x_sheet_name_dup", dict(workbook_xml=workbook('<sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="DATA" sheetId="2" r:id="rId1"/></sheets>'))),
    ("x_sheet_id_dup", dict(workbook_xml=workbook('<sheets><sheet name="A" sheetId="1" r:id="rId1"/><sheet name="B" sheetId="1" r:id="rId1"/></sheets>'))),
    ("x_sheet_name_32", dict(workbook_xml=workbook(f'<sheets><sheet name="{"n" * 32}" sheetId="1" r:id="rId1"/></sheets>'))),
    ("x_sheet_name_31_ok", dict(workbook_xml=workbook(f'<sheets><sheet name="{"n" * 31}" sheetId="1" r:id="rId1"/></sheets>'))),
    ("x_sheet_rid_missing", dict(workbook_xml=workbook('<sheets><sheet name="A" sheetId="1" r:id="rId9"/></sheets>'))),
    ("x_sheet_rid_wrong_type", dict(workbook_xml=workbook('<sheets><sheet name="A" sheetId="1" r:id="rId2"/></sheets>'))),
    ("x_sheet_id_zero", dict(workbook_xml=workbook('<sheets><sheet name="A" sheetId="0" r:id="rId1"/></sheets>'))),
    # A sheet's code name: .NET's pattern, with its IsBasicLatin block and a `$`
    # that also matches before one final line feed.
    *[(f"x_code_name_{tag}", dict(sheet_xml=sheet(before=f'<sheetPr codeName="{v}"/>'))) for tag, v in [
        ("ok", "Sheet1"), ("digit", "1Sheet"), ("space", "Sheet 1"), ("dash", "a-b"), ("umlaut", "Übersicht"),
        ("cjk", "表1"), ("lf", "Sheet1&#10;"), ("lf2", "Sheet1&#10;&#10;"), ("underscore_first", "_a"), ("empty", ""),
    ]],
    # Data validation prompts: 32 and 255 characters at most.
    ("x_dv_title_32_ok", dict(sheet_xml=sheet(after=DV(TALL)))),
    ("x_dv_title_33", dict(sheet_xml=sheet(after=DV(TALL + "y")))),
    ("x_dv_title_100", dict(sheet_xml=sheet(after=DV("t" * 100)))),
    ("x_dv_prompt_256", dict(sheet_xml=sheet(after=DV("Title", "p" * 256)))),
    ("x_dv_error_title_33", dict(sheet_xml=sheet(after='<dataValidations count="1"><dataValidation type="whole" errorTitle="' + "e" * 33 + '" sqref="A2"><formula1>1</formula1></dataValidation></dataValidations>'))),
    ("x_dv_title_entity_32_ok", dict(sheet_xml=sheet(after=DV("&amp;" * 32)))),
    # Cell values against their types (CellType).
    ("x_cell_number_text", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="n"><v>abc</v></c></row>'))),
    ("x_cell_number_forms_ok", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="n"><v>1,000.5</v></c><c r="B1" t="n"><v> -1e3 </v></c><c r="C1" t="n"><v>5-</v></c><c r="D1" t="n"><v>Infinity</v></c><c r="E1" t="n"><v>1e400</v></c></row>'))),
    ("x_cell_number_empty", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="n"><v></v></c></row>'))),
    ("x_cell_bool_bad", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="b"><v>TRUE</v></c></row>'))),
    ("x_cell_bool_ok", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="b"><v>1</v></c><c r="B1" t="b"><v>false</v></c></row>'))),
    ("x_cell_date_ok", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="d"><v>2026-11-17T09:30:00.000</v></c><c r="B1" t="d"><v>2024-02-29T00:00:00.000+05:30</v></c></row>'))),
    ("x_cell_date_no_ms", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="d"><v>2026-11-17T09:30:00</v></c></row>'))),
    ("x_cell_date_z", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="d"><v>2026-11-17T09:30:00.000Z</v></c></row>'))),
    ("x_cell_date_bad_day", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="d"><v>2023-02-29T00:00:00.000</v></c></row>'))),
    ("x_cell_str_anything_ok", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="str"><f>"a"</f><v>abc</v></c><c r="B1" t="e"><v>#N/A</v></c></row>'))),
    ("x_cell_untyped_text_ok", dict(sheet_xml=sheet('<row r="1"><c r="A1"><v>abc</v></c></row>'))),
    ("x_cell_bad_type", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="number"><v>1</v></c></row>'))),
    ("x_cell_v_in_ac", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="n"><mc:AlternateContent><mc:Choice Requires="x14ac"><v>abc</v></mc:Choice></mc:AlternateContent></c></row>'))),
    ("x_cell_two_v", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="n"><v>1</v><v>abc</v></c></row>'))),
    ("x_inline_string_ok", dict(sheet_xml=sheet('<row r="1"><c r="A1" t="inlineStr"><is><t>Hi</t></is></c></row>'))),
    # Style and string indexes.
    ("x_style_index_out", dict(sheet_xml=sheet('<row r="1"><c r="A1" s="7"><v>1</v></c></row>'))),
    ("x_style_index_last_ok", dict(sheet_xml=sheet('<row r="1"><c r="A1" s="1"><v>1</v></c></row>'))),
    ("x_font_id_out", dict(extra={"xl/styles.xml": STYLES.replace('<xf numFmtId="14" fontId="0"', '<xf numFmtId="14" fontId="40"')})),
    ("x_row_style_out", dict(sheet_xml=sheet('<row r="1" s="9" customFormat="1"><c r="A1"><v>1</v></c></row>'))),
    ("x_no_styles_with_style", dict(drop=["xl/styles.xml"], sheet_xml=sheet('<row r="1"><c r="A1" s="1"><v>1</v></c></row>'))),
    ("x_cellxfs_in_ac_ok", dict(extra={"xl/styles.xml": STYLES.replace('<xf numFmtId="14"', '<mc:AlternateContent><mc:Choice Requires="x14ac"><xf numFmtId="14"').replace('applyNumberFormat="1"/>', 'applyNumberFormat="1"/></mc:Choice></mc:AlternateContent>')}, sheet_xml=sheet('<row r="1"><c r="A1" s="1"><v>1</v></c></row>'))),
    # Rows, columns and order.
    ("x_row_zero", dict(sheet_xml=sheet('<row r="0"><c r="A1"><v>1</v></c></row>'))),
    ("x_row_huge", dict(sheet_xml=sheet('<row r="1048577"><c r="A1"><v>1</v></c></row>'))),
    ("x_cols_after_data", dict(sheet_xml=sheet(after='<cols><col min="1" max="1" width="12" customWidth="1"/></cols>'))),
    ("x_col_min_zero", dict(sheet_xml=sheet(before='<cols><col min="0" max="1" width="12"/></cols>'))),
    ("x_merge_before_data", dict(sheet_xml=sheet(before='<mergeCells count="1"><mergeCell ref="A1:B1"/></mergeCells>'))),
    ("x_merge_ok", dict(sheet_xml=sheet(after='<mergeCells count="1"><mergeCell ref="A1:B1"/></mergeCells>'))),
    ("x_cell_r_bad", dict(sheet_xml=sheet('<row r="1"><c r="1A"><v>1</v></c></row>'))),
    ("x_unknown_child", dict(sheet_xml=sheet('<row r="1"><c r="A1"><v>1</v><x14ac:bogus/></c></row>'))),
    ("x_ignorable_attr_ok", dict(sheet_xml=sheet('<row r="1" x14ac:dyDescent="0.25"><c r="A1"><v>1</v></c></row>'))),
    ("x_hyperlink_rid_missing", dict(sheet_xml=sheet(after='<hyperlinks><hyperlink ref="A1" r:id="rId7"/></hyperlinks>'))),
    ("x_hyperlink_ok", dict(sheet_xml=sheet(after='<hyperlinks><hyperlink ref="A1" r:id="rId7"/></hyperlinks>'), sheet_rels=[("rId7", "hyperlink", "https://example.com", ' TargetMode="External"')])),
    ("x_page_setup_rid_ok", dict(sheet_xml=sheet(after='<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup orientation="portrait" r:id="rId1"/>'),
                                 sheet_rels=[("rId1", "printerSettings", "../printerSettings/printerSettings1.bin")], extra={"xl/printerSettings/printerSettings1.bin": b"\x00" * 16})),
    # Package shape.
    ("x_shared_strings_missing", dict(drop=["xl/sharedStrings.xml"], sheet_xml=sheet('<row r="1"><c r="A1"><v>1</v></c></row>'))),
    ("x_sheet_missing_part", dict(drop=["xl/worksheets/sheet1.xml"])),
    ("x_main_ct_shared", dict(ct=[("/xl/workbook.xml", SCT + "sharedStrings+xml")])),
    ("x_two_styles", dict(wb_rels=[("rId9", "styles", "styles2.xml")], extra={"xl/styles2.xml": STYLES}, ct=[("/xl/styles2.xml", SCT + "styles+xml")])),
    ("x_styles_wrong_ct", dict(ct=[("/xl/styles.xml", SCT + "sharedStrings+xml")])),
    ("x_slide_in_workbook", dict(wb_rels=[("rId9", "slide", "slide1.xml")], extra={"xl/slide1.xml": f'{XML}<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree/></p:cSld></p:sld>'},
                                 ct=[("/xl/slide1.xml", "application/vnd.openxmlformats-officedocument.presentationml.slide+xml")])),
    ("x_video_in_sheet", dict(sheet_rels=[("rId5", "video", "../media/v.mp4")], extra={"xl/media/v.mp4": b"\x00" * 8})),
    ("x_media_in_sheet", dict(sheet_rels=[("rId5", "http://schemas.microsoft.com/office/2007/relationships/media", "../media/v.mp4")], extra={"xl/media/v.mp4": b"\x00" * 8})),
    ("x_template_ok", dict(ct=[("/xl/workbook.xml", SCT + "template.main+xml")])),
    ("x_macro_ct_ok", dict(ct=[("/xl/workbook.xml", "application/vnd.ms-excel.sheet.macroEnabled.main+xml")])),
    ("x_main_ct_unknown", dict(ct=[("/xl/workbook.xml", "application/xml")])),
    ("x_root_is_worksheet", dict(workbook_xml=sheet())),
    ("x_undeclared_attr", dict(workbook_xml=workbook(SHEETS, ' bogus="1"'))),
    ("x_strict_ok", dict(workbook_xml=f'{XML}<workbook xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main" xmlns:r="http://purl.oclc.org/ooxml/officeDocument/relationships" conformance="strict">{SHEETS}</workbook>',
                         sheet_xml=f'{XML}<worksheet xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main"><sheetData/></worksheet>',
                         extra={"_rels/.rels": rels(("rId1", "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument", "xl/workbook.xml"))})),
]
for n, kw in xl:
    made.append(xlsx(n, **kw))

# --------------------------------------------------------------------- decks
A = "http://schemas.openxmlformats.org/drawingml/2006/main"
P_NS = f'xmlns:a="{A}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"'
PCT = "application/vnd.openxmlformats-officedocument.presentationml."
GRP = '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
SHAPE = lambda i=2, name="Title 1", xfrm='<a:xfrm><a:off x="838200" y="365125"/><a:ext cx="10515600" cy="1325563"/></a:xfrm>', text="Hello": (
    f'<p:sp><p:nvSpPr><p:cNvPr id="{i}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{xfrm}</p:spPr>'
    f'<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>')
CLR = '<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>'
THEME = (f'{XML}<a:theme xmlns:a="{A}" name="T"><a:themeElements><a:clrScheme name="C">'
         + "".join(f'<a:{k}><a:srgbClr val="{v}"/></a:{k}>' for k, v in [("dk1", "000000"), ("lt1", "FFFFFF"), ("dk2", "44546A"), ("lt2", "E7E6E6"), ("accent1", "4472C4"), ("accent2", "ED7D31"), ("accent3", "A5A5A5"), ("accent4", "FFC000"), ("accent5", "5B9BD5"), ("accent6", "70AD47"), ("hlink", "0563C1"), ("folHlink", "954F72")])
         + '</a:clrScheme><a:fontScheme name="F"><a:majorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>'
         '<a:fmtScheme name="M"><a:fillStyleLst>' + '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>' * 3 + '</a:fillStyleLst><a:lnStyleLst>' + '<a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>' * 3
         + '</a:lnStyleLst><a:effectStyleLst>' + '<a:effectStyle><a:effectLst/></a:effectStyle>' * 3 + '</a:effectStyleLst><a:bgFillStyleLst>' + '<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>' * 3
         + '</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>')


def slide(shapes=SHAPE(), after="", attrs=""):
    return f'{XML}<p:sld {P_NS}{attrs}><p:cSld><p:spTree>{GRP}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{after}</p:sld>'


def presentation(ids='<p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst>', size='<p:sldSz cx="12192000" cy="6858000"/>', before_ids=""):
    return (f'{XML}<p:presentation {P_NS}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>{before_ids}{ids}'
            f'{size}<p:notesSz cx="6858000" cy="9144000"/></p:presentation>')


def pptx(name, pres=None, slide_xml=None, extra=None, pres_rels=(), slide_rels=(), ct=(), drop=(), layout_rels=None):
    files = {
        "[Content_Types].xml": types(DEFAULTS, [("/ppt/presentation.xml", PCT + "presentation.main+xml"), ("/ppt/slides/slide1.xml", PCT + "slide+xml"),
                                                ("/ppt/slideLayouts/slideLayout1.xml", PCT + "slideLayout+xml"), ("/ppt/slideMasters/slideMaster1.xml", PCT + "slideMaster+xml"),
                                                ("/ppt/theme/theme1.xml", "application/vnd.openxmlformats-officedocument.theme+xml"), *ct]),
        "_rels/.rels": rels(("rId1", "officeDocument", "ppt/presentation.xml")),
        "ppt/presentation.xml": pres or presentation(),
        "ppt/_rels/presentation.xml.rels": rels(("rId1", "slideMaster", "slideMasters/slideMaster1.xml"), ("rId2", "slide", "slides/slide1.xml"), ("rId3", "theme", "theme/theme1.xml"), *pres_rels),
        "ppt/slides/slide1.xml": slide_xml or slide(),
        "ppt/slides/_rels/slide1.xml.rels": rels(("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"), *slide_rels),
        "ppt/slideLayouts/slideLayout1.xml": f'{XML}<p:sldLayout {P_NS} type="title" preserve="1"><p:cSld name="Title Slide"><p:spTree>{GRP}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>',
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels": layout_rels or rels(("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")),
        "ppt/slideMasters/slideMaster1.xml": f'{XML}<p:sldMaster {P_NS}><p:cSld><p:spTree>{GRP}</p:spTree></p:cSld>{CLR}<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>',
        "ppt/slideMasters/_rels/slideMaster1.xml.rels": rels(("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"), ("rId2", "theme", "../theme/theme1.xml")),
        "ppt/theme/theme1.xml": THEME,
    }
    files.update(extra or {})
    for d in drop:
        files.pop(d, None)
    write(name + ".pptx", files)
    return name + ".pptx"


TIMING = lambda spid: (f'<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:par><p:cTn id="2" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst>'
                       f'<p:childTnLst><p:set><p:cBhvr><p:cTn id="3" dur="1" fill="hold"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set></p:childTnLst>'
                       '</p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>')
VIDEO = ('<p:pic><p:nvPicPr><p:cNvPr id="4" name="Video"/><p:cNvPicPr/><p:nvPr><a:videoFile r:link="rId5"/><p:extLst><p:ext uri="{DAA4B4D4-6D71-4841-9C94-3DE7FCFB9230}">'
         '<p14:media xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main" r:embed="rId6"/></p:ext></p:extLst></p:nvPr></p:nvPicPr><p:blipFill><a:blip r:embed="rId7"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>'
         '<p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>')
VIDEO_RELS = [("rId5", "video", "../media/v.mp4"), ("rId6", "http://schemas.microsoft.com/office/2007/relationships/media", "../media/v.mp4"), ("rId7", "image", "../media/i.png")]
MEDIA = {"ppt/media/v.mp4": b"\x00" * 8, "ppt/media/i.png": b"\x89PNG\r\n\x1a\n"}
pp = [
    ("p_ok_base", {}),
    ("p_slide_id_low", dict(pres=presentation('<p:sldIdLst><p:sldId id="255" r:id="rId2"/></p:sldIdLst>'))),
    ("p_slide_id_dup", dict(pres=presentation('<p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="256" r:id="rId2"/></p:sldIdLst>'))),
    ("p_slide_id_high", dict(pres=presentation('<p:sldIdLst><p:sldId id="2147483648" r:id="rId2"/></p:sldIdLst>'))),
    ("p_slide_rid_missing", dict(pres=presentation('<p:sldIdLst><p:sldId id="256" r:id="rId9"/></p:sldIdLst>'))),
    ("p_slide_rid_wrong_type", dict(pres=presentation('<p:sldIdLst><p:sldId id="256" r:id="rId3"/></p:sldIdLst>'))),
    ("p_no_slides_ok", dict(pres=presentation(""))),
    ("p_ids_before_masters", dict(pres=f'{XML}<p:presentation {P_NS}><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>')),
    ("p_slide_size_small", dict(pres=presentation(size='<p:sldSz cx="100" cy="100"/>'))),
    ("p_no_notes_size", dict(pres=presentation().replace('<p:notesSz cx="6858000" cy="9144000"/>', ""))),
    ("p_shape_id_dup", dict(slide_xml=slide(SHAPE(2) + SHAPE(2, "Body 2")))),
    ("p_shape_id_zero", dict(slide_xml=slide(SHAPE(0)))),
    ("p_shape_off_negative_ok", dict(slide_xml=slide(SHAPE(xfrm='<a:xfrm><a:off x="-100" y="-100"/><a:ext cx="10" cy="10"/></a:xfrm>')))),
    ("p_shape_ext_negative", dict(slide_xml=slide(SHAPE(xfrm='<a:xfrm><a:off x="0" y="0"/><a:ext cx="-10" cy="10"/></a:xfrm>')))),
    ("p_shape_ext_huge", dict(slide_xml=slide(SHAPE(xfrm='<a:xfrm><a:off x="0" y="0"/><a:ext cx="27273042316901" cy="10"/></a:xfrm>')))),
    ("p_shape_rot_float", dict(slide_xml=slide(SHAPE(xfrm='<a:xfrm rot="1.5"><a:off x="0" y="0"/><a:ext cx="10" cy="10"/></a:xfrm>')))),
    ("p_text_empty_run_ok", dict(slide_xml=slide(SHAPE(text="")))),
    ("p_rpr_order", dict(slide_xml=slide(SHAPE().replace('<a:rPr lang="en-US"/>', '<a:rPr lang="en-US"><a:latin typeface="Arial"/><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:rPr>')))),
    ("p_color_bad", dict(slide_xml=slide(SHAPE().replace('<a:rPr lang="en-US"/>', '<a:rPr lang="en-US"><a:solidFill><a:srgbClr val="F00"/></a:solidFill></a:rPr>')))),
    ("p_font_size_low", dict(slide_xml=slide(SHAPE().replace('<a:rPr lang="en-US"/>', '<a:rPr lang="en-US" sz="99"/>')))),
    ("p_timing_spid_ok", dict(slide_xml=slide(after=TIMING(2)))),
    ("p_timing_spid_missing", dict(slide_xml=slide(after=TIMING(9)))),
    ("p_video_ok", dict(slide_xml=slide(SHAPE() + VIDEO), slide_rels=VIDEO_RELS, extra=MEDIA, ct=[("/ppt/media/i.png", "image/png")])),
    ("p_video_missing_file", dict(slide_xml=slide(SHAPE() + VIDEO), slide_rels=VIDEO_RELS, extra={"ppt/media/i.png": MEDIA["ppt/media/i.png"]})),
    ("p_video_on_layout", dict(layout_rels=rels(("rId1", "slideMaster", "../slideMasters/slideMaster1.xml"), ("rId5", "video", "../media/v.mp4")), extra=MEDIA)),
    ("p_media_on_layout", dict(layout_rels=rels(("rId1", "slideMaster", "../slideMasters/slideMaster1.xml"), ("rId6", "http://schemas.microsoft.com/office/2007/relationships/media", "../media/v.mp4")), extra=MEDIA)),
    ("p_audio_on_presentation", dict(pres_rels=[("rId9", "audio", "media/v.mp4")], extra=MEDIA)),
    ("p_slide_no_layout", dict(extra={"ppt/slides/_rels/slide1.xml.rels": rels()})),
    ("p_layout_no_master", dict(layout_rels=rels())),
    ("p_two_layouts_on_slide", dict(slide_rels=[("rId2", "slideLayout", "../slideLayouts/slideLayout1.xml")])),
    ("p_printer_settings_ok", dict(pres_rels=[("rId9", "printerSettings", "printerSettings/printerSettings1.bin")], extra={"ppt/printerSettings/printerSettings1.bin": b"\x00" * 16})),
    ("p_worksheet_in_deck", dict(pres_rels=[("rId9", "worksheet", "sheet1.xml")], extra={"ppt/sheet1.xml": f'{XML}<worksheet xmlns="{S}"><sheetData/></worksheet>'}, ct=[("/ppt/sheet1.xml", SCT + "worksheet+xml")])),
    ("p_slide_missing_part", dict(drop=["ppt/slides/slide1.xml"])),
    ("p_slide_wrong_ct", dict(ct=[("/ppt/slides/slide1.xml", PCT + "slideLayout+xml")])),
    ("p_template_ok", dict(ct=[("/ppt/presentation.xml", PCT + "template.main+xml")])),
    ("p_slideshow_ok", dict(ct=[("/ppt/presentation.xml", PCT + "slideshow.main+xml")])),
    ("p_root_is_slide", dict(pres=slide())),
    ("p_tags_dup", dict(slide_xml=slide().replace("</p:cSld>", '<p:custDataLst><p:tags r:id="rId8"/></p:custDataLst></p:cSld>'), slide_rels=[("rId8", "tags", "../tags/tag1.xml")],
                        extra={"ppt/tags/tag1.xml": f'{XML}<p:tagLst {P_NS}><p:tag name="A" val="1"/><p:tag name="a" val="2"/></p:tagLst>'}, ct=[("/ppt/tags/tag1.xml", PCT + "tags+xml")])),
    ("p_comment_authors_dup", dict(pres_rels=[("rId9", "commentAuthors", "commentAuthors.xml")], ct=[("/ppt/commentAuthors.xml", PCT + "commentAuthors+xml")],
                                   extra={"ppt/commentAuthors.xml": f'{XML}<p:cmAuthorLst {P_NS}><p:cmAuthor id="0" name="A" initials="A" lastIdx="1" clrIdx="0"/><p:cmAuthor id="0" name="B" initials="B" lastIdx="1" clrIdx="0"/></p:cmAuthorLst>'})),
    ("p_unknown_in_sptree", dict(slide_xml=slide(SHAPE() + "<p:bogus/>"))),
    ("p_ac_shape_ok", dict(slide_xml=slide(f'<mc:AlternateContent xmlns:mc="{MC}"><mc:Choice xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main" Requires="p14">{SHAPE(3)}</mc:Choice><mc:Fallback>{SHAPE(3)}</mc:Fallback></mc:AlternateContent>'))),
]
for n, kw in pp:
    made.append(pptx(n, **kw))

with open(os.path.join(OUT, "list.txt"), "w") as f:
    f.write("".join(os.path.join(os.path.abspath(OUT), n) + "\n" for n in made))
print(len(made), "cases")
