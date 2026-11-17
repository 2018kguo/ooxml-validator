//! Opens a package and validates every part reachable from it, as the SDK's
//! DocumentValidator does: relationships from the package down, each part's
//! class decided by its relationship type among its parent's known children
//! (anything else is an extended part, which has no schema).

use crate::schema::{Arg, ConstraintKind, NONE, PartChild};
use crate::schema_gen::{CONSTRAINTS, PART_CHILDREN, PARTS, TARGET_VERSION};
use crate::semantic::{Facts, Rels};
use crate::validate::{ValidationError, validate_part};
use crate::xml::{Event, Reader};
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::sync::OnceLock;

const PACKAGE: &str = "WordprocessingDocument";

fn class(name: &str) -> Option<usize> {
    PARTS.iter().position(|p| p.name == name)
}

fn rules(parent: usize) -> &'static [PartChild] {
    let (s, n) = PARTS[parent].children;
    &PART_CHILDREN[s as usize..(s + n) as usize]
}

/// The child class of `parent` a relationship type names.
fn child_class(parent: usize, rel: &str) -> Option<usize> {
    rules(parent).iter().map(|c| c.class as usize).find(|&c| PARTS[c].rel == rel)
}

/// The SDK's part factory is global: a known relationship type makes its
/// class anywhere (then reported as not allowed there). Where several classes
/// share a type, the one a Word package can hold.
fn global_class(rel: &str) -> Option<usize> {
    static WORD: OnceLock<Vec<bool>> = OnceLock::new();
    let word = WORD.get_or_init(|| {
        let mut seen = vec![false; PARTS.len()];
        let mut stack = vec![class(PACKAGE).unwrap()];
        while let Some(c) = stack.pop() {
            if !std::mem::replace(&mut seen[c], true) {
                stack.extend(rules(c).iter().map(|r| r.class as usize));
            }
        }
        seen
    });
    let mut all = (0..PARTS.len()).filter(|&c| !rel.is_empty() && PARTS[c].rel == rel);
    let first = all.next()?;
    Some(std::iter::once(first).chain(all).find(|&c| word[c]).unwrap_or(first))
}

