//! The SDK's XPath for an element (XmlPath.TryBuildXPath): `/w:document[1]/w:body[1]/w:p[3]`.
//!
//! Each step is the element's prefix, local name and 1-based position among
//! earlier siblings with the same name. The prefix is the first non-empty one
//! declared for the element's namespace on it or an ancestor
//! (OpenXmlElement.LookupPrefix), else the one it was written with, else the
//! SDK's own prefix for a namespace it knows, else the namespace URI. Built
//! after validation in a second, cheap pass over only the parts that have
//! errors, so valid documents never pay for it.

use std::collections::HashMap;

use crate::schema_gen::{NAMESPACES, PREFIXES};
use crate::xml::{Event, Reader};

struct Step<'a> {
    /// Namespace declarations on this element: (prefix, URI), in order.
    decls: Vec<(&'a [u8], &'a [u8])>,
    uri: &'a [u8],
    prefix: &'a [u8],
    local: &'a [u8],
    index: u32,
    /// Children seen so far: (URI as the SDK reads it, local name) → count.
    kids: HashMap<(&'a [u8], &'a [u8]), u32>,
    /// This step of the path, once an error below needed it.
    text: Option<String>,
}

/// XPaths of the elements whose names start at `wanted` (byte offsets in `xml`),
/// in a part of a Strict package or not.
pub fn xpaths(xml: &[u8], wanted: &[usize], strict: bool) -> HashMap<usize, String> {
    let mut out = HashMap::new();
    let mut wanted = wanted.to_vec();
    wanted.sort_unstable();
    wanted.dedup();
    if wanted.is_empty() {
        return out;
    }
    let mut reader = Reader::new(xml);
    let mut stack: Vec<Step> = vec![];
    while let Ok(event) = reader.next() {
        match event {
            Event::Start { name, empty } => {
                let decls: Vec<_> = reader
                    .attrs
                    .iter()
                    .filter_map(|a| if a.name == b"xmlns" { Some((&b""[..], a.value)) } else { a.name.strip_prefix(b"xmlns:").map(|p| (p, a.value)) })
                    .collect();
                let (prefix, local) = match memchr::memchr(b':', name) {
                    Some(i) => (&name[..i], &name[i + 1..]),
                    None => (&b""[..], name),
                };
                let uri = decls
                    .iter()
                    .rev()
                    .chain(stack.iter().rev().flat_map(|s| s.decls.iter().rev()))
                    .find(|d| d.0 == prefix)
                    .map_or(&b""[..], |d| d.1);
                let index = match stack.last_mut() {
                    Some(parent) => {
                        // Siblings count by .NET type: Strict and extended namespaces read as Transitional.
                        let n = parent.kids.entry((crate::strict::namespace(uri, strict).unwrap_or(uri), local)).or_insert(0);
                        *n += 1;
                        *n
                    }
                    None => 1,
                };
                stack.push(Step { decls, uri, prefix, local, index, kids: HashMap::new(), text: None });
                let at = name.as_ptr() as usize - xml.as_ptr() as usize;
                if wanted.binary_search(&at).is_ok() {
                    out.insert(at, path(&mut stack, strict));
                    if out.len() == wanted.len() {
                        break;
                    }
                }
                if empty {
                    stack.pop();
                }
            }
            Event::End { .. } => drop(stack.pop()),
            Event::Eof => break,
            Event::Text { .. } => {}
        }
    }
    out
}

fn path(stack: &mut [Step], strict: bool) -> String {
    // Each step's text is worked out once, however many errors lie below it.
    for i in 0..stack.len() {
        if stack[i].text.is_none() {
            let text = step(&stack[..=i], strict);
            stack[i].text = Some(text);
        }
    }
    stack.iter().filter_map(|s| s.text.as_deref()).collect()
}

/// "/prefix:local[index]" for the last of `stack`.
fn step(stack: &[Step], strict: bool) -> String {
    let step = &stack[stack.len() - 1];
    let mut s = String::from("/");
    let declared = stack.iter().rev().flat_map(|a| a.decls.iter()).find(|d| d.1 == step.uri && !d.0.is_empty() && !step.uri.is_empty());
    // Bound to no prefix (a default namespace): the SDK's own prefix for it.
    let known = || {
        let uri = crate::strict::namespace(step.uri, strict).unwrap_or(step.uri);
        NAMESPACES.iter().position(|u| !u.is_empty() && u.as_bytes() == uri).map(|i| PREFIXES[i].as_bytes())
    };
    let prefix = declared.map(|d| d.0).or_else(|| (!step.prefix.is_empty()).then_some(step.prefix)).or_else(known).unwrap_or(b"");
    if !prefix.is_empty() {
        s.push_str(&String::from_utf8_lossy(prefix));
        s.push(':');
    } else if !step.uri.is_empty() {
        s.push_str(&String::from_utf8_lossy(step.uri));
        s.push(':');
    }
    s.push_str(&String::from_utf8_lossy(step.local));
    s.push_str(&format!("[{}]", step.index));
    s
}
