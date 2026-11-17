//! split FILE.docx: time the well-formedness prepass against full validation.
fn main() {
    let bytes = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    for _ in 0..3 {
        let t = std::time::Instant::now();
        let r = proof::package::check(&bytes);
        let all = t.elapsed();
        let t = std::time::Instant::now();
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).unwrap();
        let n = proof::package::validate(&mut zip).len();
        eprintln!("check {all:?} ({} errors); validate only {:?} ({n})", r.errors.len(), t.elapsed());
    }
}
