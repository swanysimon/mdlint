#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum LogLevel {
    Silent,
    Quiet,
    #[default]
    Default,
    Verbose,
}
