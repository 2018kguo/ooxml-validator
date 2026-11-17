"""Small documents with the problems real producers write (LibreOffice,
JavaScript and PDF converters, hand-built XML), plus the near misses that
must stay valid: one .docx per shape. agree.sh checks the SDK and
ooxml-validator agree on every one.

usage: python3 fuzz/cases.py OUT_DIR   (writes OUT_DIR/*.docx and OUT_DIR/list.txt)"""
import os, sys, zipfile

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
NS = (f'xmlns:w="{W}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
      'xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" '
      'xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" mc:Ignorable="w14 w15"')
CT = {"styles": "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
      "settings": "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml",
      "numbering": "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml",
      "comments": "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
      "fontTable": "application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"}
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
XML = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'


def doc(body, sect=True):
    s = '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>' if sect else ""
    return f"{XML}<w:document {NS}><w:body>{body}{s}</w:body></w:document>"


def part(name, inner):
    return f"{XML}<w:{name} {NS}>{inner}</w:{name}>"


STYLES_OK = '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
BASE = {
    "word/styles.xml": part("styles", STYLES_OK),
    "word/settings.xml": part("settings", '<w:defaultTabStop w:val="720"/>'),
    "word/numbering.xml": part("numbering", '<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>'),
    "word/fontTable.xml": part("fonts", '<w:font w:name="Calibri"><w:charset w:val="00"/></w:font>'),
}


def build(name, body=None, parts=None, rels_extra="", ct_extra="", raw=None, drop=(), doc_xml=None):
    files = dict(BASE)
    files["word/document.xml"] = doc_xml if doc_xml is not None else doc(body or "<w:p/>")
    files.update(parts or {})
    for d in drop:
        files.pop(d, None)
    kinds = {"word/styles.xml": "styles", "word/settings.xml": "settings", "word/numbering.xml": "numbering", "word/comments.xml": "comments", "word/fontTable.xml": "fontTable"}
    rels = "".join(f'<Relationship Id="rId{i}" Type="{REL}{k}" Target="{p[5:]}"/>' for i, (p, k) in enumerate(kinds.items(), 10) if p in files)
    over = "".join(f'<Override PartName="/{p}" ContentType="{CT[k]}"/>' for p, k in kinds.items() if p in files)
    files.setdefault("[Content_Types].xml", f'{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>{over}{ct_extra}</Types>')
    files.setdefault("_rels/.rels", f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}officeDocument" Target="word/document.xml"/></Relationships>')
    files.setdefault("word/_rels/document.xml.rels", f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}{rels_extra}</Relationships>')
    for k, v in (raw or {}).items():
        files[k] = v
    with zipfile.ZipFile(os.path.join(OUT, name + ".docx"), "w", zipfile.ZIP_DEFLATED) as z:
        for k, v in files.items():
            if v is not None:
                z.writestr(k, v if isinstance(v, bytes) else v.encode())


P = lambda inner: f"<w:p>{inner}</w:p>"
R = lambda rpr="", t="x": f"<w:r>{f'<w:rPr>{rpr}</w:rPr>' if rpr else ''}<w:t>{t}</w:t></w:r>"
TBL = lambda tcpr="", tblpr='<w:tblW w:w="0" w:type="auto"/>', grid='<w:gridCol w:w="2000"/>', trpr="": f'<w:tbl><w:tblPr>{tblpr}</w:tblPr><w:tblGrid>{grid}</w:tblGrid><w:tr>{trpr}<w:tc>{f"<w:tcPr>{tcpr}</w:tcPr>" if tcpr else ""}<w:p/></w:tc></w:tr></w:tbl>'

