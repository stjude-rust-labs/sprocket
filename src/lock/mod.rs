//! Image resolution utilities for `sprocket dev lock`.

pub mod file;
pub mod oci;
pub mod sylabs;

/// The user agent for HTTP requests.
pub(crate) const USER_AGENT: &str = concat!(
    env!("CARGO_PKG_NAME"),
    "/",
    env!("CARGO_PKG_VERSION"),
    " +",
    env!("CARGO_PKG_REPOSITORY")
);
