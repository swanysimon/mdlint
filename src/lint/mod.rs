pub mod aliases;
mod engine;
mod result;
mod rule;
pub mod rules;

pub use engine::LintEngine;
pub use result::LintResult;
pub use rule::{Rule, RuleRegistry};