/// Resolves a relationship target against its source part's folder.
fn resolve(source: &str, target: &str) -> String {
    let target = percent_decode(target);
    let mut segments: Vec<&str> = if target.starts_with('/') {
        vec![]
    } else {
        let dir = source.rsplit_once('/').map_or("", |d| d.0);
        dir.split('/').filter(|s| !s.is_empty()).collect()
    };
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    segments.join("/")
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Each relationship in a .rels part: (id, type, target, external).
fn relationships(xml: &[u8]) -> Vec<(String, String, String, bool)> {
    let mut out = vec![];
    let mut r = Reader::new(xml);
    while let Ok(e) = r.next() {
        match e {
            Event::Eof => break,
            Event::Start { name, .. } if name.ends_with(b"Relationship") => {
                let get = |k: &[u8]| r.attrs.iter().find(|a| a.name == k).map(|a| String::from_utf8_lossy(a.value).into_owned());
                if let (Some(t), Some(target)) = (get(b"Type"), get(b"Target")) {
                    out.push((get(b"Id").unwrap_or_default(), t, target, get(b"TargetMode").as_deref() == Some("External")));
                }
            }
            _ => {}
        }
    }
    out
}

fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, name)) => format!("{dir}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// A part reached from the package, and where it sits in the part tree.
struct Node {
    path: String,
    class: Option<usize>,
    /// Index of the part that first reached it (None: the package).
    parent: Option<usize>,
    /// Child parts: (class, node index, relationship type).
    children: Vec<(Option<usize>, usize, String)>,
    facts: Facts,
    has_root: bool,
}

/// SemanticConstraint.GetReferencedPart: ".", "..", "/A/B" from the package,
/// or "A/B" from the current part, by part class name; each step must be
/// the only child of that class.
fn referenced(nodes: &[Node], package: &[(Option<usize>, usize, String)], current: usize, path: &str) -> Option<usize> {
    match path {
        "." => return Some(current),
        ".." => return nodes[current].parent,
        _ => {}
    }
    let (mut kids, rest) = match path.strip_prefix('/') {
        Some(r) => (package, r),
        None => (&nodes[current].children[..], path),
    };
    let mut found = None;
    for step in rest.split('/') {
        let mut matches = kids.iter().filter(|(c, _, _)| c.is_some_and(|c| PARTS[c].name == step));
        let n = matches.next()?.1;
        if matches.next().is_some() {
            return None;
        }
        found = Some(n);
        kids = &nodes[n].children;
    }
    found
}

/// A package's XML entries (.xml and .rels), each inflated once, in zip order.
pub struct Files {
    pub entries: Vec<(String, Vec<u8>)>,
    index: HashMap<String, usize>,
}

impl Files {
    pub fn new(entries: Vec<(String, Vec<u8>)>) -> Files {
        let index = entries.iter().enumerate().map(|(i, e)| (e.0.clone(), i)).collect();
        Files { entries, index }
    }

    fn get(&self, name: &str) -> Option<(usize, &[u8])> {
        self.index.get(name).map(|&i| (i, &self.entries[i].1[..]))
    }
}

/// What validating a package found.
pub struct Outcome {
    pub errors: Vec<ValidationError>,
    /// Entries validated as parts (their well-formedness is checked there).
    pub parsed: Vec<bool>,
    /// The first part, in zip order, that is not well-formed XML.
    pub malformed: Option<(usize, ValidationError)>,
}

/// Validates every reachable part of a package: discovers the part tree from
/// the relationship parts, validates the parts in parallel, then checks the
/// package's structure and the constraints that span parts.
pub fn validate(files: &Files) -> Outcome {
    let package = class(PACKAGE).expect("package class");
    let mut nodes: Vec<Node> = vec![];
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut top: Vec<(Option<usize>, usize, String)> = vec![];
    let mut rels_of: Vec<Rels> = vec![];
    let read_rels = |path: &str| files.get(path).map(|(_, x)| relationships(x)).unwrap_or_default();
    // (path, class, parent node, relationship type)
    let mut queue: VecDeque<(String, Option<usize>, Option<usize>, String)> = VecDeque::new();
    for (_, t, target, external) in read_rels("_rels/.rels") {
        if !external {
            queue.push_back((resolve("", &target), child_class(package, &t).or_else(|| global_class(&t)), None, t));
        }
    }
    while let Some((path, cls, parent, rel)) = queue.pop_front() {
        let n = match index.get(&path) {
            Some(&n) => n,
            None => {
                let n = nodes.len();
                nodes.push(Node { path: path.clone(), class: cls, parent, children: vec![], facts: Facts::default(), has_root: false });
                rels_of.push(Rels::default());
                index.insert(path.clone(), n);
                n
            }
        };
        let cls = nodes[n].class;
        match parent {
            Some(p) => nodes[p].children.push((cls, n, rel)),
            None => top.push((cls, n, rel)),
        }
        if n + 1 != nodes.len() {
            continue; // reached before
        }
        if std::env::var_os("PROOF_TRACE").is_some() {
            eprintln!("part {path} {:?}", cls.map(|c| PARTS[c].name));
        }
        if cls.is_some_and(|c| PARTS[c].version > TARGET_VERSION) {
            continue;
        }
        let rels = read_rels(&rels_path(&path));
        rels_of[n] = Rels { types: rels.iter().map(|(id, t, _, _)| (id.clone(), t.clone())).collect() };
        for (_, t, target, external) in rels {
            if !external {
                let child = cls.and_then(|c| child_class(c, &t)).or_else(|| global_class(&t));
                queue.push_back((resolve(&path, &target), child, Some(n), t));
            }
        }
    }

    // The parts to validate, largest first, over a few threads.
    let mut jobs: Vec<(usize, usize, &[u8], u16)> = nodes
        .iter()
        .enumerate()
        .filter_map(|(n, node)| {
            let c = node.class.filter(|&c| PARTS[c].version <= TARGET_VERSION && PARTS[c].root != NONE)?;
            let (entry, xml) = files.get(&node.path)?;
            Some((n, entry, xml, PARTS[c].root))
        })
        .collect();
    jobs.sort_by_key(|j| std::cmp::Reverse(j.2.len()));
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(jobs.len()).min(4).max(1);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut results: Vec<Option<crate::validate::PartResult>> = (0..nodes.len()).map(|_| None).collect();
    std::thread::scope(|scope| {
        let work = || {
            let mut out = vec![];
            loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(n, _, xml, root)) = jobs.get(k) else { break };
                out.push((n, validate_part(xml, &format!("/{}", nodes[n].path), root, &rels_of[n])));
            }
            out
        };
        let handles: Vec<_> = (1..threads).map(|_| scope.spawn(work)).collect();
        for (n, r) in work().into_iter().chain(handles.into_iter().flat_map(|h| h.join().expect("validator thread"))) {
            results[n] = Some(r);
        }
    });

    let mut parsed = vec![false; files.entries.len()];
    let mut malformed: Option<(usize, ValidationError)> = None;
    let mut errors = vec![];
    let mut deferred = vec![];
    // In discovery order, as the SDK reports parts.
    for (n, r) in results.into_iter().enumerate() {
        let Some(r) = r else { continue };
        let entry = files.get(&nodes[n].path).map(|e| e.0).unwrap_or(usize::MAX);
        parsed[entry] = true;
        if let Some(e) = r.errors.iter().find(|e| e.id == "xml_parse")
            && malformed.as_ref().is_none_or(|m| entry < m.0)
        {
            malformed = Some((entry, e.clone()));
        }
        errors.extend(r.errors);
        nodes[n].facts = r.facts;
        nodes[n].has_root = true;
        deferred.extend(r.deferred.into_iter().map(|d| (n, d)));
    }

    // Package structure (PackageValidator), reported before the parts.
    let mut structure = vec![];
    let containers = std::iter::once((Some(package), None, &top)).chain(nodes.iter().map(|n| (n.class, Some(n.path.as_str()), &n.children)));
    for (cls, path, kids) in containers {
        let Some(c) = cls else { continue }; // an extended part has no rules
        let part = path.map_or(String::new(), |p| format!("/{p}"));
        let mut pkg = |id: &'static str, what: String| structure.push(ValidationError { id, description: what, part: part.clone(), node: String::new(), error_type: "Package" });
        for (k, _, rel) in kids.iter() {
            if child_class(c, rel).is_none() && k.is_some_and(|k| PARTS[k].version <= TARGET_VERSION) {
                pkg("Pkg_PartIsNotAllowed", format!("{} cannot hold a part of type {rel}", PARTS[c].name));
            }
        }
        for r in rules(c) {
            let k = &PARTS[r.class as usize];
            if k.version > TARGET_VERSION {
                continue;
            }
            let occurs = kids.iter().filter(|(_, _, rel)| rel == k.rel).count();
            if r.required && occurs == 0 {
                pkg("Pkg_RequiredPartDoNotExist", format!("{} requires a part of type {}", PARTS[c].name, k.rel));
            }
            if !r.many && occurs > 1 {
                pkg("Pkg_OnlyOnePartAllowed", format!("{} can only have one part of type {}", PARTS[c].name, k.rel));
            }
        }
    }
    structure.append(&mut errors);
    let mut errors = structure;
    // Package-level constraints, now every part is known.
    for (n, d) in deferred {
        let c = &CONSTRAINTS[d.constraint as usize];
        let Arg::S(path) = c.args[1] else { continue };
        let target = referenced(&nodes, &top, n, path).filter(|&t| nodes[t].has_root);
        let (id, ok) = match c.kind {
            ConstraintKind::ReferenceExist => {
                let (Arg::Q(ens, el), Arg::Q(_, attr)) = (&c.args[2], &c.args[4]) else { continue };
                let ok = target.and_then(|t| nodes[t].facts.refs.get(&(*ens, *el, *attr))).is_some_and(|set| set.contains(&d.value));
                ("Sem_MissingReferenceElement", ok)
            }
            ConstraintKind::IndexReference => {
                let parent = match c.args[2] {
                    Arg::Q(ns, l) => Some((ns, l)),
                    _ => None,
                };
                let Arg::N(base) = c.args[5] else { continue };
                let count = target.and_then(|t| nodes[t].facts.counts.get(&parent).copied()).unwrap_or(0) as f64;
                let index: f64 = d.value.trim().parse().unwrap_or(0.0);
                ("Sem_MissingIndexedElement", index < count + base)
            }
            _ => continue,
        };
        if !ok {
            errors.push(ValidationError { id, description: format!("{:?} failed for '{}'.", c.kind, d.value), part: format!("/{}", nodes[n].path), node: d.node, error_type: "Semantic" });
        }
    }
    Outcome { errors, parsed, malformed }
}

