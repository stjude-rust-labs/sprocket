//! Implementation of support for Amazon AWS S3 URLs.

use anyhow::Context;
use anyhow::Result;
use tracing::warn;
use url::Url;

use crate::config::S3StorageConfig;

/// The S3 storage domain suffix.
const S3_STORAGE_DOMAIN_SUFFIX: &str = ".amazonaws.com";

/// The default S3 URL region.
const DEFAULT_REGION: &str = "us-east-1";

/// Rewrites an S3 URL (s3://) into a HTTPS URL.
pub(crate) fn rewrite_url(config: &S3StorageConfig, url: &Url) -> Result<Url> {
    assert_eq!(url.scheme(), "s3");

    let region = config.region.as_deref().unwrap_or(DEFAULT_REGION);

    let bucket = url
        .host_str()
        .with_context(|| format!("invalid S3 URL `{url}`: bucket name is missing"))?;

    match (url.query(), url.fragment()) {
        (None, None) => format!(
            "https://{bucket}.s3.{region}{S3_STORAGE_DOMAIN_SUFFIX}{path}",
            path = url.path()
        ),
        (None, Some(fragment)) => {
            format!(
                "https://{bucket}.s3.{region}{S3_STORAGE_DOMAIN_SUFFIX}{path}#{fragment}",
                path = url.path()
            )
        }
        (Some(query), None) => {
            format!(
                "https://{bucket}.s3.{region}{S3_STORAGE_DOMAIN_SUFFIX}{path}?{query}",
                path = url.path()
            )
        }
        (Some(query), Some(fragment)) => {
            format!(
                "https://{bucket}.s3.{region}{S3_STORAGE_DOMAIN_SUFFIX}{path}?{query}#{fragment}",
                path = url.path()
            )
        }
    }
    .parse()
    .with_context(|| format!("invalid S3 URL `{url}`"))
}

/// Applies S3 presigned signatures to the given URL.
///
/// Returns `true` if the URL was for S3 or `false` if it was not.
pub(crate) fn apply_auth(config: &S3StorageConfig, url: &mut Url) -> bool {
    if let Some(url::Host::Domain(domain)) = url.host() {
        if let Some(domain) = domain.strip_suffix(S3_STORAGE_DOMAIN_SUFFIX) {
            // If the URL already has a query string, don't modify it
            if url.query().is_some() {
                return true;
            }

            // There are three supported URL formats:
            // 1) Path style without region, e.g. `https://s3.amazonaws.com/<bucket>/<object>`
            // 2) Path style with region, e.g. `https://s3.<region>.amazonaws.com/<bucket>/<object>`.
            // 3) Virtual-host style, e.g. `https://<bucket>.s3.<region>.amazonaws.com/<object>`.
            let bucket = if domain == "s3"
                || domain == "S3"
                || domain.starts_with("s3.")
                || domain.starts_with("S3.")
            {
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

                // Ensure the URL is for the S3 service
                match subdomains.next() {
                    Some("s3") | Some("S3") => {}
                    _ => return true,
                }

                bucket
            };

            if let Some(sig) = config.auth.get(bucket) {
                // Warn if the scheme isn't https, as we won't be applying the auth.
                if url.scheme() != "https" {
                    warn!("S3 URL `{url}` is not using HTTPS: authentication will not be used");
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
        // Default region is us-east-1
        let url = rewrite_url(&Default::default(), &"s3://foo/bar/baz".parse().unwrap()).unwrap();
        assert_eq!(
            url.as_str(),
            "https://foo.s3.us-east-1.amazonaws.com/bar/baz"
        );

        // Ensure users can change the default region via the config
        let config = S3StorageConfig {
            region: Some("us-west-1".to_string()),
            ..Default::default()
        };
        let url = rewrite_url(&config, &"s3://foo/bar/baz#qux".parse().unwrap()).unwrap();
        assert_eq!(
            url.as_str(),
            "https://foo.s3.us-west-1.amazonaws.com/bar/baz#qux"
        );

        let url = rewrite_url(&config, &"s3://foo/bar/baz?qux=quux".parse().unwrap()).unwrap();
        assert_eq!(
            url.as_str(),
            "https://foo.s3.us-west-1.amazonaws.com/bar/baz?qux=quux"
        );

        let url = rewrite_url(
            &config,
            &"s3://foo/bar/baz?qux=quux&jam=cakes#frag".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://foo.s3.us-west-1.amazonaws.com/bar/baz?qux=quux&jam=cakes#frag"
        );
    }

    #[test]
    fn it_applies_auth() {
        let mut config = S3StorageConfig::default();
        config
            .auth
            .insert("bucket1".to_string(), "token1=foo".to_string());

        config
            .auth
            .insert("bucket2".to_string(), "?token2=bar".to_string());

        // Not an S3 URL
        let mut url = "https://example.com/bar/baz".parse().unwrap();
        assert!(!apply_auth(&config, &mut url));
        assert_eq!(url.as_str(), "https://example.com/bar/baz");

        // Not using HTTPS
        let mut url = "http://s3.us-east-1.amazonaws.com/bucket1/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "http://s3.us-east-1.amazonaws.com/bucket1/bar"
        );

        // Unknown bucket (path without region)
        let mut url = "https://s3.amazonaws.com/foo/bar".parse().unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(url.as_str(), "https://s3.amazonaws.com/foo/bar");

        // Unknown bucket (path with region)
        let mut url = "https://s3.us-east-1.amazonaws.com/foo/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(url.as_str(), "https://s3.us-east-1.amazonaws.com/foo/bar");

        // Unknown bucket (vhost)
        let mut url = "https://foo.s3.us-east-1.amazonaws.com/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(url.as_str(), "https://foo.s3.us-east-1.amazonaws.com/bar");

        // Matching with first token (path without region)
        let mut url = "https://s3.amazonaws.com/bucket1/bar".parse().unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://s3.amazonaws.com/bucket1/bar?token1=foo"
        );

        // Matching with first token(path with region)
        let mut url = "https://s3.us-east-1.amazonaws.com/bucket1/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://s3.us-east-1.amazonaws.com/bucket1/bar?token1=foo"
        );

        // Matching with first token (vhost)
        let mut url = "https://bucket1.s3.us-east-1.amazonaws.com/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://bucket1.s3.us-east-1.amazonaws.com/bar?token1=foo"
        );

        // Matching with second token (path without region)
        let mut url = "https://s3.amazonaws.com/bucket2/bar".parse().unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://s3.amazonaws.com/bucket2/bar?token2=bar"
        );

        // Matching with second token(path with region)
        let mut url = "https://s3.us-east-1.amazonaws.com/bucket2/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://s3.us-east-1.amazonaws.com/bucket2/bar?token2=bar"
        );

        // Matching with second token (vhost)
        let mut url = "https://bucket2.s3.us-east-1.amazonaws.com/bar"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://bucket2.s3.us-east-1.amazonaws.com/bar?token2=bar"
        );

        // Matching with query params already present
        let mut url = "https://bucket2.s3.us-east-1.amazonaws.com/bar?a=b"
            .parse()
            .unwrap();
        assert!(apply_auth(&config, &mut url));
        assert_eq!(
            url.as_str(),
            "https://bucket2.s3.us-east-1.amazonaws.com/bar?a=b"
        );
    }
}
