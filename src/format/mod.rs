mod default;
mod gitlab;
mod json;
mod junit;
mod sarif;

pub use default::DefaultFormatter;
pub use gitlab::GitlabFormatter;
pub use json::JsonFormatter;
pub use junit::JunitFormatter;
pub use sarif::SarifFormatter;

use crate::lint::LintResult;

pub trait Formatter {
    fn format(&self, result: &LintResult) -> String;
    fn supports_color(&self) -> bool {
        false
    }
}
