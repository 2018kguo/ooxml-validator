//! A streaming OOXML validator that gives the Open XML SDK's verdicts.
//!
//! ```no_run
//! let report = ooxml_validator::check(&std::fs::read("report.docx").unwrap());
//! for e in &report.errors {
//!     println!("{} {} {}: {}", e.part, e.node, e.id, e.description);
//! }
//! ```

pub use package::{Report, check};
pub use validate::ValidationError;

pub mod encoding;
pub mod names;
pub mod package;
pub mod strict;
pub mod particle;
pub mod schema;
pub mod semantic;
#[allow(clippy::all)]
pub mod schema_gen;
pub mod validate;
pub mod values;
pub mod xml;
pub mod xpath;
#[cfg(target_arch = "wasm32")]
pub mod wasm;
