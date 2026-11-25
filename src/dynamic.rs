//! Optionally support dynamically loaded languages.
//! Enabled by `feature = "dyn"`.

/// This struct is not constructible and only exists
/// to silence a 'unused lifetime parameter' error.
#[cfg(not(feature = "dyn"))]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub(crate) struct Absurd<'a>(core::marker::PhantomData<&'a ()>);

#[cfg(feature = "dyn")]
/// Dynamically loaded patterns.
pub(crate) struct Dyn {
    /// (left,right)-hyphenmin of the language.
    pub bounds: (usize, usize),
    /// Raw encoding of the automata.
    pub trie: &'a [u8],
}

