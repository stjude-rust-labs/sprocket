//! `sprocket module verify`.

use clap::Parser;
use clap::ValueEnum;
use wdl_modules::dependency::DependencyName;
use wdl_modules::module::Module;
use wdl_modules::resolver::ResolverError;
use wdl_modules::resolver::VerifyLockedReport;
use wdl_modules::signing::ModuleSignature;

use crate::commands::CommandResult;
use crate::commands::module::ActionColor;
use crate::commands::module::Locator;
use crate::commands::module::build_resolver;
use crate::commands::module::discover;
use crate::commands::module::print_action;
use crate::commands::module::render_signer;
use crate::commands::module::require_lockfile;
use crate::commands::module::trace_project;
use crate::config::Config;

/// Arguments to `sprocket module verify`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Limit verification to one subsystem. Defaults to every available check.
    pub target: Option<VerifyTarget>,

    /// Require every package in scope to have a cryptographic signature.
    #[arg(long)]
    pub strict: bool,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// A subsystem verified by `sprocket module verify`.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum VerifyTarget {
    /// Verify `module.sig` against the current module contents.
    Signature,
    /// Verify `module-lock.json` against fetched dependency contents.
    Lockfile,
}

/// Runs `sprocket module verify`.
pub async fn verify(args: Args, config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!(
        target = ?args.target,
        strict = args.strict,
        "starting `sprocket module verify`"
    );
    let project = discover(&args.locator)?;
    trace_project("module verify", &project);
    match args.target {
        Some(VerifyTarget::Signature) => verify_signature(&project, colorize)?,
        Some(VerifyTarget::Lockfile) => {
            let unsigned = verify_lockfile(&project, &config, colorize, args.strict)?;
            fail_if_strict_unsigned(None, &unsigned, args.strict)?;
        }
        None => verify_all(&project, &config, colorize, args.strict)?,
    }

    Ok(())
}

fn verify_all(
    project: &crate::commands::module::Project,
    config: &Config,
    colorize: bool,
    strict: bool,
) -> anyhow::Result<()> {
    let mut checked = 0usize;
    let mut unsigned_current = None;
    let mut unsigned_dependencies = Vec::new();
    if project.root.join(wdl_modules::SIGNATURE_FILENAME).exists() {
        tracing::debug!("verifying module signature as part of full verification");
        verify_signature(project, colorize)?;
        checked += 1;
    } else {
        unsigned_current = Some(project.manifest.name.as_str().to_string());
        print_unsigned_current_summary(colorize, strict);
    }
    if project.lockfile_path.exists() {
        tracing::debug!("verifying lockfile as part of full verification");
        unsigned_dependencies = verify_lockfile(project, config, colorize, strict)?;
        checked += 1;
    }
    fail_if_strict_unsigned(unsigned_current.as_deref(), &unsigned_dependencies, strict)?;
    if checked == 0 {
        tracing::debug!("full verification found no signature or lockfile");
        anyhow::bail!(
            "nothing to verify; run `sprocket module sign` or `sprocket module lock` first"
        );
    }
    Ok(())
}

fn verify_signature(
    project: &crate::commands::module::Project,
    colorize: bool,
) -> anyhow::Result<()> {
    let signature_path = project.root.join(wdl_modules::SIGNATURE_FILENAME);
    tracing::trace!(signature = %signature_path.display(), "reading module signature");
    let bytes = std::fs::read(&signature_path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => {
            anyhow::anyhow!("no `module.sig`; run `sprocket module sign` or verify `lockfile`")
        }
        _ => anyhow::Error::new(source).context(format!("reading `{}`", signature_path.display())),
    })?;
    let signature = ModuleSignature::parse(&bytes).map_err(anyhow::Error::from)?;
    let digest = wdl_modules::hash::hash_directory(&project.root).map_err(anyhow::Error::from)?;
    tracing::debug!(digest = %digest, "hashed module content for signature verification");
    signature.verify(&digest).map_err(anyhow::Error::from)?;

    print_action(
        "Verified",
        format!("signature ({digest})"),
        colorize,
        ActionColor::Green,
    );
    Ok(())
}

fn verify_lockfile(
    project: &crate::commands::module::Project,
    config: &Config,
    colorize: bool,
    strict: bool,
) -> anyhow::Result<Vec<DependencyName>> {
    tracing::trace!(lockfile = %project.lockfile_path.display(), "reading module lockfile");
    let lock = require_lockfile(project)?;

    let module = Module::new(project.manifest.clone(), project.root.clone());
    let resolver = build_resolver(config, lock)?;
    tracing::debug!("verifying locked dependencies from cache");

    let VerifyLockedReport {
        verified,
        unsigned,
        errors,
    } = resolver
        .verify_locked_report(&module)
        .map_err(anyhow::Error::from)?;

    if !unsigned.is_empty() {
        print_unsigned_dependency_summary(unsigned.len(), colorize, strict);
    }

    if !errors.is_empty() {
        let mut untrusted = 0usize;
        let mut problems = Vec::new();
        for (_, err) in errors {
            match err {
                ResolverError::UntrustedSigner {
                    dep,
                    signer,
                    identity,
                } => {
                    untrusted += 1;
                    let signer = render_signer(&signer, identity.as_ref());
                    problems.push(format!("`{dep}` signer is untrusted ({signer})"));
                }
                ResolverError::NotFetched { dep } => {
                    problems.push(format!(
                        "`{dep}` is not fetched in the module cache; run `sprocket module fetch`"
                    ));
                }
                other => problems.push(other.to_string()),
            }
        }

        if untrusted > 0 && untrusted == problems.len() {
            return Err(anyhow::anyhow!(
                "{untrusted} modules are untrusted:\n  {}\n  accept signer trust changes with \
                 `sprocket module trust all`",
                problems.join("\n  "),
            ));
        }

        return Err(anyhow::anyhow!(
            "lockfile verification found {} problems:\n  {}",
            problems.len(),
            problems.join("\n  ")
        ));
    }

    print_action(
        "Verified",
        format!("{verified} dependencies"),
        colorize,
        ActionColor::Green,
    );
    Ok(unsigned)
}

fn print_unsigned_current_summary(colorize: bool, strict: bool) {
    let (verb, color) = unsigned_action(strict);
    print_action(
        verb,
        "signature verification for current module (no `module.sig`)",
        colorize,
        color,
    );
}

fn print_unsigned_dependency_summary(unsigned: usize, colorize: bool, strict: bool) {
    let (verb, color) = unsigned_action(strict);
    print_action(verb, unsigned_dependency_summary(unsigned), colorize, color);
}

fn unsigned_action(strict: bool) -> (&'static str, ActionColor) {
    if strict {
        ("Failed", ActionColor::Red)
    } else {
        ("Skipped", ActionColor::Cyan)
    }
}

fn unsigned_dependency_summary(unsigned: usize) -> String {
    match unsigned {
        1 => "signature verification for 1 dependency without a signature".to_string(),
        count => format!("signature verification for {count} dependencies without signatures"),
    }
}

fn fail_if_strict_unsigned(
    current: Option<&str>,
    dependencies: &[DependencyName],
    strict: bool,
) -> anyhow::Result<()> {
    if !strict {
        return Ok(());
    }

    let mut problems = Vec::new();
    if let Some(current) = current {
        problems.push(format!("`{current}` (current module) has no `module.sig`"));
    }
    problems.extend(
        dependencies.iter().map(|dependency| {
            format!("dependency `{}` has no `module.sig`", dependency.manifest())
        }),
    );

    if problems.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "strict verification requires signatures for every package:\n  {}",
            problems.join("\n  ")
        );
    }
}
