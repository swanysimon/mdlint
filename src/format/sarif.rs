use crate::format::Formatter;
use crate::lint::LintResult;

pub struct SarifFormatter;

impl Default for SarifFormatter {
    fn default() -> Self {
        Self::new()
    }
}

impl SarifFormatter {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Formatter for SarifFormatter {
    fn format(&self, _result: &LintResult) -> String {
        todo!("Implement SARIF formatter")
    }
}
