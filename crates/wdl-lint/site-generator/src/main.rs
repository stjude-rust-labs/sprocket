//! Static site generator for `wdl-lint`/`wdl-analysis` lints.

mod components;

use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use clap::Parser;
use maud::DOCTYPE;
use maud::html;
use serde_json::json;
use strum::VariantArray;
use tracing::level_filters::LevelFilter;
use walkdir::WalkDir;
use wdl_lint::Config;
use wdl_lint::Tag;

use crate::components::LintRule;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    if let Err(e) = real_main() {
        tracing::error!("failed to generate wdl-lint site: {e}");
        std::process::exit(1);
    }
}

/// A JS array of all enabled-by-default tags.
fn default_tags() -> String {
    let tags_str = Tag::VARIANTS
        .iter()
        .map(|tag| format!("'{tag}'"))
        .collect::<Vec<_>>();
    format!("[{}]", tags_str.join(", "))
}

/// The CLI arguments of the generator.
#[derive(clap::Parser)]
struct Args {
    /// Open the `index.html` after generation.
    #[arg(long)]
    open: bool,
    /// The current release tag of `wdl-lint`.
    #[clap(long)]
    wdl_lint_tag: Option<String>,
    /// The current release tag of `wdl-analysis`.
    #[clap(long)]
    wdl_analysis_tag: Option<String>,
}

/// The main program logic.
fn real_main() -> anyhow::Result<()> {
    let args = Args::parse();

    let dist_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
    if dist_dir.exists() {
        std::fs::remove_dir_all(&dist_dir)?;
    }

    std::fs::create_dir(&dist_dir)?;
    tracing::info!("Copying static files to `{}`", dist_dir.display());
    copy_files_to_dist(&dist_dir)?;

    let mut sorted_lint_rules = wdl_lint::rules(&Config::default())
        .into_iter()
        .map(LintRule::WdlLint)
        .collect::<Vec<_>>();
    sorted_lint_rules.sort_by_key(|rule| rule.id());

    let mut sorted_analysis_rules = wdl_analysis::rules()
        .into_iter()
        .map(LintRule::WdlAnalysis)
        .collect::<Vec<_>>();
    sorted_analysis_rules.sort_by_key(|rule| rule.id());

    dump_default_state_json(&args, &sorted_lint_rules, &sorted_analysis_rules)?;
    compile_external()?;

    let html = html! {
        (DOCTYPE)
        html
            x-data="{ DEFAULT_THEME: 'dark'}"
            x-bind:class="(localStorage.getItem('theme') ?? DEFAULT_THEME) === 'light' ? 'light' : 'dark'"
        {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                meta name="description" content="";

                title { "WDL Lint Registry" }

                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link href="https://fonts.googleapis.com/css2?family=DM+Sans:ital,opsz,wght@0,9..40,100..1000;1,9..40,100..1000&display=swap" rel="stylesheet";
                link rel="stylesheet" href="style.css";

                script type="module" src="index.js" {}
            }

            body {
                div class="main__container" x-data="App" {
                    header class="sticky top-0 z-40 w-full backdrop-blur-md bg-slate-950/80 border-b border-slate-800 mb-8" {
                        div class="max-w-5xl mx-auto px-4 h-16 flex items-center justify-between gap-4" {
                            div class="font-bold text-xl text-slate-100 flex items-center gap-2" {
                                span class="h-6" {
                                    "WDL Lints"
                                }
                            }

                            (components::searchbox())

                            div class="flex flex-row-reverse items-start justify-between" {
                                button
                                x-on:click="
                                document.documentElement.classList.toggle('light')
                                localStorage.setItem('theme', document.documentElement.classList.contains('light') ? 'light' : 'dark')
                                "
                                class="p-2 rounded-lg border border-slate-800 bg-slate-900 text-slate-400 hover:text-yellow-400 transition-colors" {
                                    "☀︎"
                                }
                            }
                        }
                    }

                    div class="flex justify-center mb-8" {
                        div class="bg-slate-900 p-1 rounded-xl border border-slate-800 inline-flex" {
                            (components::tab("wdl-lint"))
                            (components::tab("wdl-analysis"))
                        }
                    }

                    div class="border-none" {
                        div x-show="tab === 'wdl-lint'" {
                            (components::wdl_lint_view())
                        }

                        div x-show="tab === 'wdl-analysis'" {
                            (components::wdl_analysis_view())
                        }
                    }
                }
            }
        }
    };

    let index = dist_dir.join("index.html");
    std::fs::write(&index, html.0)?;

    if args.open {
        opener::open(index)?;
    }

    Ok(())
}

/// Gets the `web-common` dir at the root of the project.
fn web_common_dir() -> PathBuf {
    let web_common_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("project root should exist")
        .join("web-common");
    assert!(web_common_dir.is_dir());
    web_common_dir
}

/// Gets the `static` dir
fn static_dir() -> std::io::Result<PathBuf> {
    let static_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("static");
    if !static_dir.is_dir() {
        tracing::error!(
            "Couldn't find static directory, searched `{}`",
            static_dir.display()
        );
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            static_dir.to_string_lossy(),
        ));
    }

    Ok(static_dir)
}

