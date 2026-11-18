//! Opens a package and validates every part reachable from it, as the SDK's
//! DocumentValidator does: relationships from the package down, each part's
//! class decided by its relationship type among its parent's known children
//! (anything else is an extended part, which has no schema).

use crate::schema::{Arg, ConstraintKind, NONE, PartChild};
use crate::schema_gen::{CONSTRAINTS, FACTORIES, PART_CHILDREN, PARTS, TARGET_VERSION};
use crate::semantic::{Facts, Rels};
use crate::validate::{ValidationError, validate_part};
use crate::xml::{Event, Reader};
use std::collections::{HashMap, VecDeque};
use std::io::Read;

/// The kind of package, as the SDK class that opens it: its main part, its
/// tree of part classes, and the application semantic constraints filter on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// WordprocessingDocument: .docx, .docm, .dotx, .dotm.
    Word,
    /// SpreadsheetDocument: .xlsx, .xlsm, .xltx, .xltm, .xlam.
    Excel,
    /// PresentationDocument: .pptx, .pptm, .potx, .potm, .ppsx, .ppsm, .ppam.
    PowerPoint,
}

impl Format {
    /// The SDK's package class.
    pub fn package(self) -> &'static str {
        match self {
            Format::Word => "WordprocessingDocument",
            Format::Excel => "SpreadsheetDocument",
            Format::PowerPoint => "PresentationDocument",
        }
    }

    /// Its ApplicationType bit, as semantic constraints are filtered.
    pub fn app(self) -> u8 {
        match self {
            Format::Word => 1,
            Format::Excel => 2,
            Format::PowerPoint => 4,
        }
    }

    /// The format a file name's extension names, if any.
    pub fn from_extension(path: &str) -> Option<Format> {
        let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
        match ext.as_str() {
            "docx" | "docm" | "dotx" | "dotm" => Some(Format::Word),
            "xlsx" | "xlsm" | "xltx" | "xltm" | "xlam" => Some(Format::Excel),
            "pptx" | "pptm" | "potx" | "potm" | "ppsx" | "ppsm" | "ppam" => Some(Format::PowerPoint),
            _ => None,
        }
    }

    /// The format a main part's content type names (each package class's
    /// GetDocumentType); None for one no class knows.
    pub fn from_content_type(ct: &str) -> Option<Format> {
        let ct = ct.trim().to_ascii_lowercase();
        if ct.contains("wordprocessingml") || ct.starts_with("application/vnd.ms-word.") {
            Some(Format::Word)
        } else if ct.contains("spreadsheetml") || ct.starts_with("application/vnd.ms-excel.") {
            Some(Format::Excel)
        } else if ct.contains("presentationml") || ct.starts_with("application/vnd.ms-powerpoint.") {
            Some(Format::PowerPoint)
        } else {
            None
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn noun(self) -> &'static str {
        match self {
            Format::Word => "DOCX",
            Format::Excel => "XLSX",
            Format::PowerPoint => "PPTX",
        }
    }
}

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

