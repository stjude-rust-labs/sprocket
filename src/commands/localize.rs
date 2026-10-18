//! Implementation of the `localize` subcommand.
//!
//! When a workflow runs on a cloud backend (e.g. TES), the files named in the
//! run's `outputs.json` are URLs to remote storage. The `localize` command
//! downloads those files and directories to the local file system and writes a
//! new `outputs.json` that refers to the downloaded copies.

use std::collections::HashMap;
use std::fs;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use anyhow::bail;
use clap::Parser;
use cloud_copy::UrlExt as _;
use futures::StreamExt as _;
use futures::stream;
use serde_json::Value as JsonValue;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use url::Url;
use wdl::engine::Config as EngineConfig;
use wdl::engine::config::SecretString;

use crate::Config;
use crate::commands::Action;
use crate::commands::CommandOutput;
use crate::commands::CommandResult;

/// The name of the outputs file within a run directory.
const OUTPUTS_FILE: &str = "outputs.json";

/// The name of the default destination directory within a run directory.
const DEFAULT_DESTINATION: &str = "localized";

/// The URL schemes that identify a remote output.
///
/// `file` URLs are intentionally excluded as they are already local.
const REMOTE_SCHEMES: &[&str] = &["http", "https", "az", "s3", "gs"];

/// The action reported for a localized output.
const LOCALIZE: Action = Action::new("Localized", "localize");

/// Arguments to the `localize` subcommand.
#[derive(Parser, Debug)]
#[clap(disable_version_flag = true)]
pub struct Args {
    /// The run whose outputs should be localized.
    ///
    /// This may be the path to a run directory (e.g.
    /// `out/runs/<target>/<timestamp>` or `out/runs/<target>/_latest`) or the
    /// path to the `outputs.json` file inside of one.
    ///
    /// Note that `outputs.json` does not record the WDL type of each output, so
    /// any string output that is a remote storage URL (`s3://`, `az://`,
    /// `gs://`, `http://`, or `https://`) is treated as a remote file or
    /// directory.
    #[clap(value_name = "RUN")]
    pub run: PathBuf,

    /// The directory to download the outputs into.
    ///
    /// Defaults to `localized` within the run directory.
    ///
    /// Each remote output is downloaded to
    /// `<DESTINATION>/<output name>/<file or directory name>`. A localized copy
    /// of `outputs.json` that refers to the downloaded files is written to
    /// `<DESTINATION>/outputs.json`.
    #[clap(short, long, value_name = "DESTINATION")]
    pub destination: Option<PathBuf>,

    /// Overwrite files that already exist in the destination.
    #[clap(long)]
    pub overwrite: bool,

    /// Show what would be downloaded without downloading anything.
    #[clap(long)]
    pub dry_run: bool,

    /// The remote storage credentials to use.
    #[clap(flatten)]
    pub storage: StorageArgs,
}

/// The remote storage credentials accepted by the `localize` subcommand.
///
/// These mirror the equivalent arguments of the `run` subcommand.
#[derive(Parser, Debug)]
pub struct StorageArgs {
    /// The Azure Storage account name to use.
    #[clap(long, env, value_name = "NAME", requires = "azure_access_key")]
    pub azure_account_name: Option<String>,

    /// The Azure Storage access key to use.
    #[clap(
        long,
        env,
        hide_env_values(true),
        value_name = "KEY",
        requires = "azure_account_name"
    )]
    pub azure_access_key: Option<SecretString>,

    /// The AWS Access Key ID to use; overrides configuration.
    #[clap(long, env, value_name = "ID", requires = "aws_secret_access_key")]
    pub aws_access_key_id: Option<String>,

    /// The AWS Secret Access Key to use; overrides configuration.
    #[clap(
        long,
        env,
        hide_env_values(true),
        value_name = "KEY",
        requires = "aws_access_key_id"
    )]
    pub aws_secret_access_key: Option<SecretString>,

    /// The default AWS region; overrides configuration.
    #[clap(long, env, value_name = "REGION")]
    pub aws_default_region: Option<String>,

    /// The Google Cloud Storage HMAC access key to use; overrides
    /// configuration.
    #[clap(long, env, value_name = "KEY", requires = "google_hmac_secret")]
    pub google_hmac_access_key: Option<String>,

    /// The Google Cloud Storage HMAC secret to use; overrides configuration.
    #[clap(
        long,
        env,
        hide_env_values(true),
        value_name = "SECRET",
        requires = "google_hmac_access_key"
    )]
    pub google_hmac_secret: Option<SecretString>,
}

