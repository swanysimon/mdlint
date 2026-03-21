use lsp_types::Uri;
use std::collections::HashMap;

#[derive(Default)]
pub struct DocumentStore {
    docs: HashMap<Uri, String>,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self, uri: Uri, content: String) {
        self.docs.insert(uri, content);
    }

    pub fn update(&mut self, uri: &Uri, content: String) {
        self.docs.insert(uri.clone(), content);
    }

    pub fn close(&mut self, uri: &Uri) {
        self.docs.remove(uri);
    }

    pub fn get(&self, uri: &Uri) -> Option<&str> {
        self.docs.get(uri).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn test_uri() -> Uri {
        Uri::from_str("file:///tmp/test.md").unwrap()
    }

    #[test]
    fn test_open_and_get() {
        let mut store = DocumentStore::new();
        let uri = test_uri();
        store.open(uri.clone(), "# Hello".to_string());
        assert_eq!(store.get(&uri), Some("# Hello"));
    }

    #[test]
    fn test_update() {
        let mut store = DocumentStore::new();
        let uri = test_uri();
        store.open(uri.clone(), "# Hello".to_string());
        store.update(&uri, "# World".to_string());
        assert_eq!(store.get(&uri), Some("# World"));
    }

    #[test]
    fn test_close() {
        let mut store = DocumentStore::new();
        let uri = test_uri();
        store.open(uri.clone(), "# Hello".to_string());
        store.close(&uri);
        assert_eq!(store.get(&uri), None);
    }

    #[test]
    fn test_get_missing() {
        let store = DocumentStore::new();
        assert_eq!(store.get(&test_uri()), None);
    }
}
