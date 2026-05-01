//! Content hashing per the WDL module spec.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use thiserror::Error;

use crate::RelativePath;
use crate::RelativePathError;
use crate::tree::TreeError;

/// An error during content hashing.
#[derive(Debug, Error)]
pub enum HashError {
    /// A path supplied to [`Hasher::try_add`] failed relative-path
    /// validation.
    #[error(transparent)]
    InvalidPath(#[from] RelativePathError),

    /// An absolute path was supplied that does not live under the module
    /// root.
    #[error("absolute path `{0}` is not under the module root")]
    AbsoluteNotUnderRoot(String),

    /// A symbolic link target resolves outside the module root.
    #[error("symbolic link `{0}` resolves outside the module root")]
    SymlinkEscapesRoot(String),

    /// A new path collides under Unicode Normalization Form C (NFC) with a
    /// path that was already recorded. The spec requires the module's set
    /// of relative paths to be unique under NFC.
    #[error("path `{path}` collides with an already-recorded path under NFC form `{nfc}`")]
    AmbiguousPath {
        /// The newly-submitted path that collided.
        path: String,
        /// The shared NFC form.
        nfc: String,
    },

    /// I/O failure while reading a file.
    #[error("failed to read `{path}`")]
    Io {
        /// The path of the file that failed to read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: io::Error,
    },

    /// A module file-tree validation error (reserved-filename placement,
    /// NFC duplicate paths).
    #[error(transparent)]
    Tree(#[from] TreeError),
}

/// An error parsing a [`ContentHash`].
#[derive(Debug, Error)]
pub enum ContentHashError {
    /// The string does not start with the required `sha256:` prefix.
    #[error("content hash must start with `sha256:`")]
    MissingPrefix,

    /// The hex portion of the hash is not 64 characters.
    #[error("content hash must be exactly 64 hex characters; got {0}")]
    WrongLength(usize),

    /// The hex portion contains non-hex characters.
    #[error("content hash contains non-hex characters")]
    InvalidHex,
}

/// The prefix used in the wire form of a [`ContentHash`].
const SHA256_PREFIX: &str = "sha256:";

/// Domain-separation magic prepended to the SHA-256 input by
/// [`Hasher::finalize`].
const CONTENT_HASH_MAGIC: &[u8] = b"wdl-module-content\0v1\0";

/// A 32-byte SHA-256 module content hash.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(into = "String", try_from = "String")]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Returns the raw 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the hash as a 64-character lowercase hex string (without the
    /// `sha256:` prefix).
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl From<[u8; 32]> for ContentHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl From<ContentHash> for String {
    fn from(hash: ContentHash) -> Self {
        hash.to_string()
    }
}

impl TryFrom<String> for ContentHash {
    type Error = ContentHashError;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        s.parse()
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{SHA256_PREFIX}{}", hex::encode(self.0))
    }
}

impl FromStr for ContentHash {
    type Err = ContentHashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s
            .strip_prefix(SHA256_PREFIX)
            .ok_or(ContentHashError::MissingPrefix)?;
        if hex.len() != 64 {
            return Err(ContentHashError::WrongLength(hex.len()));
        }
        let bytes: [u8; 32] = hex::decode(hex)
            .map_err(|_| ContentHashError::InvalidHex)?
            .try_into()
            .map_err(|_| ContentHashError::WrongLength(hex.len()))?;
        Ok(Self(bytes))
    }
}

/// An incremental content hasher for a module directory.
///
/// `try_add` records relative paths into a [`BTreeSet`], so they are kept in
/// lexicographic order as they are inserted. `finalize` walks them in that
/// order, opens each file under the configured root, and feeds path bytes
/// plus raw file contents into a single SHA-256 state.
#[derive(Debug)]
pub struct Hasher {
    /// The directory under which all recorded relative paths resolve.
    root: PathBuf,
    /// The set of relative paths recorded so far, kept sorted.
    paths: BTreeSet<RelativePath>,
}