/// The verdict on one .docx: a package that cannot be read is not OK.
pub struct Report {
    pub open_error: Option<String>,
    pub errors: Vec<ValidationError>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.open_error.is_none() && self.errors.is_empty()
    }
}

/// Validates a .docx as bytes: package limits; every XML entry well formed
/// (reachable or not; the first failure in zip order is the verdict, as the
/// SDK wrapper's prepass has it); every reachable part against the schema and
/// its semantic constraints. Each entry is inflated once.
pub fn check(bytes: &[u8]) -> Report {
    let fail = |e: String| Report { open_error: Some(e), errors: vec![] };
    if bytes.len() > 100 << 20 {
        return fail("DOCX exceeds the 100 MB validation limit.".into());
    }
    let mut zip = match zip::ZipArchive::new(std::io::Cursor::new(bytes)) {
        Ok(z) => z,
        Err(e) => return fail(e.to_string()),
    };
    if zip.len() > 10000 {
        return fail("DOCX exceeds the 10000-part validation limit.".into());
    }
    let mut names = std::collections::HashSet::new();
    let mut budget: u64 = 128 << 20;
    let mut entries = vec![];
    for i in 0..zip.len() {
        let Ok(f) = zip.by_index(i) else { return fail("unreadable entry".into()) };
        let name = f.name().to_string();
        if !names.insert(name.to_lowercase()) {
            return fail("DOCX contains ambiguous duplicate ZIP part names.".into());
        }
        let lower = name.to_lowercase();
        if !lower.ends_with(".xml") && !lower.ends_with(".rels") {
            continue;
        }
        if f.size() > budget {
            return fail("DOCX XML exceeds the 128 MB validation limit.".into());
        }
        budget -= f.size();
        // Exactly the declared size, read in one go; never more than the budget.
        let mut xml = Vec::with_capacity(f.size() as usize);
        if let Err(e) = f.take(budget + 1).read_to_end(&mut xml) {
            return fail(e.to_string());
        }
        entries.push((name, xml));
    }
    let files = Files::new(entries);
    let mut out = validate(&files);
    // Entries no part validation read are checked as XML alone.
    for (i, (name, xml)) in files.entries.iter().enumerate() {
        if out.parsed[i] || out.malformed.as_ref().is_some_and(|m| m.0 < i) {
            continue;
        }
        if let Err(message) = well_formed(xml) {
            out.malformed = Some((i, ValidationError { id: "xml_parse", description: message, part: format!("/{name}"), node: String::new(), error_type: "Xml" }));
            break;
        }
    }
    if let Some((_, e)) = out.malformed {
        return Report { open_error: Some(e.description.clone()), errors: vec![e] };
    }
    // Every error: a caller matching the SDK wrapper keeps the first 50.
    Report { open_error: None, errors: out.errors }
}

