//! A local repository of files from a remote GitHub repository.

use async_recursion::async_recursion;
use chrono::Utc;
use indexmap::IndexMap;
use log::debug;
use log::info;
use log::warn;
use octocrab::etag::EntityTag;
use octocrab::models::repos::ContentItems;
use octocrab::Octocrab;
use reqwest::header::ETAG;
use reqwest::Client;
use urlencoding::encode;

pub mod builder;
pub mod cache;
pub mod identifier;
pub mod options;

pub use builder::Builder;
pub use cache::Cache;
pub use identifier::Identifier;
pub use options::Options;

/// The URL to ping when checking if GitHub has applied rate limiting.
const RATE_LIMIT_PING_URL: &str = "https://api.github.com";

/// The time to sleep between requests when checking if GitHub has applied rate
/// limiting.
const RATE_LIMIT_SLEEP_TIME: i64 = 60;

/// The substring to look for in the response to detect whether GitHub has
/// applied rate limiting.
const RATE_LIMIT_EXCEEDED: &str = "API rate limit exceeded";

/// The HTTP response header indicating when rate limiting will be lifted by
/// GitHub.
const RATE_LIMIT_RESET_HEADER: &str = "X-RateLimit-Reset";

/// The user agent to set when sending HTTP requests.
const USER_AGENT: &str = "wdl-grammar gauntlet";

/// An error related to a [`Repository`].
#[derive(Debug)]
pub enum Error {
    /// An error related to the cache.
    Cache(cache::Error),

