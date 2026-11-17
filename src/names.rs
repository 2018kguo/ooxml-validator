//! Element and attribute names interned to ids, so matching a child or an
//! attribute compares integers: one hash per name read, then no memcmp scans.

use crate::schema_gen::{ATTRS, CHILDREN, TYPES};
use rustc_hash::FxHashMap;
use std::sync::OnceLock;

/// No name in the tables.
pub const UNKNOWN: u32 = u32::MAX;

pub struct Names {
    ids: FxHashMap<(u32, &'static [u8]), u32>,
    /// Per CHILDREN entry, its name id.
    pub child: Vec<u32>,
    /// Per ATTRS entry, its name id.
    pub attr: Vec<u32>,
    /// Per TYPES entry, its element name id.
    pub ty: Vec<u32>,
}

impl Names {
    pub fn id(&self, ns: u32, local: &[u8]) -> u32 {
        // The map is keyed by 'static slices; a lookup by a shorter-lived one
        // is the same bytes.
        let key: (u32, &'static [u8]) = (ns, unsafe { std::mem::transmute::<&[u8], &'static [u8]>(local) });
        self.ids.get(&key).copied().unwrap_or(UNKNOWN)
    }
}

pub fn names() -> &'static Names {
    static N: OnceLock<Names> = OnceLock::new();
    N.get_or_init(|| {
        let mut ids: FxHashMap<(u32, &'static [u8]), u32> = FxHashMap::default();
        let mut intern = |ns: u16, local: &'static str| {
            let n = ids.len() as u32;
            *ids.entry((ns as u32, local.as_bytes())).or_insert(n)
        };
        let child = CHILDREN.iter().map(|c| intern(c.ns, c.local)).collect();
        let attr = ATTRS.iter().map(|a| intern(a.ns, a.local)).collect();
        let ty = TYPES.iter().map(|t| intern(t.ns, t.local)).collect();
        Names { ids, child, attr, ty }
    })
}
