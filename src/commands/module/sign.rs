//! `sprocket dev module sign`.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use anyhow::Context as _;
use clap::Parser;
use wdl_modules::signing::ModuleSignature;
use wdl_modules::signing::SignerIdentity;
use wdl_modules::signing::SigningKey;
use wdl_modules::signing::VerifyingKey;
use wdl_modules::signing::parse_openssh_public_key_identity;

use super::project::Locator;
use super::project::discover;
use super::project::trace_project;
use crate::commands::CommandResult;
use crate::commands::output::Action;
use crate::commands::output::CommandOutput;

const SIGN: Action = Action::new("Signed", "sign");

/// Arguments to `sprocket dev module sign`.
#[derive(Parser, Debug)]
pub struct Args {
    /// OpenSSH-format Ed25519 private key path.
    #[arg(long)]
    pub key: Option<PathBuf>,

    /// Output path for the signature (defaults to `<module-root>/module.sig`).
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Shared module locator.
    #[command(flatten)]
    locator: Locator,
}

/// Runs `sprocket dev module sign`.
pub async fn sign(args: Args, output: CommandOutput) -> CommandResult<()> {
    tracing::trace!(
        explicit_key = args.key.is_some(),
        explicit_output = args.output.is_some(),
        "starting `sprocket dev module sign`"
    );
    let project = discover(&args.locator)?;
    trace_project("module sign", &project);
    let digest = wdl_modules::hash::hash_directory(&project.root).map_err(anyhow::Error::from)?;
    tracing::debug!(digest = %digest, "hashed module content for signing");
    let key = signing_key_path(args.key, &project.root)?;
    tracing::debug!(key = %key.display(), "selected signing key");
    let key_text =
        std::fs::read_to_string(&key).with_context(|| format!("reading `{}`", key.display()))?;
    let signing_key = SigningKey::from_openssh(&key_text).map_err(anyhow::Error::from)?;
    let identity = signer_identity_for_key_path(&key, signing_key.verifying_key());
    let module_signature =
        ModuleSignature::new(&signing_key, &digest, identity).map_err(anyhow::Error::from)?;

    let signature_path = args
        .output
        .unwrap_or_else(|| project.root.join(wdl_modules::SIGNATURE_FILENAME));
    write_signature_atomically(&signature_path, &module_signature)?;
    tracing::debug!(signature = %signature_path.display(), "wrote module signature");

    output.completed(SIGN, format!("module `{}`", project.manifest.name));
    output.detail("Digest", digest);
    output.detail("Signature", signature_path.display());
    Ok(())
}

fn signing_key_path(explicit: Option<PathBuf>, working_dir: &Path) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        tracing::trace!("using explicit signing key path");
        return Ok(path);
    }

    if let Some(path) = git_signing_key_path(working_dir) {
        tracing::trace!("using signing key path from Git config");
        return Ok(path);
    }

    let home = home_dir()
        .ok_or_else(|| anyhow::anyhow!("could not determine home directory; specify `--key`"))?;
    let path = default_ed25519_key_path(&home);
    if !path.exists() {
        tracing::debug!(key = %path.display(), "default signing key was not found");
        anyhow::bail!(
            "no ed25519 signing key found; configure git `user.signingkey`, create `{}`, or \
             specify `--key`",
            path.display()
        );
    }
    Ok(path)
}

fn git_signing_key_path(working_dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["config", "--get", "user.signingkey"])
        .current_dir(working_dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let configured = String::from_utf8(output.stdout).ok()?;
    let configured = configured.trim();
    if configured.is_empty()
        || configured.starts_with("ssh-ed25519 ")
        || configured.starts_with("key::")
    {
        tracing::trace!("ignoring Git signing key because it is not a private key path");
        return None;
    }

    let expanded = shellexpand::tilde(configured);
    let path = PathBuf::from(expanded.as_ref());
    if path.extension().and_then(|ext| ext.to_str()) == Some("pub") {
        let private = path.with_extension("");
        if private.is_file() {
            tracing::trace!("inferred private signing key from configured public key");
            return Some(private);
        }
    }

    if path.exists() {
        tracing::trace!("using existing Git signing key path");
        return Some(path);
    }

    None
}

