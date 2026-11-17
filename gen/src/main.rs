//! Reads the Open XML SDK's schema data (MIT) and writes `src/schema_gen.rs`:
//! static tables resolved for FileFormatVersions.Microsoft365, the version the
//! SDK validator is run at. Version gating is applied here, so the runtime
//! never looks at a version.
//!
//! Usage: gen <Open-XML-SDK checkout> <out.rs>

use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;

const VERSIONS: [&str; 8] = [
    "Office2007", "Office2010", "Office2013", "Office2016", "Office2019", "Office2021", "Microsoft365", "Office2024",
];

fn version(v: Option<&Value>) -> usize {
    let name = v.and_then(Value::as_str).unwrap_or("Office2007");
    VERSIONS.iter().position(|x| *x == name).unwrap_or_else(|| panic!("version {name}"))
}

// Microsoft365's position: AtLeast(v) holds for every v but Office2024.
const TARGET: usize = 6;
fn at_least(v: usize) -> bool {
    v <= TARGET
}

struct Out {
    ns: Vec<String>,
    ns_ids: HashMap<String, u16>,
    prefixes: HashMap<String, String>,
}

impl Out {
    fn ns(&mut self, uri: &str) -> u16 {
        if let Some(&i) = self.ns_ids.get(uri) {
            return i;
        }
        let i = self.ns.len() as u16;
        self.ns.push(uri.to_string());
        self.ns_ids.insert(uri.to_string(), i);
        i
    }
    // "w:val" -> (ns id, "val"); an unprefixed attribute has no namespace.
    fn qname(&mut self, q: &str) -> (u16, String) {
        match q.split_once(':') {
            Some((p, l)) => {
                let uri = self.prefixes.get(p).unwrap_or_else(|| panic!("prefix {p}")).clone();
                (self.ns(&uri), l.to_string())
            }
            None => (self.ns(""), q.to_string()),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sdk = std::path::Path::new(&args[1]);
    let data = sdk.join("data");
    let namespaces: Value = serde_json::from_str(&std::fs::read_to_string(data.join("namespaces.json")).unwrap()).unwrap();
    let mut out = Out { ns: vec![], ns_ids: HashMap::new(), prefixes: HashMap::new() };
    out.ns("");
    // Every known namespace gets an id up front, with the version that
    // introduced it (OpenXmlNamespaceResolver: a prefix without one is 2007).
    let mut ns_version: HashMap<String, usize> = HashMap::new();
    for n in namespaces.as_array().unwrap() {
        let uri = n["Uri"].as_str().unwrap();
        out.prefixes.insert(n["Prefix"].as_str().unwrap().into(), uri.into());
        out.ns(uri);
        ns_version.insert(uri.into(), version(n.get("Version")));
    }

    // Every schema type by its name ("w:CT_R/w:r"), and enums by type name.
    let mut types: BTreeMap<String, Value> = BTreeMap::new();
    let mut enums: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
    // EnumValue<Ns.Class> names an enum by its .NET class: the class's prefix maps to its namespace.
    let typed: Value = serde_json::from_str(&std::fs::read_to_string(data.join("typed/namespaces.json")).unwrap()).unwrap();
    let dotnet: HashMap<String, String> = typed.as_array().unwrap().iter()
        .map(|x| (x["Prefix"].as_str().unwrap().into(), x["Namespace"].as_str().unwrap().into())).collect();
    let mut enum_class: HashMap<String, String> = HashMap::new(); // "Ns.Class" -> schema type
    let mut file_class: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut files: Vec<_> = std::fs::read_dir(data.join("schemas")).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    for f in files {
        let d: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        let stem = f.file_stem().unwrap().to_string_lossy().into_owned();
        for t in d["Types"].as_array().into_iter().flatten() {
            types.insert(t["Name"].as_str().unwrap().into(), t.clone());
            file_class.entry((stem.clone(), t["ClassName"].as_str().unwrap().to_string())).or_default().push(t["Name"].as_str().unwrap().to_string());
        }
        for e in d["Enums"].as_array().into_iter().flatten() {
            let values = e["Facets"].as_array().unwrap().iter()
                .map(|x| (x["Value"].as_str().unwrap().to_string(), version(x.get("Version"))))
                .collect();
            let ty = e["Type"].as_str().unwrap();
            if let Some(ns) = ty.split_once(':').and_then(|(p, _)| dotnet.get(p)) {
                enum_class.insert(format!("{ns}.{}", e["Name"].as_str().unwrap()), ty.to_string());
            }
            enums.insert(ty.into(), values);
        }
    }
    let names: Vec<String> = types.keys().cloned().collect();
    let id: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (n.as_str(), i)).collect();
    // A derived type names its base by class name; prefer the base in its own namespace prefix.
    let mut by_class: HashMap<&str, Vec<&str>> = HashMap::new();
    for (n, t) in &types {
        by_class.entry(t["ClassName"].as_str().unwrap()).or_default().push(n);
    }
    let base_of = |n: &str| -> Option<usize> {
        let t = &types[n];
        let c = by_class.get(t["BaseClass"].as_str()?)?;
        let prefix = n.split(':').next();
        let pick = c.iter().find(|x| x.split(':').next() == prefix).or(c.first())?;
        Some(id[*pick])
    };

    // Semantic constraints, scraped from the generated C# (ConfigureMetadata's
    // builder.AddConstraint calls); kept when they apply to Word at the target.
    let mut constraints_of: HashMap<String, Vec<String>> = HashMap::new();
    {
        let dir = sdk.join("generated/DocumentFormat.OpenXml/DocumentFormat.OpenXml.Generator/DocumentFormat.OpenXml.Generator.OpenXmlGenerator");
        for e in std::fs::read_dir(&dir).unwrap() {
            let path = e.unwrap().path();
            let file = path.file_name().unwrap().to_string_lossy().into_owned();
            let Some(stem) = file.strip_suffix(".g.cs") else { continue };
            let src = std::fs::read_to_string(&path).unwrap();
            let mut class = String::new();
            for line in src.lines() {
                let l = line.trim();
                if let Some(i) = l.find(" partial class ") {
                    if l.starts_with("public") {
                        class = l[i + 15..].split([' ', ':']).next().unwrap().to_string();
                    }
                }
                let Some(rest) = l.strip_prefix("builder.AddConstraint(new ") else { continue };
                let (kind, rest) = rest.split_once('(').unwrap();
                let (args, tail) = constraint_args(rest);
                let app = tail.split("Application = ").nth(1).map(|a| a.split([',', '}']).next().unwrap().trim().to_string());
                if app.as_ref().is_some_and(|a| !a.contains("Word")) {
                    continue;
                }
                let ver = tail.split("Version = FileFormatVersions.").nth(1).map(|v| v.split([',', ' ', '}']).next().unwrap().to_string());
                if !at_least(version(ver.map(Value::String).as_ref())) {
                    continue;
                }
                let args: Vec<String> = args.iter().map(|a| match a {
                    Arg::Q(q) => {
                        let (ns, local) = match q.split_once(':') {
                            // An unknown prefix resolves to no namespace (ParseQName).
                            Some((p, l)) if out.prefixes.contains_key(p) => out.qname(q).0.to_string().pipe(|n| (n, l.to_string())),
                            Some((_, l)) => ("0".to_string(), l.to_string()),
                            None => ("0".to_string(), q.clone()),
                        };
                        format!("Arg::Q({ns}, {local:?})")
                    }
                    Arg::S(x) => format!("Arg::S({x:?})"),
                    Arg::B(b) => format!("Arg::B({b})"),
                    Arg::N(n) if n.is_infinite() => format!("Arg::N(f64::{}INFINITY)", if *n < 0.0 { "NEG_" } else { "" }),
                    Arg::N(n) => format!("Arg::N({n:?})"),
                    Arg::Null => "Arg::Null".to_string(),
                    Arg::A(v) => format!("Arg::A(&{v:?})"),
                }).collect();
                let row = format!("    Constraint {{ kind: ConstraintKind::{}, args: &[{}] }},", kind.trim_end_matches("Constraint"), args.join(", "));
                for t in file_class.get(&(stem.to_string(), class.clone())).into_iter().flatten() {
                    constraints_of.entry(t.clone()).or_default().push(row.clone());
                }
            }
        }
    }
    let mut constraint_rows = String::new();
    let mut nconstraints = 0u32;

    let mut s = String::new();
    writeln!(s, "// Generated by gen/ from the Open XML SDK's schema data (MIT, (c) Microsoft).").unwrap();
    writeln!(s, "// Resolved for FileFormatVersions.Microsoft365. Do not edit.\n").unwrap();
    writeln!(s, "use crate::schema::*;\n").unwrap();

    // Validators, attributes, particles, children: flat arrays with ranges.
    let mut validators = String::new();
    let mut nvalidators = 0u32;
    let mut patterns: Vec<String> = vec![];
    let mut enum_ids: BTreeMap<String, u16> = BTreeMap::new();
    // The SDK generator's GetSimpleType table (SimpleTypeExtensions.cs): a
    // validator with a Type runs on the text re-parsed as that value class.
    let simple: HashMap<String, String> = {
        let src = std::fs::read_to_string(sdk.join("gen/DocumentFormat.OpenXml.Generator.Models/Generators/Elements/SimpleTypeExtensions.cs")).unwrap();
        let consts: HashMap<&str, &str> = src.lines().filter_map(|l| {
            let l = l.trim().strip_prefix("private const string ")?;
            let (n, v) = l.split_once(" = ")?;
            Some((n, v.trim_end_matches(';').trim_matches('"')))
        }).collect();
        src.lines().filter_map(|l| {
            let l = l.trim().strip_prefix("{ \"")?.strip_suffix(" },")?;
            let (q, v) = l.split_once("\", ")?;
            let v = consts.get(v).copied().unwrap_or(v.trim_matches('"'));
            Some((q.to_string(), v.to_string()))
        }).collect()
    };
    assert!(simple.len() > 50, "simple type table");
    let mut emit_validators = |vs: Option<&Value>, out: &mut Out, enum_ids: &mut BTreeMap<String, u16>, patterns: &mut Vec<String>| -> (u32, u16) {
        let start = nvalidators;
        for v in vs.and_then(Value::as_array).into_iter().flatten() {
            // VersionedValidator.IsValid: InitialVersion means AtLeast, Version alone means exactly.
            let applies = if v["IsInitialVersion"].as_bool() == Some(true) {
                at_least(version(v.get("Version")))
            } else if let Some(x) = v.get("Version") {
                version(Some(x)) == TARGET
            } else {
                true
            };
            if !applies {
                continue;
            }
            let mut a: BTreeMap<&str, &str> = BTreeMap::new();
            for g in v["Arguments"].as_array().into_iter().flatten() {
                if let (Some(n), Some(x)) = (g["Name"].as_str(), g["Value"].as_str()) {
                    a.insert(n, x);
                }
            }
            // RequiredValidator { IsRequired = false } requires nothing.
            if v["Name"] == "RequiredValidator" && a.get("IsRequired").is_some_and(|x| x.eq_ignore_ascii_case("false")) {
                continue;
            }
            let num = |k: &str| a.get(k).map_or("None".to_string(), |x| format!("Some({x})"));
            let flag = |k: &str| a.get(k).is_some_and(|x| x.eq_ignore_ascii_case("true"));
            let pattern = a.get("Pattern").map_or("NONE".to_string(), |p| {
                let i = patterns.iter().position(|x| x == p).unwrap_or_else(|| {
                    patterns.push(p.to_string());
                    patterns.len() - 1
                });
                i.to_string()
            });
            let ty = v["Type"].as_str().unwrap_or("");
            let enum_id = enums.get(ty).map_or("NONE".to_string(), |_| {
                let n = enum_ids.len() as u16;
                enum_ids.entry(ty.to_string()).or_insert(n).to_string()
            });
            let _ = out;
            let value = if ty.is_empty() {
                "None".to_string()
            } else if v["IsList"].as_bool() == Some(true) {
                "Some(ValueKind::ListValue)".to_string()
            } else if let Some(k) = simple.get(ty) {
                format!("Some(ValueKind::{})", k.split('<').next().unwrap())
            } else if enums.contains_key(ty) {
                "Some(ValueKind::EnumValue)".to_string()
            } else {
                panic!("no simple type for {ty}")
            };
            writeln!(validators,
                "    Validator {{ kind: ValidatorKind::{}, union: {}, xsd: {:?}, value: {value}, enum_id: {enum_id}, min_inclusive: {}, max_inclusive: {}, min_exclusive: {}, max_exclusive: {}, min_length: {}, max_length: {}, length: {}, pattern: {pattern}, token: {}, ncname: {}, qname: {}, id: {}, uri: {}, positive: {}, non_negative: {}, list: {} }},",
                match v["Name"].as_str().unwrap() {
                    "StringValidator" => "String",
                    "NumberValidator" => "Number",
                    "RequiredValidator" => "Required",
                    "EnumValidator" => "Enum",
                    "OfficeVersionValidator" => "OfficeVersion",
                    other => panic!("validator {other}"),
                },
                v["UnionId"].as_i64().map_or(-1, |x| x),
                ty,
                num("MinInclusive"), num("MaxInclusive"), num("MinExclusive"), num("MaxExclusive"),
                num("MinLength"), num("MaxLength"), num("Length"),
                flag("IsToken"), flag("IsNcName"), flag("IsQName"), flag("IsId"), flag("IsUri"),
                flag("IsPositive"), flag("IsNonNegative"), v["IsList"].as_bool().unwrap_or(false),
            ).unwrap();
            nvalidators += 1;
        }
        (start, (nvalidators - start) as u16)
    };

    let mut attrs = String::new();
    let mut nattrs = 0u32;
    let mut children = String::new();
    let mut nchildren = 0u32;
    let mut particles = String::new();
    let mut nparticles = 0u32;
    let mut type_rows = String::new();

    // A particle tree, built for Microsoft365 (CompositeParticle.Build with
    // filterVersion: a later version's entry for the same element replaces
    // the earlier one). Returns the root particle ids (one per Occurs entry).
    fn build(p: &Value, id: &HashMap<&str, usize>, out: &mut Out, rows: &mut Vec<String>) -> Vec<u32> {
        let kind = p["Kind"].as_str().unwrap_or("Element");
        let initial = version(p.get("InitialVersion"));
        let occurs: Vec<(i64, i64, usize)> = match p["Occurs"].as_array() {
            None => vec![(1, 1, initial)],
            Some(list) => list.iter().map(|o| {
                let v = if o["IncludeVersion"].as_bool() == Some(true) { version(o.get("Version")) } else { initial };
                (o["Min"].as_i64().unwrap_or(0), o["Max"].as_i64().unwrap_or(0), v)
            }).collect(),
        };
        let mut ids = vec![];
        for (min, max, v) in occurs {
            if !at_least(v) {
                continue;
            }
            // Children first, filtered as Builder.Add does.
            let mut kids: Vec<(u32, usize, Option<usize>, bool)> = vec![]; // (id, version, element type, is group)
            for item in p["Items"].as_array().into_iter().flatten() {
                for k in build(item, id, out, rows) {
                    let row = &rows[k as usize];
                    let elem = row.split("element: ").nth(1).and_then(|x| x.split(',').next()).and_then(|x| x.trim().parse::<usize>().ok()).filter(|&x| x != usize::from(u16::MAX));
                    let kv = row.split("version: ").nth(1).and_then(|x| x.split(',').next()).and_then(|x| x.trim().parse::<usize>().ok()).unwrap();
                    let group = row.contains("ParticleKind::Group");
                    if !kids.is_empty() {
                        let require_filter = p["RequireFilter"].as_bool() == Some(true);
                        kids.retain(|(_, ov, oe, og)| {
                            let equal = match (elem, oe) {
                                (Some(a), Some(b)) => a == *b,
                                _ => require_filter && group && *og,
                            };
                            !(equal && kv > *ov)
                        });
                    }
                    kids.push((k, kv, elem, group));
                }
            }
            let (element, ns) = match kind {
                "Element" => (id[p["Name"].as_str().unwrap()], 0),
                "Any" => (usize::from(u16::MAX), match p["Namespace"].as_str() {
                    None | Some("##any") => 0,
                    Some("##other") => 1,
                    Some("##local") => 2,
                    Some("##targetNamespace") => 3,
                    Some(other) => panic!("any namespace {other}"),
                }),
                _ => (usize::from(u16::MAX), 0),
            };
            let items: Vec<String> = kids.iter().map(|k| k.0.to_string()).collect();
            let me = rows.len() as u32;
            rows.push(format!(
                "Particle {{ kind: ParticleKind::{kind}, min: {min}, max: {max}, element: {element}, any: {ns}, version: {v}, items: &[{}] }},",
                items.join(", ")
            ));
            ids.push(me);
        }
        let _ = out;
        ids
    }
    let mut rows: Vec<String> = vec![];

    for n in &names {
        let t = &types[n];
        // Own attributes plus inherited ones (base first, as the SDK's metadata builder merges them).
        let mut chain = vec![id[n.as_str()]];
        while let Some(b) = base_of(&names[*chain.last().unwrap()]) {
            if chain.contains(&b) {
                break;
            }
            chain.push(b);
        }
        let astart = nattrs;
        let mut lists: Vec<Value> = vec![];
        // The one hand-written base with metadata (CustomXmlElement.cs): an
        // unprefixed uri, and w:element as an NCName.
        if types[&names[*chain.last().unwrap()]]["BaseClass"].as_str() == Some("CustomXmlElement") {
            lists.push(serde_json::json!([
                {"QName": ":uri", "Type": "StringValue"},
                {"QName": "w:element", "Type": "StringValue", "Validators": [{"Name": "StringValidator", "Arguments": [{"Type": "Boolean", "Name": "IsNcName", "Value": "True"}]}]},
            ]));
        }
        lists.extend(chain.iter().rev().map(|&c| types[&names[c]]["Attributes"].clone()));
        for list in &lists {
            for a in list.as_array().into_iter().flatten() {
                let (ns, local) = out.qname(a["QName"].as_str().unwrap());
                let (vs, vn) = emit_validators(a.get("Validators"), &mut out, &mut enum_ids, &mut patterns);
                let value = a["Type"].as_str().unwrap();
                let (kind, enum_type) = match value.split_once('<') {
                    Some((k, rest)) => (k, rest.trim_end_matches('>')),
                    None => (value, ""),
                };
                let enum_id = if kind == "EnumValue" {
                    let ty = enum_class.get(enum_type).unwrap_or_else(|| panic!("enum class {enum_type}"));
                    let n = enum_ids.len() as u16;
                    enum_ids.entry(ty.clone()).or_insert(n).to_string()
                } else {
                    "NONE".to_string()
                };
                writeln!(attrs, "    Attr {{ ns: {ns}, local: {local:?}, value: ValueKind::{kind}, enum_id: {enum_id}, validators: ({vs}, {vn}) }},").unwrap();
                nattrs += 1;
            }
        }
        // Children the element knows (its factory), with their type ids.
        let cstart = nchildren;
        let mut seen = std::collections::HashSet::new();
        // The SDK's generator registers the listed children plus every
        // element its particle names (SdtBlock's sdtPr is only there).
        fn particle_names<'j>(p: &'j serde_json::Value, out: &mut Vec<&'j str>) {
            if p["Kind"].as_str().unwrap_or("Element") == "Element" && let Some(n) = p["Name"].as_str() {
                out.push(n);
            }
            for item in p["Items"].as_array().into_iter().flatten() {
                particle_names(item, out);
            }
        }
        for &c in &chain {
            let t = &types[&names[c]];
            let mut listed: Vec<&str> = t["Children"].as_array().into_iter().flatten()
                .chain(t["AdditionalElements"].as_array().into_iter().flatten())
                .map(|k| k["Name"].as_str().or(k.as_str()).unwrap())
                .collect();
            if let Some(p) = t.get("Particle") {
                particle_names(p, &mut listed);
            }
            for name in listed {
                let Some(&kid) = id.get(name) else { continue };
                let local = name.split_once('/').unwrap().1;
                if local.is_empty() || !seen.insert(local.to_string()) {
                    continue;
                }
                let (ns, l) = out.qname(local);
                writeln!(children, "    Child {{ ns: {ns}, local: {l:?}, ty: {kid} }},").unwrap();
                nchildren += 1;
            }
        }
        let particle = chain.iter().find_map(|&c| types[&names[c]].get("Particle"))
            .map(|p| build(p, &id, &mut out, &mut rows))
            .and_then(|ids| ids.last().copied())
            .map_or("NONE32".to_string(), |x| x.to_string());
        let (tvs, tvn) = emit_validators(chain.iter().find_map(|&c| types[&names[c]].get("Validators")), &mut out, &mut enum_ids, &mut patterns);
        let (ns, local) = match n.split_once('/').map(|x| x.1).filter(|x| !x.is_empty()) {
            Some(q) => out.qname(q),
            None => (0, String::new()),
        };
        let leaf = if chain.iter().any(|&c| types[&names[c]]["IsLeafText"].as_bool() == Some(true)) {
            2
        } else if chain.iter().any(|&c| types[&names[c]]["IsLeafElement"].as_bool() == Some(true)) {
            1
        } else {
            0
        };
        let cstart2 = nconstraints;
        for &c in &chain {
            for row in constraints_of.get(&names[c]).into_iter().flatten() {
                writeln!(constraint_rows, "{row}").unwrap();
                nconstraints += 1;
            }
        }
        let composite = chain.iter().find_map(|&c| types[&names[c]]["CompositeType"].as_str()).unwrap_or("");
        writeln!(type_rows,
            "    Type {{ name: {n:?}, ns: {ns}, local: {local:?}, leaf: {leaf}, composite: {composite:?}, part: {:?}, particle: {particle}, attrs: ({astart}, {}), children: ({cstart}, {}), validators: ({tvs}, {tvn}), constraints: ({cstart2}, {}), version: {} }},",
            t["Part"].as_str().unwrap_or(""), nattrs - astart, nchildren - cstart, nconstraints - cstart2, version(t.get("Version")),
        ).unwrap();
    }
    for r in &rows {
        writeln!(particles, "    {r}").unwrap();
        nparticles += 1;
    }

