//! Semantic constraints (Validation/Semantic/*.cs), run on each strong-typed
//! element as the SDK's second ValidatingTraverse does. Element-level ones
//! decide at once; unique values are counted over the part; references to
//! other parts are deferred to the package, which knows every part.

use crate::schema::*;
use crate::schema_gen::*;
use crate::values;
use regex::Regex;
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::sync::OnceLock;

/// A part's relationships: id → type (internal and external).
#[derive(Default)]
pub struct Rels {
    pub types: HashMap<String, String>,
}

/// What a part holds that other parts' constraints may refer to.
#[derive(Default)]
pub struct Facts {
    /// (element ns, element local, attribute local) → values present.
    pub refs: HashMap<(u16, &'static str, &'static str), HashSet<String>>,
    /// Elements by their parent's (ns, local); `None` counts every element.
    pub counts: HashMap<Option<(u16, &'static str)>, usize>,
}

/// A check that needs another part: its constraint and the value it read.
pub struct Deferred {
    pub constraint: u32,
    pub value: String,
    pub node: String,
}

/// An attribute as constraints see it (AttributeEntry): declared, maybe present.
pub struct Seen<'v> {
    pub attr: &'static Attr,
    pub value: Option<&'v str>,
}

fn q(a: &Arg) -> Option<(u16, &'static str)> {
    match *a {
        Arg::Q(ns, l) => Some((ns, l)),
        _ => None,
    }
}

/// The (element, attribute) pairs ReferenceExist constraints look up, and the
/// parents IndexReference constraints count under.
/// Whether some ReferenceExist constraint reads this element's values.
pub fn is_target(ns: u16, local: &str) -> bool {
    targets().0.iter().any(|t| t.0 == ns && t.1 == local)
}

/// Per schema type: (another part may refer to its values, IndexReference
/// counts the children of elements of this type). Looked up once per element.
pub fn type_flags(t: u16) -> (bool, bool) {
    static F: OnceLock<Vec<(bool, bool)>> = OnceLock::new();
    F.get_or_init(|| {
        crate::schema_gen::TYPES.iter().map(|ty| (is_target(ty.ns, ty.local), targets().1.contains(&Some((ty.ns, ty.local))))).collect()
    })[t as usize]
}

