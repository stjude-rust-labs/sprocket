//! Execution engine for Workflow Description Language (WDL) documents.

use std::borrow::Borrow;
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;

use lru::LruCache;
use num_enum::IntoPrimitive;
use sysinfo::CpuRefreshKind;
use sysinfo::MemoryRefreshKind;
use sysinfo::System;
use tokio::select;
use tokio::sync::OnceCell;
use wdl_analysis::Document;
use wdl_analysis::diagnostics::unknown_type;
use wdl_analysis::types::Type;
use wdl_analysis::types::TypeNameResolver;
use wdl_analysis::types::v1::AstTypeConverter;
use wdl_ast::Diagnostic;
use wdl_ast::Span;
use wdl_ast::TreeNode;

mod backend;
pub(crate) mod cache;
pub mod config;
mod diagnostics;
mod digest;
mod engine;
mod eval;
mod http;
mod inputs;
mod lock;
mod outputs;
mod path;
mod stdlib;
mod tree;
mod units;
mod value;

pub use config::Config;
pub use engine::*;
pub use eval::*;
pub use inputs::*;
pub use outputs::*;
pub use path::*;
use units::*;
pub use value::*;

use crate::cache::Hashable;

/// One gibibyte (GiB) as a float.
///
/// This is defined as a constant as it's a commonly performed conversion.
const ONE_GIBIBYTE: f64 = 1024.0 * 1024.0 * 1024.0;

/// The number of initial expected task names.
///
/// This controls the initial size of the bloom filter and how many names are
/// prepopulated into a name generator.
const INITIAL_EXPECTED_NAMES: usize = 1000;

/// Resolves a type name from a document.
///
/// This function will import the type into the type cache if not already
/// cached.
fn resolve_type_name(document: &Document, name: &str, span: Span) -> Result<Type, Diagnostic> {
    document
        .struct_by_name(name)
        .map(|s| s.ty().expect("struct should have type").clone())
        .or_else(|| {
            document
                .enum_by_name(name)
                .map(|e| e.ty().expect("enum should have type").clone())
        })
        .ok_or_else(|| unknown_type(name, span))
}

/// Converts a V1 AST type to an analysis type.
fn convert_ast_type_v1<N: TreeNode>(
    document: &Document,
    ty: &wdl_ast::v1::Type<N>,
) -> Result<Type, Diagnostic> {
    /// Used to resolve a type name from a document.
    struct Resolver<'a>(&'a Document);

    impl TypeNameResolver for Resolver<'_> {
        fn resolve(&mut self, name: &str, span: Span) -> Result<Type, Diagnostic> {
            resolve_type_name(self.0, name, span)
        }
    }

    AstTypeConverter::new(Resolver(document)).convert_type(ty)
}

/// Cached information about the host system.
static SYSTEM: LazyLock<System> = LazyLock::new(|| {
    let mut system = System::new();
    system.refresh_cpu_list(CpuRefreshKind::nothing());
    system.refresh_memory_specifics(MemoryRefreshKind::nothing().with_ram());
    system
});

/// Represents either file or directory content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoPrimitive)]
#[repr(u8)]
enum ContentKind {
    /// The content is a single file.
    File,
    /// The content is a directory.
    Directory,
    /// The content is a single temporary file.
    ///
    /// A digest for a temporary file should always be "strong" and the metadata
    /// of the file should be otherwise ignored.
    TempFile,
}

impl Hashable for ContentKind {
    fn hash(&self, hasher: &mut blake3::Hasher) {
        hasher.update(&[(*self).into()]);
    }
}

impl From<ContentKind> for crankshaft::engine::task::input::Type {
    fn from(value: ContentKind) -> Self {
        match value {
            ContentKind::File | ContentKind::TempFile => Self::File,
            ContentKind::Directory => Self::Directory,
        }
    }
}

/// Represents an LRU cache that supports asynchronous initialization of
/// entries.
struct Cache<K, V>(Mutex<LruCache<K, Arc<OnceCell<V>>>>);

impl<K, V> Cache<K, V>
where
    K: Hash + Eq,
{
    /// Constructs a new cache with the given capacity
    fn new(capacity: NonZeroUsize) -> Self {
        Self(Mutex::new(LruCache::new(capacity)))
    }

    /// Gets an entry from a cache with a reference to a key.
    ///
    /// If the entry already exists in the cache, the existing entry is cloned
    /// and returned.
    ///
    /// If the entry does not exist in the cache, the initialization function is
    /// called to create a new value and the returned value is inserted into the
    /// cache.
    ///
    /// When the cache is at capacity, the least recently used entry is evicted
    /// prior to inserting a new entry.
    ///
    /// If an entry of the same key is currently being initialized by another
    /// call to `get` or `get_by_ref`, the call will wait for the initialization
    /// to complete; the given cancellation token can be used to cancel the
    /// wait.
    ///
    /// Returns `Ok(None)` if the operation was canceled.
    ///
    /// # Panics
    ///
    /// Panics if the cache's inner mutex was poisoned.
    pub async fn get_by_ref<Q, F, E>(
        &self,
        key: &Q,
        cancellation: &CancellationContext,
        init: F,
    ) -> Result<Option<V>, E>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized + ToOwned<Owned = K>,
        V: Clone,
        F: AsyncFnOnce() -> Result<V, E>,
    {
        let value = {
            let mut cache = self.0.lock().expect("failed to lock cache");
            cache.get_or_insert_ref(key, Default::default).clone()
        };

        let token = cancellation.first();
        select! {
            biased;
            _ = token.cancelled() => {
                Ok(None)
            }
            r = value.get_or_try_init(|| async { init().await }) => {
                r.map(|v| Some(v.clone()))
            }
        }
    }

    /// Gets an entry from a cache by an owned key.
    ///
    /// If the entry already exists in the cache, the existing entry is cloned
    /// and returned.
    ///
    /// If the entry does not exist in the cache, the initialization function is
    /// called to create a new value and the returned value is inserted into the
    /// cache.
    ///
    /// When the cache is at capacity, the least recently used entry is evicted
    /// prior to inserting a new entry.
    ///
    /// If an entry of the same key is currently being initialized by another
    /// call to `get` or `get_by_ref`, the call will wait for the initialization
    /// to complete; the given cancellation token can be used to cancel the
    /// wait.
    ///
    /// Returns `Ok(None)` if the operation was canceled.
    ///
    /// # Panics
    ///
    /// Panics if the cache's inner mutex was poisoned.
    pub async fn get<F, E>(
        &self,
        key: K,
        cancellation: &CancellationContext,
        init: F,
    ) -> Result<Option<V>, E>
    where
        V: Clone,
        F: AsyncFnOnce() -> Result<V, E>,
    {
        let value = {
            let mut cache = self.0.lock().expect("failed to lock cache");
            cache.get_or_insert(key, Default::default).clone()
        };

        let token = cancellation.first();
        select! {
            biased;
            _ = token.cancelled() => {
                Ok(None)
            }
            r = value.get_or_try_init(|| async { init().await }) => {
                r.map(|v| Some(v.clone()))
            }
        }
    }

    /// Clears the cache.
    ///
    /// # Panics
    ///
    /// Panics if the cache's internal mutex was poisoned.
    #[allow(unused)]
    pub fn clear(&self) {
        self.0.lock().expect("failed to lock the cache").clear();
    }
}