impl Hasher {
    /// Creates a new [`Hasher`] rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            paths: BTreeSet::new(),
        }
    }

    /// Returns an iterator over the recorded paths in lexicographic order.
    pub fn paths(&self) -> impl Iterator<Item = &RelativePath> {
        self.paths.iter()
    }

    /// Records a path relative to the hasher's root.
    ///
    /// Accepts an absolute path under `root` (which is converted to a relative
    /// form) or any [`RelativePath`]-convertible input.
    pub fn try_add(&mut self, path: impl Into<String>) -> Result<&mut Self, HashError> {
        let raw = path.into();

        let candidate = Path::new(&raw);
        let relative = if candidate.is_absolute() {
            candidate
                .strip_prefix(&self.root)
                .map_err(|_| HashError::AbsoluteNotUnderRoot(raw.clone()))?
        } else {
            candidate
        };

        let rel = RelativePath::try_from(relative)?;

        let nfc = rel.as_str().to_string();
        if !self.paths.insert(rel) {
            return Err(HashError::AmbiguousPath { path: raw, nfc });
        }
        Ok(self)
    }

    /// Computes the [`ContentHash`] of the recorded paths.
    ///
    /// Each file's full path is canonicalized (resolving symbolic links)
    /// before reading; if the resolved target falls outside the module
    /// root, the module is rejected per the spec's symlink-containment
    /// rule. Without this check, a symbolic link inside the module could
    /// pull bytes from elsewhere on the filesystem into the digest.
    pub fn finalize(self) -> Result<ContentHash, HashError> {
        let canonical_root = std::fs::canonicalize(&self.root).map_err(|source| HashError::Io {
            path: self.root.clone(),
            source,
        })?;

        let mut sha = Sha256::new();
        sha.update(CONTENT_HASH_MAGIC);
        // NOTE: paths are sorted by the [`BTreeSet`].
        for relative in &self.paths {
            let bytes = relative.as_str().as_bytes();
            sha.update((bytes.len() as u64).to_le_bytes());
            sha.update(bytes);

            let abs = self.root.join(relative);
            let canonical_abs = std::fs::canonicalize(&abs).map_err(|source| HashError::Io {
                path: abs.clone(),
                source,
            })?;

            if !canonical_abs.starts_with(&canonical_root) {
                return Err(HashError::SymlinkEscapesRoot(relative.as_str().to_string()));
            }

            let mut file = File::open(&canonical_abs).map_err(|source| HashError::Io {
                path: canonical_abs.clone(),
                source,
            })?;
            let len = file
                .metadata()
                .map_err(|source| HashError::Io {
                    path: canonical_abs.clone(),
                    source,
                })?
                .len();
            sha.update(len.to_le_bytes());
            io::copy(&mut file, &mut sha).map_err(|source| HashError::Io {
                path: canonical_abs,
                source,
            })?;
        }

        sha.update((self.paths.len() as u64).to_le_bytes());
        Ok(ContentHash::from(<[u8; 32]>::from(sha.finalize())))
    }
}

