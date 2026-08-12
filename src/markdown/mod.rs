mod front_matter;
mod parser;

pub use front_matter::{FrontMatter, FrontMatterType, detect_front_matter, extract_title};
pub use parser::MarkdownParser;
