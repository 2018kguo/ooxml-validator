//! proof FILE.docx...: validates each document's main part and prints one
//! JSON line per file: {"path", "ok", "ids", "parts", "first"}.

use std::io::Write;

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    let out = std::io::stdout();
    let mut out = out.lock();
    for path in std::env::args().skip(1) {
        let result = std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| {
            let r = proof::package::check(&b);
            match r.open_error {
                Some(e) if r.errors.is_empty() => Err(e),
                _ => Ok(r.errors),
            }
        });
        match result {
            Ok(errors) => {
                let ids: Vec<String> = errors.iter().map(|e| format!("\"{}\"", e.id)).collect();
                let parts: Vec<String> = errors.iter().map(|e| format!("\"{}\"", esc(&e.part))).collect();
                let first = errors.first().map_or(String::new(), |e| format!("{} {}", e.node, e.description));
                writeln!(out, "{{\"path\":\"{}\",\"ok\":{},\"ids\":[{}],\"parts\":[{}],\"first\":\"{}\"}}", esc(&path), errors.is_empty(), ids.join(","), parts.join(","), esc(&first)).unwrap();
            }
            Err(e) => writeln!(out, "{{\"path\":\"{}\",\"ok\":false,\"ids\":[\"open\"],\"first\":\"{}\"}}", esc(&path), esc(&e)).unwrap(),
        }
    }
}