pub fn targets() -> &'static (HashSet<(u16, &'static str, &'static str)>, HashSet<Option<(u16, &'static str)>>) {
    static T: OnceLock<(HashSet<(u16, &'static str, &'static str)>, HashSet<Option<(u16, &'static str)>>)> = OnceLock::new();
    T.get_or_init(|| {
        let mut refs = HashSet::default();
        let mut parents = HashSet::default();
        for c in &CONSTRAINTS {
            match c.kind {
                ConstraintKind::ReferenceExist => {
                    let (ens, el) = q(&c.args[2]).unwrap();
                    refs.insert((ens, el, q(&c.args[4]).unwrap().1));
                }
                ConstraintKind::IndexReference => {
                    parents.insert(q(&c.args[2]));
                }
                _ => {}
            }
        }
        (refs, parents)
    })
}

fn patterns() -> &'static HashMap<&'static str, Regex> {
    static P: OnceLock<HashMap<&'static str, Regex>> = OnceLock::new();
    P.get_or_init(|| {
        CONSTRAINTS
            .iter()
            .filter(|c| c.kind == ConstraintKind::AttributeValuePattern)
            .filter_map(|c| match c.args[1] {
                Arg::S(p) => {
                    let anchored = if p.starts_with('^') && p.ends_with('$') { p.to_string() } else { format!("^{p}$") };
                    Regex::new(&anchored).ok().map(|r| (p, r))
                }
                _ => None,
            })
            .collect()
    })
}

/// SemanticConstraint.TryFindAttribute: a declared attribute by local name,
/// preferring one in the given namespace.
pub fn find<'s, 'v>(seen: &'s [Seen<'v>], (ns, local): (u16, &str)) -> Option<&'s Seen<'v>> {
    let mut found = None;
    for s in seen {
        if s.attr.local == local {
            found = Some(s);
            if s.attr.ns == ns {
                return found;
            }
        }
    }
    found
}

/// GetAttrNumVal: hex for HexBinaryValue, else digits with an optional point
/// (NumberStyles.AllowDecimalPoint: no sign, no space, no exponent).
fn number(s: &Seen) -> Option<f64> {
    let v = s.value?;
    if s.attr.value == ValueKind::HexBinaryValue {
        return i64::from_str_radix(v.trim(), 16).ok().map(|n| n as f64);
    }
    let digits = v.bytes().filter(u8::is_ascii_digit).count();
    let points = v.bytes().filter(|&b| b == b'.').count();
    (digits > 0 && points <= 1 && digits + points == v.len()).then(|| v.parse().ok()).flatten()
}

/// The bool a boolean-like value class holds, if it parses.
fn as_bool(kind: ValueKind, v: &str) -> Option<bool> {
    use ValueKind::*;
    match kind {
        BooleanValue => match v.trim_matches([' ', '\t', '\n', '\r']) {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        OnOffValue => match v {
            "true" | "on" | "1" => Some(true),
            "false" | "off" | "0" => Some(false),
            _ => None,
        },
        TrueFalseValue => match v {
            "true" | "t" => Some(true),
            "false" | "f" => Some(false),
            _ => None,
        },
        TrueFalseBlankValue => match v {
            "true" | "t" => Some(true),
            "false" | "f" | "" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// SemanticConstraint.AttributeValueEquals.
fn equals(s: &Seen, value: &str) -> bool {
    use ValueKind::*;
    let Some(text) = s.value else { return false };
    let kind = s.attr.value;
    if kind == HexBinaryValue {
        return match (i64::from_str_radix(text, 16), i64::from_str_radix(value, 16)) {
            (Ok(a), Ok(b)) => a == b,
            (Err(_), _) => true, // no value
            _ => false,
        };
    }
    if matches!(kind, BooleanValue | OnOffValue | TrueFalseValue | TrueFalseBlankValue) {
        let Some(b) = as_bool(kind, text) else { return false };
        let parsed = match value.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        };
        if parsed == Some(b) {
            return true;
        }
    }
    text == value
}

fn present<'s, 'v>(seen: &'s [Seen<'v>], a: &Arg) -> Option<&'s Seen<'v>> {
    find(seen, q(a)?).filter(|s| s.value.is_some())
}

fn nonempty<'v>(seen: &[Seen<'v>], a: &Arg) -> Option<&'v str> {
    present(seen, a).and_then(|s| s.value).filter(|v| !v.is_empty())
}

fn strings(a: &Arg) -> &'static [&'static str] {
    match *a {
        Arg::A(v) => v,
        _ => &[],
    }
}

