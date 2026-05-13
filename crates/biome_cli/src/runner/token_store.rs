use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Represents a single tokenized file with its tokens and byte offsets.
#[derive(Clone, Debug)]
pub struct TokenizedFile {
    pub path: String,
    pub tokens: Vec<String>,
    pub byte_offsets: Vec<usize>,
}

/// Thread-safe store of tokenized files across the entire crawl.
#[derive(Clone)]
pub struct TokenStore {
    files: Arc<Mutex<HashMap<String, TokenizedFile>>>,
}

impl TokenStore {
    /// Create a new token store.
    pub fn new() -> Self {
        Self {
            files: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a tokenized file to the store.
    pub fn add_file(&self, file: TokenizedFile) {
        let mut files = self.files.lock().unwrap();
        files.insert(file.path.clone(), file);
    }

    /// Get all tokenized files from the store.
    pub fn get_all_files(&self) -> Vec<TokenizedFile> {
        let files = self.files.lock().unwrap();
        files.values().cloned().collect()
    }

    /// Get a specific tokenized file by path.
    pub fn get_file(&self, path: &str) -> Option<TokenizedFile> {
        let files = self.files.lock().unwrap();
        files.get(path).cloned()
    }
}

impl Default for TokenStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-local storage for the TokenStore during a crawl session.
thread_local! {
    static CROSS_FILE_TOKEN_STORE: std::cell::RefCell<Option<TokenStore>> = std::cell::RefCell::new(None);
}

/// Get or create the thread-local cross-file token store.
pub fn get_cross_file_token_store() -> TokenStore {
    CROSS_FILE_TOKEN_STORE.with(|store| {
        let mut store_ref = store.borrow_mut();
        if let Some(token_store) = &*store_ref {
            token_store.clone()
        } else {
            let token_store = TokenStore::new();
            *store_ref = Some(token_store.clone());
            token_store
        }
    })
}

/// Set the thread-local cross-file token store.
pub fn set_cross_file_token_store(token_store: TokenStore) {
    CROSS_FILE_TOKEN_STORE.with(|store| {
        *store.borrow_mut() = Some(token_store);
    });
}

/// Clear the thread-local cross-file token store.
pub fn clear_cross_file_token_store() {
    CROSS_FILE_TOKEN_STORE.with(|store| {
        *store.borrow_mut() = None;
    });
}
