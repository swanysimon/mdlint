pub mod loader;
mod merge;
mod types;

pub use loader::ConfigLoader;
pub use merge::{merge_configs, merge_many_configs, merge_rule_configs};
pub use types::{Config, RuleConfig};