/// Outcome of one constraint on one element.
pub enum Outcome {
    Pass,
    Error(&'static str),
    /// A value to count for uniqueness in this part.
    Unique(String),
    Defer(String),
}

/// Runs an element-level constraint against an element's attributes.
pub fn check(c: &Constraint, seen: &[Seen], rels: &Rels) -> Outcome {
    use ConstraintKind::*;
    use Outcome::*;
    let a = c.args;
    let data_type = "Sem_AttributeValueDataTypeDetailed";
    let fail = |bad: bool, id| if bad { Error(id) } else { Pass };
    match c.kind {
        AttributeValueRange => {
            let Some(s) = present(seen, &a[0]).filter(|s| !s.value.unwrap().is_empty() && values::is_valid(s.attr.value, s.attr.enum_id, s.value.unwrap())) else { return Pass };
            let Some(v) = number(s) else { return Pass };
            let (Arg::N(min), Arg::B(min_in), Arg::N(max), Arg::B(max_in)) = (&a[2], &a[3], &a[4], &a[5]) else { return Pass };
            let low = if *min_in { !(*min <= v) } else { !(*min < v) };
            let high = if *max_in { !(v <= *max) } else { !(v < *max) };
            fail(low || high, data_type)
        }
        AttributeValueLength => {
            let Some(v) = present(seen, &a[0]).and_then(|s| s.value) else { return Pass };
            let (Arg::N(min), Arg::N(max)) = (&a[1], &a[2]) else { return Pass };
            let len = v.encode_utf16().count() as f64;
            fail(len < *min || len > *max, data_type)
        }
        AttributeValueSet => {
            let Some(s) = present(seen, &a[0]).filter(|s| !s.value.unwrap().is_empty()) else { return Pass };
            let Arg::B(valid) = a[1] else { return Pass };
            let contains = strings(&a[2]).iter().any(|v| equals(s, v));
            fail(!(!valid ^ contains), data_type)
        }
        AttributeValuePattern => {
            let Some(v) = nonempty(seen, &a[0]) else { return Pass };
            let Arg::S(p) = a[1] else { return Pass };
            fail(patterns().get(p).is_some_and(|r| !r.is_match(v)), data_type)
        }
        AttributeMutualExclusive => fail(a.iter().filter(|x| present(seen, x).is_some()).count() > 1, "Sem_AttributeMutualExclusive"),
        AttributeCannotOmit => fail(q(&a[0]).and_then(|x| find(seen, x)).is_some_and(|s| s.value.is_none()), "Sem_MissRequiredAttribute"),
        AttributeRequiredConditionToValue => {
            let missing = q(&a[0]).and_then(|x| find(seen, x)).is_some_and(|s| s.value.is_none());
            let Arg::S(value) = a[2] else { return Pass };
            fail(missing && present(seen, &a[1]).is_some_and(|c| equals(c, value)), "Sem_AttributeRequiredConditionToValue")
        }
        AttributeAbsentConditionToNonValue | AttributeAbsentConditionToValue => {
            if present(seen, &a[0]).is_none() {
                return Pass;
            }
            let Some(cond) = present(seen, &a[1]) else { return Pass };
            let any = strings(&a[2]).iter().any(|v| equals(cond, v));
            if c.kind == AttributeAbsentConditionToNonValue {
                fail(!any, "Sem_AttributeAbsentConditionToNonValue")
            } else {
                fail(any, "Sem_AttributeAbsentConditionToValue")
            }
        }
        AttributeValueConditionToAnother => {
            let Some(s) = present(seen, &a[0]) else { return Pass };
            if strings(&a[2]).iter().any(|v| equals(s, v)) {
                return Pass;
            }
            let Some(cond) = present(seen, &a[1]) else { return Pass };
            fail(strings(&a[3]).iter().any(|v| equals(cond, v)), "Sem_AttributeValueConditionToAnother")
        }
        AttributeValueLessEqualToAnother => {
            let (Some(x), Some(y)) = (present(seen, &a[0]).and_then(number), present(seen, &a[1]).and_then(number)) else { return Pass };
            let Arg::B(can_equal) = a[2] else { return Pass };
            fail(!((x < y && !can_equal) || (x <= y && can_equal)), "Sem_AttributeValueLessEqualToAnother")
        }
        UniqueAttributeValue => {
            // A parent-scoped constraint never reports (ValidateCore returns at once).
            if !matches!(a[2], Arg::Null) {
                return Pass;
            }
            match present(seen, &a[0]).and_then(|s| s.value) {
                Some(v) => {
                    let Arg::B(case_sensitive) = a[1] else { return Pass };
                    Unique(if case_sensitive { v.to_string() } else { v.to_lowercase() })
                }
                None => Pass,
            }
        }
        RelationshipExist => {
            let Some(v) = nonempty(seen, &a[0]) else { return Pass };
            fail(!rels.types.contains_key(v), "Sem_InvalidRelationshipId")
        }
        RelationshipType => {
            let Some(v) = nonempty(seen, &a[0]) else { return Pass };
            let Arg::S(want) = a[1] else { return Pass };
            fail(rels.types.get(v).is_some_and(|t| t != want), "Sem_IncorrectRelationshipType")
        }
        ReferenceExist => nonempty(seen, &a[0]).map_or(Pass, |v| Defer(v.to_string())),
        IndexReference => match nonempty(seen, &a[0]).map(|v| (v, v.parse::<i32>())) {
            Some((v, Ok(_))) => Defer(v.to_string()),
            _ => Pass,
        },
    }
}
