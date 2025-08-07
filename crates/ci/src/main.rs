//! This binary is used to bump the version of all crates in this repository and
//! then publish them to crates.io. The binary is intended to be run from the
//! root of the repository and will recursively search for all `Cargo.toml`
//! files to bump the version of all crates.
//!
//! The binary was adapted from this script written by the bytecodealliance
//! team: <https://github.com/bytecodealliance/cargo-component/blob/5cf73a6e8fee84c12f6f0c13bf74ebe938fa9514/ci/publish.rs>
//!
//! The binary is intended to be run in two phases:
//!
//! 1. `cargo run --bin ci -- bump` - this will bump the version of all crates
//!    in the repository. By default this will bump the major version of all
//!    crates. This can be overridden with the `--patch` flag to bump the patch
//!    version instead.
//!
//! 2. `cargo run --bin ci -- publish` - this will publish all crates in the
//!    repository to crates.io.
//!
//! The binary will automatically skip crates that have already been published
//! at the version that we're trying to publish. This means that the binary can
//! be re-run if necessary and it will only attempt to publish new crates.
//!
//! The binary will also automatically update the dependencies of crates to
//! point to the new version of crates that we're bumping. This means that if
//! `wdl-ast` depends on `wdl-grammar` and we're bumping `wdl-grammar` then
//! `wdl-ast` will be updated to depend on the new version of `wdl-grammar`.
//!
//! The binary will also automatically retry publishing crates that fail to
//! publish. This is because crates.io can sometimes be rate-limited or have
//! other issues that prevent crates from being published. The binary will
//! automatically retry publishing crates that fail to publish up to 10 times.
//!
//! The binary will also automatically skip crates that are not in the list of
//! crates to publish. This is to ensure that we only publish crates that are
//! intended to be published.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use clap::Parser;
use toml_edit::DocumentMut;

/// Crates names to publish.
// Note that this list must be topologically sorted by dependencies.
const SORTED_CRATES_TO_PUBLISH: &[&str] = &[
    "wdl-grammar",
    "wdl-ast",
    "wdl-format",
    "wdl-analysis",
    "wdl-doc",
    "wdl-lint",
    "wdl-engine",
    "wdl-lsp",
    "wdl-cli",
    "wdl",
];

/// Paths to ignore.
const IGNORE_PATHS: &[&str] = &["target", "tests", "examples", "benches", "book", "docs"];

/// An in-memory representation of a crate.
#[derive(Debug, Clone)]
struct Crate {
    /// The manifest file.
    manifest: DocumentMut,

    /// The path to the manifest.
    manifest_path: PathBuf,

    /// The path to the changelog.
    changelog_path: Option<PathBuf>,

    /// The name of the crate.
    name: String,

    /// The version of the crate.
    version: String,

    /// Whether the version should be bumped.
    should_bump: bool,
}

/// The command line arguments.
#[derive(Parser)]
struct Args {
    /// The subcommand.
    #[clap(subcommand)]
    command: Subcommand,
}

/// The subcommand to use.
#[derive(Parser)]
enum Subcommand {
    /// Request to bump a crate/crates.
    Bump(Bump),

    /// Publishes a crate/crates.
    Publish(Publish),
}

/// The arguments to the `bump` subcommand.
#[derive(Parser)]
struct Bump {
    /// Whether or not the bump should be a patch version increase.
    #[clap(short, long)]
    patch: bool,

    /// The list of crate names to bump.
    #[clap(short, long)]
    crates_to_bump: Vec<String>,
}

/// The arguments to the `publish` subcommand.
#[derive(Parser)]
struct Publish {
    /// Whether or not to perform a dry-run of the publishing.
    #[clap(short, long)]
    dry_run: bool,
}

/// The main function.
#[tokio::main]
async fn main() {
    let mut all_crates: Vec<Rc<RefCell<Crate>>> = Vec::new();
    find_crates(".".as_ref(), &mut all_crates);

    let publish_order = SORTED_CRATES_TO_PUBLISH
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, i))
        .collect::<HashMap<_, _>>();
    all_crates.sort_by_key(|krate| publish_order.get(&krate.borrow().name[..]));

    let opts = Args::parse();
    match opts.command {
        Subcommand::Bump(Bump {
            patch,
            crates_to_bump,
        }) => {
            let crates_to_bump: Vec<Rc<RefCell<Crate>>> = if !crates_to_bump.is_empty() {
                all_crates
                    .iter()
                    .skip_while(|krate| !crates_to_bump.contains(&krate.borrow().name))
                    .cloned()
                    .collect()
            } else {
                all_crates
                    .iter()
                    .filter(|krate| {
                        SORTED_CRATES_TO_PUBLISH.contains(&krate.borrow().name.as_str())
                    })
                    .cloned()
                    .collect()
            };
            if crates_to_bump.is_empty() {
                println!("no crates found to bump");
                return;
            }
            for krate in all_crates.iter() {
                krate.borrow_mut().should_bump = crates_to_bump
                    .iter()
                    .any(|k| k.borrow().name == krate.borrow().name);
            }
            for krate in &all_crates {
                bump_version(&krate.borrow(), &crates_to_bump, patch);
            }
            // update the lock file
            assert!(
                Command::new("cargo")
                    .arg("fetch")
                    .status()
                    .unwrap()
                    .success()
            );
        }
        Subcommand::Publish(Publish { dry_run }) => {
            // We have so many crates to publish we're frequently either
            // rate-limited or we run into issues where crates can't publish
            // successfully because they're waiting on the index entries of
            // previously-published crates to propagate. This means we try to
            // publish in a loop and we remove crates once they're successfully
            // published. Failed-to-publish crates get enqueued for another try
            // later on.
            for _ in 0..3 {
                let mut retry = Vec::new();
                for krate in all_crates {
                    let (name, version, manifest_path) = {
                        let krate = krate.borrow();
                        (
                            krate.name.clone(),
                            krate.version.clone(),
                            krate.manifest_path.clone(),
                        )
                    };

                    if !publish(&name, &version, &manifest_path, dry_run).await {
                        retry.push(krate);
                    }
                }

                all_crates = retry;
                if all_crates.is_empty() {
                    break;
                }

                println!(
                    "{} crates failed to publish, waiting for a bit to retry",
                    all_crates.len(),
                );
                thread::sleep(Duration::from_secs(40));
            }

            assert!(all_crates.is_empty(), "failed to publish all crates");
        }
    }
}

