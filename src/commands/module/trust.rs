//! `sprocket module trust`.

use anyhow::Context as _;
use clap::Parser;
use clap::Subcommand;
use wdl_modules::Lockfile;
use wdl_modules::resolver::TrustStore;
use wdl_modules::signing::SignerIdentity;
use wdl_modules::signing::VerifyingKey;
use wdl_modules::signing::parse_openssh_public_key_identity;

use crate::commands::CommandResult;
use crate::commands::module::ActionColor;
use crate::commands::module::Locator;
use crate::commands::module::accept_lockfile_signers;
use crate::commands::module::discover;
use crate::commands::module::print_action;
use crate::commands::module::trace_project;
use crate::config::Config;

/// Subcommands of `sprocket module trust`.
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

/// Arguments to `sprocket module trust list`.
#[derive(Parser, Debug)]
pub struct ListArgs {
    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Arguments to `sprocket module trust add`.
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

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Arguments to `sprocket module trust all`.
#[derive(Parser, Debug)]
pub struct AllArgs {
    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Arguments to `sprocket module trust remove`.
#[derive(Parser, Debug)]
pub struct RemoveArgs {
    /// OpenSSH-format public keys, or public-key file paths.
    #[arg(required = true)]
    pub keys: Vec<String>,

    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Arguments to `sprocket module trust destroy`.
#[derive(Parser, Debug)]
pub struct DestroyArgs {
    /// Shared module locator.
    #[command(flatten)]
    pub locator: Locator,
}

/// Runs `sprocket module trust`.
pub async fn trust(cmd: TrustCommands, config: Config, colorize: bool) -> CommandResult<()> {
    match cmd {
        TrustCommands::List(args) => list(args, config, colorize).await,
        TrustCommands::Add(args) => add(args, config, colorize).await,
        TrustCommands::All(args) => all(args, config, colorize).await,
        TrustCommands::Remove(args) => remove(args, config, colorize).await,
        TrustCommands::Destroy(args) => destroy(args, config, colorize).await,
    }
}

/// Runs `sprocket module trust list`.
pub async fn list(args: ListArgs, _config: Config, _colorize: bool) -> CommandResult<()> {
    tracing::trace!("starting `sprocket module trust list`");
    let project = discover(&args.locator)?;
    trace_project("module trust list", &project);
    let trust_path = crate::analysis::default_trust_path(project.manifest_path.parent());
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    tracing::trace!(trust_store = %trust_path.display(), "loading module trust store");
    let store = TrustStore::load_or_default(&trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;
    tracing::debug!(
        trust_store = %trust_path.display(),
        keys = store.keys.len(),
        "loaded module trust store"
    );

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

/// Runs `sprocket module trust all`.
pub async fn all(args: AllArgs, _config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!("starting `sprocket module trust all`");
    let project = discover(&args.locator)?;
    trace_project("module trust all", &project);

    let lockfile = load_lockfile_required(&project.lockfile_path)?;
    let trust_path = crate::analysis::default_trust_path(project.manifest_path.parent());
    let trusted = accept_lockfile_signers(&trust_path, &lockfile)?;

    print_action(
        "Trusted",
        format!("{trusted} signer keys"),
        colorize,
        ActionColor::Green,
    );
    Ok(())
}

fn load_lockfile_required(path: &std::path::Path) -> anyhow::Result<Lockfile> {
    let bytes = std::fs::read(path).with_context(|| format!("reading `{}`", path.display()))?;
    Lockfile::parse(&bytes).with_context(|| format!("parsing `{}`", path.display()))
}

/// Runs `sprocket module trust add`.
pub async fn add(args: AddArgs, _config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!("starting `sprocket module trust add`");
    let project = discover(&args.locator)?;
    trace_project("module trust add", &project);
    let trust_path = crate::analysis::default_trust_path(project.manifest_path.parent());
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    tracing::trace!(trust_store = %trust_path.display(), "loading module trust store");
    let mut store = TrustStore::load_or_default(&trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;
    tracing::debug!(
        trust_store = %trust_path.display(),
        keys = store.keys.len(),
        "loaded module trust store"
    );

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
        print_action("Trusted", key.to_openssh(), colorize, ActionColor::Green);
    }

    tracing::trace!(trust_store = %trust_path.display(), "writing module trust store");
    store
        .save(&trust_path)
        .with_context(|| format!("writing trust store at `{}`", trust_path.display()))?;
    tracing::debug!(
        trust_store = %trust_path.display(),
        keys = store.keys.len(),
        "wrote module trust store"
    );
    Ok(())
}

struct ParsedTrustKey {
    key: VerifyingKey,
    name: Option<String>,
    email: Option<String>,
}

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

/// Runs `sprocket module trust remove`.
pub async fn remove(args: RemoveArgs, _config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!("starting `sprocket module trust remove`");
    let project = discover(&args.locator)?;
    trace_project("module trust remove", &project);
    let trust_path = crate::analysis::default_trust_path(project.manifest_path.parent());
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    tracing::trace!(trust_store = %trust_path.display(), "loading module trust store");
    let mut store = TrustStore::load_or_default(&trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;
    tracing::debug!(
        trust_store = %trust_path.display(),
        keys = store.keys.len(),
        "loaded module trust store"
    );

    let mut parsed = Vec::new();
    for key in &args.keys {
        parsed.push(parse_key_arg(key)?.key);
    }

    let mut removed_any = false;
    for key in parsed {
        if store.remove_key(&key) {
            removed_any = true;
            print_action(
                "Removed",
                format!("trust for {}", key.to_openssh()),
                colorize,
                ActionColor::Green,
            );
        }
    }

    if !removed_any {
        tracing::debug!("no matching trusted module key found");
        return Err(anyhow::anyhow!("no matching trusted keys").into());
    }

    tracing::trace!(trust_store = %trust_path.display(), "writing module trust store");
    store
        .save(&trust_path)
        .with_context(|| format!("writing trust store at `{}`", trust_path.display()))?;
    tracing::debug!(
        trust_store = %trust_path.display(),
        keys = store.keys.len(),
        "wrote module trust store"
    );
    Ok(())
}

/// Runs `sprocket module trust destroy`.
pub async fn destroy(args: DestroyArgs, _config: Config, colorize: bool) -> CommandResult<()> {
    tracing::trace!("starting `sprocket module trust destroy`");
    let project = discover(&args.locator)?;
    trace_project("module trust destroy", &project);
    let trust_path = crate::analysis::default_trust_path(project.manifest_path.parent());
    tracing::info!(trust_store = %trust_path.display(), "using module trust store");
    tracing::trace!(trust_store = %trust_path.display(), "loading module trust store");
    let mut store = TrustStore::load_or_default(&trust_path)
        .with_context(|| format!("loading trust store at `{}`", trust_path.display()))?;
    tracing::debug!(
        trust_store = %trust_path.display(),
        keys = store.keys.len(),
        "loaded module trust store"
    );

    store.keys.clear();
    tracing::trace!(trust_store = %trust_path.display(), "writing module trust store");
    store
        .save(&trust_path)
        .with_context(|| format!("writing trust store at `{}`", trust_path.display()))?;
    tracing::debug!(trust_store = %trust_path.display(), "wrote module trust store");
    print_action("Removed", "all trusted keys", colorize, ActionColor::Green);
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