/// Computes the content hash of a directory by walking it (excluding the
/// spec-mandated exclusions `module.sig` and `module-lock.json`).
pub fn hash_directory(root: impl AsRef<Path>) -> Result<ContentHash, HashError> {
    let root = root.as_ref();
    let mut hasher = Hasher::new(root.to_path_buf());
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|source| HashError::Io {
            path: dir.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| HashError::Io {
                path: dir.clone(),
                source,
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|source| HashError::Io {
                path: path.clone(),
                source,
            })?;
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            // SAFETY: `path` was produced by `read_dir(dir)` for a `dir`
            // whose ancestor stack started at `root`, so it always lives
            // under `root`.
            let rel_path = path.strip_prefix(root).unwrap();
            let rel = rel_path
                .to_str()
                .ok_or(RelativePathError::NonUtf8)?
                .replace('\\', "/");
            if rel == crate::SIGNATURE_FILENAME || rel == crate::LOCKFILE_FILENAME {
                continue;
            }
            hasher.try_add(rel)?;
        }
    }

    crate::tree::validate_tree(hasher.paths())?;

    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn round_trips_via_display() {
        let bytes = [0xAB; 32];
        let hash = ContentHash::from(bytes);
        let s = hash.to_string();
        assert!(s.starts_with("sha256:"));
        let parsed: ContentHash = s.parse().unwrap();
        assert_eq!(parsed, hash);
    }

    #[test]
    fn rejects_missing_prefix() {
        assert!(matches!(
            "ab".repeat(32).parse::<ContentHash>(),
            Err(ContentHashError::MissingPrefix)
        ));
    }

    #[test]
    fn rejects_bad_hex() {
        let s = format!("sha256:{}", "g".repeat(64));
        assert!(matches!(
            s.parse::<ContentHash>(),
            Err(ContentHashError::InvalidHex)
        ));
    }

    #[test]
    fn rejects_unrecoverable_paths() {
        let dir = tempdir().unwrap();
        let mut h = Hasher::new(dir.path().to_path_buf());
        for bad in [
            "",                          // empty
            ".",                         // resolves to empty
            "..",                        // escapes root
            "../escape",                 // escapes root
            "/somewhere/not/under/root", // absolute, not under root
            "has\0null",                 // null byte
            "C:/win",                    // Windows drive letter
            "c:\\win",                   // lowercase drive letter
        ] {
            assert!(h.try_add(bad).is_err(), "accepted `{bad}`");
        }
    }

    #[test]
    fn normalizes_relative_paths() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("foo.txt"), b"x").unwrap();

        let mut h_clean = Hasher::new(dir.path().to_path_buf());
        h_clean.try_add("foo.txt").unwrap();

        let mut h_dotty = Hasher::new(dir.path().to_path_buf());
        h_dotty.try_add("./bar/../foo.txt").unwrap();

        assert_eq!(h_clean.finalize().unwrap(), h_dotty.finalize().unwrap());
    }

    #[test]
    fn accepts_absolute_under_root() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("foo.txt"), b"x").unwrap();

        let mut h_rel = Hasher::new(dir.path().to_path_buf());
        h_rel.try_add("foo.txt").unwrap();

        let mut h_abs = Hasher::new(dir.path().to_path_buf());
        h_abs
            .try_add(dir.path().join("foo.txt").to_string_lossy().to_string())
            .unwrap();

        assert_eq!(h_rel.finalize().unwrap(), h_abs.finalize().unwrap());
    }

    #[test]
    fn hashes_two_files_deterministically() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"alpha").unwrap();
        fs::write(dir.path().join("b.txt"), b"beta").unwrap();

        let mut h1 = Hasher::new(dir.path().to_path_buf());
        h1.try_add("a.txt").unwrap().try_add("b.txt").unwrap();
        let d1 = h1.finalize().unwrap();

        // Same files, opposite add order.
        let mut h2 = Hasher::new(dir.path().to_path_buf());
        h2.try_add("b.txt").unwrap().try_add("a.txt").unwrap();
        let d2 = h2.finalize().unwrap();

        assert_eq!(d1, d2, "digests should match regardless of `try_add` order");
    }

    #[test]
    fn detects_path_content_boundary_collision() {
        // Without per-field length prefixes, `{a: "Xbc"}` and `{aXbc: ""}`
        // would both feed the byte stream `aXbc` into the hasher and collide.
        // The path-length and content-length prefixes shift the boundary,
        // making the encoding injective.
        let dir1 = tempdir().unwrap();
        fs::write(dir1.path().join("a"), b"Xbc").unwrap();

        let dir2 = tempdir().unwrap();
        fs::write(dir2.path().join("aXbc"), b"").unwrap();

        let d1 = hash_directory(dir1.path()).unwrap();
        let d2 = hash_directory(dir2.path()).unwrap();
        assert_ne!(d1, d2);
    }

    #[test]
    fn excludes_module_sig_and_lockfile() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"keep").unwrap();
        let d_clean = hash_directory(dir.path()).unwrap();

        fs::write(dir.path().join(crate::SIGNATURE_FILENAME), b"sig").unwrap();
        fs::write(dir.path().join(crate::LOCKFILE_FILENAME), b"lock").unwrap();
        let d_with_excludes = hash_directory(dir.path()).unwrap();

        assert_eq!(d_clean, d_with_excludes);
    }

    #[test]
    fn hash_directory_rejects_nested_reserved_filename() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(
            dir.path().join("nested").join(crate::MANIFEST_FILENAME),
            b"x",
        )
        .unwrap();
        let err = hash_directory(dir.path()).unwrap_err();
        assert!(matches!(
            err,
            HashError::Tree(crate::TreeError::ReservedFilename {
                name: crate::MANIFEST_FILENAME,
                ..
            })
        ));
    }

    #[test]
    fn finalize_errors_on_missing_file() {
        let dir = tempdir().unwrap();
        let mut h = Hasher::new(dir.path().to_path_buf());
        h.try_add("missing.txt").unwrap();
        assert!(matches!(h.finalize(), Err(HashError::Io { .. })));
    }

    #[test]
    fn rejects_paths_colliding_under_nfc() {
        let dir = tempdir().unwrap();
        let mut h = Hasher::new(dir.path().to_path_buf());

        // Both forms of `é` normalize to the same NFC sequence.
        let precomposed = "caf\u{00E9}.wdl";
        let decomposed = "cafe\u{0301}.wdl";

        h.try_add(precomposed).unwrap();
        let err = h.try_add(decomposed).unwrap_err();
        assert!(matches!(err, HashError::AmbiguousPath { .. }));
    }

    #[test]
    fn nfc_normalizes_recorded_paths() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("caf\u{00E9}.wdl"), b"x").unwrap();

        let mut h_nfc = Hasher::new(dir.path().to_path_buf());
        h_nfc.try_add("caf\u{00E9}.wdl").unwrap();

        let mut h_nfd = Hasher::new(dir.path().to_path_buf());
        h_nfd.try_add("cafe\u{0301}.wdl").unwrap();

        assert_eq!(h_nfc.finalize().unwrap(), h_nfd.finalize().unwrap());
    }
}
