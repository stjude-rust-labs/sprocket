//! `sprocket dev module verify`.

use clap::Parser;
use clap::ValueEnum;
use wdl_modules::dependency::DependencyName;
use wdl_modules::hash::ContentHash;
use wdl_modules::module::Module;
use wdl_modules::resolver::ResolverError;
use wdl_modules::resolver::VerifyLockedReport;
use wdl_modules::signing::ModuleSignature;

use super::project::Locator;
use super::project::Project;
use super::project::discover;
use super::project::require_lockfile;
use super::project::trace_project;
use super::resolver::ResolverEnvironment;
use super::signer_policy::render_signer;
use crate::commands::CommandResult;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;
use crate::config::Config;

const VERIFY: Action = Action::new("Verified", "verify");

/// Arguments to `sprocket dev module verify`.
#[derive(Parser, Debug)]
pub struct Args {
    /// Limit verification to one subsystem. Defaults to every available check.
    pub target: Option<VerifyTarget>,

    /// Require every package in scope to have a cryptographic signature.
    #[arg(long)]
    pub require_signatures: bool,

    /// Shared module locator.
    #[command(flatten)]
    locator: Locator,
}

/// A subsystem verified by `sprocket dev module verify`.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum VerifyTarget {
    /// Verify `module.sig` against the current module contents.
    Signature,
    /// Verify `module-lock.json` against fetched dependency contents.
    Lockfile,
}

/// Runs `sprocket dev module verify`.
pub async fn verify(args: Args, config: Config, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        target = ?args.target,
        require_signatures = args.require_signatures,
        "starting `sprocket dev module verify`"
    );
    let project = discover(&args.locator)?;
    trace_project("module verify", &project);
    let checksum = project.validate().map_err(anyhow::Error::from)?;
    output.completed(VERIFY, "module structure");
    match args.target {
        Some(VerifyTarget::Signature) => verify_signature(&project, &checksum, output)?,
        Some(VerifyTarget::Lockfile) => {
            let unsigned = verify_lockfile(&project, &config, output, args.require_signatures)?;
            fail_if_required_signatures_missing(None, &unsigned, false, args.require_signatures)?;
        }
        None => verify_all(
            &project,
            &config,
            &checksum,
            output,
            args.require_signatures,
        )?,
    }

    Ok(())
}

/// Verifies every signature and lockfile available for the current module.
fn verify_all(
    project: &Project,
    config: &Config,
    checksum: &ContentHash,
    output: CommandOutput,
    require_signatures: bool,
) -> anyhow::Result<()> {
    let mut unsigned_current = None;
    let mut unsigned_dependencies = Vec::new();
    if project
        .root()
        .join(wdl_modules::SIGNATURE_FILENAME)
        .exists()
    {
        tracing::debug!("verifying module signature as part of full verification");
        verify_signature(project, checksum, output)?;
    } else {
        unsigned_current = Some(project.manifest().name.as_str().to_string());
        print_unsigned_current_summary(output, require_signatures);
    }

    let missing_dependency_lockfile = if project.lockfile_path().exists() {
        tracing::debug!("verifying lockfile as part of full verification");
        unsigned_dependencies = verify_lockfile(project, config, output, require_signatures)?;
        false
    } else if require_signatures && !project.manifest().dependencies.is_empty() {
        output.failed("signature verification for dependencies (no `module-lock.json`)");
        true
    } else {
        output.skipped("lockfile verification (no `module-lock.json`)");
        false
    };

    fail_if_required_signatures_missing(
        unsigned_current.as_deref(),
        &unsigned_dependencies,
        missing_dependency_lockfile,
        require_signatures,
    )?;
    Ok(())
}

