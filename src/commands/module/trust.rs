//! `sprocket dev module trust`.

use anyhow::Context as _;
use clap::Parser;
use clap::Subcommand;
use wdl_modules::signing::SignerIdentity;
use wdl_modules::signing::VerifyingKey;
use wdl_modules::signing::parse_openssh_public_key_identity;

use super::project::Locator;
use super::project::discover;
use super::project::require_lockfile;
use super::project::trace_project;
use super::trust_store::TrustStoreFile;
use crate::commands::CommandResult;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;

const REMOVE: Action = Action::new("Removed", "remove");
const TRUST: Action = Action::new("Trusted", "trust");

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
    locator: Locator,
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
pub async fn trust(cmd: TrustCommands, output: CommandOutput) -> CommandResult<()> {
    match cmd {
        TrustCommands::List(_) => list(output).await,
        TrustCommands::Add(args) => add(args, output).await,
        TrustCommands::All(args) => all(args, output).await,
        TrustCommands::Remove(args) => remove(args, output).await,
        TrustCommands::Destroy(_) => destroy(output).await,
    }
}

/// Runs `sprocket dev module trust list`.
pub async fn list(output: CommandOutput) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust list`");
    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    let store = TrustStoreFile::load(trust_path)?.into_store();

    if store.keys.is_empty() {
        tracing::info!("no trusted module keys configured");
        output.payload("no trusted keys");
        return Ok(());
    }

    for key in store.trusted_keys() {
        let meta = store.identity(key).map(format_identity).unwrap_or_default();
        output.payload(format!("{}{}", key.to_openssh(), meta));
    }

    Ok(())
}

/// Runs `sprocket dev module trust all`.
pub async fn all(args: AllArgs, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust all`");
    let project = discover(&args.locator)?;
    trace_project("module trust all", &project);

    let lockfile = require_lockfile(&project)?;
    let trust_path = crate::analysis::default_trust_path();
    let trusted = TrustStoreFile::load(trust_path)?.accept_lockfile_signers(&lockfile)?;

    output.completed(TRUST, format!("{trusted} signer keys"));
    Ok(())
}

/// Runs `sprocket dev module trust add`.
pub async fn add(args: AddArgs, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust add`");
    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    let mut trust_file = TrustStoreFile::load(trust_path)?;
    let store = trust_file.store_mut();

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
            if args.name.is_some() || args.email.is_some() {
                parsed_key.comment = None;
            }
        }

        let key = parsed_key.key;
        if store.insert_key(key) {
            tracing::debug!("added trusted module key");
        } else {
            tracing::debug!("trusted module key already exists");
        }
        store.upsert_identity(key, parsed_key.name, parsed_key.email, parsed_key.comment);
        output.completed(TRUST, key.to_openssh());
    }

    trust_file.save()?;
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
    /// Optional unstructured public key comment.
    comment: Option<String>,
}

impl ParsedTrustKey {
    /// Builds a parsed trust key from authenticated or public key metadata.
    fn new(key: VerifyingKey, identity: Option<SignerIdentity>) -> Self {
        match identity {
            Some(SignerIdentity::Signer { name, email }) => Self {
                key,
                name: Some(name),
                email: Some(email),
                comment: None,
            },
            Some(SignerIdentity::Comment { comment }) => Self {
                key,
                name: None,
                email: None,
                comment: Some(comment),
            },
            None => Self {
                key,
                name: None,
                email: None,
                comment: None,
            },
        }
    }
}

/// Parses a trust key argument as an inline OpenSSH key or a key file path.
fn parse_key_arg(key: &str) -> anyhow::Result<ParsedTrustKey> {
    if let Ok(parsed) = VerifyingKey::from_openssh(key.trim()) {
        tracing::trace!("parsed inline trust key");
        return Ok(ParsedTrustKey::new(
            parsed,
            parse_openssh_public_key_identity(key),
        ));
    }

    let key_text =
        std::fs::read_to_string(key).with_context(|| format!("reading public key from `{key}`"))?;
    let parsed = VerifyingKey::from_openssh(key_text.trim())
        .with_context(|| format!("parsing OpenSSH public key from `{key}`"))?;
    Ok(ParsedTrustKey::new(
        parsed,
        parse_openssh_public_key_identity(&key_text),
    ))
}

/// Runs `sprocket dev module trust remove`.
pub async fn remove(args: RemoveArgs, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust remove`");
    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    let mut trust_file = TrustStoreFile::load(trust_path)?;
    let store = trust_file.store_mut();

    let mut parsed = Vec::new();
    for key in &args.keys {
        parsed.push(parse_key_arg(key)?.key);
    }

    let mut removed_any = false;
    for key in parsed {
        if store.remove_key(&key) {
            removed_any = true;
            output.completed(REMOVE, format!("trust for {}", key.to_openssh()));
        }
    }

    if !removed_any {
        tracing::debug!("no matching trusted module key found");
        return Err(anyhow::anyhow!("no matching trusted keys").into());
    }

    trust_file.save()?;
    Ok(())
}

/// Runs `sprocket dev module trust destroy`.
pub async fn destroy(output: CommandOutput) -> CommandResult<()> {
    tracing::trace!("starting `sprocket dev module trust destroy`");
    let trust_path = crate::analysis::default_trust_path();
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    let mut trust_file = TrustStoreFile::load(trust_path)?;

    trust_file.store_mut().clear();
    trust_file.save()?;
    output.completed(REMOVE, "all trusted keys");
    Ok(())
}

/// Formats optional trusted signer identity metadata for display.
fn format_identity(identity: &wdl_modules::resolver::TrustedIdentity) -> String {
    if let Some(comment) = identity.comment.as_deref() {
        return format!(" ({comment})");
    }
    match (identity.name.as_deref(), identity.email.as_deref()) {
        (Some(name), Some(email)) => format!(" ({name} <{email}>)"),
        (Some(name), None) => format!(" ({name})"),
        (None, Some(email)) => format!(" (<{email}>)"),
        (None, None) => String::new(),
    }
}