impl StorageArgs {
    /// Applies the storage arguments to the given engine configuration.
    fn apply(&self, config: &mut EngineConfig) {
        if self.azure_account_name.is_some() || self.azure_access_key.is_some() {
            let auth = config.storage.azure.auth.get_or_insert_default();
            if let Some(name) = &self.azure_account_name {
                auth.account_name = name.clone();
            }

            if let Some(key) = &self.azure_access_key {
                auth.access_key = key.clone();
            }
        }

        if let Some(region) = &self.aws_default_region {
            config.storage.s3.region = Some(region.clone());
        }

        if self.aws_access_key_id.is_some() || self.aws_secret_access_key.is_some() {
            let auth = config.storage.s3.auth.get_or_insert_default();
            if let Some(id) = &self.aws_access_key_id {
                auth.access_key_id = id.clone();
            }

            if let Some(secret) = &self.aws_secret_access_key {
                auth.secret_access_key = secret.clone();
            }
        }

        if self.google_hmac_access_key.is_some() || self.google_hmac_secret.is_some() {
            let auth = config.storage.google.auth.get_or_insert_default();
            if let Some(key) = &self.google_hmac_access_key {
                auth.access_key = key.clone();
            }

            if let Some(secret) = &self.google_hmac_secret {
                auth.secret = secret.clone();
            }
        }
    }
}

/// A run located on the file system.
#[derive(Debug, PartialEq, Eq)]
struct ResolvedRun {
    /// The path to the run's `outputs.json` file.
    outputs_file: PathBuf,
    /// The path to the directory containing the outputs file.
    run_dir: PathBuf,
}

/// Locates the outputs file of the run at the given path.
///
/// The path may be a run directory or an outputs file.
fn resolve_run(path: &Path) -> Result<ResolvedRun> {
    if path.is_dir() {
        let outputs_file = path.join(OUTPUTS_FILE);
        if !outputs_file.is_file() {
            bail!(
                "`{dir}` does not contain an `{OUTPUTS_FILE}` file; the run may not have \
                 completed successfully",
                dir = path.display()
            );
        }

        return Ok(ResolvedRun {
            outputs_file,
            run_dir: path.to_path_buf(),
        });
    }

    if path.is_file() {
        let run_dir = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        };

        return Ok(ResolvedRun {
            outputs_file: path.to_path_buf(),
            run_dir,
        });
    }

    bail!(
        "run path `{path}` does not exist or is not a run directory or outputs file",
        path = path.display()
    );
}

/// Parses a string as a remote storage URL.
///
/// Returns `None` if the string is not a URL with a remote scheme.
fn parse_remote_url(s: &str) -> Option<Url> {
    let url = Url::parse(s.trim()).ok()?;
    REMOTE_SCHEMES
        .contains(&url.scheme())
        .then_some(url)
        .filter(|url| url.has_host())
}

/// Decodes the percent-encoded bytes of a URL path segment.
///
/// Invalid escapes are left as is.
fn percent_decode(s: &str) -> String {
    /// Gets the value of a hexadecimal digit.
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }

    let bytes = s.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            decoded.push((high << 4) | low);
            i += 3;
            continue;
        }

        decoded.push(bytes[i]);
        i += 1;
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

/// Makes a string safe to use as a single path component.
fn sanitize_component(s: &str) -> String {
    let sanitized: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' | ':' => '_',
            c => c,
        })
        .collect();

    match sanitized.as_str() {
        "" | "." | ".." => "_".to_string(),
        _ => sanitized,
    }
}

/// Gets the name of the file or directory a URL refers to.
fn url_name(url: &Url) -> String {
    let name = url
        .path_segments()
        .and_then(|mut segments| segments.rfind(|s| !s.is_empty()))
        .map(percent_decode)
        .or_else(|| url.host_str().map(str::to_string))
        .unwrap_or_else(|| "output".to_string());

    sanitize_component(&name)
}

/// A remote output found in an outputs file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteOutput {
    /// The location of the value within the outputs file.
    ///
    /// The first component is the name of the output; any remaining components
    /// are array indexes or object keys.
    location: Vec<String>,
    /// The URL of the remote file or directory.
    url: Url,
}

