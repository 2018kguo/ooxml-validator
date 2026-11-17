# ooxml-validator: a streaming OOXML validator

Goal: give the same verdict as Microsoft's Open XML SDK validator
(`OpenXmlValidator(FileFormatVersions.Microsoft365)`, DocumentFormat.OpenXml
3.5.1) on WordprocessingML packages, an order of magnitude faster, with no .NET.

## Why

On a 166-page document (6.9 MB `document.xml`) the SDK takes ~1.1 s inside
`Validate`: 38% building its DOM, 34% attribute checks (each union member that
fails builds a localized error string, then discards it). ooxml-validator gives the same
verdict for the whole package in 0.2 s and 19 MB (the SDK: ~1.9 s, 190 MB).

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