/// What XDocument.Load over an XmlReader accepts: well-formed XML (the
/// tokenizer's checks), every prefix declared, no attribute twice by
/// namespace and local name, and xml:space only "default" or "preserve".
fn well_formed(xml: &[u8]) -> Result<(), String> {
    let mut r = Reader::new(xml);
    let mut scopes: Vec<(&[u8], &[u8])> = vec![(b"xml", b"http://www.w3.org/XML/1998/namespace"), (b"xmlns", b"http://www.w3.org/2000/xmlns/")];
    let mut marks: Vec<usize> = vec![];
    loop {
        match r.next().map_err(|e| e.message)? {
            Event::Eof => return Ok(()),
            Event::Start { name, empty } => {
                marks.push(scopes.len());
                for a in &r.attrs {
                    if a.name == b"xmlns" {
                        scopes.push((b"", a.value));
                    } else if let Some(p) = a.name.strip_prefix(b"xmlns:") {
                        if a.value.is_empty() {
                            return Err(format!("Cannot use a prefix with an empty namespace ({}).", String::from_utf8_lossy(p)));
                        }
                        scopes.push((p, a.value));
                    }
                }
                let resolve = |q: &[u8]| -> Result<(Option<&[u8]>, Vec<u8>), String> {
                    match memchr::memchr(b':', q) {
                        None => Ok((None, q.to_vec())),
                        Some(i) => {
                            let p = &q[..i];
                            let uri = scopes.iter().rev().find(|s| s.0 == p).map(|s| s.1);
                            uri.map(|u| (Some(u), q[i + 1..].to_vec())).ok_or_else(|| format!("'{}' is an undeclared prefix.", String::from_utf8_lossy(p)))
                        }
                    }
                };
                resolve(name)?;
                let mut seen: Vec<(Option<&[u8]>, Vec<u8>)> = vec![];
                for a in &r.attrs {
                    if a.name == b"xmlns" {
                        continue;
                    }
                    let key = resolve(a.name)?;
                    if seen.contains(&key) {
                        return Err(format!("'{}' is a duplicate attribute name.", String::from_utf8_lossy(a.name)));
                    }
                    seen.push(key);
                    if a.name == b"xml:space" && a.value != b"default" && a.value != b"preserve" {
                        return Err(format!("'{}' is an invalid xml:space value.", String::from_utf8_lossy(a.value)));
                    }
                }
                if empty {
                    scopes.truncate(marks.pop().unwrap());
                }
            }
            Event::End { .. } => scopes.truncate(marks.pop().unwrap_or(2)),
            Event::Text { .. } => {}
        }
    }
}
