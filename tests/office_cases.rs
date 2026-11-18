//! Workbooks and decks with the error ids the Open XML SDK 3.5.1 reports for
//! them (each recorded with fuzz/oracle). Each pins a behavior a corpus or
//! fuzzing run once found ooxml-validator getting wrong, or a rule easy to break.

use ooxml_validator::{Format, Report, check, check_as};
use std::io::Write;

const XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
const S: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const RELS_CT: &str = r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>"#;

fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut out);
    for (name, body) in files {
        z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(body).unwrap();
    }
    z.finish().unwrap();
    out.into_inner()
}

fn rels(items: &[(&str, &str, &str)]) -> String {
    let body: String = items.iter().map(|(id, t, target)| format!(r#"<Relationship Id="{id}" Type="{}" Target="{target}"/>"#, if t.starts_with("http") { t.to_string() } else { format!("{REL}{t}") })).collect();
    format!(r#"{XML}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#)
}

/// A workbook of one sheet, each part passed through `edit`, plus `extra` files.
fn xlsx_with(edit: impl Fn(&str, String) -> String, extra: &[(&'static str, Vec<u8>)]) -> Vec<u8> {
    let ct = "application/vnd.openxmlformats-officedocument.spreadsheetml.";
    let parts = [
        ("[Content_Types].xml", format!(r#"{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{RELS_CT}<Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="{ct}sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{ct}worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="{ct}styles+xml"/></Types>"#)),
        ("_rels/.rels", rels(&[("rId1", "officeDocument", "xl/workbook.xml")])),
        ("xl/workbook.xml", format!(r#"{XML}<workbook xmlns="{S}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#)),
        ("xl/_rels/workbook.xml.rels", rels(&[("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "styles", "styles.xml")])),
        ("xl/worksheets/sheet1.xml", format!(r#"{XML}<worksheet xmlns="{S}"><sheetData><row r="1"><c r="A1" s="1"><v>1</v></c></row></sheetData></worksheet>"#)),
        ("xl/styles.xml", format!(r#"{XML}<styleSheet xmlns="{S}"><fonts count="1"><font/></fonts><fills count="1"><fill/></fills><borders count="1"><border/></borders><cellXfs count="2"><xf/><xf/></cellXfs></styleSheet>"#)),
    ];
    let mut files: Vec<(&str, Vec<u8>)> = parts.into_iter().map(|(n, b)| (n, edit(n, b).into_bytes())).collect();
    files.extend(extra.iter().cloned());
    zip(&files)
}

fn xlsx(edit: impl Fn(&str, String) -> String) -> Vec<u8> {
    xlsx_with(edit, &[])
}
fn replace(part: &'static str, from: &'static str, to: &'static str) -> impl Fn(&str, String) -> String {
    move |n, b| if n == part { assert!(b.contains(from), "{from}"); b.replace(from, to) } else { b }
}

fn ids(r: &Report) -> Vec<&'static str> {
    assert_eq!(r.open_error, None);
    let mut v: Vec<&'static str> = r.errors.iter().map(|e| e.id).collect();
    v.sort();
    v.dedup();
    v
}

fn xlsx_ids(edit: impl Fn(&str, String) -> String) -> Vec<&'static str> {
    ids(&check(&xlsx(edit)))
}

#[test]
fn base_workbook_is_valid() {
    assert_eq!(xlsx_ids(|_, b| b), [""; 0]);
}

#[test]
fn named_ranges_before_the_sheet_list() {
    // The incident: a writer put definedNames ahead of sheets.
    let names = r#"<definedNames><definedName name="Total">Sheet1!$A$1</definedName></definedNames>"#;
    let edit = move |n: &str, b: String| if n == "xl/workbook.xml" { b.replace("<sheets>", &format!("{names}<sheets>")) } else { b };
    assert_eq!(xlsx_ids(edit), ["Sch_UnexpectedElementContentExpectingComplex"]);
}

#[test]
fn data_validation_prompt_title_is_32_characters_at_most() {
    let dv = |title: &str| {
        let s = format!(r#"</sheetData><dataValidations count="1"><dataValidation type="list" showInputMessage="1" promptTitle="{title}" sqref="A2"><formula1>"Yes,No"</formula1></dataValidation></dataValidations>"#);
        move |n: &str, b: String| if n == "xl/worksheets/sheet1.xml" { b.replace("</sheetData>", &s) } else { b }
    };
    assert_eq!(xlsx_ids(dv(&"t".repeat(32))), [""; 0]);
    assert_eq!(xlsx_ids(dv(&"t".repeat(33))), ["Sem_AttributeValueDataTypeDetailed"]);
}

#[test]
fn cell_values_suit_their_types() {
    // CellType's own validator (Sem_CellValue): a Number parses, a Boolean is
    // 0/1/true/false, a Date has milliseconds; other types aren't checked.
    let cell = |c: &'static str| replace("xl/worksheets/sheet1.xml", r#"<c r="A1" s="1"><v>1</v></c>"#, c);
    assert_eq!(xlsx_ids(cell(r#"<c r="A1" t="n"><v>abc</v></c>"#)), ["Sem_CellValue"]);
    assert_eq!(xlsx_ids(cell(r#"<c r="A1" t="n"><v></v></c>"#)), ["Sem_CellValue"]);
    assert_eq!(xlsx_ids(cell(r#"<c r="A1" t="n"><v> 1,000.5e3- </v></c>"#)), [""; 0]);
    assert_eq!(xlsx_ids(cell(r#"<c r="A1" t="b"><v>TRUE</v></c>"#)), ["Sem_CellValue"]);
    assert_eq!(xlsx_ids(cell(r#"<c r="A1" t="d"><v>2026-11-17T09:30:00</v></c>"#)), ["Sem_CellValue"]);
    assert_eq!(xlsx_ids(cell(r#"<c r="A1" t="d"><v>2024-02-29T00:00:00.000+05:30</v></c>"#)), [""; 0]);
    assert_eq!(xlsx_ids(cell(r#"<c r="A1" t="str"><v>abc</v></c>"#)), [""; 0]);
    // Only a direct x:v child is the cell's value.
    let ac = r#"<c r="A1" t="n" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:AlternateContent><mc:Choice Requires="mc"><v>abc</v></mc:Choice></mc:AlternateContent></c>"#;
    assert!(!xlsx_ids(cell(ac)).contains(&"Sem_CellValue"));
}

#[test]
fn style_indexes_count_what_the_sdk_traverses() {
    // IndexReference counts the elements ValidatingTraverse yields under
    // cellXfs: an AlternateContent counts as one, what it holds doesn't.
    let s2 = replace("xl/worksheets/sheet1.xml", r#"s="1""#, r#"s="2""#);
    assert_eq!(xlsx_ids(&s2), ["Sem_MissingIndexedElement"]);
    let ac = |n: &str, b: String| {
        let b = s2(n, b);
        if n == "xl/styles.xml" { b.replace("<xf/><xf/>", r#"<xf/><xf/><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="mc"><xf/><xf/></mc:Choice></mc:AlternateContent>"#) } else { b }
    };
    assert_eq!(xlsx_ids(ac), [""; 0]);
}

#[test]
fn excel_constraints_run_only_for_excel() {
    // Application = Excel: x:sheet's name is unique without case in a workbook.
    let two = replace("xl/workbook.xml", r#"<sheet name="Sheet1" sheetId="1" r:id="rId1"/>"#, r#"<sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="DATA" sheetId="2" r:id="rId1"/>"#);
    assert_eq!(xlsx_ids(&two), ["Sem_UniqueAttributeValue"]);
}

#[test]
fn a_worksheet_cannot_reference_video() {
    // Only some part classes know data parts (IKnownDataPartFeature); a slide does.
    let mp4 = replace("[Content_Types].xml", RELS_CT, r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="mp4" ContentType="video/mp4"/>"#);
    let extra = [("xl/worksheets/_rels/sheet1.xml.rels", rels(&[("rId5", "video", "../media/v.mp4")]).into_bytes()), ("xl/media/v.mp4", vec![0; 8])];
    assert_eq!(ids(&check(&xlsx_with(mp4, &extra))), ["Pkg_DataPartReferenceIsNotAllowed"]);
}

#[test]
fn parts_need_a_content_type_and_a_uri_name() {
    // Without a Default for "rels" no relationship part is a part: the package
    // has no workbook, and nothing else is validated.
    assert_eq!(xlsx_ids(replace("[Content_Types].xml", RELS_CT, "")), ["Pkg_RequiredPartDoNotExist"]);
    // A relationship part must be in the relationships namespace.
    let bare = |n: &str, b: String| if n == "xl/_rels/workbook.xml.rels" { b.replace(r#" xmlns="http://schemas.openxmlformats.org/package/2006/relationships""#, "") } else { b };
    assert!(check(&xlsx(bare)).open_error.is_some());
    // A zip name with a space isn't a part URI, so it can't be found.
    let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#.as_bytes().to_vec();
    let spaced = xlsx_with(
        |n, b| match n {
            "xl/_rels/workbook.xml.rels" => b.replace("worksheets/sheet1.xml", "worksheets/sheet%201.xml"),
            "[Content_Types].xml" => b.replace("/xl/worksheets/sheet1.xml", "/xl/worksheets/sheet%201.xml"),
            _ => b,
        },
        &[("xl/worksheets/sheet 1.xml", sheet)],
    );
    assert!(check(&spaced).open_error.is_some());
    // An escaped unreserved character is the character; an escaped reserved one is not.
    assert!(check(&xlsx(|n, b| if n == "xl/_rels/workbook.xml.rels" { b.replace("sheet1.xml", "sheet%31.xml") } else { b })).ok());
    assert!(check(&xlsx(|n, b| if n == "xl/_rels/workbook.xml.rels" { b.replace("styles.xml", "style%73.xml") } else { b })).ok());
    let bang = xlsx_with(|n, b| if n == "xl/_rels/workbook.xml.rels" { b.replace("styles.xml", "styles%21.xml") } else { b }, &[]);
    assert!(check(&bang).open_error.is_some());
}

#[test]
fn zip_entries_are_read_as_dotnet_reads_them() {
    // A wrong CRC-32 is not checked.
    let mut bytes = xlsx(|_, b| b);
    let at = bytes.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
    bytes[at + 16] ^= 0xff;
    let r = check(&bytes);
    assert!(r.ok(), "{:?}", r.open_error);
    // Folder entries are not parts: "xl/" and "XL/" don't clash.
    let folders = xlsx_with(|_, b| b, &[("xl/", vec![]), ("XL/", vec![])]);
    assert!(check(&folders).ok());
}


/// A deck of `slides` slides sharing one layout.
fn pptx(slides: usize, extra_rels: &str, extra: &[(&str, &[u8])]) -> Vec<u8> {
    let p = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
    let ct = "application/vnd.openxmlformats-officedocument.presentationml.";
    let grp = r#"<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>"#;
    let mut overrides = format!(r#"<Override PartName="/ppt/presentation.xml" ContentType="{ct}presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="{ct}slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="{ct}slideLayout+xml"/>"#);
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    let mut ids = String::new();
    let mut pres_rels = vec![("rId1".to_string(), "slideMaster".to_string(), "slideMasters/slideMaster1.xml".to_string())];
    for n in 1..=slides {
        overrides += &format!(r#"<Override PartName="/ppt/slides/slide{n}.xml" ContentType="{ct}slide+xml"/>"#);
        ids += &format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 255 + n, n + 1);
        pres_rels.push((format!("rId{}", n + 1), "slide".into(), format!("slides/slide{n}.xml")));
        files.push((format!("ppt/slides/slide{n}.xml"), format!(r#"{XML}<p:sld {p}>{grp}</p:sld>"#).into_bytes()));
        files.push((format!("ppt/slides/_rels/slide{n}.xml.rels"), rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")]).into_bytes()));
    }
    let pr: Vec<(&str, &str, &str)> = pres_rels.iter().map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str())).collect();
    files.push(("[Content_Types].xml".into(), format!(r#"{XML}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{RELS_CT}<Default Extension="xml" ContentType="application/xml"/><Default Extension="bin" ContentType="application/octet-stream"/>{overrides}</Types>"#).into_bytes()));
    files.push(("_rels/.rels".into(), rels(&[("rId1", "officeDocument", "ppt/presentation.xml")]).into_bytes()));
    files.push(("ppt/presentation.xml".into(), format!(r#"{XML}<p:presentation {p}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#).into_bytes()));
    files.push(("ppt/_rels/presentation.xml.rels".into(), rels(&pr).replace("</Relationships>", &format!("{extra_rels}</Relationships>")).into_bytes()));
    files.push(("ppt/slideMasters/slideMaster1.xml".into(), format!(r#"{XML}<p:sldMaster {p}>{grp}<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>"#).into_bytes()));
    files.push(("ppt/slideMasters/_rels/slideMaster1.xml.rels".into(), rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")]).into_bytes()));
    files.push(("ppt/slideLayouts/slideLayout1.xml".into(), format!(r#"{XML}<p:sldLayout {p}>{grp}</p:sldLayout>"#).into_bytes()));
    files.push(("ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(), rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")]).into_bytes()));
    files.extend(extra.iter().map(|(n, b)| (n.to_string(), b.to_vec())));
    let refs: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, b)| (n.as_str(), b.clone())).collect();
    zip(&refs)
}

#[test]
fn a_shared_layout_reached_twice_in_a_row_is_read_once() {
    // Slides 1 and 2 reach the layout one after the other; its relationships
    // were read again, and its one master counted twice (Pkg_OnlyOnePartAllowed).
    let r = check(&pptx(2, "", &[]));
    assert!(r.ok(), "{:?}", r.errors.first().map(|e| &e.description));
}

#[test]
fn a_deck_holds_printer_settings_as_an_extended_part() {
    // PresentationDocument's part factory has no printer settings class, so the
    // relationship makes an extended part, whatever its content type.
    let rel = format!(r#"<Relationship Id="rId99" Type="{REL}printerSettings" Target="printerSettings/printerSettings1.bin"/>"#);
    let r = check(&pptx(1, &rel, &[("ppt/printerSettings/printerSettings1.bin", &[0; 16])]));
    assert!(r.ok(), "{:?} {:?}", r.open_error, r.errors.first().map(|e| &e.description));
}

#[test]
fn a_package_is_opened_as_its_main_part_names() {
    // By content type, by root element when the type names no class, or as asked.
    let deck = pptx(1, "", &[]);
    assert_eq!(ooxml_validator::package::Files::new(vec![], Default::default()).format(), Format::Word);
    assert!(check(&deck).ok());
    let as_word = check_as(&deck, Some(Format::Word));
    assert!(!as_word.ok(), "a deck opened as a Word document has no Word main part");
    let generic = xlsx(replace("[Content_Types].xml", "sheet.main+xml", "fake+xml"));
    assert!(check(&generic).ok());
}
