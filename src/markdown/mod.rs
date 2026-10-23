mod front_matter;
mod parser;

pub use front_matter::{FrontMatter, FrontMatterType, detect_front_matter};
pub use parser::MarkdownParser;
