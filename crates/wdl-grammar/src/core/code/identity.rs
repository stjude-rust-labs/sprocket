use std::num::NonZeroUsize;

use crate::Version;

#[derive(Debug)]
pub enum Error {
    InvalidIndex(usize),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidIndex(index) => write!(f, "invalid index: {index}"),
        }
    }
}

impl std::error::Error for Error {}

type Result<T> = std::result::Result<T, Error>;

pub struct Identity {
    pub index: NonZeroUsize,
    pub grammar: Version,
}

impl Identity {
    pub fn try_new(grammar: Version, index: usize) -> Result<Self> {
        Ok(Self { index, grammar })
    }
}
