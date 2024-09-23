//! This binary is used to bump the version of all crates in this repository and
//! then publish them to crates.io. The binary is intended to be run from the
//! root of the repository and will recursively search for all `Cargo.toml`
//! files to bump the version of all crates.
//!
//! The binary was adapted from this script written by the bytecodealliance
//! team: https://github.com/bytecodealliance/cargo-component/blob/5cf73a6e8fee84c12f6f0c13bf74ebe938fa9514/ci/publish.rs
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

// note that this list must be topologically sorted by dependencies
const SORTED_CRATES_TO_PUBLISH: &[&str] = &[
    "wdl-grammar",
    "wdl-ast",
    "wdl-lint",
    "wdl-analysis",
    "wdl-lsp",
    "wdl",
];

const IGNORE_PATHS: &[&str] = &["target", "tests", "examples", "benches", "book", "docs"];

#[derive(Debug, Clone)]
struct Crate {
    manifest: DocumentMut,
    path: PathBuf,
    name: String,
    version: String,
    should_bump: bool,
}

#[derive(Parser)]
struct Opts {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    Bump(Bump),
    Publish(Publish),
}

#[derive(Parser)]
struct Bump {
    #[clap(short, long)]
    patch: bool,

    #[clap(short, long)]
    crates_to_bump: Vec<String>,
}

#[derive(Parser)]
struct Publish {
    #[clap(short, long)]
    dry_run: bool,
}

fn main() {
    let mut all_crates: Vec<Rc<RefCell<Crate>>> = Vec::new();
    find_crates(".".as_ref(), &mut all_crates);

    let publish_order = SORTED_CRATES_TO_PUBLISH
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, i))
        .collect::<HashMap<_, _>>();
    all_crates.sort_by_key(|krate| publish_order.get(&krate.borrow().name[..]));

    let opts = Opts::parse();
    match opts.subcmd {
        SubCommand::Bump(Bump {
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
        SubCommand::Publish(Publish { dry_run }) => {
            // We have so many crates to publish we're frequently either
            // rate-limited or we run into issues where crates can't publish
            // successfully because they're waiting on the index entries of
            // previously-published crates to propagate. This means we try to
            // publish in a loop and we remove crates once they're successfully
            // published. Failed-to-publish crates get enqueued for another try
            // later on.
            for _ in 0..10 {
                all_crates.retain(|krate| !publish(&krate.borrow(), dry_run));

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

fn find_crates(dir: &Path, dst: &mut Vec<Rc<RefCell<Crate>>>) {
    if dir.join("Cargo.toml").exists() {
        if let Some(krate) = read_crate(&dir.join("Cargo.toml")) {
            dst.push(Rc::new(RefCell::new(krate)));
        }
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

fn read_crate(manifest_path: &Path) -> Option<Crate> {
    let contents = fs::read_to_string(manifest_path).expect("failed to read manifest");
    let mut manifest =
        toml_edit::DocumentMut::from_str(&contents).expect("failed to parse manifest");
    let package = match manifest.get_mut("package") {
        Some(p) => p,
        None => return None, // workspace manifests don't have a package section
    };
    let name = package["name"].as_str().expect("name").to_string();
    let version = package["version"].as_str().expect("version").to_string();
    Some(Crate {
        manifest,
        path: manifest_path.to_path_buf(),
        name,
        version,
        should_bump: false,
    })
}

fn bump_version(krate: &Crate, crates: &[Rc<RefCell<Crate>>], patch: bool) {
    let mut new_manifest = krate.manifest.clone();

    if krate.should_bump {
        let next_version = bump(&krate.version, patch);
        new_manifest["package"]["version"] = toml_edit::value(next_version);
    }

    // Update the dependencies of this crate to point to the new version of
    // crates that we're bumping.
    let dependencies = match new_manifest["dependencies"].as_table_mut() {
        Some(d) => d,
        None => return,
    };
    for (dep_name, dep) in dependencies.iter_mut() {
        if crates.iter().any(|k| *k.borrow().name == *dep_name) {
            let dep_version = bump(dep["version"].as_str().expect("dep version"), patch);
            dep["version"] = toml_edit::value(dep_version.clone());
        }
    }

    fs::write(&krate.path, new_manifest.to_string()).expect("failed to write new manifest");
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
    } else if minor != 0 {
        format!("0.{}.0", minor + 1)
    } else {
        format!("0.0.{}", patch + 1)
    }
}

fn publish(krate: &Crate, dry_run: bool) -> bool {
    if !SORTED_CRATES_TO_PUBLISH.iter().any(|s| *s == krate.name) {
        return true;
    }

    // First make sure the crate isn't already published at this version. This
    // binary may be re-run and there's no need to re-attempt previous work.
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(format!("https://crates.io/api/v1/crates/{}", krate.name))
        .send()
        .expect("failed to get crate info");
    if response.status().is_success() {
        let text = response.text().expect("failed to get response text");
        if text.contains(&format!("\"newest_version\":\"{}\"", krate.version)) {
            println!(
                "skip publish {} because {} is latest version",
                krate.name, krate.version,
            );
            return true;
        }
    }

    let mut command = Command::new("cargo");
    let command = command
        .arg("publish")
        .current_dir(krate.path.parent().unwrap());
    let status = if dry_run {
        command.arg("--dry-run").status().unwrap()
    } else {
        command.status().unwrap()
    };

    if !status.success() {
        println!("FAIL: failed to publish `{}`: {}", krate.name, status);
        return false;
    }

    true
}
