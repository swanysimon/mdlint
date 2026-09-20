mod engine;
mod result;
mod rule;
pub mod rules;

pub use engine::LintEngine;
// Shared with the formatter: reflow honours `<!-- mdlint-disable MD013 -->` too.
pub(crate) use engine::parse_inline_config;
pub use result::LintResult;
pub use rule::{Rule, RuleRegistry};
