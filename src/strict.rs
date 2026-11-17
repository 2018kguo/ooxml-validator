//! ISO 29500 Strict documents, read as Transitional the way the SDK reads
//! them (OpenXmlNamespaceResolver, XmlConvertingReader, the Wordprocessing
//! classes overriding StrictTranslateAttribute). gen fails if the SDK's set of
//! those classes changes, so this port can't fall behind silently.

use crate::schema_gen::{EXTENDED_NAMESPACES, STRICT_NAMESPACES, STRICT_RELATIONSHIPS, STRICT_TYPES, TYPES};
use std::borrow::Cow;
use std::sync::OnceLock;

/// A Strict relationship type's Transitional equivalent.
pub fn relationship(t: &str) -> Option<&'static str> {
    STRICT_RELATIONSHIPS.iter().find(|p| p.0 == t).map(|p| p.1)
}

/// The namespace (or attribute value) the SDK sees for `uri`: Strict ones
/// translated when the package is Strict, else the extended Word namespaces.
pub fn namespace(uri: &[u8], strict: bool) -> Option<&'static [u8]> {
    if !uri.starts_with(b"http://") {
        return None;
    }
    let table: &[(&str, &str)] = if strict { &STRICT_NAMESPACES } else { &EXTENDED_NAMESPACES };
    table.iter().find(|p| p.0.as_bytes() == uri).map(|p| p.1.as_bytes())
}

/// The Strict-translating class of a type, as an index into `translate`'s match.
pub fn class(ty: u16) -> Option<u8> {
    static C: OnceLock<Vec<u8>> = OnceLock::new();
    let c = C.get_or_init(|| {
        let mut v = vec![u8::MAX; TYPES.len()];
        for &(name, k) in &STRICT_TYPES {
            if let Some(i) = TYPES.iter().position(|t| t.name == name) {
                v[i] = k;
            }
        }
        v
    });
    Some(c[ty as usize]).filter(|&k| k != u8::MAX)
}

/// An attribute as an element loads it: namespace id, local name, raw value.
pub type Raw<'a> = (u32, &'a [u8], Cow<'a, [u8]>);

/// Applies class `k`'s StrictTranslateAttribute to an element's attributes,
/// in document order (later ones overwrite earlier, as loading does).
pub fn translate<'a>(k: u8, attrs: &mut Vec<Raw<'a>>) {
    let swap = |v: &[u8], map: &[(&'static str, &'static str)]| map.iter().find(|p| p.0.as_bytes() == v).map(|p| p.1.as_bytes());
    let lr = [("start", "left"), ("end", "right")];
    match k {
        // ConditionalFormatStyle (binary, 12 digits) and TableLook (hex, 4):
        // each boolean attribute folds into val.
        0 | 7 => {
            let (bits, radix, width): (&[(&str, i64)], u32, usize) = if k == 0 {
                (&[("firstRow", 0x800), ("lastRow", 0x400), ("firstColumn", 0x200), ("lastColumn", 0x100), ("oddVBand", 0x80), ("evenVBand", 0x40), ("oddHBand", 0x20), ("evenHBand", 0x10), ("firstRowLastColumn", 0x8), ("firstRowFirstColumn", 0x4), ("lastRowFirstColumn", 0x2), ("lastRowLastColumn", 0x1)], 2, 12)
            } else {
                (&[("firstRow", 0x20), ("lastRow", 0x40), ("firstColumn", 0x80), ("lastColumn", 0x100), ("noHBand", 0x200), ("noVBand", 0x400)], 16, 4)
            };
            let mut out: Vec<Raw<'a>> = Vec::with_capacity(attrs.len());
            for (ns, local, value) in attrs.drain(..) {
                let Some(&(_, bit)) = bits.iter().find(|b| b.0.as_bytes() == local) else {
                    out.push((ns, local, value));
                    continue;
                };
                let on = &*value == b"true" || &*value == b"1";
                let at = out.iter().position(|a| a.0 == ns && a.1 == b"val");
                let current = at.map(|i| std::str::from_utf8(&out[i].2).ok().and_then(|s| i64::from_str_radix(s, radix).ok()).unwrap_or(0));
                let v = match current {
                    Some(c) if on => c | bit,
                    Some(c) => c & !bit,
                    None if on => bit,
                    None => 0,
                };
                let text = if radix == 2 { format!("{v:0width$b}") } else { format!("{v:0width$x}") };
                match at {
                    Some(i) => out[i].2 = Cow::Owned(text.into_bytes()),
                    None => out.push((ns, b"val", Cow::Owned(text.into_bytes()))),
                }
            }
            *attrs = out;
        }
        // Document: conformance="strict" is consumed.
        1 => attrs.retain(|a| !(a.1 == b"conformance" && &*a.2 == b"strict")),
        // Indentation: left/right are start/end.
        2 => {
            for a in attrs.iter_mut() {
                if let Some(l) = swap(a.1, &[("left", "start"), ("leftChars", "startChars"), ("right", "end"), ("rightChars", "endChars")]) {
                    a.1 = l;
                }
            }
        }
        // Justification: any attribute's start/end value.
        3 => {
            for a in attrs.iter_mut() {
                if let Some(v) = swap(&a.2, &lr) {
                    a.2 = Cow::Borrowed(v);
                }
            }
        }
        // StylePaneSortMethods, TabStop, TableJustification, TextDirection: val.
        _ => {
            let map: &[(&str, &str)] = match k {
                4 => &[("name", "0000"), ("priority", "0001"), ("default", "0002"), ("font", "0003"), ("basedOn", "0004"), ("type", "0005")],
                8 => &[("lr", "btLr"), ("tb", "lrTb"), ("tbV", "lrTbV"), ("lrV", "tbLrV"), ("rl", "tbRl"), ("rlV", "tbRlV")],
                _ => &lr,
            };
            for a in attrs.iter_mut() {
                if a.1 == b"val"
                    && let Some(v) = swap(&a.2, map)
                {
                    a.2 = Cow::Borrowed(v);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(k: u8, attrs: &[(&'static str, &'static str)]) -> Vec<(String, String)> {
        let mut v: Vec<Raw> = attrs.iter().map(|(l, v)| (1, l.as_bytes(), Cow::Borrowed(v.as_bytes()))).collect();
        translate(k, &mut v);
        v.into_iter().map(|(_, l, v)| (String::from_utf8_lossy(l).into(), String::from_utf8_lossy(&v).into())).collect()
    }

    #[test]
    fn folds_flags_and_renames_as_the_sdk() {
        assert_eq!(run(7, &[("firstRow", "1"), ("noVBand", "true")]), [("val".into(), "0420".into())]);
        assert_eq!(run(0, &[("val", "000000000000"), ("firstRow", "1"), ("lastRowLastColumn", "0")]), [("val".into(), "100000000000".into())]);
        assert_eq!(run(2, &[("left", "720")]), [("start".into(), "720".into())]);
        assert_eq!(run(5, &[("val", "end"), ("pos", "end")]), [("val".into(), "right".into()), ("pos".into(), "end".into())]);
        assert!(run(1, &[("conformance", "strict")]).is_empty());
    }
}
