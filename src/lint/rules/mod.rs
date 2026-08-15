mod md001;
mod md003;
mod md004;
mod md005;
mod md006;
mod md007;
mod md009;
mod md010;
mod md011;
mod md012;
mod md013;
mod md014;
mod md018;
mod md019;
mod md020;
mod md021;
mod md022;
mod md023;
mod md024;
mod md025;
mod md026;
mod md027;
mod md028;
mod md029;
mod md030;
mod md031;
mod md032;
mod md033;
mod md034;
mod md035;
mod md036;
mod md037;
mod md038;
mod md039;
mod md040;
mod md041;
mod md042;
mod md043;
mod md044;
mod md045;
mod md046;
mod md047;
mod md048;
mod md049;
mod md050;
mod md051;
mod md052;
mod md053;
mod md054;
mod md055;
mod md056;
mod md058;
mod md059;
mod md060;

pub use md001::MD001;
pub use md003::MD003;
pub use md004::MD004;
pub use md005::MD005;
pub use md006::MD006;
pub use md007::MD007;
pub use md009::MD009;
pub use md010::MD010;
pub use md011::MD011;
pub use md012::MD012;
pub use md013::MD013;
pub use md014::MD014;
pub use md018::MD018;
pub use md019::MD019;
pub use md020::MD020;
pub use md021::MD021;
pub use md022::MD022;
pub use md023::MD023;
pub use md024::MD024;
pub use md025::MD025;
pub use md026::MD026;
pub use md027::MD027;
pub use md028::MD028;
pub use md029::MD029;
pub use md030::MD030;
pub use md031::MD031;
pub use md032::MD032;
pub use md033::MD033;
pub use md034::MD034;
pub use md035::MD035;
pub use md036::MD036;
pub use md037::MD037;
pub use md038::MD038;
pub use md039::MD039;
pub use md040::MD040;
pub use md041::MD041;
pub use md042::MD042;
pub use md043::MD043;
pub use md044::MD044;
pub use md045::MD045;
pub use md046::MD046;
pub use md047::MD047;
pub use md048::MD048;
pub use md049::MD049;
pub use md050::MD050;
pub use md051::MD051;
pub use md052::MD052;
pub use md053::MD053;
pub use md054::MD054;
pub use md055::MD055;
pub use md056::MD056;
pub use md058::MD058;
pub use md059::MD059;
pub use md060::MD060;

use crate::lint::rule::RuleRegistry;

/// Create a registry with all built-in rules
#[must_use]
pub fn create_default_registry() -> RuleRegistry {
    let mut registry = RuleRegistry::new();

    registry.register(Box::new(MD001));
    registry.register(Box::new(MD003));
    registry.register(Box::new(MD004));
    registry.register(Box::new(MD005));
    // MD006 is deprecated and not enabled by default in markdownlint
    // registry.register(Box::new(MD006));
    registry.register(Box::new(MD007));
    registry.register(Box::new(MD009));
    registry.register(Box::new(MD010));
    registry.register(Box::new(MD011));
    registry.register(Box::new(MD012));
    registry.register(Box::new(MD013));
    registry.register(Box::new(MD014));
    registry.register(Box::new(MD018));
    registry.register(Box::new(MD019));
    registry.register(Box::new(MD020));
    registry.register(Box::new(MD021));
    registry.register(Box::new(MD022));
    registry.register(Box::new(MD023));
    registry.register(Box::new(MD024));
    registry.register(Box::new(MD025));
    registry.register(Box::new(MD026));
    registry.register(Box::new(MD027));
    registry.register(Box::new(MD028));
    registry.register(Box::new(MD029));
    registry.register(Box::new(MD030));
    registry.register(Box::new(MD031));
    registry.register(Box::new(MD032));
    registry.register(Box::new(MD033));
    registry.register(Box::new(MD034));
    registry.register(Box::new(MD035));
    registry.register(Box::new(MD036));
    registry.register(Box::new(MD037));
    registry.register(Box::new(MD038));
    registry.register(Box::new(MD039));
    registry.register(Box::new(MD040));
    registry.register(Box::new(MD041));
    registry.register(Box::new(MD042));
    registry.register(Box::new(MD043));
    registry.register(Box::new(MD044));
    registry.register(Box::new(MD045));
    registry.register(Box::new(MD046));
    registry.register(Box::new(MD047));
    registry.register(Box::new(MD048));
    registry.register(Box::new(MD049));
    registry.register(Box::new(MD050));
    registry.register(Box::new(MD051));
    registry.register(Box::new(MD052));
    registry.register(Box::new(MD053));
    registry.register(Box::new(MD054));
    registry.register(Box::new(MD055));
    registry.register(Box::new(MD056));
    registry.register(Box::new(MD058));
    registry.register(Box::new(MD059));
    registry.register(Box::new(MD060));

    registry
}

/// Renders violations exactly as `mdlint check` prints them, one line per
/// violation, so tests assert the diagnostic a user actually sees rather than
/// a test-only shape — a change to the displayed format shows up here too.
/// The path is a fixed stand-in; context lines and colour are disabled.
#[cfg(test)]
pub(crate) fn rendered(violations: &[crate::types::Violation]) -> Vec<String> {
    use crate::format::{DefaultFormatter, Formatter as _};

    let mut result = crate::lint::LintResult::new();
    result.add_file_result(
        std::path::PathBuf::from("test.md"),
        violations.to_vec(),
        Vec::new(),
    );
    DefaultFormatter::without_context(false)
        .format(&result)
        .lines()
        .filter_map(|line| line.strip_prefix("  ").map(str::to_owned))
        .collect()
}
