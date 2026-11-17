//! Child order, as the SDK's particle validators decide it
//! (Validation/Schema/*ParticleValidator.cs, ElementParticle.cs). They run over
//! an element's markup-compatibility-flattened child list; a matched element
//! is an index into it.

use crate::schema::{NONE, ParticleKind, Particle};
use crate::schema_gen::PARTICLES;

/// One child as particle matching sees it.
#[derive(Clone, Copy, Debug)]
pub struct Kid {
    /// Its schema type under this parent, or NONE for an unknown element.
    pub ty: u16,
    /// Its namespace id (dynamic ids past the table for unknown namespaces).
    pub ns: u32,
    /// An unknown element whose name the parent's factory knows (inlined
    /// from an ignorable wrapper): Sch_InvalidElementContentWrongType.
    pub named: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Match {
    No,
    Partial,
    Matched,
}

#[derive(Clone, Copy, Debug)]
pub struct Info {
    pub m: Match,
    /// The last child matched, as an index.
    pub last: Option<usize>,
}

pub struct Ctx<'k> {
    pub kids: &'k [Kid],
    /// The parent's namespace, for ##other and ##targetNamespace.
    pub parent_ns: u32,
}

fn p(id: u32) -> &'static Particle {
    &PARTICLES[id as usize]
}

fn more_than(p: &Particle, count: u32) -> bool {
    p.max == 0 || p.max > count
}

impl Ctx<'_> {
    fn ty(&self, i: usize) -> Option<u16> {
        self.kids.get(i).map(|k| k.ty)
    }

    pub fn try_match(&self, id: u32, start: usize) -> Info {
        let q = p(id);
        match q.kind {
            ParticleKind::Element => self.element(q, start),
            ParticleKind::Any => self.any(q, start),
            // AllParticleValidator.TryMatch is TryMatchOnce.
            ParticleKind::All => self.once(id, start),
            _ if q.max == 1 => self.once(id, start),
            _ => {
                // CompositeParticleValidator.TryMatch.
                let mut count = 0;
                let mut next = start;
                let mut info = Info { m: Match::No, last: None };
                while next < self.kids.len() && more_than(q, count) {
                    let child = self.once(id, next);
                    match child.m {
                        Match::No => break,
                        Match::Matched => {
                            count += 1;
                            info.last = child.last;
                            // A match consumes a child (a sequence matches only
                            // with one); guard the loop all the same.
                            let Some(l) = child.last else { break };
                            next = l + 1;
                        }
                        Match::Partial => {
                            return Info { m: Match::Partial, last: child.last };
                        }
                    }
                }
                info.m = if count == 0 {
                    Match::No
                } else if count >= q.min {
                    Match::Matched
                } else if self.required(id) {
                    Match::Partial
                } else {
                    Match::Matched
                };
                info
            }
        }
    }

    fn element(&self, q: &Particle, start: usize) -> Info {
        if self.ty(start) != Some(q.element) {
            return Info { m: Match::No, last: None };
        }
        if q.max == 1 {
            return Info { m: Match::Matched, last: Some(start) };
        }
        let mut count = 0;
        let mut i = start;
        let mut last = None;
        while i < self.kids.len() && more_than(q, count) && self.kids[i].ty == q.element {
            count += 1;
            last = Some(i);
            i += 1;
        }
        Info { m: if count >= q.min { Match::Matched } else { Match::Partial }, last }
    }

    fn any_once(&self, q: &Particle, start: usize) -> Info {
        let Some(kid) = self.kids.get(start) else { return Info { m: Match::No, last: None } };
        // Element namespace "" is id 0.
        let ok = match q.any {
            0 => true,
            2 => kid.ns == 0,
            1 => kid.ns == 0 || kid.ns != self.parent_ns,
            _ => kid.ns == self.parent_ns,
        };
        Info { m: if ok { Match::Matched } else { Match::No }, last: ok.then_some(start) }
    }

    fn any(&self, q: &Particle, start: usize) -> Info {
        if q.max == 1 {
            return self.any_once(q, start);
        }
        let mut count = 0;
        let mut next = start;
        let mut last = None;
        while next < self.kids.len() && more_than(q, count) {
            let one = self.any_once(q, next);
            if one.m == Match::No {
                break;
            }
            count += 1;
            last = one.last;
            next = one.last.unwrap() + 1;
        }
        let m = if count == 0 {
            Match::No
        } else if count >= q.min {
            Match::Matched
        } else {
            Match::Partial
        };
        Info { m, last }
    }

    /// TryMatchOnce of a composite particle.
    fn once(&self, id: u32, start: usize) -> Info {
        let q = p(id);
        match q.kind {
            ParticleKind::Sequence => self.sequence(q, start),
            ParticleKind::Choice => {
                for &c in q.items {
                    if start >= self.kids.len() {
                        break;
                    }
                    let child = self.try_match(c, start);
                    if child.m != Match::No {
                        return child;
                    }
                }
                Info { m: Match::No, last: None }
            }
            // GroupParticleValidator: its one child, a choice or sequence.
            ParticleKind::Group => self.try_match(q.items[0], start),
            ParticleKind::All => self.all(q, start),
            // ElementParticle.TryMatchOnce (unreachable from TryMatch) and Any.
            ParticleKind::Element => {
                let differs = self.ty(start) != Some(q.element);
                Info { m: if differs { Match::Matched } else { Match::No }, last: differs.then_some(start) }
            }
            ParticleKind::Any => self.any_once(q, start),
        }
    }

    fn sequence(&self, q: &Particle, start: usize) -> Info {
        let mut next = start;
        let mut last = None;
        let mut index = 0;
        while index < q.items.len() && next < self.kids.len() {
            let c = q.items[index];
            let child = self.try_match(c, next);
            match child.m {
                Match::No => {
                    if self.required(c) {
                        return if next == start { Info { m: Match::No, last: None } } else { Info { m: Match::Partial, last } };
                    }
                    index += 1;
                }
                Match::Matched => {
                    last = child.last;
                    next = last.map_or(next, |l| l + 1);
                    index += 1;
                }
                Match::Partial => return Info { m: Match::Partial, last: child.last },
            }
        }
        if index == q.items.len() {
            return Info { m: if last.is_some() { Match::Matched } else { Match::No }, last };
        }
        // Children ran out: the rest must not require anything.
        if q.items[index..].iter().any(|&c| self.required(c)) {
            return Info { m: Match::Partial, last };
        }
        Info { m: Match::Matched, last }
    }

    fn all(&self, q: &Particle, start: usize) -> Info {
        let members: Vec<(u16, u32)> = q.items.iter().map(|&c| p(c)).filter(|c| c.kind == ParticleKind::Element).map(|c| (c.element, c.min)).collect();
        let mut seen = vec![false; members.len()];
        let mut last = None;
        let mut next = start;
        while let Some(ty) = self.ty(next) {
            let Some(i) = members.iter().position(|m| m.0 == ty) else { break };
            if seen[i] {
                break;
            }
            seen[i] = true;
            last = Some(next);
            next += 1;
        }
        if last.is_none() {
            return Info { m: Match::No, last: None };
        }
        let partial = members.iter().zip(&seen).any(|(m, &s)| !s && m.1 == 1);
        Info { m: if partial { Match::Partial } else { Match::Matched }, last }
    }

    /// GetRequiredElements(null): whether the particle requires a child.
    pub fn required(&self, id: u32) -> bool {
        required(id)
    }
}

