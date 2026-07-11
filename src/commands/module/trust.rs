//! `sprocket dev module trust`.

use anyhow::Context as _;
use clap::Parser;
use clap::Subcommand;
use wdl_modules::signing::SignerIdentity;
use wdl_modules::signing::VerifyingKey;
use wdl_modules::signing::parse_openssh_public_key_identity;

use crate::commands::CommandResult;
use crate::commands::module::Locator;
use crate::commands::module::accept_lockfile_signers;
use crate::commands::module::discover;
use crate::commands::module::load_trust_store;
use crate::commands::module::require_lockfile;
use crate::commands::module::save_trust_store;
use crate::commands::module::trace_project;
use crate::commands::printer::Printer;
use crate::config::Config;

/// Subcommands of `sprocket dev module trust`.
#[derive(Subcommand, Debug)]
pub enum TrustCommands {
    /// List trusted keys.
    List(ListArgs),
    /// Add a trusted key.
    Add(AddArgs),
    /// Trust all signer changes in the current module.
    All(AllArgs),
    /// Remove a trusted key.
    Remove(RemoveArgs),
    /// Remove all trusted keys.
    Destroy(DestroyArgs),
}

/// Arguments to `sprocket dev module trust list`.
#[derive(Parser, Debug)]
pub struct ListArgs {}

/// Arguments to `sprocket dev module trust add`.
#[derive(Parser, Debug)]
pub struct AddArgs {
    /// OpenSSH-format public keys, or public-key file paths.
    #[arg(required = true)]
    pub keys: Vec<String>,
    /// Optional signer display name.
    #[arg(long)]
    pub name: Option<String>,
    /// Optional signer email.
    #[arg(long)]
    pub email: Option<String>,
}

/// Arguments to `sprocket dev module trust all`.
#[derive(Parser, Debug)]
pub struct AllArgs {
    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Arguments to `sprocket dev module trust remove`.
#[derive(Parser, Debug)]
pub struct RemoveArgs {
    /// OpenSSH-format public keys, or public-key file paths.
    #[arg(required = true)]
    pub keys: Vec<String>,
}

/// Arguments to `sprocket dev module trust destroy`.
#[derive(Parser, Debug)]
pub struct DestroyArgs {}

/// Runs `sprocket dev module trust`.
pub async fn trust(cmd: TrustCommands, config: Config, printer: Printer) -> CommandResult<()> {
    match cmd {
        TrustCommands::List(args) => list(args, config).await,
        TrustCommands::Add(args) => add(args, config, printer).await,
        TrustCommands::All(args) => all(args, config, printer).await,
        TrustCommands::Remove(args) => remove(args, config, printer).await,
        TrustCommands::Destroy(args) => destroy(args, config, printer).await,
    }
}

/// Runs `sprocket dev module trust list`.
pub async fn list(_args: ListArgs, _config: Config) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust list`");
    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    let store = load_trust_store(&trust_path)?;

    if store.keys.is_empty() {
        tracing::info!("no trusted module keys configured");
        println!("no trusted keys");
        return Ok(());
    }

    for key in store.trusted_keys() {
        let meta = store.identity(key).map(format_identity).unwrap_or_default();
        println!("{}{}", key.to_openssh(), meta);
    }

    Ok(())
}

/// Runs `sprocket dev module trust all`.
pub async fn all(args: AllArgs, _config: Config, printer: Printer) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust all`");
    let project = discover(&args.locator)?;
    trace_project("module trust all", &project);

    let lockfile = require_lockfile(&project)?;
    let trust_path = crate::analysis::default_trust_path();
    let trusted = accept_lockfile_signers(&trust_path, &lockfile)?;

    printer.status("Trusted", format!("{trusted} signer keys"));
    Ok(())
}

/// Runs `sprocket dev module trust add`.
pub async fn add(args: AddArgs, _config: Config, printer: Printer) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust add`");
    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    let mut store = load_trust_store(&trust_path)?;

    if args.keys.len() > 1 && (args.name.is_some() || args.email.is_some()) {
        return Err(anyhow::anyhow!("`--name` and `--email` require exactly one key").into());
    }

    let mut parsed = Vec::new();
    for key in &args.keys {
        parsed.push(parse_key_arg(key)?);
    }

    for mut parsed_key in parsed {
        if args.keys.len() == 1 {
            if args.name.is_some() {
                parsed_key.name = args.name.clone();
            }
            if args.email.is_some() {
                parsed_key.email = args.email.clone();
            }
        }

        let key = parsed_key.key;
        if store.insert_key(key) {
            tracing::debug!("added trusted module key");
        } else {
            tracing::debug!("trusted module key already exists");
        }
        store.upsert_identity(key, parsed_key.name, parsed_key.email);
        printer.status("Trusted", key.to_openssh());
    }

    save_trust_store(&trust_path, &store)?;
    Ok(())
}

/// A trust key parsed from the CLI along with any identity metadata.
struct ParsedTrustKey {
    /// The parsed verifying key.
    key: VerifyingKey,
    /// Optional signer display name.
    name: Option<String>,
    /// Optional signer email.
    email: Option<String>,
}

/// Parses a trust key argument as an inline OpenSSH key or a key file path.
fn parse_key_arg(key: &str) -> anyhow::Result<ParsedTrustKey> {
    if let Ok(parsed) = VerifyingKey::from_openssh(key.trim()) {
        tracing::trace!("parsed inline trust key");
        let SignerIdentity { name, email } =
            parse_openssh_public_key_identity(key).unwrap_or_default();
        return Ok(ParsedTrustKey {
            key: parsed,
            name,
            email,
        });
    }

    let key_text =
        std::fs::read_to_string(key).with_context(|| format!("reading public key from `{key}`"))?;
    let parsed = VerifyingKey::from_openssh(key_text.trim())
        .with_context(|| format!("parsing OpenSSH public key from `{key}`"))?;
    let SignerIdentity { name, email } =
        parse_openssh_public_key_identity(&key_text).unwrap_or_default();
    Ok(ParsedTrustKey {
        key: parsed,
        name,
        email,
    })
}

/// Runs `sprocket dev module trust remove`.
pub async fn remove(args: RemoveArgs, _config: Config, printer: Printer) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust remove`");
    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    let mut store = load_trust_store(&trust_path)?;

    let mut parsed = Vec::new();
    for key in &args.keys {
        parsed.push(parse_key_arg(key)?.key);
    }

    let mut removed_any = false;
    for key in parsed {
        if store.remove_key(&key) {
            removed_any = true;
            printer.status("Removed", format!("trust for {}", key.to_openssh()));
        }
    }

    if !removed_any {
        tracing::debug!("no matching trusted module key found");
        return Err(anyhow::anyhow!("no matching trusted keys").into());
    }

    save_trust_store(&trust_path, &store)?;
    Ok(())
}

/// Runs `sprocket dev module trust destroy`.
pub async fn destroy(_args: DestroyArgs, _config: Config, printer: Printer) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust destroy`");
    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    let mut store = load_trust_store(&trust_path)?;

    store.clear();
    save_trust_store(&trust_path, &store)?;
    printer.status("Removed", "all trusted keys");
    Ok(())
}

fn format_identity(identity: &wdl_modules::resolver::TrustedIdentity) -> String {
    match (identity.name.as_deref(), identity.email.as_deref()) {
        (Some(name), Some(email)) => format!(" ({name} <{email}>)"),
        (Some(name), None) => format!(" ({name})"),
        (None, Some(email)) => format!(" (<{email}>)"),
        (None, None) => String::new(),
    }
}
