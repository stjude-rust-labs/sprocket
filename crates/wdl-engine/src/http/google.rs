//! Implementation of support for Google Cloud Storage URLs.

use std::borrow::Cow;

use anyhow::Context;
use anyhow::Result;
use tracing::warn;
use url::Url;

use crate::config::GoogleStorageConfig;

/// The Google Storage domain.
const GOOGLE_STORAGE_DOMAIN: &str = "storage.googleapis.com";

/// Rewrites a Google Cloud Storage URL (gs://) into a HTTPS URL.
pub(crate) fn rewrite_url(url: &Url) -> Result<Url> {
    assert_eq!(url.scheme(), "gs");

    let bucket = url.host_str().with_context(|| {
        format!("invalid Google Cloud Storage URL `{url}`: bucket name is missing")
    })?;

    match (url.query(), url.fragment()) {
        (None, None) => format!(
            "https://{bucket}.{GOOGLE_STORAGE_DOMAIN}{path}",
            path = url.path()
        ),
        (None, Some(fragment)) => {
            format!(
                "https://{bucket}.{GOOGLE_STORAGE_DOMAIN}{path}#{fragment}",
                path = url.path()
            )
        }
        (Some(query), None) => format!(
            "https://{bucket}.{GOOGLE_STORAGE_DOMAIN}{path}?{query}",
            path = url.path()
        ),
        (Some(query), Some(fragment)) => {
            format!(
                "https://{bucket}.{GOOGLE_STORAGE_DOMAIN}{path}?{query}#{fragment}",
                path = url.path()
            )
        }
    }
    .parse()
    .with_context(|| format!("invalid Google Cloud Storage URL `{url}`"))
}

/// Applies Google Cloud Storage presigned signatures to the given URL.
///
/// Returns `(false, _)` if the URL is not for Google Cloud Storage; the
/// returned URL is unmodified.
///
/// Returns `(true, _)` if the URL is for Google Cloud Storage. If auth was
/// applied, the returned URL is modified to include it; otherwise the original
/// URL is returned unmodified.
pub(crate) fn apply_auth<'a>(
    config: &GoogleStorageConfig,
    url: Cow<'a, Url>,
) -> (bool, Cow<'a, Url>) {
    // Find the prefix of the domain; if empty, it indicates a path style URL
    let prefix = match url.host().and_then(|host| match host {
        url::Host::Domain(domain) => domain.strip_suffix(GOOGLE_STORAGE_DOMAIN),
        _ => None,
    }) {
        Some(prefix) => prefix,
        None => return (false, url),
    };

    // If the URL already has a query string, don't modify it
    if url.query().is_some() {
        return (true, url);
    }

    // There are two supported URL formats:
    // 1) Path style e.g. `https://storage.googleapis.com/<bucket>/<object>`
    // 2) Virtual-host style, e.g. `https://<bucket>.storage.googleapis.com/<object>`.
    let bucket = if prefix.is_empty() {
        // This is a path style URL; bucket is first path segment
        match url.path_segments().and_then(|mut segments| segments.next()) {
            Some(bucket) => bucket,
            None => return (true, url),
        }
    } else {
        // This is a virtual-host style URL; bucket should be followed with a single dot
        let mut iter = prefix.split('.');
        match (iter.next(), iter.next(), iter.next()) {
            (Some(bucket), Some(""), None) => bucket,
            _ => return (true, url),
        }
    };

    if let Some(sig) = config.auth.get(bucket) {
        if url.scheme() == "https" {
            let sig = sig.strip_prefix('?').unwrap_or(sig);
            let mut url = url.into_owned();
            url.set_query(Some(sig));
            return (true, Cow::Owned(url));
        }

        // Warn if the scheme isn't https, as we won't be applying the auth.
        warn!(
            "Google Cloud Storage URL `{url}` is not using HTTPS: authentication will not be used"
        );
    }

    (true, url)
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn it_rewrites_urls() {
        let url = rewrite_url(&"gs://foo/bar/baz".parse().unwrap()).unwrap();
        assert_eq!(url.as_str(), "https://foo.storage.googleapis.com/bar/baz");

        let url = rewrite_url(&"gs://foo/bar/baz#qux".parse().unwrap()).unwrap();
        assert_eq!(
            url.as_str(),
            "https://foo.storage.googleapis.com/bar/baz#qux"
        );

        let url = rewrite_url(&"gs://foo/bar/baz?qux=quux".parse().unwrap()).unwrap();
        assert_eq!(
            url.as_str(),
            "https://foo.storage.googleapis.com/bar/baz?qux=quux"
        );

        let url =
            rewrite_url(&"gs://foo/bar/baz?qux=quux&jam=cakes#frag".parse().unwrap()).unwrap();
        assert_eq!(
            url.as_str(),
            "https://foo.storage.googleapis.com/bar/baz?qux=quux&jam=cakes#frag"
        );
    }

    #[test]
    fn it_applies_auth() {
        fn assert_auth(
            config: &GoogleStorageConfig,
            url: &str,
            expected_match: bool,
            expected: &str,
        ) {
            let (matches, url) = apply_auth(config, Cow::Owned(url.parse().unwrap()));
            assert_eq!(matches, expected_match);
            assert_eq!(url.as_str(), expected);
        }

        let mut config = GoogleStorageConfig::default();
        config
            .auth
            .insert("bucket1".to_string(), "token1=foo".to_string());

        config
            .auth
            .insert("bucket2".to_string(), "?token2=bar".to_string());

        // Not an GS URL
        assert_auth(
            &config,
            "https://example.com/bar/baz",
            false,
            "https://example.com/bar/baz",
        );

        // Not using HTTPS
        assert_auth(
            &config,
            "http://storage.googleapis.com/bucket1/foo/bar",
            true,
            "http://storage.googleapis.com/bucket1/foo/bar",
        );

        // Unknown bucket (path)
        assert_auth(
            &config,
            "https://storage.googleapis.com/foo/bar/baz",
            true,
            "https://storage.googleapis.com/foo/bar/baz",
        );

        // Unknown bucket (vhost)
        assert_auth(
            &config,
            "https://foo.storage.googleapis.com/bar/baz",
            true,
            "https://foo.storage.googleapis.com/bar/baz",
        );

        // Matching with first auth token (path)
        assert_auth(
            &config,
            "https://storage.googleapis.com/bucket1/foo/bar",
            true,
            "https://storage.googleapis.com/bucket1/foo/bar?token1=foo",
        );

        // Matching with first auth token (vhost)
        assert_auth(
            &config,
            "https://bucket1.storage.googleapis.com/foo/bar",
            true,
            "https://bucket1.storage.googleapis.com/foo/bar?token1=foo",
        );

        // Matching with second auth token (path)
        assert_auth(
            &config,
            "https://storage.googleapis.com/bucket2/foo/bar",
            true,
            "https://storage.googleapis.com/bucket2/foo/bar?token2=bar",
        );

        // Matching with second auth token (vhost)
        assert_auth(
            &config,
            "https://bucket2.storage.googleapis.com/foo/bar",
            true,
            "https://bucket2.storage.googleapis.com/foo/bar?token2=bar",
        );

        // Matching with query params already present
        assert_auth(
            &config,
            "https://storage.googleapis.com/bucket2/foo/bar?a=b",
            true,
            "https://storage.googleapis.com/bucket2/foo/bar?a=b",
        );
    }
}
