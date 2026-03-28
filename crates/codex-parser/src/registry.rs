use std::collections::HashMap;

use crate::language::LanguageAdapter;

/// Maps file extensions to language adapters.
///
/// Rust's `Box<dyn LanguageAdapter>` is a *trait object* — a fat pointer
/// (data ptr + vtable ptr) that gives you dynamic dispatch without knowing
/// the concrete type at compile time.  In Scala this is just a value typed
/// as the trait: `val adapter: LanguageAdapter = new RustAdapter`.
///
/// The `Vec` owns the adapters; the `HashMap` stores indices into it rather
/// than duplicate `Box`es, keeping ownership unambiguous.
pub struct ParserRegistry {
    adapters: Vec<Box<dyn LanguageAdapter>>,
    /// extension (no dot) → index into `adapters`
    extension_map: HashMap<String, usize>,
}

impl ParserRegistry {
    /// Create an empty registry with no adapters registered.
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
            extension_map: HashMap::new(),
        }
    }

    /// Register a language adapter and claim all its file extensions.
    ///
    /// If two adapters claim the same extension the last one wins — useful
    /// for overriding a built-in with a custom implementation.
    pub fn register(&mut self, adapter: Box<dyn LanguageAdapter>) {
        let index = self.adapters.len();
        for ext in adapter.file_extensions() {
            self.extension_map.insert(ext.to_string(), index);
        }
        self.adapters.push(adapter);
    }

    /// Look up an adapter by file extension (without the leading dot).
    ///
    /// Returns `None` when no adapter is registered for that extension —
    /// callers should convert this to `ParserError::UnsupportedLanguage`.
    ///
    /// The return type `Option<&dyn LanguageAdapter>` is Rust's way of saying
    /// "a borrowed reference to some value that implements LanguageAdapter" —
    /// equivalent to `Option[LanguageAdapter]` in Scala where the value is a
    /// shared reference, not a copy.
    pub fn get_adapter(&self, extension: &str) -> Option<&dyn LanguageAdapter> {
        self.extension_map
            .get(extension)
            .map(|&idx| self.adapters[idx].as_ref())
    }

    /// Return the language IDs of all registered adapters (in registration order).
    pub fn languages(&self) -> Vec<&str> {
        self.adapters.iter().map(|a| a.language_id()).collect()
    }

    /// Build a registry pre-loaded with all built-in language adapters.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(crate::languages::rust::RustAdapter));
        registry
    }
}

/// `Default` delegates to `with_defaults` so that `ParserRegistry::default()`
/// gives you a fully-configured registry — analogous to a Scala companion
/// `object`'s `apply()` factory method.
impl Default for ParserRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}
