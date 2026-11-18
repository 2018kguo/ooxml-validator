//! Small documents with the error ids the Open XML SDK 3.5.1 reports for them
//! (recorded with fuzz/oracle). Each pins a behavior a fuzzing or corpus run
//! once found ooxml-validator getting wrong, or a rule easy to break.

use std::io::Write;

const TRANSITIONAL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";

fn docx(document: &str) -> Vec<u8> {
    docx_rel(TRANSITIONAL, document)
}

fn docx_rel(rel: &str, document: &str) -> Vec<u8> {
    let rels = format!(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{rel}" Target="word/document.xml"/></Relationships>"#);
    let mut out = std::io::Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut out);
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let files = [
        ("[Content_Types].xml", r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#),
        ("_rels/.rels", rels.as_str()),
        ("word/document.xml", document),
    ];
    for (name, body) in files {
        z.start_file(name, o).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    }
    z.finish().unwrap();
    out.into_inner()
}

fn ids(document: &str) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = ooxml_validator::package::check(&docx(document)).errors.iter().map(|e| e.id).collect();
    v.sort();
    v.dedup();
    v
}

#[test]
fn bad_rel() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><w:hyperlink r:id="rId99" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:r><w:t>x</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#), ["Sem_InvalidRelationshipId"]);
}

#[test]
fn choice_bad_requires() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><mc:AlternateContent><mc:Choice Requires="nope"><w:r/></mc:Choice></mc:AlternateContent></w:p></w:body></w:document>"#), ["MC_InvalidRequiresAttribute"]);
}

#[test]
fn duplicate_id() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><w:bookmarkStart w:id="1" w:name="a"/><w:bookmarkEnd w:id="1"/><w:bookmarkStart w:id="1" w:name="b"/><w:bookmarkEnd w:id="1"/></w:p></w:body></w:document>"#), ["Sem_UniqueAttributeValue"]);
}

/// The SDK's XPaths: a duplicate value at its last element, last first.
#[test]
fn xpaths() {
    let bm = |vals: &[u8]| vals.iter().enumerate().map(|(i, v)| format!(r#"<w:p><w:bookmarkStart w:id="{v}" w:name="b{i}"/><w:bookmarkEnd w:id="{v}"/></w:p>"#)).collect::<String>();
    let doc = |body: &str| format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#);
    let paths = |body: &str| ooxml_validator::package::check(&docx(&doc(body))).errors.into_iter().map(|e| (e.id, e.xpath.unwrap_or_default())).collect::<Vec<_>>();
    let unique = "Sem_UniqueAttributeValue";
    let at = |p: u8, el: &str| format!("/w:document[1]/w:body[1]/w:p[{p}]/w:{el}[1]");
    let mut three = paths(&bm(&[1, 1, 1]));
    three.sort();
    assert_eq!(three, [(unique, at(3, "bookmarkEnd")), (unique, at(3, "bookmarkStart"))]);
    let mut two = paths(&bm(&[1, 2, 1, 2]));
    two.sort();
    assert_eq!(two, [(unique, at(3, "bookmarkEnd")), (unique, at(3, "bookmarkStart")), (unique, at(4, "bookmarkEnd")), (unique, at(4, "bookmarkStart"))]);
    // The prefix LookupPrefix finds from the element up: here the one it declares.
    let mut other = paths(r#"<w:p><x:r xmlns:x="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><x:bad/></x:r><nons/></w:p>"#);
    other.sort();
    let invalid = "Sch_InvalidElementContentExpectingComplex";
    assert_eq!(other, [(invalid, "/w:document[1]/w:body[1]/w:p[1]".to_string()), (invalid, "/w:document[1]/w:body[1]/w:p[1]/x:r[1]".to_string())]);
}

#[test]
fn empty_ac() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><mc:AlternateContent/></w:p></w:body></w:document>"#), ["Sch_IncompleteContentExpectingComplex"]);
}

#[test]
fn fallback_children() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><w:r><w:rPr><mc:AlternateContent><mc:Choice Requires="w14"><w:b/></mc:Choice><mc:Fallback><w:b/><w:b/></mc:Fallback></mc:AlternateContent></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"#), ["Sch_UnexpectedElementContentExpectingComplex"]);
}

#[test]
fn fallback_first() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><mc:AlternateContent><mc:Fallback><w:r/></mc:Fallback></mc:AlternateContent></w:p></w:body></w:document>"#), ["Sch_IncompleteContentExpectingComplex"]);
}

#[test]
fn ignored_wrapper() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" mc:Ignorable="zz"><w:body><w:p><w:r><w:rPr><zz:wrap><w:b/></zz:wrap></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"#), Vec::<&str>::new());
}

#[test]
fn math_val_optional() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p/><w:sectPr/></w:body></w:document>"#), Vec::<&str>::new());
}

#[test]
fn max_length() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><w:r><w:rPr><w:rFonts w:ascii="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"/></w:rPr></w:r></w:p></w:body></w:document>"#), ["Sch_AttributeValueDataTypeDetailed"]);
}

#[test]
fn processed_wrapper() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" mc:Ignorable="zz" mc:ProcessContent="zz:wrap"><w:body><w:p><w:r><w:rPr><zz:wrap><w:b/></zz:wrap></w:rPr><w:t>x</w:t></w:r></w:p></w:body></w:document>"#), ["Sch_InvalidElementContentWrongType"]);
}

