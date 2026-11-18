//! Shapes of the generated schema tables (`schema_gen.rs`).

pub const NONE: u16 = u16::MAX;
pub const NONE32: u32 = u32::MAX;

/// An element type. `attrs`, `children` and `validators` are (start, len)
/// ranges into ATTRS, CHILDREN and VALIDATORS; base types are flattened in.
pub struct Type {
    pub name: &'static str,
    pub ns: u16,
    pub local: &'static str,
    /// 0 composite, 1 leaf (no children), 2 leaf text (text content).
    pub leaf: u8,
    pub composite: &'static str,
    pub part: &'static str,
    pub particle: u32,
    pub attrs: (u32, u16),
    pub children: (u32, u16),
    pub validators: (u32, u16),
    /// Semantic constraints, as CONSTRAINTS ranges.
    pub constraints: (u32, u16),
    pub version: u8,
}

pub struct Attr {
    pub ns: u16,
    pub local: &'static str,
    pub value: ValueKind,
    pub enum_id: u16,
    pub validators: (u32, u16),
}

pub struct PartClass {
    pub name: &'static str,
    pub rel: &'static str,
    /// The content type a part of this class must have ("" when any).
    pub content_type: &'static str,
    /// Root element type, or NONE for a part with no XML root (images).
    pub root: u16,
    pub version: u8,
    /// Child part classes, as PART_CHILDREN ranges.
    pub children: (u16, u16),
    /// Data parts its relationships may reference (IKnownDataPartFeature):
    /// audio 1, video 2, media 4.
    pub data: u8,
}

/// A part class a container may hold (its IPartConstraintFeature rule).
pub struct PartChild {
    pub class: u16,
    /// MaxOccursGreatThanOne.
    pub many: bool,
    /// MinOccursIsNonZero.
    pub required: bool,
}

pub struct Child {
    pub ns: u16,
    pub local: &'static str,
    pub ty: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParticleKind {
    Element,
    Sequence,
    Choice,
    All,
    Group,
    Any,
}

pub struct Particle {
    pub kind: ParticleKind,
    pub min: u32,
    /// 0 is unbounded.
    pub max: u32,
    pub element: u16,
    /// Any: 0 ##any, 1 ##other, 2 ##local, 3 ##targetNamespace.
    pub any: u8,
    pub version: u8,
    pub items: &'static [u32],
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValidatorKind {
    String,
    Number,
    Required,
    Enum,
    OfficeVersion,
}

pub struct Validator {
    pub kind: ValidatorKind,
    /// Union members share an id; -1 is not a union member.
    pub union: i8,
    pub xsd: &'static str,
    /// The value class the text is re-parsed as before this validator runs
    /// (SimpleTypeValidator<T>), when the attribute's own class is another.
    pub value: Option<ValueKind>,
    /// The enum, when `value` is EnumValue.
    pub enum_id: u16,
    pub min_inclusive: Option<i64>,
    pub max_inclusive: Option<i64>,
    pub min_exclusive: Option<i64>,
    pub max_exclusive: Option<i64>,
    pub min_length: Option<i64>,
    pub max_length: Option<i64>,
    pub length: Option<i64>,
    pub pattern: u16,
    pub token: bool,
    pub ncname: bool,
    pub qname: bool,
    pub id: bool,
    pub uri: bool,
    pub positive: bool,
    pub non_negative: bool,
    pub list: bool,
}

/// The SDK's attribute value classes (OpenXmlSimpleType subclasses).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueKind {
    StringValue,
    BooleanValue,
    EnumValue,
    UInt32Value,
    Int32Value,
    TrueFalseValue,
    IntegerValue,
    Int64Value,
    HexBinaryValue,
    OnOffValue,
    DoubleValue,
    DateTimeValue,
    ListValue,
    ByteValue,
    Base64BinaryValue,
    UInt16Value,
    UInt64Value,
    DecimalValue,
    SingleValue,
    TrueFalseBlankValue,
    Int16Value,
    SByteValue,
}

/// A semantic constraint argument, as its C# constructor took it.
#[derive(Debug)]
pub enum Arg {
    /// A qualified name: namespace id (0 when none) and local name.
    Q(u16, &'static str),
    S(&'static str),
    B(bool),
    N(f64),
    Null,
    A(&'static [&'static str]),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConstraintKind {
    AttributeValueRange,
    AttributeValueLength,
    UniqueAttributeValue,
    AttributeValueSet,
    IndexReference,
    RelationshipExist,
    RelationshipType,
    ReferenceExist,
    AttributeMutualExclusive,
    AttributeValuePattern,
    AttributeValueConditionToAnother,
    AttributeAbsentConditionToNonValue,
    AttributeRequiredConditionToValue,
    AttributeCannotOmit,
    AttributeAbsentConditionToValue,
    AttributeValueLessEqualToAnother,
}

pub struct Constraint {
    pub kind: ConstraintKind,
    /// The applications it runs for (ApplicationType): Word 1, Excel 2, PowerPoint 4.
    pub apps: u8,
    pub args: &'static [Arg],
}
