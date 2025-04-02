//! Implementation of support for Amazon AWS S3 URLs.

use std::borrow::Cow;

use anyhow::Context;
use anyhow::Result;
use tracing::warn;
use url::Url;

use crate::config::S3StorageConfig;

/// The AWS domain suffix.
const AWS_DOMAIN_SUFFIX: &str = ".amazonaws.com";

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
            "https://{bucket}.s3.{region}{AWS_DOMAIN_SUFFIX}{path}",
            path = url.path()
        ),
        (None, Some(fragment)) => {
            format!(
                "https://{bucket}.s3.{region}{AWS_DOMAIN_SUFFIX}{path}#{fragment}",
                path = url.path()
            )
        }
        (Some(query), None) => {
            format!(
                "https://{bucket}.s3.{region}{AWS_DOMAIN_SUFFIX}{path}?{query}",
                path = url.path()
            )
        }
        (Some(query), Some(fragment)) => {
            format!(
                "https://{bucket}.s3.{region}{AWS_DOMAIN_SUFFIX}{path}?{query}#{fragment}",
                path = url.path()
            )
        }
    }
    .parse()
    .with_context(|| format!("invalid S3 URL `{url}`"))
}

/// Applies S3 presigned signatures to the given URL.
///
/// Returns `(false, _)` if the URL is not for S3; the returned URL is
/// unmodified.
///
/// Returns `(true, _)` if the URL is for S3. If auth was applied, the returned
/// URL is modified to include it; otherwise the original URL is returned
/// unmodified.
pub(crate) fn apply_auth<'a>(config: &S3StorageConfig, url: Cow<'a, Url>) -> (bool, Cow<'a, Url>) {
    // Find the prefix of the domain
    let prefix = match url.host().and_then(|host| match host {
        url::Host::Domain(domain) => domain.strip_suffix(AWS_DOMAIN_SUFFIX),
        _ => None,
    }) {
        Some(prefix) => prefix,
        None => return (false, url),
    };

    // If the URL already has a query string, don't modify it
    if url.query().is_some() {
        return (true, url);
    }

    // There are three supported URL formats:
    // 1) Path style without region, e.g. `https://s3.amazonaws.com/<bucket>/<object>`
    // 2) Path style with region, e.g. `https://s3.<region>.amazonaws.com/<bucket>/<object>`.
    // 3) Virtual-host style, e.g. `https://<bucket>.s3.<region>.amazonaws.com/<object>`.
    let bucket = if prefix == "s3"
        || prefix == "S3"
        || prefix.starts_with("s3.")
        || prefix.starts_with("S3.")
    {
        // This is a path style URL; bucket is first path segment
        match url.path_segments().and_then(|mut segments| segments.next()) {
            Some(bucket) => bucket,
            None => return (true, url),
        }
    } else {
        // This is a virtual-host style URL; bucket should be followed with `s3`.
        let mut iter = prefix.split('.');
        match (iter.next(), iter.next()) {
            (Some(bucket), Some("s3")) | (Some(bucket), Some("S3")) => bucket,
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
        warn!("S3 URL `{url}` is not using HTTPS: authentication will not be used");
    }

    (true, url)
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
        fn assert_auth(config: &S3StorageConfig, url: &str, expected_match: bool, expected: &str) {
            let (matches, url) = apply_auth(config, Cow::Owned(url.parse().unwrap()));
            assert_eq!(matches, expected_match);
            assert_eq!(url.as_str(), expected);
        }

        let mut config = S3StorageConfig::default();
        config
            .auth
            .insert("bucket1".to_string(), "token1=foo".to_string());

        config
            .auth
            .insert("bucket2".to_string(), "?token2=bar".to_string());

        // Not an S3 URL
        assert_auth(
            &config,
            "https://example.com/bar/baz",
            false,
            "https://example.com/bar/baz",
        );

        // Not using HTTPS
        assert_auth(
            &config,
            "http://s3.us-east-1.amazonaws.com/bucket1/bar",
            true,
            "http://s3.us-east-1.amazonaws.com/bucket1/bar",
        );

        // Unknown bucket (path without region)
        assert_auth(
            &config,
            "https://s3.amazonaws.com/foo/bar",
            true,
            "https://s3.amazonaws.com/foo/bar",
        );

        // Unknown bucket (path with region)
        assert_auth(
            &config,
            "https://s3.us-east-1.amazonaws.com/foo/bar",
            true,
            "https://s3.us-east-1.amazonaws.com/foo/bar",
        );

        // Unknown bucket (vhost)
        assert_auth(
            &config,
            "https://foo.s3.us-east-1.amazonaws.com/bar",
            true,
            "https://foo.s3.us-east-1.amazonaws.com/bar",
        );

        // Matching with first token (path without region)
        assert_auth(
            &config,
            "https://s3.amazonaws.com/bucket1/bar",
            true,
            "https://s3.amazonaws.com/bucket1/bar?token1=foo",
        );

        // Matching with first token(path with region)
        assert_auth(
            &config,
            "https://s3.us-east-1.amazonaws.com/bucket1/bar",
            true,
            "https://s3.us-east-1.amazonaws.com/bucket1/bar?token1=foo",
        );

        // Matching with first token (vhost)
        assert_auth(
            &config,
            "https://bucket1.s3.us-east-1.amazonaws.com/bar",
            true,
            "https://bucket1.s3.us-east-1.amazonaws.com/bar?token1=foo",
        );

        // Matching with second token (path without region)
        assert_auth(
            &config,
            "https://s3.amazonaws.com/bucket2/bar",
            true,
            "https://s3.amazonaws.com/bucket2/bar?token2=bar",
        );

        // Matching with second token(path with region)
        assert_auth(
            &config,
            "https://s3.us-east-1.amazonaws.com/bucket2/bar",
            true,
            "https://s3.us-east-1.amazonaws.com/bucket2/bar?token2=bar",
        );

        // Matching with second token (vhost)
        assert_auth(
            &config,
            "https://bucket2.s3.us-east-1.amazonaws.com/bar",
            true,
            "https://bucket2.s3.us-east-1.amazonaws.com/bar?token2=bar",
        );

        // Matching with query params already present
        assert_auth(
            &config,
            "https://bucket2.s3.us-east-1.amazonaws.com/bar?a=b",
            true,
            "https://bucket2.s3.us-east-1.amazonaws.com/bar?a=b",
        );
    }
}
