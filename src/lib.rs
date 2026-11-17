//! A streaming OOXML validator that gives the Open XML SDK's verdicts.

pub mod names;
pub mod package;
pub mod particle;
pub mod schema;
pub mod semantic;
#[allow(clippy::all)]
pub mod schema_gen;
pub mod validate;
pub mod values;
pub mod xml;