    /// Missing an expected header.
    MissingHeader(&'static str),

    /// An error related to [`octocrab`].
    Octocrab(octocrab::Error),

    /// An error from [`reqwest`].
    Reqwest(reqwest::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Cache(err) => write!(f, "cache error: {err}"),
            Error::MissingHeader(header) => {
                write!(f, "missing header: {header}")
            }
            Error::Octocrab(err) => write!(f, "octocrab error: {err}"),
            Error::Reqwest(err) => write!(f, "reqwest error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

/// A [`Result`](std::result::Result) with an [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

/// A repository of GitHub files.
#[derive(Debug)]
pub struct Repository {
    /// The local cache of the repository files.
    cache: Cache,

    /// The GitHub client.
    client: Octocrab,

    /// The name for the [`Repository`] expressed as an [`Identifier`].
    identifier: Identifier,

    /// The options for operating the [`Repository`].
    options: Options,
}

impl Repository {
    /// Gets the cache from the [`Repository`] by reference.
    #[allow(dead_code)]
    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Gets the client from the [`Repository`] by reference.
    #[allow(dead_code)]
    pub fn client(&self) -> &Octocrab {
        &self.client
    }

    /// Gets the repository identifier from the [`Repository`] by reference.
    #[allow(dead_code)]
    pub fn identifier(&self) -> &Identifier {
        &self.identifier
    }

    /// Gets the options from the [`Repository`] by reference.
    #[allow(dead_code)]
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Hydrates the repository by contacting GitHub.
    ///
    /// This occurs by traversing the files in the repository, comparing the
    /// `etag` HTTP response header to the one stored locally to see if any of
    /// the files have changed, updating any files that have changed and,
    /// finally, returning a map of those files and their contents.
    ///
    /// **Note:** only files with a `.wdl` extension are considered.
    async fn hydrate_from_remote(&mut self) -> Result<IndexMap<String, String>> {
        info!("{}: hydrating from remote.", self.identifier);

        let content = get_remote_repo_content(&self.client, &self.identifier, None).await?;

        dive_for_wdl(self, content).await
    }

    /// Hydrates the repository simply by looking at local files.
    async fn hydrate_from_cache(&mut self) -> Result<IndexMap<String, String>> {
        info!("{}: hydrating from local cache.", self.identifier);

        let mut map = IndexMap::new();

        for (path, _) in self.cache.registry().entries() {
            // SAFETY: the first unwrap is safe because, since we are assuming a
            // well-formed cache, this should always unwrap.
            //
            // The second unwrap is safe because we just checked that the path
            // exists in the registry, so retreiving the value for that path
            // will always unwrap.
            let entry = self.cache.get(path).unwrap().unwrap();
            map.insert(path.clone(), entry.contents().to_string());
        }

        Ok(map)
    }

    /// Hydrates the repository according to the [`Options`] set.
    pub async fn hydrate(&mut self) -> Result<IndexMap<String, String>> {
        match self.options.hydrate_remote {
            true => self.hydrate_from_remote().await,
            false => self.hydrate_from_cache().await,
        }
    }
}

/// Dives into a [`ContentItems`] to pull out any `.wdl` files. This function is
/// called recursively as directories are encountered within the repository.
#[async_recursion]
async fn dive_for_wdl(
    repository: &mut Repository,
    content: ContentItems,
) -> Result<IndexMap<String, String>> {
    let mut result = IndexMap::new();

    for item in content.items {
        if let Some(download_url) = item.download_url
            && item.path.ends_with(".wdl")
        {
            let entry = match repository.cache.get(&item.path) {
                Ok(entry) => entry,
                Err(err) => return Err(Error::Cache(err)),
            };

            let etag = retrieve_etag(&download_url).await?;

            if let Some(entry) = entry {
                // SAFETY: this should always unwrap, as we are getting the etag
                // directly from the GitHub server.
                if entry.etag() == &etag.parse::<EntityTag>().unwrap() {
                    debug!("{}: etags match, using cached version.", item.path);
                    result.insert(item.path, entry.contents().to_owned());
                    continue;
                } else {
                    debug!(
                        "{}: etags don't match, overwriting with latest version.",
                        item.path
                    );
                }
            } else {
                debug!("{}: cache entry not found, downloading.", item.path);
            }

            let response = reqwest::get(download_url).await.map_err(Error::Reqwest)?;
            let contents = response.text().await.map_err(Error::Reqwest)?;

            repository
                .cache
                .insert(&item.path, etag, &contents)
                .map_err(Error::Cache)?;
            result.insert(item.path, contents);
        } else if item.r#type == "dir" {
            result.extend(
                dive_for_wdl(
                    repository,
                    get_remote_repo_content(
                        &repository.client,
                        &repository.identifier,
                        Some(&item.path),
                    )
                    .await?,
                )
                .await?,
            )
        }
    }

    Ok(result)
}

/// A function to pull out content for a particular path within a GitHub
/// repository. If the `path` is [`None`], the root of the repository is
/// searched.
///
/// **Note:** rate limiting is handled at this level.
#[async_recursion]
async fn get_remote_repo_content<'a: 'async_recursion>(
    client: &Octocrab,
    identifier: &Identifier,
    path: Option<&'a str>,
) -> Result<ContentItems> {
    debug!(
        "{}: searching for files{}",
        identifier,
        path.map(|s| format!(" at path `{}`", s))
            .unwrap_or_default()
    );

    let binding = client.repos(identifier.organization(), identifier.name());
    let mut request = binding.get_content();

    if let Some(path) = path {
        request = request.path(encode(path));
    }

    match request.send().await {
        Ok(result) => Ok(result),
        Err(err) => match &err {
            octocrab::Error::GitHub { source, .. } => {
                debug!("error: {err}");
                if source.message.contains(RATE_LIMIT_EXCEEDED) {
                    wait_for_timeout().await?;
                    get_remote_repo_content(client, identifier, path).await
                } else {
                    Err(Error::Octocrab(err))
                }
            }
            _ => {
                debug!("error: {err}");
                Err(Error::Octocrab(err))
            }
        },
    }
}

/// A simple function to loop while we wait for rate-limiting applied by GitHub
/// to lift.
async fn wait_for_timeout() -> Result<()> {
    let client = Client::builder().user_agent(USER_AGENT).build().unwrap();

    let response = client
        .head(RATE_LIMIT_PING_URL)
        .send()
        .await
        .map_err(Error::Reqwest)?;

    let timestamp = response
        .headers()
        .get(RATE_LIMIT_RESET_HEADER)
        .map(Ok)
        .unwrap_or(Err(Error::MissingHeader(RATE_LIMIT_RESET_HEADER)))
        .map(|s| s.to_str().unwrap().parse::<i64>().unwrap())?;

    let mut first_loop = true;

    loop {
        let duration = timestamp
            .checked_sub(Utc::now().timestamp())
            .unwrap_or_else(|| panic!("overflow when computing duration"));

        if duration == 0 || duration.is_negative() {
            break;
        }

        // SAFETY: this should always cast, as we just checked above if the
        // duration was negative and broke out of the loop if so.
        let sleep_for = std::cmp::min(duration, RATE_LIMIT_SLEEP_TIME) as u64;

        if first_loop {
            warn!("rate limit: activated.");
        }

        warn!(
            "rate limit: sleeping for {} seconds ({} seconds remaining).",
            sleep_for, duration
        );

        std::thread::sleep(std::time::Duration::from_secs(sleep_for));
        first_loop = false;
    }

    Ok(())
}

/// A utility function to grab an `etag` HTTP response header from a URL.
async fn retrieve_etag(download_url: &str) -> Result<String> {
    let response = Client::builder()
        .build()
        .map_err(Error::Reqwest)?
        .head(download_url)
        .send()
        .await
        .map_err(Error::Reqwest)?;

    Ok(response
        .headers()
        .get(ETAG)
        .map(Ok)
        .unwrap_or(Err(Error::MissingHeader(ETAG.as_str())))?
        .to_str()
        // SAFETY: for GitHub URLs (which is the only thing this method is used
        // to retrieve), the `etag` header will always be present, so this will
        // always unwrap.
        .unwrap()
        .to_string())
}
