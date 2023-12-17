//! Document versions.

use grammar::v1::Rule;
use pest::iterators::Pair;
use wdl_grammar as grammar;
use wdl_macros::check_node;

/// An error when parsing a [`Version`].
#[derive(Debug)]
pub enum ParseError {
    /// An unknown version.
    UnknownVersion(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::UnknownVersion(version) => {
                write!(f, "unknown version: {version}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// An error related to a [`Version`].
#[derive(Debug)]
pub enum Error {
    /// A parse error.
    Parse(ParseError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Parse(err) => write!(f, "parse error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A document version.
#[derive(Clone, Eq, PartialEq)]
pub enum Version {
    /// WDL v1.0
    OneDotZero,

    /// WDL v1.1
    OneDotOne,
}

impl std::fmt::Debug for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::OneDotZero => write!(f, "WDL v1.0"),
            Version::OneDotOne => write!(f, "WDL v1.1"),
        }
    }
}

impl std::str::FromStr for Version {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1.1" => Ok(Self::OneDotOne),
            "1.0" => Ok(Self::OneDotZero),
            _ => Err(Error::Parse(ParseError::UnknownVersion(s.to_string()))),
        }
    }
}

impl TryFrom<Pair<'_, grammar::v1::Rule>> for Version {
    type Error = Error;

    fn try_from(node: Pair<'_, grammar::v1::Rule>) -> Result<Self, Self::Error> {
        check_node!(node, version);

        for node in node.into_inner().flatten() {
            if node.as_rule() == Rule::version_release {
                return node.as_str().parse();
            }
        }

        unreachable!(
            "`version` node must be required by the grammar to always contain a `version_release` \
             node"
        )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_parses_valid_document_versions() {
        let pt = wdl_grammar::v1::parse("version 1.0").unwrap();
        crate::v1::parse(pt.into_tree().unwrap()).unwrap();

        let pt = wdl_grammar::v1::parse("version 1.1").unwrap();
        crate::v1::parse(pt.into_tree().unwrap()).unwrap();
    }

    #[test]
    fn it_fails_to_parse_an_invalid_version() {
        let pt = wdl_grammar::v1::parse("version 1.2").unwrap();
        let err = crate::v1::parse(pt.into_tree().unwrap()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "document error: version error: parse error: unknown version: 1.2"
        );
    }
}