    writeln!(s, "pub static NAMESPACES: [&str; {}] = {:?};\n", out.ns.len(), out.ns).unwrap();
    let versions: Vec<u8> = out.ns.iter().map(|u| ns_version.get(u).map_or(u8::MAX, |&v| v as u8)).collect();
    writeln!(s, "/// The version that introduced each namespace (u8::MAX: not a known namespace).\npub static NAMESPACE_VERSIONS: [u8; {}] = {:?};\n", versions.len(), versions).unwrap();
    writeln!(s, "pub const TARGET_VERSION: u8 = {TARGET};\n").unwrap();
    writeln!(s, "pub static TYPES: [Type; {}] = [\n{type_rows}];\n", names.len()).unwrap();
    writeln!(s, "pub static ATTRS: [Attr; {nattrs}] = [\n{attrs}];\n").unwrap();
    writeln!(s, "pub static CHILDREN: [Child; {nchildren}] = [\n{children}];\n").unwrap();
    writeln!(s, "pub static CONSTRAINTS: [Constraint; {nconstraints}] = [\n{constraint_rows}];\n").unwrap();
    // Parts (data/parts/*.json): the package's tree of part classes by
    // relationship type, each with its root element type (inherited from its
    // Base, as StyleDefinitionsPart from StylesPart).
    {
        let mut parts: BTreeMap<String, Value> = BTreeMap::new();
        for e in std::fs::read_dir(data.join("parts")).unwrap() {
            let v: Value = serde_json::from_str(&std::fs::read_to_string(e.unwrap().path()).unwrap()).unwrap();
            parts.insert(v["Name"].as_str().unwrap().to_string(), v);
        }
        let names: Vec<&String> = parts.keys().collect();
        let index = |n: &str| names.iter().position(|x| x.as_str() == n);
        let mut table = String::new();
        let mut kids = String::new();
        let mut nkids = 0;
        for n in &names {
            let mut p = &parts[n.as_str()];
            let mut chain = vec![p];
            while let Some(b) = p["Base"].as_str().and_then(|b| parts.get(b)) {
                chain.push(b);
                p = b;
            }
            let field = |k: &str| chain.iter().find_map(|c| c[k].as_str());
            // Hand-written bases (Packaging/StylesPart.cs, CustomUIPart.cs) name their roots in C#.
            let handwritten = chain.iter().find_map(|c| match c["Base"].as_str() {
                Some("StylesPart") => Some("w:CT_Styles"),
                Some("CustomUIPart") => Some("mso:CT_CustomUI"),
                _ => None,
            });
            let root = match (field("Root").or(handwritten), field("RootElement")) {
                (Some(r), Some(e)) => {
                    let head = format!("{r}/");
                    let tail = format!(":{e}");
                    let found: Vec<_> = id.iter().filter(|(k, _)| k.starts_with(&head) && k.ends_with(&tail)).collect();
                    let same = format!("{head}{}{tail}", r.split(':').next().unwrap());
                    let pick = found.iter().find(|(k, _)| **k == same).or(found.first()).unwrap_or_else(|| panic!("part root {r} {e}"));
                    pick.1.to_string()
                }
                _ => "NONE".to_string(),
            };
            let start = nkids;
            for c in parts[n.as_str()]["Children"].as_array().into_iter().flatten() {
                if let Some(i) = index(c["Name"].as_str().unwrap()) {
                    writeln!(kids, "    PartChild {{ class: {i}, many: {}, required: {} }},",
                        c["MaxOccursGreatThanOne"].as_bool().unwrap_or(false), c["MinOccursIsNonZero"].as_bool().unwrap_or(false)).unwrap();
                    nkids += 1;
                }
            }
            writeln!(table, "    PartClass {{ name: {n:?}, rel: {:?}, content_type: {:?}, root: {root}, version: {}, children: ({start}, {}) }},",
                parts[n.as_str()]["RelationshipType"].as_str().unwrap_or(""), field("ContentType").unwrap_or(""), version(parts[n.as_str()].get("Version")), nkids - start).unwrap();
        }
        writeln!(s, "pub static PARTS: [PartClass; {}] = [\n{table}];\n", names.len()).unwrap();
        writeln!(s, "pub static PART_CHILDREN: [PartChild; {nkids}] = [\n{kids}];\n").unwrap();
    }
    writeln!(s, "pub static PARTICLES: [Particle; {nparticles}] = [\n{particles}];\n").unwrap();
    writeln!(s, "pub static VALIDATORS: [Validator; {nvalidators}] = [\n{validators}];\n").unwrap();
    writeln!(s, "pub static PATTERNS: [&str; {}] = {:?};\n", patterns.len(), patterns).unwrap();
    let mut enum_rows = vec![String::new(); enum_ids.len()];
    for (name, &i) in &enum_ids {
        let values: Vec<&str> = enums[name].iter().filter(|(_, v)| at_least(*v)).map(|(x, _)| x.as_str()).collect();
        enum_rows[i as usize] = format!("    ({name:?}, &{values:?}),");
    }
    writeln!(s, "pub static ENUMS: [(&str, &[&str]); {}] = [\n{}\n];", enum_rows.len(), enum_rows.join("\n")).unwrap();
    // Strict (ISO 29500) documents: the SDK reads them as Transitional. Its
    // namespace resolver's pair tables, and the Word classes that also map
    // attribute values (StrictTranslateAttribute).
    let resolver = std::fs::read_to_string(sdk.join("src/DocumentFormat.OpenXml.Framework/Features/OpenXmlNamespaceResolver.cs")).unwrap();
    let pairs = |text: &str| -> Vec<(String, String)> {
        let mut v = vec![];
        let mut rest = text;
        while let Some(i) = rest.find("{ \"") {
            let (a, r) = cs_string(rest[i + 2..].trim_start());
            let (b, r) = cs_string(r.trim_start().trim_start_matches(',').trim_start());
            v.push((a, b));
            rest = r;
        }
        v
    };
    for (table, start) in [("EXTENDED_NAMESPACES", "_extendedNamespaces = new()"), ("STRICT_NAMESPACES", "_strictTransitionalNamespaces = new()"), ("STRICT_RELATIONSHIPS", "_strictTransitionalRelationshipPairs = new()")] {
        let from = resolver.find(start).unwrap_or_else(|| panic!("{start}"));
        let body = &resolver[from..from + resolver[from..].find("};").unwrap()];
        let v = pairs(body);
        assert!(!v.is_empty(), "{table}");
        writeln!(s, "pub static {table}: [(&str, &str); {}] = {v:?};\n", v.len()).unwrap();
    }
    // Hand-written classes with their own metadata: only CustomXmlElement,
    // ported above. Another one in an SDK release stops generation.
    let mut handwritten = vec![];
    for f in std::fs::read_dir(sdk.join("src/DocumentFormat.OpenXml/Schema")).unwrap().flat_map(|d| std::fs::read_dir(d.unwrap().path()).into_iter().flatten()) {
        let path = f.unwrap().path();
        if std::fs::read_to_string(&path).unwrap_or_default().contains("void ConfigureMetadata(") {
            handwritten.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert_eq!(handwritten, ["CustomXmlElement.cs"], "hand-written element metadata changed in the SDK: port it in gen");
    // Classes that rewrite their attributes when the package is Strict. Their
    // logic is ported by hand in src/strict.rs; a new or removed one in an SDK
    // release stops generation, so the port can't drift silently.
    const STRICT_CLASSES: [&str; 9] = ["ConditionalFormatStyle", "Document", "Indentation", "Justification", "StylePaneSortMethods", "TabStop", "TableJustification", "TableLook", "TextDirection"];
    let mut found = vec![];
    for f in std::fs::read_dir(sdk.join("src/DocumentFormat.OpenXml/Schema/Wordprocessing")).unwrap() {
        let text = std::fs::read_to_string(f.unwrap().path()).unwrap();
        if text.contains("override bool StrictTranslateAttribute") {
            found.push(text.split("public partial class ").nth(1).unwrap().split_whitespace().next().unwrap().to_string());
        }
    }
    found.sort();
    assert_eq!(found, STRICT_CLASSES, "the SDK's Strict attribute translations changed: update src/strict.rs and STRICT_CLASSES");
    let mut strict_types = vec![];
    for ((stem, class), tys) in &file_class {
        if stem.contains("wordprocessingml_2006_main") && let Some(k) = STRICT_CLASSES.iter().position(|c| c == class) {
            strict_types.extend(tys.iter().map(|t| format!("    ({t:?}, {k}),")));
        }
    }
    strict_types.sort();
    writeln!(s, "/// (type name, index in strict::CLASSES)\npub static STRICT_TYPES: [(&str, u8); {}] = [\n{}\n];", strict_types.len(), strict_types.join("\n")).unwrap();
    std::fs::write(&args[2], s).unwrap();
    eprintln!("{} types, {nattrs} attrs, {nchildren} children, {nparticles} particles, {nvalidators} validators, {} patterns, {} enums",
        names.len(), patterns.len(), enum_ids.len());
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T> Pipe for T {}

enum Arg {
    Q(String),
    S(String),
    B(bool),
    N(f64),
    Null,
    A(Vec<String>),
}

/// A C# string literal at the start of `s` ("..." or @"..."): its value and the rest.
fn cs_string(s: &str) -> (String, &str) {
    let verbatim = s.starts_with('@');
    let body = &s[if verbatim { 2 } else { 1 }..];
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' if verbatim && body[i + 1..].starts_with('"') => {
                out.push('"');
                chars.next();
            }
            '"' => return (out, &body[i + 1..]),
            '\\' if !verbatim => {
                let (_, e) = chars.next().unwrap();
                out.push(match e {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
            }
            c => out.push(c),
        }
    }
    panic!("unterminated string")
}

/// A constraint constructor's arguments, up to its closing parenthesis; and
/// the rest of the line (the object initializer).
fn constraint_args(mut s: &str) -> (Vec<Arg>, &str) {
    let mut args = vec![];
    loop {
        s = s.trim_start_matches([' ', ',']);
        if let Some(r) = s.strip_prefix(')') {
            return (args, r);
        } else if let Some(r) = s.strip_prefix("builder.CreateQName(") {
            let (q, r) = cs_string(r);
            args.push(Arg::Q(q));
            s = r.strip_prefix(')').unwrap();
        } else if s.starts_with('"') || s.starts_with("@\"") {
            let (x, r) = cs_string(s);
            args.push(Arg::S(x));
            s = r;
        } else if let Some(mut r) = s.strip_prefix("new string[] {") {
            let mut v = vec![];
            loop {
                r = r.trim_start_matches([' ', ',']);
                if let Some(rr) = r.strip_prefix('}') {
                    r = rr;
                    break;
                }
                let (x, rr) = cs_string(r);
                v.push(x);
                r = rr;
            }
            args.push(Arg::A(v));
            s = r;
        } else if let Some(r) = s.strip_prefix("true") {
            args.push(Arg::B(true));
            s = r;
        } else if let Some(r) = s.strip_prefix("false") {
            args.push(Arg::B(false));
            s = r;
        } else if let Some(r) = s.strip_prefix("null") {
            args.push(Arg::Null);
            s = r;
        } else if let Some(r) = s.strip_prefix("double.NegativeInfinity") {
            args.push(Arg::N(f64::NEG_INFINITY));
            s = r;
        } else if let Some(r) = s.strip_prefix("double.PositiveInfinity") {
            args.push(Arg::N(f64::INFINITY));
            s = r;
        } else {
            let end = s.find([',', ')']).unwrap();
            let lit = s[..end].trim();
            let lit = match lit {
                "int.MaxValue" => "2147483647",
                "int.MinValue" => "-2147483648",
                "long.MaxValue" => "9223372036854775807",
                x => x,
            };
            let n: f64 = lit.trim_end_matches(['L', 'D', 'd']).parse().unwrap_or_else(|_| panic!("argument {}", &s[..end]));
            args.push(Arg::N(n));
            s = &s[end..];
        }
    }
}