impl RemoteOutput {
    /// Gets the path the output is downloaded to, relative to the destination.
    fn relative_path(&self) -> PathBuf {
        let mut path = PathBuf::new();
        for component in &self.location {
            path.push(sanitize_component(component));
        }

        path.push(url_name(&self.url));
        path
    }
}

/// Visits every remote URL in a JSON value.
///
/// The callback returns the replacement string for the URL, if any.
fn visit_remote_urls(
    value: &mut JsonValue,
    location: &mut Vec<String>,
    callback: &mut impl FnMut(&[String], &Url) -> Option<String>,
) {
    match value {
        JsonValue::String(s) => {
            if let Some(url) = parse_remote_url(s)
                && let Some(replacement) = callback(location, &url)
            {
                *s = replacement;
            }
        }
        JsonValue::Array(items) => {
            for (i, item) in items.iter_mut().enumerate() {
                location.push(i.to_string());
                visit_remote_urls(item, location, callback);
                location.pop();
            }
        }
        JsonValue::Object(map) => {
            for (key, item) in map.iter_mut() {
                location.push(key.clone());
                visit_remote_urls(item, location, callback);
                location.pop();
            }
        }
        _ => {}
    }
}

/// Finds all of the remote outputs in an outputs file.
fn find_remote_outputs(outputs: &JsonValue) -> Vec<RemoteOutput> {
    let mut found = Vec::new();
    let mut outputs = outputs.clone();
    visit_remote_urls(&mut outputs, &mut Vec::new(), &mut |location, url| {
        found.push(RemoteOutput {
            location: location.to_vec(),
            url: url.clone(),
        });
        None
    });
    found
}

/// Replaces every localized remote output with the path it was downloaded to.
fn rewrite_outputs(outputs: &mut JsonValue, localized: &HashMap<Vec<String>, PathBuf>) {
    visit_remote_urls(outputs, &mut Vec::new(), &mut |location, _| {
        localized
            .get(location)
            .map(|path| path.to_string_lossy().into_owned())
    });
}

/// Joins a relative path produced by a directory walk onto a local directory.
///
/// Fails if the path would escape the directory.
fn join_walk_entry(dir: &Path, entry: &str) -> Result<PathBuf> {
    let mut path = dir.to_path_buf();
    for component in Path::new(entry).components() {
        match component {
            Component::Normal(component) => path.push(component),
            Component::CurDir => {}
            _ => bail!("remote path `{entry}` is not a valid relative path"),
        }
    }

    Ok(path)
}

/// Creates the configuration used to transfer files from remote storage.
fn transfer_config(config: &EngineConfig, overwrite: bool) -> Result<cloud_copy::Config> {
    let azure = config
        .storage
        .azure
        .auth
        .as_ref()
        .map(|auth| {
            cloud_copy::AzureConfig::default()
                .with_auth(auth.account_name.clone(), auth.access_key.inner().clone())
        })
        .unwrap_or_default();

    let s3 = config
        .storage
        .s3
        .auth
        .as_ref()
        .map(|auth| {
            cloud_copy::S3Config::default().with_auth(
                auth.access_key_id.clone(),
                auth.secret_access_key.inner().clone(),
            )
        })
        .unwrap_or_default()
        .with_maybe_region(config.storage.s3.region.clone());

    let google = config
        .storage
        .google
        .auth
        .as_ref()
        .map(|auth| {
            cloud_copy::GoogleConfig::default()
                .with_auth(auth.access_key.clone(), auth.secret.inner().clone())
        })
        .unwrap_or_default();

    Ok(cloud_copy::Config::builder()
        .with_overwrite(overwrite)
        .with_hash_algorithm(config.http.hash_algorithm)
        .with_retries(
            config
                .http
                .retries
                .try_into()
                .context("invalid HTTP retries")?,
        )
        .with_azure(azure)
        .with_s3(s3)
        .with_google(google)
        .build())
}

/// Everything needed to download from remote storage.
#[derive(Clone)]
struct Downloader {
    /// The transfer configuration.
    config: cloud_copy::Config,
    /// The HTTP client.
    client: cloud_copy::HttpClient,
    /// Used to cancel in-progress transfers.
    cancel: CancellationToken,
    /// Used to report transfer progress.
    events: Option<broadcast::Sender<cloud_copy::TransferEvent>>,
}