/// GetRequiredElements(null) != empty, precomputed per particle.
pub fn required(id: u32) -> bool {
    static R: std::sync::OnceLock<Vec<bool>> = std::sync::OnceLock::new();
    R.get_or_init(|| (0..PARTICLES.len() as u32).map(required_slow).collect())[id as usize]
}

fn required_slow(id: u32) -> bool {
    let q = p(id);
    match q.kind {
        ParticleKind::Element | ParticleKind::Any => q.min > 0,
        // Choice: required only if every alternative requires something.
        ParticleKind::Choice => q.min > 0 && q.items.iter().all(|&c| required_slow(c)),
        // Sequence, Group, All (CompositeParticleValidator): any child requires.
        _ => q.min > 0 && q.items.iter().any(|&c| required_slow(c)),
    }
}

/// The outcome of CompositeParticleValidator.Validate for an element's
/// children: None if they match, else the index of the offending child
/// (None inside for incomplete content) and whether it is incomplete.
pub enum Verdict {
    Ok,
    /// Required children are missing (Sch_IncompleteContentExpectingComplex).
    Incomplete,
    /// This child is not allowed where it is.
    Unexpected(usize),
    /// All: this member repeats (Sch_AllElement).
    Repeated(usize),
    /// All: this child is not a member; whatever it is, the SDK says
    /// Sch_InvalidElementContentExpectingComplex.
    NotMember(usize),
}

pub fn validate(root: u32, kids: &[Kid], parent_ns: u32) -> Verdict {
    let ctx = Ctx { kids, parent_ns };
    let q = p(root);
    if kids.is_empty() {
        return if q.min == 0 || !required(root) { Verdict::Ok } else { Verdict::Incomplete };
    }
    let info = ctx.try_match(root, 0);
    let next = info.last.map_or(0, |l| l + 1);
    match info.m {
        Match::Matched if next >= kids.len() => Verdict::Ok,
        m => {
            if q.kind == ParticleKind::All {
                // AllParticleValidator.EmitInvalidElementError.
                if next >= kids.len() {
                    return Verdict::Ok;
                }
                let member = q.items.iter().any(|&c| p(c).kind == ParticleKind::Element && p(c).element == kids[next].ty && kids[next].ty != NONE);
                return if m != Match::No && member { Verdict::Repeated(next) } else { Verdict::NotMember(next) };
            }
            // EmitInvalidElementError re-runs the match collecting expected
            // children; the verdict is the same match.
            if m == Match::Partial && next >= kids.len() {
                return Verdict::Incomplete;
            }
            Verdict::Unexpected(next)
        }
    }
}