/// Verifies the current module's signature against its content digest.
fn verify_signature(
    project: &Project,
    checksum: &ContentHash,
    output: CommandOutput,
) -> anyhow::Result<()> {
    let signature_path = project.root().join(wdl_modules::SIGNATURE_FILENAME);
    tracing::trace!(signature = %signature_path.display(), "reading module signature");
    let bytes = std::fs::read(&signature_path).map_err(|source| match source.kind() {
        std::io::ErrorKind::NotFound => {
            anyhow::anyhow!("no `module.sig`; run `sprocket dev module sign` or verify `lockfile`")
        }
        _ => anyhow::Error::new(source).context(format!("reading `{}`", signature_path.display())),
    })?;
    let signature = ModuleSignature::parse(&bytes).map_err(anyhow::Error::from)?;
    signature.verify(checksum).map_err(anyhow::Error::from)?;

    output.completed(VERIFY, "module signature");
    output.detail("Digest", checksum);
    Ok(())
}

/// Verifies locked dependency contents and reports unsigned dependencies.
fn verify_lockfile(
    project: &Project,
    config: &Config,
    output: CommandOutput,
    require_signatures: bool,
) -> anyhow::Result<Vec<DependencyName>> {
    tracing::trace!(lockfile = %project.lockfile_path().display(), "reading module lockfile");
    let lock = require_lockfile(project)?;

    let module = Module::new(
        std::sync::Arc::new(project.manifest().clone()),
        project.root().to_path_buf(),
    );
    let environment = ResolverEnvironment::from_config(config)?;
    let resolver = environment.resolver(lock)?;
    tracing::debug!("verifying locked dependencies from cache");

    let VerifyLockedReport {
        verified,
        unsigned,
        errors,
    } = resolver
        .verify_locked_report(&module)
        .map_err(anyhow::Error::from)?;

    if !unsigned.is_empty() {
        print_unsigned_dependency_summary(unsigned.len(), output, require_signatures);
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
                        "`{dep}` is not fetched in the module cache; run `sprocket dev module \
                         fetch`"
                    ));
                }
                other => problems.push(other.to_string()),
            }
        }

        if untrusted > 0 && untrusted == problems.len() {
            return Err(anyhow::anyhow!(
                "{untrusted} modules are untrusted:\n  {}\n  accept signer trust changes with \
                 `sprocket dev module trust all`",
                problems.join("\n  "),
            ));
        }

        return Err(anyhow::anyhow!(
            "lockfile verification found {} problems:\n  {}",
            problems.len(),
            problems.join("\n  ")
        ));
    }

    output.completed(
        VERIFY,
        format!(
            "{verified} {}",
            if verified == 1 {
                "dependency"
            } else {
                "dependencies"
            }
        ),
    );
    Ok(unsigned)
}

/// Reports that the current module has no signature.
fn print_unsigned_current_summary(output: CommandOutput, require_signatures: bool) {
    print_unsigned_summary(
        output,
        require_signatures,
        "signature verification for current module (no `module.sig`)",
    );
}

/// Reports the number of locked dependencies without signatures.
fn print_unsigned_dependency_summary(
    unsigned: usize,
    output: CommandOutput,
    require_signatures: bool,
) {
    print_unsigned_summary(
        output,
        require_signatures,
        unsigned_dependency_summary(unsigned),
    );
}

/// Prints a red `Failed` when signatures are required and a cyan `Skipped`
/// otherwise.
fn print_unsigned_summary(
    output: CommandOutput,
    require_signatures: bool,
    rest: impl std::fmt::Display,
) {
    if require_signatures {
        output.failed(rest);
    } else {
        output.skipped(rest);
    }
}

/// Formats an unsigned dependency count for verification output.
fn unsigned_dependency_summary(unsigned: usize) -> String {
    match unsigned {
        1 => "signature verification for 1 dependency without a signature".to_string(),
        count => format!("signature verification for {count} dependencies without signatures"),
    }
}

/// Rejects unsigned modules and dependencies when signatures are required.
fn fail_if_required_signatures_missing(
    current: Option<&str>,
    dependencies: &[DependencyName],
    missing_dependency_lockfile: bool,
    require_signatures: bool,
) -> anyhow::Result<()> {
    if !require_signatures {
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
    if missing_dependency_lockfile {
        problems.push(
            "dependencies require `module-lock.json`; run `sprocket dev module lock`".to_string(),
        );
    }

    if problems.is_empty() {
        Ok(())
    } else {
        anyhow::bail!(
            "verification with `--require-signatures` requires signatures for every package:\n  {}",
            problems.join("\n  ")
        );
    }
}
