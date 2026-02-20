//! URL conversion utilities.

use std::str::FromStr;

use ls_types::Uri;
use url::Url;

/// A utility trait to convert [`Url`]s to [`Uri`]s.
pub trait UriToUrl {
    /// Attempt to convert a [`Url`] to a [`Uri`].
    fn try_into_url(&self) -> Result<Url, <Url as FromStr>::Err>;
}

impl UriToUrl for Uri {
    fn try_into_url(&self) -> Result<Url, <Url as FromStr>::Err> {
        Url::from_str(self.as_str())
    }
}

/// A utility trait to convert [`Uri`]s to [`Url`]s.
pub trait UrlToUri {
    /// Attempt to convert a [`Uri`] to a [`Url`].
    fn try_into_uri(&self) -> Result<Uri, <Uri as FromStr>::Err>;
}

impl UrlToUri for Url {
    fn try_into_uri(&self) -> Result<Uri, <Uri as FromStr>::Err> {
        Uri::from_str(self.as_str())
    }
}
