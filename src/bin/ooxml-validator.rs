//! ooxml-validator [--json] [--max-errors N] FILE.docx...
//!
//! Validates each document as Microsoft's Open XML SDK validator would and
//! prints its errors; exits 1 if any document is invalid. --json prints one
//! object per file: {"path","ok","openError","errors":[{"id","description",
//! "part","node","errorType"}]}. --max-errors caps each file's list (default
//! 1000, the SDK's; 0 for no limit).

use std::io::Write;

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut as_json, mut max, mut paths) = (false, 1000usize, vec![]);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--json" => as_json = true,
            "--max-errors" => max = args.next().and_then(|n| n.parse().ok()).unwrap_or_else(|| usage()),
            "-h" | "--help" => usage(),
            _ => paths.push(a),
        }
    }
    if paths.is_empty() {
        usage();
    }
    let out = std::io::stdout();
    let mut out = out.lock();
    let mut all_ok = true;
    for path in paths {
        let report = match std::fs::read(&path) {
            Ok(bytes) => ooxml_validator::check(&bytes),
            Err(e) => ooxml_validator::Report { open_error: Some(e.to_string()), errors: vec![] },
        };
        all_ok &= report.ok();
        let errors = &report.errors[..if max == 0 { report.errors.len() } else { report.errors.len().min(max) }];
        if as_json {
            let body = report.to_json(max);
            writeln!(out, "{{\"path\":{},{}", ooxml_validator::package::json(&path), &body[1..]).unwrap();
        } else if let Some(e) = &report.open_error {
            writeln!(out, "{path}: cannot be opened: {e}").unwrap();
        } else if report.errors.is_empty() {
            writeln!(out, "{path}: ok").unwrap();
        } else {
            let n = report.errors.len();
            writeln!(out, "{path}: {n} error{}", if n == 1 { "" } else { "s" }).unwrap();
            for e in errors {
                writeln!(out, "  {} {} {}: {}", e.part, e.node, e.id, e.description).unwrap();
            }
        }
    }
    std::process::exit(if all_ok { 0 } else { 1 });
}

fn usage() -> ! {
    eprintln!("usage: ooxml-validator [--json] [--max-errors N] FILE.docx...");
    std::process::exit(2);
}
