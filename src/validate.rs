//! Validates one XML part in a single pass, as the SDK's DocumentValidator
//! validates a loaded part (SchemaTypeValidator over ValidatingTraverse), but
//! with no tree: a frame per open element, and each composite element's
//! markup-compatibility-flattened child list, matched when it closes.

use crate::particle::{self, Kid, Verdict};
use crate::schema::*;
use crate::schema_gen::*;
use crate::semantic::{self, Deferred, Facts, Outcome, Rels, Seen};
use crate::values;
use crate::xml::{Event, Reader, unescape};
use regex::Regex;
use std::collections::HashMap;
use std::sync::OnceLock;

const MC: &[u8] = b"http://schemas.openxmlformats.org/markup-compatibility/2006";
const XML_NS: &[u8] = b"http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &[u8] = b"http://www.w3.org/2000/xmlns/";

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub id: &'static str,
    pub description: String,
    pub part: String,
    /// The element's qualified name.
    pub node: String,
    pub error_type: &'static str,
}

fn ns_ids() -> &'static HashMap<&'static [u8], u32> {
    static IDS: OnceLock<HashMap<&'static [u8], u32>> = OnceLock::new();
    IDS.get_or_init(|| NAMESPACES.iter().enumerate().map(|(i, u)| (u.as_bytes(), i as u32)).collect())
}

fn patterns() -> &'static [Regex] {
    static RES: OnceLock<Vec<Regex>> = OnceLock::new();
    RES.get_or_init(|| PATTERNS.iter().map(|p| Regex::new(&format!("^(?:{p})$")).expect("pattern")).collect())
}

/// What a frame on the element stack is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A strong-typed element, validated.
    Typed(u16),
    /// Children belong to the nearest typed ancestor: an mc:Choice or
    /// mc:Fallback that was selected, or an ignorable element whose content
    /// is processed.
    Transparent,
    /// mc:AlternateContent; `chosen` once a branch is selected; `state` is
    /// AlternateContentValidator's (0 start, 1 after a Choice, 2 after the
    /// Fallback); `count` its element children.
    Alternate { chosen: bool, state: u8, count: u16 },
    /// An ignorable element whose content is processed: its children join
    /// the nearest typed ancestor's, as unknown elements.
    Opaque,
    /// Not validated, nor anything inside it.
    Skip,
}

struct Frame {
    kind: Kind,
    ns: u32,
    /// Namespace bindings and MC entries this element pushed.
    bindings: usize,
    ignorable: usize,
    process: usize,
    kids_start: usize,
    text_start: usize,
    /// Leaf element already reported for having a child.
    leaf_child: bool,
    name_start: usize,
}

pub struct Part<'a> {
    reader: Reader<'a>,
    part: &'a str,
    bindings: Vec<(&'a [u8], &'a [u8])>,
    ignorable: Vec<u32>,
    /// ProcessContent entries: (namespace, local name or "*").
    process: Vec<(u32, &'a [u8])>,
    frames: Vec<Frame>,
    kids: Vec<Kid>,
    text: String,
    names: String,
    /// Unknown namespaces get ids past the table.
    dynamic: HashMap<&'a [u8], u32>,
    pub errors: Vec<ValidationError>,
    rels: &'a Rels,
    /// UniqueAttributeValue: constraint → (value → count, an element name).
    unique: HashMap<u32, (HashMap<String, u32>, String)>,
    pub facts: Facts,
    pub deferred: Vec<Deferred>,
    /// Per open AlternateContent: the errors its own MC attributes raised,
    /// which the SDK raises again for each Choice and Fallback.
    ac_mc: Vec<Vec<ValidationError>>,
}

/// One part's outcome: its errors, and what the package needs to finish.
pub struct PartResult {
    pub errors: Vec<ValidationError>,
    pub facts: Facts,
    pub deferred: Vec<Deferred>,
}

impl<'a> Part<'a> {
    pub fn new(xml: &'a [u8], part: &'a str, rels: &'a Rels) -> Self {
        Part {
            rels,
            unique: HashMap::new(),
            ac_mc: vec![],
            facts: Facts::default(),
            deferred: vec![],
            reader: Reader::new(xml),
            part,
            bindings: vec![(b"xml", XML_NS)],
            ignorable: vec![],
            process: vec![],
            frames: Vec::with_capacity(64),
            kids: Vec::with_capacity(1024),
            text: String::new(),
            names: String::new(),
            dynamic: HashMap::new(),
            errors: vec![],
        }
    }

