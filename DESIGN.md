# How ooxml-validator works

Goal: give the same verdict as Microsoft's Open XML SDK validator
(`OpenXmlValidator(FileFormatVersions.Microsoft365)`, DocumentFormat.OpenXml
3.5.1) on WordprocessingML packages, an order of magnitude faster, with no .NET.

## Why

Profiled on a 166-page document (6.9 MB `document.xml`), the SDK spends 38% of
`Validate` building its DOM and 34% checking attributes: each union member that
fails builds a localized error string, then discards it. ooxml-validator reads
each part once as a token stream and never builds a tree.

## Shape

- `gen/`: reads the SDK's MIT-licensed schema data (`data/schemas/*.json`,
  `data/namespaces.json`) and its generated constraint registrations
  (`generated/**/*.g.cs`, `builder.AddConstraint(...)`), and writes
  `src/schema_gen.rs`: static tables addressed by `u16`/`u32` ids. Nothing
  from the SDK is read at run time.
- `src/`: zip reader, a borrowed-bytes XML tokenizer (memchr scans to markup;
  values are slices of the input, unescaped only when `&` occurs), and a
  validator that walks the token stream once with an element stack — no DOM.
- Output: one JSON line per file with the verdict and error ids; the same
  ids the SDK reports.

## Faithfulness

The SDK is the oracle. Port its algorithms, not the XSD spec:
particle validators (greedy Sequence/Choice/All/Any/Group, as the SDK runs them),
version-gated validators and enum members, union semantics, simple-type
parsing (`Int32Value`, `OnOffValue`, `HexBinaryValue`, ...), markup
compatibility (Ignorable, AlternateContent, ProcessContent), unknown elements,
semantic constraints (with Application = Word filtering), package validation.

Checked by differential runs against the SDK (`fuzz/oracle`, `fuzz/diff.py`)
on real-world documents and on mutants of them (`fuzz/mutate.py`: delete,
duplicate, swap, move, rename elements; drop, add, perturb attributes; wrap in
AlternateContent or ignorable elements). The invariant: ooxml-validator never accepts a
package the SDK rejects. Error id sets should match too.

## SDK semantics confirmed from source (v3.5.1)

Versions (generator resolves all of these for Microsoft365):
- Validator gating (VersionedValidator.IsValid): IsInitialVersion → AtLeast(Version);
  Version alone → exactly that version; neither → always.
- Particle Occurs: no key → (1,1); missing Min/Max → 0; Max 0 = unbounded. Several
  entries = per-version variants; CompositeParticle.Build(version) keeps all whose
  version ≤ target, and Builder.Add drops an earlier entry for the same element type
  (or, under RequireFilter, any earlier group) when a later-version one arrives.

Attributes (SchemaTypeValidator.ValidateAttributes / ValidateValue):
- Every *declared* attribute runs its validators, present or not; validators run in
  order and stop at the first that adds an error (one error per attribute).