impl Downloader {
    /// Downloads a single remote file to a local path.
    async fn download_file(&self, url: &Url, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create directory `{dir}`", dir = parent.display())
            })?;
        }

        cloud_copy::copy(
            self.config.clone(),
            self.client.clone(),
            url,
            path,
            self.cancel.clone(),
            self.events.clone(),
        )
        .await
        .map_err(|e| match e {
            cloud_copy::Error::LocalDestinationExists(path) => anyhow!(
                "`{path}` already exists; use `--overwrite` to replace existing files",
                path = path.display()
            ),
            e => anyhow!(e).context(format!("failed to download `{url}`", url = url.display())),
        })
    }

    /// Lists the files within a remote directory.
    ///
    /// Returns `None` if the URL is not a directory.
    async fn list_directory(&self, url: &Url) -> Result<Option<Vec<String>>> {
        match cloud_copy::walk(self.config.clone(), self.client.clone(), url.clone()).await {
            Ok(entries) if entries.is_empty() => Ok(None),
            Ok(entries) => Ok(Some(entries)),
            // Not a cloud storage URL, so it cannot be a directory
            Err(cloud_copy::Error::UnsupportedUrl(_)) => Ok(None),
            Err(e) => {
                Err(anyhow!(e).context(format!("failed to list `{url}`", url = url.display())))
            }
        }
    }

    /// Downloads a remote file or directory to a local path.
    async fn download(&self, url: &Url, path: &Path) -> Result<()> {
        let Some(entries) = self.list_directory(url).await? else {
            return self.download_file(url, path).await;
        };

        // Treat the URL as a directory so that entries join beneath it
        let mut base = url.clone();
        if let Ok(mut segments) = base.path_segments_mut() {
            segments.pop_if_empty().push("");
        }

        for entry in entries {
            let source = base.join(&entry).with_context(|| {
                format!(
                    "failed to join `{entry}` to `{base}`",
                    base = base.display()
                )
            })?;
            let destination = join_walk_entry(path, &entry)?;
            self.download_file(&source, &destination).await?;
        }

        Ok(())
    }
}