/// Finds crates in a particular directory.
fn find_crates(dir: &Path, dst: &mut Vec<Rc<RefCell<Crate>>>) {
    if dir.join("Cargo.toml").exists()
        && let Some(krate) = read_crate(&dir.join("Cargo.toml"))
    {
        dst.push(Rc::new(RefCell::new(krate)));
    }

    for entry in dir.read_dir().unwrap() {
        let entry = entry.unwrap();
        if IGNORE_PATHS.iter().any(|p| entry.path().ends_with(p)) {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            find_crates(&entry.path(), dst);
        }
    }
}

/// Reads a crate from a manifest.
fn read_crate(manifest_path: &Path) -> Option<Crate> {
    let contents = fs::read_to_string(manifest_path).expect("failed to read manifest");
    let mut manifest =
        toml_edit::DocumentMut::from_str(&contents).expect("failed to parse manifest");

    let package = manifest.get_mut("package")?;
    let name = package["name"].as_str().expect("name").to_string();
    let version = package["version"].as_str().expect("version").to_string();

    let changelog_path = manifest_path.with_file_name("CHANGELOG.md");

    Some(Crate {
        manifest,
        manifest_path: manifest_path.to_path_buf(),
        changelog_path: if changelog_path.exists() {
            Some(changelog_path)
        } else {
            None
        },
        name,
        version,
        should_bump: false,
    })
}

/// Bumps the version of a crate.
fn bump_version(krate: &Crate, crates: &[Rc<RefCell<Crate>>], patch: bool) {
    let mut new_manifest = krate.manifest.clone();

    if krate.should_bump {
        let next_version = bump(&krate.version, patch);
        new_manifest["package"]["version"] = toml_edit::value(next_version);
    }

    // Update the dependencies of this crate to point to the new version of
    // crates that we're bumping.
    let dependencies = new_manifest["dependencies"].as_table_mut();
    if let Some(dependencies) = dependencies {
        for (dep_name, dep) in dependencies.iter_mut() {
            if crates.iter().any(|k| *k.borrow().name == *dep_name) {
                let dep_version = bump(dep["version"].as_str().expect("dep version"), patch);
                dep["version"] = toml_edit::value(dep_version.clone());
            }
        }
    }

    fs::write(&krate.manifest_path, new_manifest.to_string())
        .expect("failed to write new manifest");

    if let Some(changelog_path) = &krate.changelog_path
        && krate.should_bump
    {
        let todays_date = chrono::Local::now().format("%m-%d-%Y");
        let mut changelog = fs::read_to_string(changelog_path).expect("failed to read changelog");
        changelog = changelog.replace(
            "## Unreleased",
            &format!(
                "## Unreleased\n\n## {} - {}",
                bump(&krate.version, patch),
                todays_date
            ),
        );
        fs::write(changelog_path, changelog).expect("failed to write changelog");
    }
}

/// Performs a major version bump increment on the semver version `version`.
///
/// This function will perform a semver-major-version bump on the `version`
/// specified. This is used to calculate the next version of a crate in this
/// repository since we're currently making major version bumps for all our
/// releases. This may end up getting tweaked as we stabilize crates and start
/// doing more minor/patch releases, but for now this should do the trick.
fn bump(version: &str, patch_bump: bool) -> String {
    let mut iter = version.split('.').map(|s| s.parse::<u32>().unwrap());
    let major = iter.next().expect("major version");
    let minor = iter.next().expect("minor version");
    let patch = iter.next().expect("patch version");

    if patch_bump {
        return format!("{}.{}.{}", major, minor, patch + 1);
    }
    if major != 0 {
        format!("{}.0.0", major + 1)
    } else {
        format!("0.{}.0", minor + 1)
    }
}

/// Publishes a crate.
async fn publish(name: &str, version: &str, manifest_path: &Path, dry_run: bool) -> bool {
    if !SORTED_CRATES_TO_PUBLISH.contains(&name) {
        return true;
    }

    // First make sure the crate isn't already published at this version. This
    // binary may be re-run and there's no need to re-attempt previous work.
    let client = reqwest::Client::new();
    let req = client
        .get(format!("https://crates.io/api/v1/crates/{name}"))
        .header("User-Agent", "curl/8.7.1"); // crates.io requires a user agent apparently
    let response = req.send().await.expect("failed to get crate info");
    if response.status().is_success() {
        let text = response.text().await.expect("failed to get response text");
        if text.contains(&format!("\"newest_version\":\"{version}\"")) {
            println!("skip publish {name} because {version} is latest version",);
            return true;
        }
    } else {
        println!(
            "skip publish {} because failed to get crate info: {}",
            name,
            response.status()
        );
        return false;
    }

    let mut command = Command::new("cargo");
    let command = command
        .arg("publish")
        .current_dir(manifest_path.parent().unwrap());
    let status = if dry_run {
        command.arg("--dry-run").status().unwrap()
    } else {
        command.status().unwrap()
    };

    if !status.success() {
        println!("FAIL: failed to publish `{name}`: {status}");
        return false;
    }

    true
}
