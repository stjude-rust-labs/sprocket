//! A file that registers the remote files cached locally and their `etag`s.

use std::fs::File;

use std::io::Write;
use std::path::Path;
use std::path::PathBuf;

use indexmap::IndexMap;
use octocrab::etag::EntityTag;

use toml::map::Map;
use toml::Value;

/// A parse error related to a [`Registry`].
#[derive(Debug)]
pub enum ParseError {
    /// An error parsing an entity tag.
    EntityTag(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::EntityTag(reason) => write!(f, "entity tag: {reason}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// An error related to a [`Registry`].
#[derive(Debug)]
pub enum Error {
    /// An input/output error.
    InputOutput(std::io::Error),

    /// The provided path is missing a parent.
    MissingParent(PathBuf),

    /// A parse error.
    Parse(ParseError),

    /// Attempted to save the results to a file, but this [`Registry`] is an
    /// in-memory registry.
    SaveOnUnbackedRegistry,

    /// A TOML deserialization error.
    TomlDeserialization(toml::de::Error),

    /// A TOML serialization error.
    TomlSerialization(toml::ser::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InputOutput(err) => write!(f, "i/o error: {err}"),
            Error::MissingParent(path) => write!(f, "missing parent: {}", path.display()),
            Error::Parse(err) => write!(f, "parse error: {err}"),
            Error::SaveOnUnbackedRegistry => write!(f, "cannot save an in-memory registry to file"),
            Error::TomlDeserialization(err) => write!(f, "toml deserialization error: {err}"),
            Error::TomlSerialization(err) => write!(f, "toml serialization error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
type Result<T> = std::result::Result<T, Error>;

/// A file-backed registry file containing a mapping from paths to `etag`s.
///
/// This is useful when creating a cache, as it will store the retrieved etag
/// values for all of the files we store in the cache. We can then use this
/// registry to determine whether a file needs to be redownloaded.
#[derive(Debug, Default)]
pub struct Registry {
    /// The path to the registry file.
    path: Option<PathBuf>,

    /// The mapping of paths to [`etag`](EntityTag)s.
    entries: IndexMap<String, EntityTag>,
}

impl Registry {
    /// Gets a reference to the inner entries.
    pub fn entries(&self) -> &IndexMap<String, EntityTag> {
        &self.entries
    }

    /// Attempts to insert an entity tag into the [`Registry`].
    pub fn try_insert_etag(
        &mut self,
        path: impl AsRef<str>,
        value: impl AsRef<str>,
    ) -> Result<Option<EntityTag>> {
        let etag = value
            .as_ref()
            .parse()
            .map_err(|reason| Error::Parse(ParseError::EntityTag(reason)))?;
        Ok(self.entries.insert(path.as_ref().to_string(), etag))
    }

    /// Saves a [`Registry`] to its backed file.
    pub fn save(&self) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .map(Ok)
            .unwrap_or(Err(Error::SaveOnUnbackedRegistry))?;

        let map = self
            .entries
            .iter()
            .map(|(key, value)| (key.to_owned(), toml::Value::String(value.to_string())))
            .collect::<Map<String, toml::Value>>();

        let contents = toml::to_string_pretty(&map).map_err(Error::TomlSerialization)?;

        let mut file = File::create(path).map_err(Error::InputOutput)?;
        file.write_all(contents.as_bytes())
            .map_err(Error::InputOutput)
    }
}

impl TryFrom<PathBuf> for Registry {
    type Error = Error;

    fn try_from(path: PathBuf) -> Result<Self> {
        match path.exists() {
            true => {
                let contents = std::fs::read_to_string(&path).map_err(Error::InputOutput)?;
                let entries = entries_from_string(&contents)?;

                Ok(Self {
                    path: Some(path),
                    entries,
                })
            }
            false => {
                let parent = path
                    .parent()
                    .map(Ok)
                    .unwrap_or(Err(Error::MissingParent(path.clone())))?;
                std::fs::create_dir_all(parent).map_err(Error::InputOutput)?;

                Ok(Self {
                    path: Some(path),
                    entries: Default::default(),
                })
            }
        }
    }
}

impl TryFrom<&Path> for Registry {
    type Error = Error;

    fn try_from(path: &Path) -> Result<Self> {
        let path = path.to_path_buf();
        Self::try_from(path)
    }
}

/// Pulls a list of entries from the contents of a TOML file and returns them
/// within an [`IndexMap`]. This is used when loading a [`Registry`] from an
/// existing file.
fn entries_from_string(contents: &str) -> Result<IndexMap<String, EntityTag>> {
    contents
        .parse::<toml::Table>()
        .map_err(Error::TomlDeserialization)?
        .into_iter()
        .map(|(key, value)| {
            match value {
                Value::String(value) => {
                    let value = value
                        .parse::<EntityTag>()
                        .map_err(|reason| Error::Parse(ParseError::EntityTag(reason)))?;
                    Ok((key, value))
                }
                // SAFETY: none of these other value types will be
                // created by this code. As such, if any other type is
                // encountered, then that must be because of human
                // intervention in the file.
                _ => unreachable!(),
            }
        })
        .collect::<Result<IndexMap<String, EntityTag>>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_builds_from_a_toml_string_correctly() -> Result<()> {
        let entries = entries_from_string(r#"hello = "\"W/abcd1234\"""#)?;

        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries.get("hello").unwrap().to_string(),
            String::from(r#""W/abcd1234""#)
        );

        Ok(())
    }

    #[test]
    #[should_panic]
    fn it_fails_when_an_unexpected_element_exists_in_the_map() {
        entries_from_string(
            r#"[section]
        hello = "\"W/abcd1234\"""#,
        )
        .unwrap();
    }
}
