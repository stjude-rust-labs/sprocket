//! Implementation of support for Google Cloud Storage URLs.

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
/// Returns `true` if the URL was for Google Cloud Storage or `false` if it was
/// not.
pub(crate) fn apply_auth(config: &GoogleStorageConfig, url: &mut Url) -> bool {
    if let Some(url::Host::Domain(domain)) = url.host() {
        if let Some(domain) = domain.strip_suffix(GOOGLE_STORAGE_DOMAIN) {
            // If the URL already has a query string, don't modify it
            if url.query().is_some() {
                return true;
            }

            // There are two supported URL formats:
            // 1) Path style e.g. `https://storage.googleapis.com/<bucket>/<object>`
            // 2) Virtual-host style, e.g. `https://<bucket>.storage.googleapis.com/<object>`.
            let bucket = if domain.is_empty() {
                // This is a path style URL; bucket is first path segment
                let mut segments = match url.path_segments() {
                    Some(segments) => segments,
                    None => return true,
                };

                match segments.next() {
                    Some(bucket) => bucket,
                    None => return true,
                }
            } else {
                // This is a virtual-host style URL; bucket is first subdomain
                let mut subdomains = domain.split('.');
                let bucket = match subdomains.next() {
                    Some(bucket) => bucket,
                    None => return true,
                };

                // Ensure there is nothing between the subdomain and the GCS domain
                match subdomains.next() {
                    Some("") => {}
                    _ => return true,
                }

                bucket
            };

            if let Some(sig) = config.auth.get(bucket) {
                // Warn if the scheme isn't https, as we won't be applying the auth.
                if url.scheme() != "https" {
                    warn!(
                        "Google Cloud Storage URL `{url}` is not using HTTPS: authentication will \
                         not be used"
                    );
                    return true;
                }

                let sig = sig.strip_prefix('?').unwrap_or(sig);
                url.set_query(Some(sig));
            }

            return true;
        }
    }

    false
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
        let mut config = GoogleStorageConfig::default();
        config
            .auth
            .insert("bucket1".to_string(), "token1=foo".to_string());

        config
            .auth
            .insert("bucket2".to_string(), "?token2=bar".to_string());

        // Not an GS URL
        let mut url = "https://example.com/bar/baz".parse().unwrap();
        assert!(!apply_auth(&config, &mut url));
        assert_eq!(url.as_str(), "https://example.com/bar/baz");

        // Not using HTTPS
        let mut url = "http://storage.googleapis.com/bucket1/foo/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "http://storage.googleapis.com/bucket1/foo/bar"
        );

        // Unknown bucket (path)
        let mut url = "https://storage.googleapis.com/foo/bar/baz"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(url.as_str(), "https://storage.googleapis.com/foo/bar/baz");

        // Unknown bucket (vhost)
        let mut url = "https://foo.storage.googleapis.com/bar/baz"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(url.as_str(), "https://foo.storage.googleapis.com/bar/baz");

        // Matching with first auth token (path)
        let mut url = "https://storage.googleapis.com/bucket1/foo/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://storage.googleapis.com/bucket1/foo/bar?token1=foo"
        );

        // Matching with first auth token (vhost)
        let mut url = "https://bucket1.storage.googleapis.com/foo/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://bucket1.storage.googleapis.com/foo/bar?token1=foo"
        );

        // Matching with second auth token (path)
        let mut url = "https://storage.googleapis.com/bucket2/foo/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://storage.googleapis.com/bucket2/foo/bar?token2=bar"
        );

        // Matching with second auth token (vhost)
        let mut url = "https://bucket2.storage.googleapis.com/foo/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://bucket2.storage.googleapis.com/foo/bar?token2=bar"
        );

        // Matching with query params already present
        let mut url = "https://storage.googleapis.com/bucket2/foo/bar?a=b"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://storage.googleapis.com/bucket2/foo/bar?a=b"
        );
    }
}