    fn lookup(&self, prefix: &[u8]) -> Option<&'a [u8]> {
        self.bindings.iter().rev().find(|b| b.0 == prefix).map(|b| b.1)
    }

    fn ns_id(&mut self, uri: &'a [u8]) -> u32 {
        if let Some(&i) = ns_ids().get(uri) {
            return i;
        }
        let n = (NAMESPACES.len() + self.dynamic.len()) as u32;
        *self.dynamic.entry(uri).or_insert(n)
    }

    fn error(&mut self, id: &'static str, node: &str, description: String) {
        self.error_typed(id, node, description, "Schema");
    }

    fn error_typed(&mut self, id: &'static str, node: &str, description: String, error_type: &'static str) {
        self.errors.push(ValidationError { id, description, part: self.part.to_string(), node: node.to_string(), error_type });
    }

    /// The nearest typed frame, through transparent ones.
    fn owner(&self) -> Option<(usize, u16)> {
        for (i, f) in self.frames.iter().enumerate().rev() {
            match f.kind {
                Kind::Typed(t) => return Some((i, t)),
                Kind::Transparent | Kind::Alternate { .. } | Kind::Opaque => continue,
                Kind::Skip => return None,
            }
        }
        None
    }

    fn ignorable(&self, ns: u32) -> bool {
        ns != 0 && self.ignorable.contains(&ns)
    }

    fn process_content(&self, ns: u32, local: &[u8]) -> bool {
        self.process.iter().any(|&(n, l)| n == ns && (l == local || l == b"*"))
    }

    /// Validates the part, whose root must be `root` (a type id).
    pub fn run(mut self, root: u16) -> Result<PartResult, crate::xml::XmlError> {
        loop {
            match self.reader.next()? {
                Event::Eof => {
                    // One error per value seen more than once (DuplicateFinder).
                    let mut unique: Vec<_> = self.unique.drain().collect();
                    unique.sort_by_key(|u| u.0);
                    for (_, (counts, node)) in unique {
                        for (v, n) in counts {
                            if n > 1 && !v.is_empty() {
                                self.error_typed("Sem_UniqueAttributeValue", &node, format!("Attribute has duplicate value '{v}'."), "Semantic");
                            }
                        }
                    }
                    return Ok(PartResult { errors: self.errors, facts: self.facts, deferred: self.deferred });
                }
                Event::Text { raw, cdata } => {
                    if let Some(f) = self.frames.last()
                        && let Kind::Typed(t) = f.kind
                        && TYPES[t as usize].leaf == 2
                    {
                        if cdata {
                            self.text.push_str(std::str::from_utf8(raw).unwrap_or(""));
                        } else {
                            self.text.push_str(&unescape(raw, false));
                        }
                    }
                }
                Event::Start { name, empty } => {
                    self.start(name, root);
                    if empty {
                        self.end();
                    }
                }
                Event::End { .. } => self.end(),
            }
        }
    }

    fn start(&mut self, name: &'a [u8], root: u16) {
        // Namespace declarations first: they scope this element's own name.
        let mark = self.bindings.len();
        for a in &self.reader.attrs {
            if a.name == b"xmlns" {
                self.bindings.push((b"", a.value));
            } else if let Some(p) = a.name.strip_prefix(b"xmlns:") {
                self.bindings.push((p, a.value));
            }
        }
        let (prefix, local) = match memchr::memchr(b':', name) {
            Some(i) => (&name[..i], &name[i + 1..]),
            None => (&b""[..], name),
        };
        let uri = self.lookup(prefix).unwrap_or(b"");
        let ns = self.ns_id(uri);
        let qname = String::from_utf8_lossy(name).into_owned();

        // MC attributes this element carries scope its content (and itself).
        let (ig, pc) = (self.ignorable.len(), self.process.len());
        let mut mc_attrs: Vec<(&'a [u8], &'a [u8])> = vec![];
        for a in &self.reader.attrs {
            if let Some(i) = memchr::memchr(b':', a.name)
                && self.lookup(&a.name[..i]) == Some(MC)
            {
                mc_attrs.push((&a.name[i + 1..], a.value));
            }
        }
        for &(local, value) in &mc_attrs {
            match local {
                b"Ignorable" => {
                    for p in value.split(|&c| c == b' ').filter(|p| !p.is_empty()) {
                        if let Some(u) = self.lookup(p) {
                            let id = self.ns_id(u);
                            self.ignorable.push(id);
                        }
                    }
                }
                b"ProcessContent" => {
                    for q in value.split(|&c| c == b' ').filter(|p| !p.is_empty()) {
                        let mut parts = q.splitn(2, |&c| c == b':');
                        if let (Some(p), Some(l)) = (parts.next(), parts.next())
                            && let Some(u) = self.lookup(p)
                        {
                            let id = self.ns_id(u);
                            self.process.push((id, l));
                        }
                    }
                }
                _ => {}
            }
        }

        let kind = self.classify(ns, local, &qname, root);
        // IndexReference counts every element under a given parent.
        if !self.frames.is_empty() && !matches!(kind, Kind::Transparent | Kind::Alternate { .. }) {
            *self.facts.counts.entry(None).or_default() += 1;
            if let Some(Frame { kind: Kind::Typed(p), .. }) = self.frames.last() {
                let pt = &TYPES[*p as usize];
                let key = Some((pt.ns, pt.local));
                if semantic::targets().1.contains(&key) {
                    *self.facts.counts.entry(key).or_default() += 1;
                }
            }
        }
        let kids_start = self.kids.len();
        let text_start = self.text.len();
        let name_start = self.names.len();
        self.names.push_str(&qname);
        if let Kind::Typed(t) = kind {
            if !mc_attrs.is_empty() {
                self.mc_attributes(&mc_attrs, &qname);
            }
            self.attributes(t, &qname);
        } else if let Kind::Alternate { .. } = kind {
            self.extended_mc(&qname);
            let before = self.errors.len();
            if !mc_attrs.is_empty() {
                self.mc_attributes(&mc_attrs, &qname);
            }
            let raised = self.errors[before..].to_vec();
            self.ac_mc.push(raised);
        }
        self.frames.push(Frame {
            kind,
            ns,
            bindings: mark,
            ignorable: ig,
            process: pc,
            kids_start,
            text_start,
            leaf_child: false,
            name_start,
        });
    }

    /// Decides what an element is, and records it as its owner's child.
    fn classify(&mut self, ns: u32, local: &'a [u8], qname: &str, root: u16) -> Kind {
        let Some(parent) = self.frames.last() else {
            let t = &TYPES[root as usize];
            if ns_ids().get(NAMESPACES[t.ns as usize].as_bytes()) == Some(&ns) && t.local.as_bytes() == local {
                return Kind::Typed(root);
            }
            self.error("Sch_InvalidRootElement", qname, format!("The root element '{qname}' is not expected here."));
            return Kind::Skip;
        };
        let is_mc = NAMESPACES.get(ns as usize).is_some_and(|u| u.as_bytes() == MC);
        // Inside AlternateContent: check its shape as AlternateContentValidator
        // does, and pick the branch as GetContentFromACBlock does.
        if let Kind::Alternate { chosen, state, count } = parent.kind {
            let ac = self.frame_name(self.frames.len() - 1).to_string();
            let choice = is_mc && local == b"Choice";
            let fallback = is_mc && local == b"Fallback";
            let invalid = "Sch_InvalidElementContentExpectingComplex";
            let mut next = state;
            if is_mc && local == b"AlternateContent" {
                self.error(invalid, &ac, "AlternateContent cannot contain AlternateContent.".into());
            } else {
                match (state, choice, fallback) {
                    (0, true, _) => next = 1,
                    (0, _, f) => {
                        self.error("Sch_IncompleteContentExpectingComplex", &ac, "AlternateContent shall begin with a Choice.".into());
                        let _ = f;
                    }
                    (1, true, _) => {}
                    (1, _, true) => next = 2,
                    _ => self.error(invalid, &ac, format!("AlternateContent cannot hold '{qname}' here.")),
                }
                if choice || fallback {
                    self.acb(qname, choice);
                }
            }
            let selected = match local {
                _ if !is_mc => false,
                b"Choice" => !chosen && self.choice_selected(),
                b"Fallback" => !chosen,
                _ => false,
            };
            if let Some(Frame { kind: Kind::Alternate { chosen, state, count: c }, .. }) = self.frames.last_mut() {
                *chosen |= selected;
                *state = next;
                *c = count + 1;
            }
            return if selected { Kind::Transparent } else { Kind::Skip };
        }
        // Inside an ignorable element whose content is processed: unknown
        // elements, inlined into the nearest typed ancestor's children.
        if parent.kind == Kind::Opaque {
            let Some((_, owner_ty)) = self.owner() else { return Kind::Skip };
            if self.ignorable(ns) {
                return if self.process_content(ns, local) { Kind::Opaque } else { Kind::Skip };
            }
            let t = &TYPES[owner_ty as usize];
            if t.leaf != 0 {
                return Kind::Skip;
            }
            let (cs, cn) = t.children;
            let named = CHILDREN[cs as usize..cs as usize + cn as usize].iter().any(|c| {
                c.local.as_bytes() == local && ns_ids().get(NAMESPACES[c.ns as usize].as_bytes()) == Some(&ns) && TYPES[c.ty as usize].version <= TARGET_VERSION
            });
            self.kids.push(Kid { ty: NONE, ns, named });
            return Kind::Skip;
        }
        let Some((owner, owner_ty)) = self.owner() else { return Kind::Skip };
        let owner_ns = self.frames[owner].ns;
        let t = &TYPES[owner_ty as usize];
        if t.leaf != 0 {
            // A leaf's children are not validated; the first is an error.
            if !self.frames[owner].leaf_child {
                self.frames[owner].leaf_child = true;
                let name = self.frame_name(owner).to_string();
                self.error("Sch_InvalidChildinLeafElement", &name, format!("The element '{name}' is a leaf element and cannot contain children."));
            }
            return Kind::Skip;
        }
        // A strong-typed child: known to its owner's factory.
        let (cs, cn) = t.children;
        for c in &CHILDREN[cs as usize..cs as usize + cn as usize] {
            if c.local.as_bytes() == local && ns_ids().get(NAMESPACES[c.ns as usize].as_bytes()) == Some(&ns) {
                if TYPES[c.ty as usize].version <= TARGET_VERSION {
                    self.kids.push(Kid { ty: c.ty, ns, named: false });
                    return Kind::Typed(c.ty);
                }
            }
        }
        if is_mc && local == b"AlternateContent" {
            return Kind::Alternate { chosen: false, state: 0, count: 0 };
        }
        if self.ignorable(ns) {
            return if self.process_content(ns, local) { Kind::Opaque } else { Kind::Skip };
        }
        // An unknown element: a child that matches no particle; not validated.
        let _ = owner_ns;
        self.kids.push(Kid { ty: NONE, ns, named: false });
        Kind::Skip
    }

    /// An mc:Choice is selected when each prefix in Requires names a
    /// namespace whose version is exactly the target (HasVersion compares
    /// with ==, so at Microsoft365 only namespaces introduced then qualify).
    fn choice_selected(&self) -> bool {
        let Some(req) = self.reader.attrs.iter().find(|a| a.name == b"Requires").map(|a| a.value) else { return false };
        let mut any = false;
        for p in req.split(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r')).filter(|p| !p.is_empty()) {
            any = true;
            let Some(u) = self.lookup(p) else { return false };
            let Some(&id) = ns_ids().get(u) else { return false };
            if NAMESPACE_VERSIONS[id as usize] != TARGET_VERSION {
                return false;
            }
        }
        any
    }

    fn frame_name(&self, i: usize) -> &str {
        let start = self.frames[i].name_start;
        let end = self.frames.get(i + 1).map_or(self.names.len(), |f| f.name_start);
        &self.names[start..end]
    }

    fn end(&mut self) {
        let f = self.frames.pop().expect("balanced");
        if let Kind::Alternate { count, .. } = f.kind {
            self.ac_mc.pop();
            if count == 0 {
                let ac = self.names[f.name_start..].to_string();
                self.error("Sch_IncompleteContentExpectingComplex", &ac, "AlternateContent shall contain a Choice.".into());
            }
        }
        if let Kind::Typed(t) = f.kind {
            let ty = &TYPES[t as usize];
            let qname = self.names[f.name_start..].to_string();
            if ty.leaf == 2 {
                let text = self.text[f.text_start..].to_string();
                self.value(ty.validators, ValueKind::StringValue, NONE, Some(&text), false, &qname, "");
            } else if ty.leaf == 0 {
                let kids = &self.kids[f.kids_start..];
                if ty.particle != NONE32 {
                    let verdict = particle::validate(ty.particle, kids, f.ns);
                    if std::env::var_os("PROOF_TRACE").is_some() {
                        eprintln!("{qname}: {:?} -> {}", kids.iter().map(|k| (k.ty, k.named)).collect::<Vec<_>>(), matches!(verdict, Verdict::Ok));
                    }
                    match verdict {
                        Verdict::Ok => {}
                        Verdict::Incomplete => self.error("Sch_IncompleteContentExpectingComplex", &qname, format!("The element '{qname}' has incomplete content.")),
                        Verdict::Unexpected(i) => {
                            let id = if kids[i].ty != NONE {
                                "Sch_UnexpectedElementContentExpectingComplex"
                            } else if kids[i].named {
                                "Sch_InvalidElementContentWrongType"
                            } else {
                                "Sch_InvalidElementContentExpectingComplex"
                            };
                            self.error(id, &qname, format!("The element '{qname}' has an out-of-place child element ({id})."));
                        }
                        Verdict::Repeated(_) => self.error("Sch_AllElement", &qname, format!("The element '{qname}' has a repeated child element.")),
                    }
                } else if !kids.is_empty() {
                    self.error("Sch_InvalidChildinLeafElement", &qname, format!("The element '{qname}' cannot contain children."));
                }
            }
        }
        // Only a typed element owns its kids; a Choice, Fallback or processed
        // wrapper added them to its owner's.
        if let Kind::Typed(_) = f.kind {
            self.kids.truncate(f.kids_start);
        }
        self.text.truncate(f.text_start);
        self.names.truncate(f.name_start);
        self.bindings.truncate(f.bindings);
        self.ignorable.truncate(f.ignorable);
        self.process.truncate(f.process);
    }

    /// AlternateContentValidator.ValidateMcAttributesOnAcb for the element
    /// whose attributes the reader holds: its undeclared attributes.
    fn extended_mc(&mut self, qname: &str) {
        let bad: Vec<&'static str> = self
            .reader
            .attrs
            .iter()
            .filter(|a| a.name != b"xmlns" && !a.name.starts_with(b"xmlns:") && a.name != b"Requires")
            .filter(|a| memchr::memchr(b':', a.name).is_none_or(|i| self.lookup(&a.name[..i]) != Some(MC)))
            .filter_map(|a| match memchr::memchr(b':', a.name) {
                None => Some("MC_ErrorOnUnprefixedAttributeName"),
                Some(_) if a.name == b"xml:space" || a.name == b"xml:lang" => Some("MC_InvalidXmlAttribute"),
                Some(_) => None,
            })
            .collect();
        for id in bad {
            self.errors.push(ValidationError { id, description: format!("'{qname}' carries an attribute markup compatibility does not allow."), part: self.part.to_string(), node: qname.to_string(), error_type: "MarkupCompatibility" });
        }
    }

    /// A Choice or Fallback: its own attributes, its AlternateContent's MC
    /// attributes again, and a Choice's Requires prefixes.
    fn acb(&mut self, qname: &str, choice: bool) {
        // Requires is declared on Choice only.
        if !choice && self.reader.attrs.iter().any(|a| a.name == b"Requires") {
            self.errors.push(ValidationError { id: "MC_ErrorOnUnprefixedAttributeName", description: "Fallback cannot carry Requires.".into(), part: self.part.to_string(), node: qname.to_string(), error_type: "MarkupCompatibility" });
        }
        self.extended_mc(qname);
        if let Some(again) = self.ac_mc.last().cloned() {
            self.errors.extend(again);
        }
        if !choice {
            return;
        }
        match self.reader.attrs.iter().find(|a| a.name == b"Requires").map(|a| a.value) {
            None => self.mc_error("MC_MissedRequiresAttribute", qname, b""),
            Some(v) => {
                for p in v.split(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r')).filter(|p| !p.is_empty()) {
                    if self.lookup(p).is_none_or(|u| u.is_empty()) {
                        self.mc_error("MC_InvalidRequiresAttribute", qname, v);
                    }
                }
            }
        }
    }

    /// CompatibilityRuleAttributesValidator.ValidateMcAttributes.
    fn mc_attributes(&mut self, mc: &[(&'a [u8], &'a [u8])], qname: &str) {
        let get = |name: &[u8]| mc.iter().find(|a| a.0 == name).map(|a| a.1).filter(|v| !v.is_empty());
        let words = |v: &'a [u8]| v.split(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r')).filter(|w| !w.is_empty());
        let mut ignorable: Option<Vec<&[u8]>> = None;
        if let Some(v) = get(b"Ignorable") {
            let mut set = vec![];
            for p in words(v) {
                match self.lookup(p).filter(|u| !u.is_empty()) {
                    Some(u) => set.push(u),
                    None => self.mc_error("MC_InvalidIgnorableAttribute", qname, v),
                }
            }
            ignorable = Some(set);
        }
        // Each QName must name a declared prefix whose namespace is ignorable here.
        let qnames_ok = |this: &Self, v: &'a [u8], set: &[&[u8]]| {
            words(v).all(|q| {
                let parts: Vec<&[u8]> = q.split(|&c| c == b':').collect();
                parts.len() == 2 && this.lookup(parts[0]).filter(|u| !u.is_empty()).is_some_and(|u| set.contains(&u))
            })
        };
        for (attr, id) in [(&b"PreserveAttributes"[..], "MC_InvalidPreserveAttributesAttribute"), (b"PreserveElements", "MC_InvalidPreserveElementsAttribute"), (b"ProcessContent", "MC_InvalidProcessContentAttribute")] {
            if let Some(v) = get(attr) {
                let ok = ignorable.as_ref().is_some_and(|set| qnames_ok(self, v, set));
                if !ok {
                    self.mc_error(id, qname, v);
                }
                if attr == b"ProcessContent" {
                    let xml_attrs = self.reader.attrs.iter().filter(|a| a.name == b"xml:space" || a.name == b"xml:lang").count();
                    for _ in 0..xml_attrs {
                        self.mc_error("MC_InvalidXmlAttributeWithProcessContent", qname, b"");
                    }
                }
            }
        }
        if let Some(v) = get(b"MustUnderstand") {
            for p in words(v) {
                if self.lookup(p).filter(|u| !u.is_empty()).is_none() {
                    self.mc_error("MC_InvalidMustUnderstandAttribute", qname, v);
                }
            }
        }
    }

    fn mc_error(&mut self, id: &'static str, node: &str, value: &[u8]) {
        let value = String::from_utf8_lossy(value);
        self.errors.push(ValidationError { id, description: format!("The markup compatibility attribute value '{value}' is invalid."), part: self.part.to_string(), node: node.to_string(), error_type: "MarkupCompatibility" });
    }

    /// SchemaTypeValidator.ValidateAttributes.
    fn attributes(&mut self, t: u16, qname: &str) {
        let ty = &TYPES[t as usize];
        let (s, n) = ty.attrs;
        let declared = &ATTRS[s as usize..s as usize + n as usize];
        // Resolve each present attribute once: (namespace id, local, value).
        let mut present: Vec<(u32, &'a [u8], &'a [u8], bool)> = Vec::with_capacity(self.reader.attrs.len());
        let attrs: Vec<(&'a [u8], &'a [u8])> = self.reader.attrs.iter().map(|a| (a.name, a.value)).collect();
        for (name, value) in attrs {
            if name == b"xmlns" || name.starts_with(b"xmlns:") {
                continue;
            }
            let (ns, local) = match memchr::memchr(b':', name) {
                Some(i) => {
                    let uri = self.lookup(&name[..i]).unwrap_or(b"");
                    (self.ns_id(uri), &name[i + 1..])
                }
                None => (0, name),
            };
            present.push((ns, local, value, false));
        }
        let mut values: Vec<Option<String>> = Vec::with_capacity(declared.len());
        for a in declared {
            let ans = ns_ids().get(NAMESPACES[a.ns as usize].as_bytes()).copied().unwrap_or(u32::MAX);
            let found = present.iter_mut().find(|p| p.0 == ans && p.1 == a.local.as_bytes());
            let value = found.map(|p| {
                p.3 = true;
                unescape(p.2, true).into_owned()
            });
            self.value(a.validators, a.value, a.enum_id, value.as_deref(), true, qname, a.local);
            values.push(value);
        }
        self.semantics(t, declared, &values, qname);
        for &(ns, local, _, used) in &present {
            if used {
                continue;
            }
            let uri = NAMESPACES.get(ns as usize).map(|u| u.as_bytes()).or_else(|| self.dynamic.iter().find(|d| *d.1 == ns).map(|d| *d.0)).unwrap_or(b"");
            if uri == XML_NS || uri == MC || uri == XMLNS_NS || self.ignorable(ns) {
                continue;
            }
            let name = String::from_utf8_lossy(local).into_owned();
            self.error("Sch_UndeclaredAttribute", qname, format!("The '{name}' attribute is not declared."));
        }
    }

    /// The type's semantic constraints, and the values other parts may refer to.
    fn semantics(&mut self, t: u16, declared: &'static [Attr], values: &[Option<String>], qname: &str) {
        let ty = &TYPES[t as usize];
        let seen: Vec<Seen> = declared.iter().zip(values).map(|(attr, v)| Seen { attr, value: v.as_deref() }).collect();
        for &(ens, el, attr) in &semantic::targets().0 {
            if ens == ty.ns && el == ty.local
                && let Some(v) = semantic::find(&seen, (0, attr)).and_then(|s| s.value).filter(|v| !v.is_empty())
            {
                self.facts.refs.entry((ens, el, attr)).or_default().insert(v.to_string());
            }
        }
        let (cs, cn) = ty.constraints;
        for i in cs..cs + cn as u32 {
            match semantic::check(&CONSTRAINTS[i as usize], &seen, self.rels) {
                Outcome::Pass => {}
                Outcome::Error(id) => self.error_typed(id, qname, format!("Semantic constraint {:?} failed.", CONSTRAINTS[i as usize].kind), "Semantic"),
                Outcome::Unique(v) => {
                    let e = self.unique.entry(i).or_insert_with(|| (HashMap::new(), qname.to_string()));
                    *e.0.entry(v).or_default() += 1;
                }
                Outcome::Defer(value) => self.deferred.push(Deferred { constraint: i, value, node: qname.to_string() }),
            }
        }
    }

    /// SchemaTypeValidator.ValidateValue: the attribute's validators, then
    /// the default one for its value class; the first that fails ends it.
    #[allow(clippy::too_many_arguments)]
    fn value(&mut self, (vs, vn): (u32, u16), kind: ValueKind, enum_id: u16, value: Option<&str>, attribute: bool, qname: &str, local: &str) {
        let list = &VALIDATORS[vs as usize..vs as usize + vn as usize];
        let mut done_unions: u64 = 0;
        let data_type = if attribute { "Sch_AttributeValueDataTypeDetailed" } else { "Sch_ElementValueDataTypeDetailed" };
        // (error id, how many errors that validator added).
        let mut error: Option<(&'static str, u8)> = None;
        for v in list {
            if v.union >= 0 {
                let bit = 1u64 << (v.union as u32 % 64);
                if done_unions & bit != 0 {
                    continue;
                }
                done_unions |= bit;
                let members = list.iter().filter(|m| m.union == v.union);
                let ok = members.into_iter().any(|m| check(m, kind, enum_id, value, data_type).is_none());
                if !ok {
                    error = Some(("Sch_AttributeUnionFailedEx", 1));
                }
            } else {
                error = check(v, kind, enum_id, value, data_type);
            }
            if error.is_some() {
                break;
            }
        }
        if error.is_none() {
            error = default_check(kind, enum_id, value, data_type).map(|id| (id, 1));
        }
        if let Some((id, n)) = error {
            let what = if attribute { format!("attribute '{local}'") } else { "text".to_string() };
            for _ in 0..n {
                self.error(id, qname, format!("The {what} of '{qname}' has an invalid value '{}'.", value.unwrap_or("")));
            }
        }
    }
}

