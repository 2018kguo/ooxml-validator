//! bench FILE.docx [N]: runs package::check N times (default 20); prints the
//! best and median wall time. For profiling: samply record target/release/examples/bench.
fn main() {
    let mut a = std::env::args().skip(1);
    let bytes = std::fs::read(a.next().expect("FILE.docx")).unwrap();
    let n: usize = a.next().map_or(20, |x| x.parse().unwrap());
    let mut times = vec![];
    let mut errors = 0;
    for _ in 0..n {
        let t = std::time::Instant::now();
        errors = ooxml_validator::package::check(&bytes).errors.len();
        times.push(t.elapsed());
    }
    times.sort();
    eprintln!("best {:?} median {:?} ({errors} errors)", times[0], times[n / 2]);
}