cases = {
    "ok_base": dict(body=P(R())),
    # 1-8 order and repeats
    "01_rpr_order": dict(body=P(R('<w:sz w:val="22"/><w:b/>'))),
    "01_rpr_order_color": dict(body=P(R('<w:color w:val="0563C1"/><w:b/>'))),
    "01_rpr_order_many": dict(body=P(R('<w:rStyle w:val="Normal"/><w:smallCaps/><w:caps/><w:dstrike/><w:strike/><w:sz w:val="20"/><w:i/><w:u w:val="single"/><w:b/><w:effect w:val="none"/><w:rFonts w:ascii="Arial"/><w:color w:val="000000"/>'))),
    "01_rpr_order_styles": dict(parts={"word/styles.xml": part("styles", STYLES_OK + '<w:style w:type="character" w:styleId="C"><w:name w:val="C"/><w:rPr><w:sz w:val="22"/><w:b/></w:rPr></w:style>')}),
    "01_rpr_order_numbering": dict(parts={"word/numbering.xml": part("numbering", '<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="left"/><w:rPr><w:sz w:val="22"/><w:b/></w:rPr></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>')}),
    "02_outlinelvl_twice": dict(parts={"word/styles.xml": part("styles", STYLES_OK + '<w:style w:type="paragraph" w:styleId="H"><w:name w:val="H"/><w:pPr><w:spacing w:after="0"/><w:outlineLvl w:val="1"/><w:outlineLvl w:val="1"/></w:pPr></w:style>')}),
    "02_sz_twice": dict(body=P(R('<w:rFonts w:ascii="Arial"/><w:b/><w:i/><w:caps/><w:smallCaps/><w:position w:val="2"/><w:sz w:val="20"/><w:sz w:val="20"/>'))),
    "03_ppr_order": dict(body=P('<w:pPr><w:contextualSpacing/><w:pStyle w:val="Normal"/></w:pPr>' + R())),
    "03_ppr_rpr_order": dict(body=P('<w:pPr><w:rPr><w:sz w:val="24"/><w:b/></w:rPr></w:pPr>' + R())),
    "03_ppr_order_comments": dict(parts={"word/comments.xml": part("comments", '<w:comment w:id="0" w:author="A"><w:p><w:pPr><w:contextualSpacing/><w:spacing w:after="10"/></w:pPr></w:p></w:comment>')}),
    "04_tcpr_order": dict(body=TBL('<w:vAlign w:val="center"/><w:tcMar><w:top w:w="80" w:type="dxa"/></w:tcMar>') + "<w:p/>"),
    "04_tcpr_shd_order": dict(body=TBL('<w:vAlign w:val="center"/><w:shd w:val="clear" w:fill="183354"/>') + "<w:p/>"),
    "04_tcmar_order": dict(body=TBL('<w:tcMar><w:bottom w:w="120" w:type="dxa"/><w:top w:w="80" w:type="dxa"/></w:tcMar>') + "<w:p/>"),
    "05_body_after_sectpr": dict(doc_xml=f'{XML}<w:document {NS}><w:body><w:p/><w:sectPr/><w:p/></w:body></w:document>'),
    "06_settings_order": dict(parts={"word/settings.xml": part("settings", '<w:compat/><w:rsids><w:rsidRoot w:val="00112233"/></w:rsids><w:docVars><w:docVar w:name="a" w:val="b"/></w:docVars>')}),
    "07_rpr_after_content": dict(body=P('<w:r><w:lastRenderedPageBreak/><w:rPr><w:b/></w:rPr><w:t>Text</w:t></w:r>')),
    "08_tbl_no_props": dict(body='<w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>'),
    # 9-21 attribute values
    "09_shd_no_val_tcpr": dict(body=TBL('<w:shd w:fill="E8EEF5"/>') + "<w:p/>"),
    "09_shd_no_val_ppr": dict(body=P('<w:pPr><w:shd w:color="auto" w:fill="E8EEF5"/></w:pPr>')),
    "09_shd_no_val_rpr_theme": dict(body=P(R('<w:shd w:fill="E8EEF5" w:themeFillTint="7F" w:themeFillShade="BF"/>'))),
    "10_spacing_fraction": dict(body=P('<w:pPr><w:spacing w:before="120.0" w:after="80.00000000000001" w:line="278.00000000000006"/></w:pPr>')),
    "10_ind_fraction": dict(body=P('<w:pPr><w:ind w:left="-720.0000000000001" w:firstLine="360.00000000000003"/></w:pPr>')),
    "10_rpr_fraction": dict(body=P(R('<w:spacing w:val="-20.0000000000001"/><w:kern w:val="2.0000000000001"/><w:position w:val="-2.0000000000001"/><w:sz w:val="22.0000000000001"/>'))),
    "10_bdr_fraction": dict(body=P(R('<w:bdr w:val="single" w:sz="8.0000000000001" w:space="1.0"/><w:fitText w:val="2000.0000000000001" w:id="12"/>'))),
    "10_tbl_fraction": dict(body=TBL('<w:tcW w:w="2000.0" w:type="dxa"/>', '<w:tblW w:w="9350.000000000002" w:type="dxa"/>', '<w:gridCol w:w="2257.25"/>', '<w:trPr><w:trHeight w:val="379.98046875" w:hRule="atLeast"/></w:trPr>') + "<w:p/>"),
    "10_tblppr_fraction": dict(body=TBL(tblpr='<w:tblpPr w:vertAnchor="margin" w:horzAnchor="margin" w:tblpX="-12.00000000000017" w:tblpY="0"/><w:tblW w:w="0" w:type="auto"/>') + "<w:p/>"),
    "10_settings_fraction": dict(parts={"word/settings.xml": part("settings", '<w:defaultTabStop w:val="720.0000000000001"/>')}),
    "11_cols_negative": dict(body=P('<w:pPr><w:sectPr><w:cols w:num="2" w:space="0" w:equalWidth="0"><w:col w:w="5404" w:space="-1"/><w:col w:w="5400"/></w:cols></w:sectPr></w:pPr>') * 3),
    "12_tblw_pct_decimal": dict(body=TBL(tblpr='<w:tblW w:w="100.0" w:type="pct"/>') + "<w:p/>"),
    "13_theme_tint_padded": dict(body=P(R('<w:color w:val="auto" w:themeColor="accent1" w:themeTint="00007F"/><w:shd w:val="clear" w:color="auto" w:fill="auto" w:themeFill="accent1" w:themeFillShade="0000BF"/>'))),
    "14_framepr_lines0": dict(body=P('<w:pPr><w:framePr w:lines="0" w:w="7920" w:h="1980" w:hSpace="180" w:wrap="auto" w:hAnchor="page" w:xAlign="center" w:yAlign="bottom" w:hRule="exact"/></w:pPr>')),
    "15_onoff_true_tbl": dict(body=TBL(trpr='<w:trPr><w:cantSplit w:val="1"/><w:tblHeader w:val="true"/></w:trPr>') + "<w:p/>"),
    "15_onoff_styles": dict(parts={"word/styles.xml": part("styles", STYLES_OK + '<w:style w:type="paragraph" w:styleId="S"><w:name w:val="S"/><w:semiHidden w:val="0"/><w:unhideWhenUsed w:val="true"/><w:qFormat w:val="1"/></w:style>')}),
    "15_b_true_ok": dict(body=P(R('<w:b w:val="true"/>'))),
    "16_rfonts_long": dict(body=P(R('<w:rFonts w:ascii="Liberation Serif;Times New Roman" w:hAnsi="Liberation Serif;Times New Roman"/>'))),
    "17_jc_start": dict(body=P('<w:pPr><w:jc w:val="start"/></w:pPr>')),
    "17_lvljc_end": dict(parts={"word/numbering.xml": part("numbering", '<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:lvlJc w:val="end"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>')}),
    "18_charset_name": dict(parts={"word/fontTable.xml": part("fonts", '<w:font w:name="Calibri"><w:charset w:val="00" w:characterSet="windows-1252"/></w:font>')}),
    "19_docgrid_unsigned": dict(doc_xml=doc('<w:p/>', sect=False).replace("</w:body>", '<w:sectPr><w:docGrid w:type="lines" w:linePitch="360" w:charSpace="4294963199"/></w:sectPr></w:body>')),
    "20_themecolor_dk1": dict(body=P(R('<w:color w:val="000000" w:themeColor="dk1"/>'))),
    "20_themefill_bg1": dict(body=P(R('<w:shd w:val="clear" w:color="auto" w:fill="FFFFFF" w:themeFill="bg1"/>'))),
    "21_paraid_high": dict(body='<w:p w14:paraId="80000000"/>'),
    "21_paraid_zero": dict(body='<w:p w14:paraId="00000000"/>'),
    "21_paraid_ok": dict(body='<w:p w14:paraId="7FFFFFFE" w14:textId="77777777"/>'),
    # 22-24 disallowed elements
    "22_style_cs": dict(parts={"word/styles.xml": part("styles", '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:sz w:val="22"/><w:cs w:val="0"/></w:rPr></w:style>')}),
    "23_smarttag": dict(body=P(('<w:smartTag w:uri="urn:x" w:element="unit"><w:smartTagPr><w:attr w:name="value" w:val="8"/></w:smartTagPr>' + R(t="8") + "</w:smartTag>") * 2),
                        parts={"word/settings.xml": part("settings", '<w:defaultTabStop w:val="720"/><w:smartTagType w:namespaceuri="urn:x" w:name="unit"/>')}),
    "24_hyperlink_in_del": dict(body=P('<w:del w:id="7" w:author="A"><w:hyperlink w:anchor="x"><w:r><w:delText>Link</w:delText></w:r></w:hyperlink></w:del>')),
    "24_hyperlink_in_ins": dict(body=P('<w:ins w:id="7" w:author="A"><w:hyperlink w:anchor="x">' + R() + "</w:hyperlink></w:ins>")),
    # 25-29 duplicate ids
    "25_dup_ins_id": dict(body=P(f'<w:ins w:id="8" w:author="A">{R()}</w:ins><w:ins w:id="8" w:author="A">{R()}</w:ins>')),
    "25_ppr_ins_shares_id_ok": dict(body=P(f'<w:pPr><w:rPr><w:ins w:id="8" w:author="A"/></w:rPr></w:pPr><w:ins w:id="8" w:author="A">{R()}</w:ins>')),
    "25_dup_rprchange": dict(body=P(R('<w:b/><w:rPrChange w:id="5" w:author="A"><w:rPr/></w:rPrChange>')) + P(R('<w:b/><w:rPrChange w:id="5" w:author="A"><w:rPr/></w:rPrChange>'))),
    "26_tblgridchange_dup": dict(body=(TBL(grid='<w:gridCol w:w="2000"/><w:tblGridChange w:id="0"><w:tblGrid/></w:tblGridChange>') + "<w:p/>") * 2),
    "27_dup_styleid": dict(parts={"word/styles.xml": part("styles", STYLES_OK * 2)}),
    "29_dup_bookmark": dict(body=P('<w:bookmarkStart w:id="0" w:name="X"/><w:bookmarkEnd w:id="0"/><w:bookmarkStart w:id="0" w:name="X"/><w:bookmarkEnd w:id="0"/>')),
    "29_dup_comment_range": dict(body=P('<w:commentRangeStart w:id="0"/><w:commentRangeStart w:id="0"/>' + R() + '<w:commentRangeEnd w:id="0"/><w:commentRangeEnd w:id="0"/>'),
                                 parts={"word/comments.xml": part("comments", '<w:comment w:id="0" w:author="A"><w:p/></w:comment>')}),
    # 30-31 tracked changes
    "30_nested_ins": dict(body=P(f'<w:ins w:id="1" w:author="A">{R()}<w:ins w:id="2" w:author="A">{R()}</w:ins></w:ins>')),
    "30_nested_del": dict(body=P('<w:del w:id="1" w:author="A"><w:r><w:delText>x</w:delText></w:r><w:del w:id="2" w:author="A"><w:r><w:delText>y</w:delText></w:r></w:del></w:del>')),
    "31_t_under_del": dict(body=P('<w:del w:id="1" w:author="A"><w:r><w:t>x</w:t></w:r></w:del>')),
    "31_instrtext_under_del": dict(body=P('<w:del w:id="1" w:author="A"><w:r><w:instrText> DATE </w:instrText></w:r></w:del>')),
    # 32-39 namespaces and well-formedness
    "32_ignorable_undeclared": dict(doc_xml=f'{XML}<w:document xmlns:w="{W}" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="w14 wp14"><w:body><w:p/></w:body></w:document>'),
    "33_foreign_attr": dict(doc_xml=doc('<w:p xmlns:ex="urn:example:ex" ex:id="p1"/>')),
    "33_foreign_attr_ignorable_ok": dict(doc_xml=doc('<w:p/>').replace('mc:Ignorable="w14 w15"', 'xmlns:ex="urn:example:ex" mc:Ignorable="w14 w15 ex"').replace("<w:p/>", '<w:p ex:id="p1"/>')),
    "35_undeclared_prefix": dict(doc_xml=f'{XML}<w:document xmlns:w="{W}"><w:body><w:p w14:paraId="1234ABCD"/></w:body></w:document>'),
    "36_aliased_dup_attr": dict(doc_xml=f'{XML}<w:document xmlns:w="{W}"><w:body xmlns:x="urn:a" xmlns:y="urn:a" x:id="1" y:id="2"><w:p/></w:body></w:document>'),
    "37_core_ampersand": dict(raw={"docProps/core.xml": f'{XML}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:creator>Legal & Company</dc:creator></cp:coreProperties>'}),
    "38_control_char": dict(body=P(R(t="a\x01b"))),
    "38_control_char_ref": dict(body=P(R(t="a&#1;b"))),
    "39_utf8_bom_ok": dict(doc_xml="﻿" + doc(P(R()))),
    "39_utf16_bom_ok": dict(raw={"word/document.xml": "﻿".encode("utf-16-le") + doc(P(R())).replace('encoding="UTF-8"', 'encoding="UTF-16"').encode("utf-16-le")}),
    "39_utf16_no_bom": dict(raw={"word/document.xml": doc(P(R())).replace('encoding="UTF-8"', 'encoding="UTF-16"').encode("utf-16-le")}),
    "39_decl_utf8_bytes_utf16": dict(raw={"word/document.xml": b"\xff\xfe" + doc(P(R())).encode("utf-16-le")}),
    "39_truncated": dict(raw={"word/document.xml": b"\xff\xfe\x3c"}),
    "39_latin1_decl": dict(raw={"word/document.xml": doc(P(R(t="caf\xe9"))).replace('encoding="UTF-8"', 'encoding="ISO-8859-1"').encode("latin-1")}),
    # 40-44 package level
    "40_numbering_rel_to_styles": dict(drop=["word/numbering.xml"], rels_extra=f'<Relationship Id="rId99" Type="{REL}numbering" Target="styles.xml"/>'),
    "41_rid_absent": dict(body=P('<w:hyperlink r:id="absent">' + R() + "</w:hyperlink>")),
    "41_dup_rel_id": dict(rels_extra=f'<Relationship Id="rId10" Type="{REL}hyperlink" Target="http://x" TargetMode="External"/>'),
    "41_rels_wrong_root": dict(raw={"word/_rels/document.xml.rels": f'{XML}<Wrong xmlns="urn:x"/>'}),
    "41_no_officedocument_rel": dict(raw={"_rels/.rels": f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>'}),
    "41_numid_without_numbering": dict(drop=["word/numbering.xml"], body=P('<w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="3"/></w:numPr></w:pPr>')),
    "41_rels_for_missing_part": dict(raw={"word/_rels/ghost.xml.rels": f'{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}image" Target="media/a.png"/></Relationships>'}),
    "42_media_no_ct": dict(raw={"word/media/unknown.bin": b"\x00\x01"}),
    "42_override_missing_part": dict(ct_extra='<Override PartName="/word/ghost.xml" ContentType="application/xml"/>'),
    "42_duplicate_default": dict(ct_extra='<Default Extension="xml" ContentType="application/xml"/>'),
    "42_ct_wrong_root": dict(raw={"[Content_Types].xml": f'{XML}<Wrong xmlns="urn:x"/>'}),
    "42_document_no_main_ct": dict(raw={"[Content_Types].xml": f'{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/></Types>'}),
    "43_backslash_name": dict(raw={"word\\extra.xml": f"{XML}<a/>"}),
    "43_double_slash": dict(raw={"word//extra.xml": f"{XML}<a/>"}),
    "43_case_dup": dict(raw={"word/STYLES.XML": BASE["word/styles.xml"]}),
    "44_trash_ok": dict(raw={"[trash]/0000.dat": b"junk"}),
    "44_trash_bad_name": dict(raw={"[trash]/notes.dat": b"junk"}),
    "44_percent_name": dict(raw={"word/header%20one.xml": part("hdr", "<w:p/>")}, rels_extra=f'<Relationship Id="rId98" Type="{REL}header" Target="HeAdEr%20OnE.xml"/>',
                            ct_extra='<Override PartName="/word/header%20one.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>'),
}
cases.update({
    "41_rel_no_id": dict(rels_extra=f'<Relationship Type="{REL}hyperlink" Target="http://x" TargetMode="External"/>'),
    "41_rel_bad_targetmode": dict(rels_extra=f'<Relationship Id="rId97" Type="{REL}hyperlink" Target="http://x" TargetMode="Remote"/>'),
    "41_rel_other_child": dict(rels_extra='<Other/>'),
    "41_rel_id_not_ncname": dict(rels_extra=f'<Relationship Id="1abc" Type="{REL}hyperlink" Target="http://x" TargetMode="External"/>'),
    "42_duplicate_override": dict(ct_extra=f'<Override PartName="/word/styles.xml" ContentType="{CT["styles"]}"/>'),
})
for name, kw in cases.items():
    build(name, **kw)
with open(os.path.join(OUT, "list.txt"), "w") as f:
    f.write("".join(os.path.join(os.path.abspath(OUT), n + ".docx") + "\n" for n in cases))
print(len(cases), "cases")