/// The default validator by value class (AttributeMetadata.GetDefaultValidator).
fn default_check(kind: ValueKind, enum_id: u16, value: Option<&str>, data_type: &'static str) -> Option<&'static str> {
    let v = value?;
    match kind {
        ValueKind::ListValue => None,
        _ => (!values::is_valid(kind, enum_id, v)).then_some(data_type),
    }
}

/// One validator against a value; the error id it would add, if any.
/// One validator against a value: the error id it adds and how many times
/// (a StringValidator can fail a shape and a length check; a NumberValidator
/// each range check), if any.
fn check(v: &Validator, kind: ValueKind, enum_id: u16, value: Option<&str>, data_type: &'static str) -> Option<(&'static str, u8)> {
    // SimpleTypeValidator<T>: unless the value already is a T (Hex and
    // Base64 values are StringValues), the text is re-parsed as a T.
    let (kind, enum_id) = match v.value {
        Some(t) if t != kind && !(t == ValueKind::StringValue && matches!(kind, ValueKind::HexBinaryValue | ValueKind::Base64BinaryValue)) => (t, v.enum_id),
        Some(ValueKind::EnumValue) if v.enum_id != enum_id => (ValueKind::EnumValue, v.enum_id),
        _ => (kind, enum_id),
    };
    match v.kind {
        ValidatorKind::Required => value.is_none().then_some(("Sch_MissRequiredAttribute", 1)),
        ValidatorKind::OfficeVersion => None,
        ValidatorKind::Enum => {
            let s = value?;
            (!values::is_valid(kind, enum_id, s)).then_some((data_type, 1))
        }
        ValidatorKind::Number => {
            let s = value?;
            if !values::is_valid(kind, enum_id, s) {
                return Some((data_type, 1));
            }
            let n = values::number(kind, s)?;
            // Each failed check is its own error; a non-finite number fails every one present.
            let odd = !n.is_finite();
            let fails = [
                v.non_negative && (n < 0.0 || odd),
                v.positive && (n <= 0.0 || odd),
                v.max_exclusive.is_some_and(|m| n >= m as f64 || odd),
                v.min_exclusive.is_some_and(|m| n <= m as f64 || odd),
                v.min_inclusive.is_some_and(|m| n < m as f64 || odd),
                v.max_inclusive.is_some_and(|m| n > m as f64 || odd),
            ];
            let count = fails.iter().filter(|&&f| f).count() as u8;
            (count > 0).then_some((data_type, count))
        }
        ValidatorKind::String => {
            let s = value?;
            // Only a StringValue (or its subclasses) is checked here.
            if !matches!(kind, ValueKind::StringValue | ValueKind::HexBinaryValue | ValueKind::Base64BinaryValue | ValueKind::ListValue) {
                return None;
            }
            if kind == ValueKind::ListValue {
                // Each item is checked; the first failure is the error.
                return s.split([' ', '\t', '\n', '\r']).filter(|x| !x.is_empty()).find_map(|item| string_check(v, ValueKind::StringValue, item, data_type));
            }
            string_check(v, kind, s, data_type)
        }
    }
}

fn string_check(v: &Validator, kind: ValueKind, s: &str, data_type: &'static str) -> Option<(&'static str, u8)> {
    if !values::is_valid(kind, NONE, s) {
        return Some((data_type, 1));
    }
    // StringValidator.Validate: one shape check, then one length check; an
    // empty value passes the id, NCName, QName and URI checks.
    let empty = s.is_empty();
    let shape = if v.id && !ncname(s) {
        !empty
    } else if v.ncname {
        if !ncname(s) { !empty } else { s.encode_utf16().count() > 255 }
    } else if v.qname && !qname(s) {
        !empty
    } else if v.token && !token(s) {
        true
    } else if v.uri && !uri(s) {
        !empty
    } else {
        v.pattern != NONE && !patterns()[v.pattern as usize].is_match(s)
    };
    let len = values::string_length(kind, s);
    let size = v.length.is_some_and(|l| len != l) || v.max_length.is_some_and(|l| len > l) || v.min_length.is_some_and(|l| len < l);
    let count = shape as u8 + size as u8;
    (count > 0).then_some((data_type, count))
}

fn name_start(c: char) -> bool {
    c.is_ascii_alphabetic()
        || c == '_'
        || matches!(c as u32, 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0x2FF | 0x370..=0x37D | 0x37F..=0x1FFF | 0x200C..=0x200D | 0x2070..=0x218F | 0x2C00..=0x2FEF | 0x3001..=0xD7FF | 0xF900..=0xFDCF | 0xFDF0..=0xFFFD | 0x10000..=0xEFFFF)
}

fn name_char(c: char) -> bool {
    name_start(c) || c.is_ascii_digit() || matches!(c, '-' | '.' | '\u{B7}') || matches!(c as u32, 0x300..=0x36F | 0x203F..=0x2040)
}

fn ncname(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(name_start) && cs.all(name_char)
}

fn qname(s: &str) -> bool {
    match s.split_once(':') {
        Some((p, l)) => ncname(p) && ncname(l),
        None => ncname(s),
    }
}

/// TokenRestriction.VerifyTOKEN: no tab, CR or LF, no leading, trailing or
/// doubled space.
fn token(s: &str) -> bool {
    !s.contains(['\t', '\n', '\r']) && !s.starts_with(' ') && !s.ends_with(' ') && !s.contains("  ")
}

/// AnyUriRestriction: System.Uri accepts it as absolute or relative; nearly
/// any string is a relative URI.
fn uri(s: &str) -> bool {
    !s.chars().any(|c| matches!(c, '\u{0}'..='\u{1F}'))
}

/// Validates a part's XML with `root` as its root type.
pub fn validate_part(xml: &[u8], part: &str, root: u16, rels: &Rels) -> PartResult {
    match Part::new(xml, part, rels).run(root) {
        Ok(r) => r,
        Err(e) => PartResult {
            errors: vec![ValidationError { id: "xml_parse", description: e.message, part: part.to_string(), node: String::new(), error_type: "Xml" }],
            facts: Facts::default(),
            deferred: vec![],
        },
    }
}

/// A type id by name ("w:CT_Document/w:document").
pub fn type_id(name: &str) -> Option<u16> {
    TYPES.iter().position(|t| t.name == name).map(|i| i as u16)
}