/// The main function for the `localize` subcommand.
pub async fn localize(args: Args, mut config: Config, output: CommandOutput) -> CommandResult<()> {
    let run = resolve_run(&args.run)?;
    let destination = args
        .destination
        .clone()
        .unwrap_or_else(|| run.run_dir.join(DEFAULT_DESTINATION));

    let contents = fs::read_to_string(&run.outputs_file).with_context(|| {
        format!(
            "failed to read outputs file `{path}`",
            path = run.outputs_file.display()
        )
    })?;
    let mut outputs: JsonValue = serde_json::from_str(&contents).with_context(|| {
        format!(
            "failed to parse outputs file `{path}`",
            path = run.outputs_file.display()
        )
    })?;

    let remote = find_remote_outputs(&outputs);
    if remote.is_empty() {
        output.stderr("the run has no remote outputs to localize");
        output.payload(serde_json::to_string_pretty(&outputs).context("serializing outputs")?);
        return Ok(());
    }

    // Never overwrite the run's own outputs file with the localized copy
    let localized_outputs_file = destination.join(OUTPUTS_FILE);
    if let (Ok(a), Ok(b)) = (
        fs::canonicalize(&run.outputs_file),
        fs::canonicalize(&localized_outputs_file),
    ) && a == b
    {
        return Err(anyhow!(
            "the destination cannot be the run directory as `{OUTPUTS_FILE}` would be overwritten"
        )
        .into());
    }

    if args.dry_run {
        for item in &remote {
            output.planned(
                LOCALIZE,
                format!(
                    "`{url}` to `{path}`",
                    url = item.url.display(),
                    path = destination.join(item.relative_path()).display()
                ),
            );
        }

        return Ok(());
    }

    args.storage.apply(&mut config.run.engine);
    let parallelism: usize = config.run.engine.http.parallelism.into();
    let transfer = transfer_config(&config.run.engine, args.overwrite)?;

    let cancel = CancellationToken::new();
    let (events, receiver) = broadcast::channel(
        config
            .run
            .events_capacity
            .try_into()
            .context("invalid events capacity")?,
    );
    let progress = tokio::spawn(cloud_copy::cli::handle_events(
        receiver,
        output.colorize(),
        cancel.clone(),
    ));

    let downloader = Downloader {
        config: transfer,
        client: cloud_copy::HttpClient::new(),
        cancel: cancel.clone(),
        events: Some(events),
    };

    // Resolve the destination now so that the localized outputs refer to
    // absolute paths, matching the paths of a local run
    fs::create_dir_all(&destination).with_context(|| {
        format!(
            "failed to create destination directory `{path}`",
            path = destination.display()
        )
    })?;
    let destination = fs::canonicalize(&destination).with_context(|| {
        format!(
            "failed to resolve destination directory `{path}`",
            path = destination.display()
        )
    })?;

    let transfers = stream::iter(remote.iter().map(|item| {
        let downloader = downloader.clone();
        let path = destination.join(item.relative_path());
        async move {
            downloader.download(&item.url, &path).await?;
            anyhow::Ok((item.location.clone(), path))
        }
    }))
    .buffer_unordered(parallelism.max(1))
    .collect::<Vec<_>>();

    let results = tokio::select! {
        biased;

        _ = tokio::signal::ctrl_c() => {
            cancel.cancel();
            drop(downloader);
            let _ = progress.await;
            return Err(anyhow!("localization was interrupted").into());
        }
        results = transfers => results,
    };

    drop(downloader);
    let _ = progress.await;

    let mut localized = HashMap::new();
    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok((location, path)) => {
                localized.insert(location, path);
            }
            Err(e) => errors.push(e),
        }
    }

    if let Some(first) = errors.into_iter().next() {
        return Err(first
            .context("failed to localize one or more outputs")
            .into());
    }

    rewrite_outputs(&mut outputs, &localized);
    let outputs_json =
        serde_json::to_string_pretty(&outputs).context("failed to serialize localized outputs")?;
    let localized_outputs_file = destination.join(OUTPUTS_FILE);
    fs::write(&localized_outputs_file, &outputs_json).with_context(|| {
        format!(
            "failed to write `{path}`",
            path = localized_outputs_file.display()
        )
    })?;

    output.payload(outputs_json);
    output.completed_stderr(
        LOCALIZE,
        format!(
            "{count} output(s) to `{path}`; localized outputs were also written to `{file}`",
            count = localized.len(),
            path = destination.display(),
            file = localized_outputs_file.display()
        ),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn remote_urls_are_detected() {
        for url in [
            "s3://bucket/key.txt",
            "gs://bucket/key.txt",
            "az://account/container/blob.txt",
            "https://example.com/outputs/file.txt",
            "http://example.com/file.txt",
        ] {
            assert!(parse_remote_url(url).is_some(), "{url} should be remote");
        }

        for not_remote in [
            "/mnt/out/file.txt",
            "relative/file.txt",
            "file:///tmp/file.txt",
            "mailto:someone@example.com",
            "not a url",
            "",
        ] {
            assert!(
                parse_remote_url(not_remote).is_none(),
                "{not_remote} should not be remote"
            );
        }
    }

    #[test]
    fn percent_decoding() {
        assert_eq!(percent_decode("a%20b.txt"), "a b.txt");
        assert_eq!(percent_decode("plain.txt"), "plain.txt");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("bad%zzescape"), "bad%zzescape");
        assert_eq!(percent_decode("%e2%9c%93"), "\u{2713}");
        assert_eq!(percent_decode("end%2"), "end%2");
    }

    #[test]
    fn names_are_sanitized() {
        assert_eq!(sanitize_component(".."), "_");
        assert_eq!(sanitize_component("."), "_");
        assert_eq!(sanitize_component(""), "_");
        assert_eq!(sanitize_component("a/b\\c"), "a_b_c");
        assert_eq!(sanitize_component("wf.out"), "wf.out");
    }

    #[test]
    fn url_names() {
        let name = |s: &str| url_name(&Url::parse(s).unwrap());
        assert_eq!(name("s3://bucket/a/b/file.txt"), "file.txt");
        assert_eq!(name("s3://bucket/a/b/dir/"), "dir");
        assert_eq!(name("s3://bucket"), "bucket");
        assert_eq!(name("https://h/a/my%20file.txt"), "my file.txt");
        assert_eq!(name("https://h/a/..%2Fevil"), ".._evil");
    }

    #[test]
    fn finds_remote_outputs_at_any_depth() {
        let outputs = json!({
            "wf.single": "s3://bucket/work/a.txt",
            "wf.local": "/tmp/local.txt",
            "wf.text": "hello",
            "wf.number": 3,
            "wf.array": ["s3://bucket/work/b.txt", "/tmp/c.txt", "gs://bucket/d.txt"],
            "wf.struct": { "inner": { "file": "az://acct/cont/e.txt" } },
            "wf.file_url": "file:///tmp/f.txt",
        });

        let found: Vec<(String, String)> = find_remote_outputs(&outputs)
            .into_iter()
            .map(|o| (o.location.join("/"), o.url.to_string()))
            .collect();

        assert_eq!(
            found,
            // Outputs are visited in the order they appear in the outputs file
            vec![
                (
                    "wf.single".to_string(),
                    "s3://bucket/work/a.txt".to_string()
                ),
                (
                    "wf.array/0".to_string(),
                    "s3://bucket/work/b.txt".to_string()
                ),
                ("wf.array/2".to_string(), "gs://bucket/d.txt".to_string()),
                (
                    "wf.struct/inner/file".to_string(),
                    "az://acct/cont/e.txt".to_string()
                ),
            ]
        );
    }

    #[test]
    fn relative_paths_cannot_collide_or_escape() {
        let output = |location: &[&str], url: &str| RemoteOutput {
            location: location.iter().map(|s| s.to_string()).collect(),
            url: Url::parse(url).unwrap(),
        };

        // Same file name in different array elements of one output
        let first = output(&["wf.files", "0"], "s3://b/x/a.txt").relative_path();
        let second = output(&["wf.files", "1"], "s3://b/y/a.txt").relative_path();
        assert_eq!(first, Path::new("wf.files/0/a.txt"));
        assert_eq!(second, Path::new("wf.files/1/a.txt"));

        // Hostile object names and keys stay within the destination
        let hostile = output(&["wf.out", ".."], "https://h/..%2F..%2Fetc%2Fpasswd");
        let relative = hostile.relative_path();
        assert!(
            relative
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
            "{relative:?} should only contain normal components"
        );
    }

    #[test]
    fn rewrites_only_localized_locations() {
        let mut outputs = json!({
            "wf.a": "s3://bucket/a.txt",
            "wf.b": ["s3://bucket/b.txt", "/tmp/keep.txt"],
        });

        let mut localized = HashMap::new();
        localized.insert(vec!["wf.a".to_string()], PathBuf::from("/dest/wf.a/a.txt"));
        localized.insert(
            vec!["wf.b".to_string(), "0".to_string()],
            PathBuf::from("/dest/wf.b/0/b.txt"),
        );

        rewrite_outputs(&mut outputs, &localized);
        assert_eq!(
            outputs,
            json!({
                "wf.a": "/dest/wf.a/a.txt",
                "wf.b": ["/dest/wf.b/0/b.txt", "/tmp/keep.txt"],
            })
        );
    }

    #[test]
    fn walk_entries_cannot_escape() {
        let dir = Path::new("/dest/out");
        assert_eq!(
            join_walk_entry(dir, "a/b.txt").unwrap(),
            Path::new("/dest/out/a/b.txt")
        );
        assert!(join_walk_entry(dir, "../b.txt").is_err());
        assert!(join_walk_entry(dir, "a/../../b.txt").is_err());
        assert!(join_walk_entry(dir, "/etc/passwd").is_err());
    }

    #[test]
    fn resolves_runs() {
        let temp = tempfile::tempdir().unwrap();

        // A missing path
        assert!(resolve_run(&temp.path().join("missing")).is_err());

        // A directory without outputs
        let run_dir = temp.path().join("run");
        fs::create_dir(&run_dir).unwrap();
        let error = resolve_run(&run_dir).unwrap_err().to_string();
        assert!(error.contains("does not contain"), "{error}");

        // A directory with outputs and the outputs file itself
        let outputs_file = run_dir.join(OUTPUTS_FILE);
        fs::write(&outputs_file, "{}").unwrap();
        let expected = ResolvedRun {
            outputs_file: outputs_file.clone(),
            run_dir: run_dir.clone(),
        };
        assert_eq!(resolve_run(&run_dir).unwrap(), expected);
        assert_eq!(resolve_run(&outputs_file).unwrap(), expected);
    }
}