- Each attribute's list = explicit validators, a UnionValidator per union group
  (AddUnion; members' errors swallowed, Sch_AttributeUnionFailedEx if none passes),
  then a default by value class: StringValue (and its subclasses HexBinaryValue,
  Base64BinaryValue) → StringValidator; EnumValue, OnOffValue, TrueFalseBlankValue →
  EnumValidator; ListValue → ListValidator; everything else → NumberValidator.
- NumberValidator: error if !Value.IsValid; range checks only for numeric classes
  (TryGetValue fails for StringValue, so a Number member of a union on a StringValue
  always passes).
- StringValidator applies only when the value is a StringValue (or list of them);
  Length/MinLength/MaxLength count UTF-16 code units; Pattern is `\A(p)\z`.
- Undeclared attribute → Sch_UndeclaredAttribute unless its namespace is ignorable
  (MC) or xml:. MC attributes are not "extended attributes".
- Simple value parsing: XmlConvert (trimmed; Int32 allows a sign; Boolean
  true/false/1/0; Double allows INF/-INF/NaN and exponents). OnOff: true/1/on/
  false/0/off exactly. TrueFalse: true/t/false/f. TrueFalseBlank adds "". HexBinary:
  even-length hex. Empty text is no value for OpenXmlSimpleValue (IsValid false).

Children (CompositeParticleValidator and friends):
- Matching runs over the element's MC-flattened child list (GetFirstChildMc/
  GetNextChildMc): AlternateContent → its selected branch's children; an element
  in an ignorable namespace → skipped, or its children under ProcessContent;
  strong-typed elements (known to the parent's factory) are children even in an
  ignorable namespace. Unknown elements (InitialVersion None) go through MC checks.
- Validate: no children → error only if particle.min > 0 and required elements
  exist (Sch_IncompleteContentExpectingComplex). Else TryMatch root; Nomatch or
  Partial → EmitInvalidElementError; Matched with a child left → error. One error
  per element. Error id: child known to parent → Sch_UnexpectedElementContent
  ExpectingComplex, else Sch_InvalidElementContentExpectingComplex; All has
  Sch_AllElement for a repeated member.
- Element/Sequence/Choice/Group/All/Any TryMatch, TryMatchOnce, GetRequired-
  Elements as in the C# (ElementParticle.TryMatchOnce has an inverted test; it is
  unreachable through TryMatch).
- Leaf element with non-misc children → Sch_InvalidChildinLeafElement (one).
  Composite without particle (some part roots) the same. Leaf text: its text is
  validated with the type-level validators (isAttribute false: Sch_Element...).
- Traversal (ValidatingTraverse): strong-typed elements are validated; unknown
  elements are not, and their children only under ProcessContent.

## Loading the package (what the SDK throws before validating)

Any of these makes the SDK fail to open the document, and ooxml-validator
reports an open error instead of a list of errors:
- XmlReader (the wrapper's prepass, every .xml/.rels entry): not well formed,
  an undeclared prefix, an attribute twice by namespace and name, an
  `xml:space` other than default/preserve, an encoding .NET lacks. Parts are
  decoded as XmlReader decodes them: byte order mark, first bytes, declaration.
- System.IO.Packaging: a .rels without a `Relationships` root, with another
  element, or a relationship without Id/Type/Target, a non-NCName or repeated
  Id, a TargetMode other than Internal/External; `[Content_Types].xml`
  declaring a Default or Override twice. Overrides match part names with case;
  parts are found without case, percent-decoded.
- LoadReferencedPartsAndRelationships: an internal target that doesn't exist
  (hyperlinks and "NULL" targets load nothing; media, audio and video must
  exist); a part with a fixed content type and another one declared; one part
  reached by two relationship types; a part root that isn't the class's root.

## Strict documents

A Strict relationship type anywhere the package loads makes it Strict: then
namespaces (and attribute values naming them) read as Transitional, and nine
Word classes rewrite attributes (`src/strict.rs`). Without one, the extended
Word namespaces (`.../2006/3/main` and the like) read as Transitional instead.
gen stops if the SDK's set of translating classes changes.

## Staying in agreement

`fuzz/agree.sh VERSION` clones the SDK at that tag, regenerates the tables,
builds the oracle against that NuGet release, and diffs every verdict on the
SDK's own test documents, mutants of them, `fuzz/cases.py`'s problem
documents and a generated report. CI runs it on every push against
`SDK_VERSION`; weekly it fuzzes with a fresh seed and tries the newest release,
opening a pull request when it agrees and an issue when it doesn't.

## Speed

What made the difference, in order of effect (each kept only if paired A/B
runs showed it, and only with every document still agreeing):
- No allocation per element or attribute: values are borrowed slices,
  unescaped only when they contain `&`; scratch vectors are reused.
- Namespace ids resolved once, when a prefix is declared; prefixes compared as
  packed integers; element and attribute names interned to ids.
- Each zip entry inflated once (zlib-rs), parts validated on up to four
  threads, largest first.
- The UTF-8 and control-character check done once per part, not per token.