fn default_ed25519_key_path(home: &Path) -> PathBuf {
    home.join(".ssh").join("id_ed25519")
}

/// Resolves the user's home directory, preferring environment variables over
/// the platform home-directory API.
///
/// Checks `HOME`, then `USERPROFILE`, before falling back to
/// [`dirs::home_dir`]. On Windows `dirs::home_dir` consults the known-folder
/// API and ignores these environment variables, which breaks any caller (such
/// as a test) that isolates itself by overriding `HOME`/`USERPROFILE`.
/// Resolving env-first also matches how Git itself expands `~`.
fn home_dir() -> Option<PathBuf> {
    for var in ["HOME", "USERPROFILE"] {
        if let Some(value) = std::env::var_os(var)
            && !value.is_empty()
        {
            return Some(PathBuf::from(value));
        }
    }
    dirs::home_dir()
}

fn signer_identity_for_key_path(path: &Path, expected: VerifyingKey) -> Option<SignerIdentity> {
    let pub_path = if path.extension().and_then(|ext| ext.to_str()) == Some("pub") {
        path.to_path_buf()
    } else {
        path.with_extension("pub")
    };

    let key_text = std::fs::read_to_string(&pub_path).ok()?;
    let parsed = VerifyingKey::from_openssh(key_text.trim()).ok()?;
    if parsed != expected {
        tracing::warn!(
            public_key = %pub_path.display(),
            "ignoring signer identity because the public key does not match the private key",
        );
        return None;
    }

    parse_openssh_public_key_identity(&key_text)
}

fn write_signature_atomically(path: &Path, signature: &ModuleSignature) -> anyhow::Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(wdl_modules::SIGNATURE_FILENAME);
    let temp_path = path.with_file_name(format!("{file_name}.tmp"));
    let mut temp = std::fs::File::create(&temp_path)
        .with_context(|| format!("creating `{}`", temp_path.display()))?;
    signature
        .write(&mut temp)
        .with_context(|| format!("writing `{}`", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("renaming `{}` to `{}`", temp_path.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn git_signing_key_path_infers_private_key_from_public_path() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let private = dir.path().join("id_ed25519");
        let public = dir.path().join("id_ed25519.pub");
        fs::write(&private, "private")?;
        fs::write(&public, "public")?;

        let init = Command::new("git")
            .arg("init")
            .current_dir(dir.path())
            .output()?;
        assert!(init.status.success());
        let config = Command::new("git")
            .args(["config", "user.signingkey"])
            .arg(&public)
            .current_dir(dir.path())
            .output()?;
        assert!(config.status.success());

        assert_eq!(
            git_signing_key_path(dir.path()).as_deref(),
            Some(private.as_path())
        );
        Ok(())
    }

    #[test]
    fn default_ed25519_key_path_uses_home_ssh_directory() {
        let home = Path::new("/home/example");
        assert_eq!(
            default_ed25519_key_path(home),
            Path::new("/home/example").join(".ssh").join("id_ed25519")
        );
    }

    #[test]
    fn signer_identity_for_key_path_extracts_key_comment() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let private = dir.path().join("id_ed25519");
        let public = dir.path().join("id_ed25519.pub");

        std::fs::write(&private, "placeholder")?;
        std::fs::write(
            &public,
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINiRUmfYzFTjksGItM2fSm9s1eCL8NnMJGQgW724Uph1 \
             Jane Doe <jane@example.com>\n",
        )?;

        let expected: VerifyingKey =
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINiRUmfYzFTjksGItM2fSm9s1eCL8NnMJGQgW724Uph1"
                .parse()?;
        let identity = signer_identity_for_key_path(&private, expected)
            .ok_or_else(|| anyhow::anyhow!("expected signer identity"))?;
        assert_eq!(identity.name(), Some("Jane Doe"));
        assert_eq!(identity.email(), Some("jane@example.com"));
        Ok(())
    }
}