#[test]
fn undeclared_prefix() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><w:r><qq:t>x</qq:t></w:r></w:p></w:body></w:document>"#), ["xml_parse"]);
}

#[test]
fn union() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><w:pPr><w:spacing w:line="278.5"/></w:pPr></w:p></w:body></w:document>"#), ["Sch_AttributeUnionFailedEx"]);
}

#[test]
fn xml_space() {
    assert_eq!(ids(r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:zz="urn:example:zz" ><w:body><w:p><w:r><w:t xml:space="t">x</w:t></w:r></w:p></w:body></w:document>"#), ["xml_parse"]);
}


const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
const STRICT_BODY: &str = r#"<w:body><w:p><w:pPr><w:ind w:left="720"/><w:jc w:val="start"/></w:pPr></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblLook w:firstRow="1" w:noVBand="1"/></w:tblPr><w:tblGrid><w:gridCol w:w="100"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:body>"#;

#[test]
fn strict_reads_as_transitional() {
    let doc = format!(r#"<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main" w:conformance="strict">{STRICT_BODY}</w:document>"#);
    assert!(ooxml_validator::package::check(&docx_rel(STRICT, &doc)).ok());
}

#[test]
fn strict_part_under_transitional_rel_cannot_load() {
    // The SDK translates namespaces only once a Strict relationship is found;
    // otherwise the root is unknown and loading the part throws.
    let doc = format!(r#"<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main">{STRICT_BODY}</w:document>"#);
    assert!(ooxml_validator::package::check(&docx_rel(TRANSITIONAL, &doc)).open_error.is_some());
}

#[test]
fn custom_xml_attributes() {
    // CustomXmlElement (hand-written in the SDK): an unprefixed uri, w:element an NCName.
    let doc = |attrs: &str| format!(r#"<w:document {W}><w:body><w:customXml {attrs}><w:p/></w:customXml></w:body></w:document>"#);
    assert_eq!(ids(&doc(r#"uri="urn:x" w:element="a""#)), Vec::<&str>::new());
    assert_eq!(ids(&doc(r#"w:uri="urn:x" w:element="a""#)), ["Sch_UndeclaredAttribute"]);
    assert_eq!(ids(&doc(r#"w:element="1bad""#)), ["Sch_AttributeValueDataTypeDetailed"]);
}

/// A package with a styles part, after `edit` rewrites (part name, text) pairs.
fn with_styles(edit: impl Fn(&str, String) -> String) -> Vec<u8> {
    const CT: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
    let files = [
        ("[Content_Types].xml", CT.to_string()),
        ("_rels/.rels", format!(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{TRANSITIONAL}" Target="word/document.xml"/></Relationships>"#)),
        ("word/_rels/document.xml.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_string()),
        ("word/document.xml", format!(r#"<w:document {W}><w:body><w:p/></w:body></w:document>"#)),
        ("word/styles.xml", format!(r#"<w:styles {W}/>"#)),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut out);
    for (name, body) in files {
        z.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(edit(name, body).as_bytes()).unwrap();
    }
    z.finish().unwrap();
    out.into_inner()
}

#[test]
fn package_load_failures_are_open_errors() {
    // What the SDK throws while loading parts, each checked against the oracle.
    let opens = |edit: &dyn Fn(&str, String) -> String| ooxml_validator::package::check(&with_styles(edit)).open_error.is_some();
    let rels = |add: &'static str| move |n: &str, b: String| if n == "word/_rels/document.xml.rels" { b.replace("</Relationships>", &format!("{add}</Relationships>")) } else { b };
    assert!(!opens(&|_, b| b));
    // Missing target, and content types: wrong, or an Override whose case differs.
    assert!(opens(&|n, b| if n.ends_with(".rels") { b.replace(r#"Target="styles.xml""#, r#"Target="none.xml""#) } else { b }));
    assert!(opens(&|n, b| if n == "[Content_Types].xml" { b.replace("wordprocessingml.styles+xml", "foo+xml") } else { b }));
    assert!(opens(&|n, b| if n == "[Content_Types].xml" { b.replace("/word/styles.xml", "/WORD/Styles.xml") } else { b }));
    // A target named in another case is the same part.
    assert!(!opens(&|n, b| if n.ends_with(".rels") { b.replace(r#"Target="styles.xml""#, r#"Target="STYLES.XML""#) } else { b }));
    // One part under two relationship types; a missing image.
    assert!(opens(&rels(r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footer" Target="styles.xml"/>"#)));
    assert!(opens(&rels(r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/none.png"/>"#)));
    // Hyperlinks and NULL targets load nothing.
    assert!(!opens(&rels(r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="nowhere.xml"/>"#)));
    assert!(!opens(&rels(r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="NULL"/>"#)));
}

#[test]
fn large_part_is_read_whole() {
    // A part over half the 128 MB XML budget was read short and reported as
    // unclosed XML: the read limit came from the budget left after it.
    let p = "<w:p><w:r><w:t>The same sentence, many times over, makes a large part.</w:t></w:r></w:p>";
    let body = p.repeat(70_000_000 / p.len());
    let doc = format!(r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#);
    assert!(doc.len() > 64 << 20);
    let report = ooxml_validator::package::check(&docx(&doc));
    assert_eq!(report.open_error, None);
    assert!(report.errors.is_empty(), "{:?}", report.errors.first().map(|e| &e.description));
}
