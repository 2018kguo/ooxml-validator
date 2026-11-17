//! Small documents with the error ids the Open XML SDK 3.5.1 reports for them
//! (recorded with fuzz/oracle). Each pins a behavior a fuzzing or corpus run
//! once found obstat getting wrong, or a rule easy to break.

use std::io::Write;

fn docx(document: &str) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    let mut z = zip::ZipWriter::new(&mut out);
    let o = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let files = [
        ("[Content_Types].xml", r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#),
        ("_rels/.rels", r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#),
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
    let mut v: Vec<&'static str> = obstat::package::check(&docx(document)).errors.iter().map(|e| e.id).collect();
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

