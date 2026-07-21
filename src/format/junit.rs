use crate::format::Formatter;
use crate::lint::LintResult;

pub struct JunitFormatter;

impl Default for JunitFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl JunitFormatter {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Formatter for JunitFormatter {
    fn format(&self, _result: &LintResult) -> String {
        todo!("Implement JUnit formatter")
    }
}
