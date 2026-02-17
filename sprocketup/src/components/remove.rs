//! Remove components from an installation.

use crate::components::Components;
use crate::dirs::sprocketup_bin_dir;

/// Remove one or more components from the current installation.
pub async fn remove_components(components: Vec<String>) -> anyhow::Result<()> {
    let mut local_components = Components::load(None)?;

    let mut final_components = Vec::new();
    for component in components {
        let mut found = false;
        for local_component in local_components.list() {
            if local_component.name == component {
                found = true;
                break;
            }
        }

        if !found {
            tracing::warn!("component '{component}' not installed, skipping");
            continue;
        }

        final_components.push(component);
    }

    if final_components.is_empty() {
        tracing::info!("nothing to do");
        return Ok(());
    }

    let bin_dir = sprocketup_bin_dir()?;
    let mut manifest_tx = local_components.tx();
    for component in final_components {
        tracing::info!("removing component '{component}'");
        manifest_tx.removed(&component);
        let bin_path = bin_dir.join(&component);
        if !(bin_path.exists() && bin_path.is_file()) {
            tracing::warn!(
                "component '{component}' not found at '{}', ignoring",
                bin_path.display()
            );
            continue; // Someone beat us to it?
        }

        std::fs::remove_file(&bin_path)?;
    }
    manifest_tx.commit()?;

    Ok(())
}