/// Generates the default state (all lints, the current version, etc.), and
/// dumps it into [`static_dir()`].
fn dump_default_state_json(
    args: &Args,
    wdl_lint_rules: &[LintRule],
    wdl_analysis_rules: &[LintRule],
) -> anyhow::Result<()> {
    fn format_crate_version(
        crate_name: &str,
        commit_hash: &str,
        version_str: Option<&str>,
    ) -> String {
        match version_str {
            Some(version) => format!("{crate_name} v{version}"),
            None => format!("{crate_name} @ main (rev {})", commit_hash.trim()),
        }
    }

    let default_tags = Tag::VARIANTS
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let all_lints = wdl_lint_rules
        .iter()
        .map(LintRule::to_json)
        .collect::<Vec<_>>();

    let all_analysis_lints = wdl_analysis_rules
        .iter()
        .map(LintRule::to_json)
        .collect::<Vec<_>>();

    let commit_hash_output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;
    if !commit_hash_output.status.success() {
        tracing::error!(
            "Failed to run `npm install`: {}",
            String::from_utf8_lossy(&commit_hash_output.stderr)
        );
        std::process::exit(commit_hash_output.status.code().unwrap_or(1));
    }

    let commit_hash = String::from_utf8(commit_hash_output.stdout)?;

    let json = json!({
        "defaultTab": "wdl-lint",
        "defaultTags": default_tags,
        "wdlLint": {
            "allLints": all_lints,
            "currentVersion": format_crate_version("wdl-lint", &commit_hash, args.wdl_lint_tag.as_deref()),
        },
        "wdlAnalysis": {
            "allLints": all_analysis_lints,
            "currentVersion": format_crate_version("wdl-analysis", &commit_hash, args.wdl_analysis_tag.as_deref()),
        }
    });

    let output_path = static_dir()?.join("default-state.json");
    std::fs::write(output_path, json.to_string())?;
    Ok(())
}

/// Handles the compilation of the files external to this crate.
fn compile_external() -> std::io::Result<()> {
    fn npm_install(dir: impl AsRef<Path>) -> std::io::Result<()> {
        let dir = dir.as_ref();
        tracing::info!("Running `npm install` in {}", dir.display());
        let output = Command::new("npm")
            .arg("install")
            .current_dir(dir)
            .output()?;
        if !output.status.success() {
            tracing::error!(
                "Failed to run `npm install`: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            std::process::exit(output.status.code().unwrap_or(1));
        }

        Ok(())
    }

    fn build_js(dir: impl AsRef<Path>) -> std::io::Result<()> {
        let dir = dir.as_ref();
        tracing::info!("Compiling JS in `{}`", dir.display());
        let output = Command::new("npm")
            .args(["run", "build"])
            .current_dir(dir)
            .output()?;
        if !output.status.success() {
            tracing::error!(
                "Failed to run `npm run build`: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            std::process::exit(output.status.code().unwrap_or(1));
        }

        Ok(())
    }

    npm_install(web_common_dir())?;
    npm_install(env!("CARGO_MANIFEST_DIR"))?;

    tracing::info!("Generating CSS via tailwind");
    let output = Command::new("npm")
        .args(["run", "dist"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()?;
    if !output.status.success() {
        tracing::error!(
            "Failed to run `npm run dist`: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::process::exit(output.status.code().unwrap_or(1));
    }

    build_js(web_common_dir())?;
    build_js(env!("CARGO_MANIFEST_DIR"))?;

    Ok(())
}

/// Copies all compiled files recursively from [`web_common_dir()`] and
/// `./static` into `dist_dir`.
fn copy_files_to_dist(dist_dir: &Path) -> std::io::Result<()> {
    fn do_copy(src: &Path, dist_dir: &Path, skip_js: bool) -> std::io::Result<()> {
        for entry in WalkDir::new(src).min_depth(1) {
            let entry = entry?;

            // Skip any CSS, since that's handled by tailwind
            if entry.path().extension().and_then(OsStr::to_str) == Some("css") {
                continue;
            }

            if skip_js && entry.path().extension().and_then(OsStr::to_str) == Some("js") {
                continue;
            }

            let relative_path = entry.path().strip_prefix(src).unwrap();
            let dist_mapped_path = dist_dir.join(relative_path);

            if entry.metadata()?.is_dir() {
                std::fs::create_dir_all(dist_mapped_path)?;
                continue;
            }

            std::fs::copy(entry.path(), dist_mapped_path)?;
        }

        Ok(())
    }

    let static_dir = static_dir()?;

    let web_common_dist_dir = web_common_dir().join("dist");
    if !web_common_dist_dir.is_dir() {
        tracing::error!(
            "Couldn't find web-common/dist, searched `{}`",
            web_common_dist_dir.display()
        );
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            web_common_dist_dir.to_string_lossy(),
        ));
    }

    // skip JS, since esbuild handles it for us
    do_copy(&static_dir, dist_dir, true)?;
    do_copy(&web_common_dist_dir, dist_dir, false)?;

    Ok(())
}
