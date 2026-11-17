//! part ROOT_TYPE FILE.xml: validates one part, printing each error.
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let root = ooxml_validator::validate::type_id(&a[1]).expect("type");
    let xml = std::fs::read(&a[2]).unwrap();
    for e in ooxml_validator::validate::validate_part(&xml, "/part", root, &Default::default(), false).errors {
        println!("{} {} {}", e.id, e.node, e.description);
    }
}