/// The package's part factory: the class a relationship type makes anywhere
/// in this kind of package (then reported as not allowed where its parent
/// doesn't hold it); None makes an extended part.
fn global_class(format: Format, rel: &str) -> Option<usize> {
    FACTORIES[format.index()].iter().map(|&c| c as usize).find(|&c| PARTS[c].rel == rel)
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

/// A part name with its escapes undone, as URIs compare: an escaped
/// reserved character or `%` stays escaped (`%21` is not `!`).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
                && !b":/?#[]@!$&'()*+,;=%".contains(&v)
            {
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

/// Each relationship in a .rels part: (id, type, target, external). What
/// System.IO.Packaging refuses to load is an error: another root, a
/// relationship without Id, Type or Target, a repeated Id, a bad TargetMode.
fn relationships(xml: &[u8]) -> Result<Vec<(String, String, String, bool)>, String> {
    let mut out: Vec<(String, String, String, bool)> = vec![];
    const NS: &[u8] = b"http://schemas.openxmlformats.org/package/2006/relationships";
    let mut r = Reader::new(xml);
    let mut root = true;
    // In-scope namespace declarations (prefix, URI) and each open element's mark:
    // the elements must be in the relationships namespace, not just named so.
    let mut scopes: Vec<(Vec<u8>, Vec<u8>)> = vec![];
    let mut marks: Vec<usize> = vec![];
    loop {
        match r.next().map_err(|e| e.message)? {
            Event::Eof => return Ok(out),
            Event::End { .. } => scopes.truncate(marks.pop().unwrap_or(0)),
            Event::Start { name, empty } => {
                marks.push(scopes.len());
                for a in &r.attrs {
                    if a.name == b"xmlns" {
                        scopes.push((vec![], a.value.to_vec()));
                    } else if let Some(p) = a.name.strip_prefix(b"xmlns:") {
                        scopes.push((p.to_vec(), a.value.to_vec()));
                    }
                }
                let (prefix, local) = match memchr::memchr(b':', name) {
                    Some(i) => (&name[..i], &name[i + 1..]),
                    None => (&b""[..], name),
                };
                let ns = scopes.iter().rev().find(|s| s.0 == prefix).map_or(&b""[..], |s| &s.1[..]);
                let ours = ns == NS;
                if empty {
                    scopes.truncate(marks.pop().unwrap_or(0));
                }
                if root {
                    root = false;
                    if local != b"Relationships" || !ours {
                        return Err("Relationships tag expected at root level.".into());
                    }
                    continue;
                }
                if local != b"Relationship" || !ours {
                    return Err("Unrecognized tag found in Relationships XML.".into());
                }
                let get = |k: &[u8]| r.attrs.iter().find(|a| a.name == k).map(|a| String::from_utf8_lossy(a.value).into_owned());
                let (Some(id), Some(t), Some(target)) = (get(b"Id"), get(b"Type"), get(b"Target")) else {
                    return Err("A Relationship lacks its Id, Type or Target.".into());
                };
                let external = match get(b"TargetMode").as_deref() {
                    None | Some("Internal") => false,
                    Some("External") => true,
                    Some(m) => return Err(format!("'{m}' is not a valid TargetMode.")),
                };
                if !crate::validate::ncname(&id) {
                    return Err(format!("'{id}' ID is not a valid XSD ID."));
                }
                if out.iter().any(|o| o.0 == id) {
                    return Err(format!("'{id}' ID conflicts with the ID of an existing relationship for the specified source."));
                }
                // A Strict relationship reads as Transitional (and makes the package Strict).
                let t = crate::strict::relationship(&t).map_or(t, str::to_string);
                out.push((id, t, target, external));
            }
            _ => {}
        }
    }
}

/// A part name as packages compare them: percent-decoded, without case.
fn key(name: &str) -> String {
    percent_decode(name.trim_start_matches('/')).to_lowercase()
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
    /// Every entry's name by its lowercase form (part names compare without case).
    names: HashMap<String, String>,
    /// [Content_Types].xml: Default by extension (lowercased), Override by part
    /// name (as written: the SDK matches it with case, though parts without).
    defaults: HashMap<String, String>,
    overrides: HashMap<String, String>,
    /// A Default or Override declared twice: the package can't be opened.
    duplicate: Option<String>,
}

impl Files {
    pub fn new(entries: Vec<(String, Vec<u8>)>, names: HashMap<String, String>) -> Files {
        let index: HashMap<String, usize> = entries.iter().enumerate().map(|(i, e)| (key(&e.0), i)).collect();
        let (mut defaults, mut overrides) = (HashMap::new(), HashMap::new());
        let mut duplicate = None;
        if let Some(&i) = index.get("[content_types].xml") {
            let mut r = Reader::new(&entries[i].1);
            while let Ok(e) = r.next() {
                let Event::Start { name, .. } = e else {
                    if matches!(e, Event::Eof) { break } else { continue }
                };
                let get = |k: &[u8]| r.attrs.iter().find(|a| a.name == k).map(|a| String::from_utf8_lossy(a.value).into_owned());
                if name == b"Override"
                    && let Some(x) = get(b"PartName")
                    && !part_name(x.trim_start_matches('/'))
                {
                    duplicate.get_or_insert_with(|| format!("[Content_Types].xml names an invalid part, {x}."));
                }
                let replaced = match (name, get(b"ContentType")) {
                    (b"Default", Some(ct)) => get(b"Extension").and_then(|x| defaults.insert(x.to_lowercase(), ct)),
                    (b"Override", Some(ct)) => get(b"PartName").and_then(|x| overrides.insert(percent_decode(x.trim_start_matches('/')), ct)),
                    _ => None,
                };
                if replaced.is_some() {
                    duplicate.get_or_insert_with(|| format!("[Content_Types].xml declares a {} twice.", String::from_utf8_lossy(name)));
                }
            }
        }
        Files { entries, index, names, defaults, overrides, duplicate }
    }

    /// A part: an entry named as a part URI that the content types give a
    /// type (System.IO.Packaging ignores any other entry).
    fn exists(&self, path: &str) -> bool {
        self.names.get(&key(path)).is_some_and(|n| part_name(n)) && self.content_type(path).is_some()
    }

    /// A part's content type, as the package declares it.
    fn content_type(&self, path: &str) -> Option<&str> {
        let actual = self.names.get(&key(path)).map_or(path.to_string(), |n| percent_decode(n));
        let path = actual.as_str();
        self.overrides.get(path).or_else(|| self.defaults.get(&path.rsplit_once('.')?.1.to_lowercase())).map(String::as_str)
    }

    /// The format the main part names (the package's officeDocument
    /// relationship, as each package class finds its main part): by its
    /// content type, or else by its root element; Word when neither does.
    pub fn format(&self) -> Format {
        const MAIN: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
        let rels = self.get("_rels/.rels").and_then(|(_, x)| relationships(x).ok()).unwrap_or_default();
        let Some(main) = rels.iter().find(|r| r.1 == MAIN && !r.3).map(|r| resolve("", &r.2)) else { return Format::Word };
        if let Some(f) = self.content_type(&main).and_then(Format::from_content_type) {
            return f;
        }
        let mut r = Reader::new(self.get(&main).map_or(&[][..], |e| e.1));
        while let Ok(e) = r.next() {
            match e {
                Event::Start { name, .. } => {
                    return match name.rsplit(|&c| c == b':').next() {
                        Some(b"workbook") => Format::Excel,
                        Some(b"presentation") => Format::PowerPoint,
                        _ => Format::Word,
                    };
                }
                Event::Eof => break,
                _ => {}
            }
        }
        Format::Word
    }

    fn get(&self, name: &str) -> Option<(usize, &[u8])> {
        self.index.get(&key(name)).map(|&i| (i, &self.entries[i].1[..]))
    }
}

/// What validating a package found.
pub struct Outcome {
    pub errors: Vec<ValidationError>,
    /// Entries validated as parts (their well-formedness is checked there).
    pub parsed: Vec<bool>,
    /// The first part, in zip order, that is not well-formed XML.
    pub malformed: Option<(usize, ValidationError)>,
    /// The package can't be loaded as the SDK loads it.
    pub open_error: Option<String>,
}

/// Validates every reachable part of a package: discovers the part tree from
/// the relationship parts, validates the parts in parallel, then checks the
/// package's structure and the constraints that span parts.
pub fn validate(files: &Files, format: Format) -> Outcome {
    let package = class(format.package()).expect("package class");
    let mut nodes: Vec<Node> = vec![];
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut top: Vec<(Option<usize>, usize, String)> = vec![];
    let mut rels_of: Vec<Rels> = vec![];
    let mut rels_error: Option<String> = None;
    // A relationship part only counts when it is a part, and then it must be
    // typed as one (InternalRelationshipCollection).
    let rels_part = |path: &str| -> Result<Option<&[u8]>, String> {
        if !files.exists(path) {
            return Ok(None);
        }
        let ct = files.content_type(path).unwrap_or("");
        let essence = ct.split(';').next().unwrap_or("").trim();
        if !essence.eq_ignore_ascii_case("application/vnd.openxmlformats-package.relationships+xml") {
            return Err(format!("/{path}: The relationship part has content type {ct:?}."));
        }
        Ok(files.get(path).map(|e| e.1))
    };
    let mut read_rels = |path: &str| match rels_part(path).map(|x| x.map(relationships)) {
        Err(e) => {
            rels_error.get_or_insert(e);
            vec![]
        }
        Ok(x) => match x {
            Some(Ok(r)) => r,
            Some(Err(e)) => {
                rels_error.get_or_insert(format!("/{path}: {e}"));
                vec![]
            }
            None => vec![],
        },
    };
    // StrictRelationshipFound: some relationship the package loads is Strict.
    let mut strict = false;
    let mut note = |rels: &[(String, String, String, bool)], raw: Option<&[u8]>| {
        strict |= raw.is_some_and(|x| memchr::memmem::find(x, b"http://purl.oclc.org/ooxml/officeDocument/relationships/").is_some()) && !rels.is_empty();
    };
    // (path, class, parent node, relationship type)
    let mut queue: VecDeque<(String, Option<usize>, Option<usize>, String)> = VecDeque::new();
    // What the SDK throws while loading the package (any one aborts validation).
    let mut open_error: Option<String> = files.duplicate.clone();
    let mut first_rel: Vec<String> = vec![];
    // Data parts (media, audio, video) each container references: (node, type).
    let mut data_refs: Vec<(Option<usize>, String)> = vec![];
    let root_rels = read_rels("_rels/.rels");
    note(&root_rels, files.get("_rels/.rels").map(|e| e.1));
    for (_, t, target, external) in root_rels {
        if !external && loads_part(files, &t, &resolve("", &target), &target, &mut open_error, &mut |t| data_refs.push((None, t))) {
            queue.push_back((resolve("", &target), child_class(package, &t).or_else(|| global_class(format, &t)), None, t));
        }
    }
    while let Some((path, cls, parent, rel)) = queue.pop_front() {
        let reached = index.get(&path).copied();
        let n = match reached {
            Some(n) => {
                // A shared part must be reached by one relationship type.
                if first_rel[n] != rel {
                    open_error.get_or_insert_with(|| format!("The same part /{path} is referenced by different relationship types."));
                }
                n
            }
            None => {
                if !files.exists(&path) {
                    open_error.get_or_insert_with(|| format!("Part: /{path} doesn't exist in the package."));
                }
                if let Some(c) = cls
                    && !PARTS[c].content_type.is_empty()
                    && files.content_type(&path) != Some(PARTS[c].content_type)
                    // Model3DReferenceRelationshipPart.IsValidContentType: PowerPoint writes this one too.
                    && !(PARTS[c].name == "Model3DReferenceRelationshipPart" && files.content_type(&path) == Some("model/gltf.binary"))
                {
                    open_error.get_or_insert_with(|| format!("The part /{path} has content type {:?}; expected {}.", files.content_type(&path).unwrap_or(""), PARTS[c].content_type));
                }
                first_rel.push(rel.clone());
                let n = nodes.len();
                // Named as the package names it, whatever case the relationship used.
                let name = files.get(&path).map_or_else(|| path.clone(), |(i, _)| files.entries[i].0.clone());
                nodes.push(Node { path: name, class: cls, parent, children: vec![], facts: Facts::default(), has_root: false });
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
        if reached.is_some() {
            continue; // its relationships are read once
        }
        if std::env::var_os("OOXML_VALIDATOR_TRACE").is_some() {
            eprintln!("part {path} {:?}", cls.map(|c| PARTS[c].name));
        }
        if cls.is_some_and(|c| PARTS[c].version > TARGET_VERSION) {
            continue;
        }
        let rels = read_rels(&rels_path(&path));
        note(&rels, files.get(&rels_path(&path)).map(|e| e.1));
        rels_of[n] = Rels { types: rels.iter().map(|(id, t, _, _)| (id.clone(), t.clone())).collect() };
        for (_, t, target, external) in rels {
            if !external && loads_part(files, &t, &resolve(&path, &target), &target, &mut open_error, &mut |t| data_refs.push((Some(n), t))) {
                let child = cls.and_then(|c| child_class(c, &t)).or_else(|| global_class(format, &t));
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
    // OOXML_VALIDATOR_ONE_THREAD: one thread, for profiles comparable with the WebAssembly build.
    let threads = if std::env::var_os("OOXML_VALIDATOR_ONE_THREAD").is_some() { 1 } else { std::thread::available_parallelism().map_or(1, |n| n.get()).min(jobs.len()).min(4).max(1) };
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut results: Vec<Option<crate::validate::PartResult>> = (0..nodes.len()).map(|_| None).collect();
    std::thread::scope(|scope| {
        let work = || {
            let mut out = vec![];
            loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&(n, _, xml, root)) = jobs.get(k) else { break };
                out.push((n, validate_part(xml, &format!("/{}", nodes[n].path), root, &rels_of[n], strict, format)));
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
    // A data part reference its container's class doesn't know (an extended
    // part, or the package itself, knows none).
    for (n, rel) in &data_refs {
        let known = n.and_then(|n| nodes[n].class).map_or(0, |c| PARTS[c].data);
        let bit = match rel.as_str() {
            AUDIO => 1,
            VIDEO => 2,
            _ => 4,
        };
        if known & bit == 0 {
            let part = n.map_or(String::new(), |n| format!("/{}", nodes[n].path));
            structure.push(ValidationError { id: "Pkg_DataPartReferenceIsNotAllowed", description: format!("A part here cannot reference a data part of type {rel}"), part, node: String::new(), location: None, error_type: "Package", xpath: None, at: None });
        }
    }
    let containers = std::iter::once((Some(package), None, &top)).chain(nodes.iter().map(|n| (n.class, Some(n.path.as_str()), &n.children)));
    for (cls, path, kids) in containers {
        let Some(c) = cls else { continue }; // an extended part has no rules
        let part = path.map_or(String::new(), |p| format!("/{p}"));
        let mut pkg = |id: &'static str, what: String| structure.push(ValidationError { id, description: what, part: part.clone(), node: String::new(), location: None, error_type: "Package", xpath: None, at: None });
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
    let mut semantic_errors = vec![0usize; nodes.len()];
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
            // One error per element, as the SDK reports them; a part keeps its first thousand.
            let room = crate::validate::MAX_PART_ERRORS.saturating_sub(semantic_errors[n]);
            let k = (d.count as usize).min(room);
            semantic_errors[n] += k;
            for i in 0..k {
                let at = d.ats.get(i).copied().flatten();
                errors.push(ValidationError { id, description: format!("{:?} failed for '{}'.", c.kind, d.value), part: format!("/{}", nodes[n].path), node: d.node.clone(), error_type: "Semantic", location: None, xpath: None, at });
            }
        }
    }
    // XPaths, from one more pass over each part that has errors.
    let mut wanted: HashMap<String, Vec<usize>> = HashMap::new();
    for e in &errors {
        if let Some(at) = e.at {
            wanted.entry(e.part.clone()).or_default().push(at);
        }
    }
    let paths: HashMap<String, HashMap<usize, String>> = wanted
        .into_iter()
        .filter_map(|(part, at)| {
            let xml = files.get(part.strip_prefix('/')?)?.1;
            Some((part, crate::xpath::xpaths(xml, &at, strict)))
        })
        .collect();
    for e in &mut errors {
        e.xpath = e.at.and_then(|at| paths.get(&e.part)?.get(&at).cloned());
    }
    Outcome { errors, parsed, malformed, open_error: open_error.or(rels_error) }
}

/// Whether an internal relationship loads a part (LoadReferencedPartsAndRelationships):
/// hyperlinks and "NULL" targets never do; media, audio and video references load
/// a data part, which must exist.
fn loads_part(files: &Files, rel: &str, path: &str, target: &str, open_error: &mut Option<String>, data: &mut dyn FnMut(String)) -> bool {
    const HYPERLINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
    if rel == HYPERLINK || target.eq_ignore_ascii_case("NULL") {
        return false;
    }
    if [MEDIA, AUDIO, VIDEO].contains(&rel) {
        if !files.exists(path) {
            open_error.get_or_insert_with(|| format!("Part: /{path} doesn't exist in the package."));
        }
        data(rel.to_string());
        return false;
    }
    true
}

const MEDIA: &str = "http://schemas.microsoft.com/office/2007/relationships/media";
const AUDIO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/audio";
const VIDEO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/video";

/// The verdict on one .docx: a package that cannot be read is not OK.
pub struct Report {
    pub open_error: Option<String>,
    pub errors: Vec<ValidationError>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.open_error.is_none() && self.errors.is_empty()
    }

    /// {"ok","openError","errors":[{"id","description","part","node","errorType","line"?,"column"?,"xpath"?}]},
    /// with at most `max` errors (0: all).
    pub fn to_json(&self, max: usize) -> String {
        let n = if max == 0 { self.errors.len() } else { self.errors.len().min(max) };
        let errors: Vec<String> = self.errors[..n]
            .iter()
            .map(|e| {
                let at = e.location.map_or(String::new(), |(l, c)| format!(",\"line\":{l},\"column\":{c}"));
                let xpath = e.xpath.as_deref().map_or(String::new(), |x| format!(",\"xpath\":{}", json(x)));
                format!("{{\"id\":{},\"description\":{},\"part\":{},\"node\":{},\"errorType\":{}{at}{xpath}}}", json(e.id), json(&e.description), json(&e.part), json(&e.node), json(e.error_type))
            })
            .collect();
        let open = self.open_error.as_deref().map_or("null".to_string(), json);
        format!("{{\"ok\":{},\"openError\":{open},\"errors\":[{}]}}", self.ok(), errors.join(","))
    }
}

/// A JSON string literal.
pub fn json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Validates a package as bytes, opened as the class its main part's content
/// type names (a Word document when none does). See `check_as`.
pub fn check(bytes: &[u8]) -> Report {
    check_as(bytes, None)
}

/// Validates a package as bytes, as `format` (or the format its main part's
/// content type names): package limits; every XML entry well formed
/// (reachable or not; the first failure in zip order is the verdict, as the
/// SDK wrapper's prepass has it); every reachable part against the schema and
/// its semantic constraints. Each entry is inflated once.
pub fn check_as(bytes: &[u8], format: Option<Format>) -> Report {
    let fail = |e: String| Report { open_error: Some(e), errors: vec![] };
    let noun = format.map_or("The package", Format::noun);
    if bytes.len() > 100 << 20 {
        return fail(format!("{noun} exceeds the 100 MB validation limit."));
    }
    let mut zip = match zip::ZipArchive::new(std::io::Cursor::new(bytes)) {
        Ok(z) => z,
        Err(e) => return fail(e.to_string()),
    };
    // The zip reader keeps one entry per name; the central directory may list
    // a name twice, which System.IO.Packaging refuses.
    if eocd_entries(bytes).is_some_and(|n| n > zip.len()) {
        return fail(format!("{noun} contains ambiguous duplicate ZIP part names."));
    }
    if zip.len() > 10000 {
        return fail(format!("{noun} exceeds the 10000-part validation limit."));
    }
    let mut names = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    let mut budget: u64 = 128 << 20;
    let mut entries = vec![];
    // Entries whose bytes don't decode as their encoding says: (index, message).
    let mut undecodable: Option<(usize, String)> = None;
    // An XML entry that can't be read (a bad header, corrupt deflate data): the
    // SDK wrapper's prepass stops there, unless an earlier entry isn't XML.
    let mut corrupt: Option<String> = None;
    for i in 0..zip.len() {
        let entry_name = zip.name_for_index(i).map(str::to_string);
        let f = match zip.by_index(i) {
            Ok(f) => f,
            Err(e) => {
                let Some(name) = entry_name else { return fail(e.to_string()) };
                names.insert(key(&name), name.clone());
                let lower = name.to_lowercase();
                if lower.ends_with(".xml") || lower.ends_with(".rels") {
                    corrupt = Some(format!("'{name}' cannot be read: {e}"));
                    break;
                }
                continue;
            }
        };
        let name = f.name().to_string();
        // Folders are not parts: "ppt/" and "PPT/" don't clash.
        if !name.ends_with('/') && !seen.insert(name.to_lowercase()) {
            return fail(format!("{noun} contains ambiguous duplicate ZIP part names."));
        }
        names.insert(key(&name), name.clone());
        let lower = name.to_lowercase();
        if !lower.ends_with(".xml") && !lower.ends_with(".rels") {
            continue;
        }
        if f.size() > budget {
            return fail(format!("{noun} XML exceeds the 128 MB validation limit."));
        }
        let size = f.size();
        budget -= size;
        // At most the declared size, read in one go, as .NET's ZipArchive reads
        // an entry: it stops there, doesn't check the CRC-32, and ends a deflate
        // stream that runs out of input. What it read is then parsed as XML.
        let mut xml = Vec::with_capacity(size as usize);
        let method = f.compression();
        drop(f);
        // Read without the zip crate's CRC-32, which .NET doesn't check either:
        // the same zlib-rs inflate over the raw bytes, a few percent less work.
        let read = match method {
            zip::CompressionMethod::Deflated => zip.by_index_raw(i).map_err(std::io::Error::other).and_then(|raw| flate2::bufread::DeflateDecoder::new(std::io::BufReader::new(raw)).take(size).read_to_end(&mut xml)),
            zip::CompressionMethod::Stored => zip.by_index_raw(i).map_err(std::io::Error::other).and_then(|raw| raw.take(size).read_to_end(&mut xml)),
            _ => zip.by_index(i).map_err(std::io::Error::other).and_then(|f| f.take(size).read_to_end(&mut xml)),
        };
        if let Err(e) = read {
            let m = e.to_string();
            if !m.contains("checksum") && !m.contains("incomplete deflate stream") {
                corrupt = Some(format!("'{name}' cannot be read: {m}"));
                break;
            }
        }
        // Parts in UTF-16 or Latin-1 are read as text, as XmlReader reads them.
        match crate::encoding::to_utf8(&xml) {
            Ok(std::borrow::Cow::Owned(utf8)) => xml = utf8,
            Ok(_) => {}
            Err(e) => drop(undecodable.get_or_insert((entries.len(), e))),
        }
        entries.push((name, xml));
    }
    let files = Files::new(entries, names);
    let format = format.unwrap_or_else(|| files.format());
    let mut out = validate(&files, format);
    // Entries no part validation read are checked as XML alone.
    for (i, (name, xml)) in files.entries.iter().enumerate() {
        if out.parsed[i] || out.malformed.as_ref().is_some_and(|m| m.0 < i) {
            continue;
        }
        if let Err(e) = well_formed(xml) {
            let location = Some(crate::validate::line_column(xml, e.at));
            out.malformed = Some((i, ValidationError { id: "xml_parse", description: e.message, part: format!("/{name}"), node: String::new(), location, error_type: "Xml", xpath: None, at: None }));
            break;
        }
    }
    if let Some((i, message)) = undecodable
        && out.malformed.as_ref().is_none_or(|m| i < m.0)
    {
        let part = format!("/{}", files.entries[i].0);
        out.malformed = Some((i, ValidationError { id: "xml_parse", description: message, part, node: String::new(), location: None, error_type: "Xml", xpath: None, at: None }));
    }
    if let Some((_, e)) = out.malformed {
        return Report { open_error: Some(e.description.clone()), errors: vec![e] };
    }
    if let Some(e) = corrupt {
        return fail(e);
    }
    if let Some(e) = out.open_error {
        return Report { open_error: Some(e), errors: vec![] };
    }
    if let Some(e) = out.errors.iter().find(|e| e.error_type == "Open") {
        return Report { open_error: Some(e.description.clone()), errors: vec![] };
    }
    // Every error: a caller matching the SDK wrapper keeps the first 50.
    Report { open_error: None, errors: out.errors }
}

/// Whether a zip entry name (or an Override's PartName) is a part URI as
/// PackUriHelper validates one: URI characters only (no space, nothing past
/// ASCII, none of `#?\^`|{}"<>`), and each `%` escaping a byte that needs it.
fn part_name(name: &str) -> bool {
    let b = name.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'%' {
            let hex = name.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok());
            match hex {
                Some(v) if !(v.is_ascii_alphanumeric() || matches!(v, b'-' | b'.' | b'_' | b'~')) => i += 3,
                _ => return false,
            }
            continue;
        }
        if !(c.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/[]".contains(&c)) {
            return false;
        }
        i += 1;
    }
    true
}

/// The entry count the end of central directory record states (None for
/// ZIP64 or no record).
fn eocd_entries(bytes: &[u8]) -> Option<usize> {
    let tail = &bytes[bytes.len().saturating_sub(65557)..];
    let at = memchr::memmem::rfind(tail, b"PK\x05\x06")?;
    let n = u16::from_le_bytes(tail.get(at + 10..at + 12)?.try_into().ok()?);
    (n != u16::MAX).then_some(n as usize)
}

/// What XDocument.Load over an XmlReader accepts: well-formed XML (the
/// tokenizer's checks), every prefix declared, no attribute twice by
/// namespace and local name, and xml:space only "default" or "preserve".
fn well_formed(xml: &[u8]) -> Result<(), crate::xml::XmlError> {
    let mut r = Reader::new(xml);
    let mut scopes: Vec<(&[u8], &[u8])> = vec![(b"xml", b"http://www.w3.org/XML/1998/namespace"), (b"xmlns", b"http://www.w3.org/2000/xmlns/")];
    let mut marks: Vec<usize> = vec![];
    loop {
        match r.next()? {
            Event::Eof => return Ok(()),
            Event::Start { name, empty } => {
                marks.push(scopes.len());
                for a in &r.attrs {
                    if a.name == b"xmlns" {
                        scopes.push((b"", a.value));
                    } else if let Some(p) = a.name.strip_prefix(b"xmlns:") {
                        if a.value.is_empty() {
                            return Err(crate::xml::XmlError { at: r.offset(), message: format!("Cannot use a prefix with an empty namespace ({}).", String::from_utf8_lossy(p)) });
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
                resolve(name).map_err(|message| crate::xml::XmlError { at: r.offset(), message })?;
                let mut seen: Vec<(Option<&[u8]>, Vec<u8>)> = vec![];
                for a in &r.attrs {
                    if a.name == b"xmlns" {
                        continue;
                    }
                    let key = resolve(a.name).map_err(|message| crate::xml::XmlError { at: r.offset(), message })?;
                    if seen.contains(&key) {
                        return Err(crate::xml::XmlError { at: r.offset(), message: format!("'{}' is a duplicate attribute name.", String::from_utf8_lossy(a.name)) });
                    }
                    seen.push(key);
                    if a.name == b"xml:space" && a.value != b"default" && a.value != b"preserve" {
                        return Err(crate::xml::XmlError { at: r.offset(), message: format!("'{}' is an invalid xml:space value.", String::from_utf8_lossy(a.value)) });
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
