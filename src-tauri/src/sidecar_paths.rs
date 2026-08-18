//! Where the WhatsApp sidecar's Node runtime and entry script live.
//!
//! Shared by startup and by the manual restart command. They used to resolve
//! separately, and the restart path looked only under the executable's own
//! directory — which is correct in a dev tree but wrong for an installed build,
//! where the installer places both under the Tauri *resource* directory. The
//! restart therefore killed a healthy sidecar and then failed to find anything
//! to respawn, leaving WhatsApp down until the 8-second watchdog rescued it.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tauri::{Manager, Runtime};

/// Candidate locations for `server.mjs`, most specific first.
fn script_candidates(resource_dir: Option<&Path>, exe_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(rd) = resource_dir {
        // Production: bundled under resources/sidecar/whatsapp-sidecar/
        candidates.push(
            rd.join("sidecar")
                .join("whatsapp-sidecar")
                .join("server.mjs"),
        );
        // Legacy flat layout
        candidates.push(rd.join("sidecar").join("server.mjs"));
    }
    candidates.push(
        exe_dir
            .join("sidecar")
            .join("whatsapp-sidecar")
            .join("server.mjs"),
    );
    candidates.push(exe_dir.join("sidecar").join("server.mjs"));
    // Dev: the source tree. CARGO_MANIFEST_DIR is compile-time only.
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("sidecar")
            .join("whatsapp-sidecar")
            .join("server.mjs"),
    );
    candidates
}

/// Candidate locations for the bundled `node.exe`, most specific first.
fn node_candidates(resource_dir: Option<&Path>, exe_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(rd) = resource_dir {
        candidates.push(rd.join("sidecar").join("whatsapp-sidecar").join("node.exe"));
        candidates.push(rd.join("sidecar").join("node.exe"));
    }
    candidates.push(
        exe_dir
            .join("sidecar")
            .join("whatsapp-sidecar")
            .join("node.exe"),
    );
    candidates.push(exe_dir.join("sidecar").join("node.exe"));
    candidates.push(exe_dir.join("node.exe"));
    candidates
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default()
}

/// The sidecar entry script, or `None` when no candidate exists on disk.
pub fn script<R: Runtime, M: Manager<R>>(app: &M) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok();
    script_candidates(resource_dir.as_deref(), &exe_dir())
        .into_iter()
        .find(|p| p.exists())
}

/// The Node runtime: the bundled binary in a real install, else `node` on PATH
/// for a dev tree.
pub fn node<R: Runtime, M: Manager<R>>(app: &M) -> OsString {
    let resource_dir = app.path().resource_dir().ok();
    node_candidates(resource_dir.as_deref(), &exe_dir())
        .into_iter()
        .find(|p| p.exists())
        .map(PathBuf::into_os_string)
        .unwrap_or_else(|| OsString::from("node"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The restart path used to omit the resource directory entirely, so an
    // installed build could not find the script it had just killed.
    #[test]
    fn resource_dir_is_searched_before_the_executable_directory() {
        let rd = PathBuf::from("C:/Program Files/ZANPOS/resources");
        let exe = PathBuf::from("C:/Program Files/ZANPOS");
        let candidates = script_candidates(Some(rd.as_path()), &exe);

        let first_resource = candidates.iter().position(|p| p.starts_with(&rd));
        let first_exe_only = candidates
            .iter()
            .position(|p| p.starts_with(&exe) && !p.starts_with(&rd));
        assert!(
            first_resource < first_exe_only,
            "installed layout must win over the dev layout: {candidates:?}"
        );
    }

    #[test]
    fn both_bundled_layouts_are_covered() {
        let rd = PathBuf::from("/res");
        let candidates = script_candidates(Some(rd.as_path()), &PathBuf::from("/exe"));
        assert!(candidates.contains(
            &rd.join("sidecar")
                .join("whatsapp-sidecar")
                .join("server.mjs")
        ));
        assert!(candidates.contains(&rd.join("sidecar").join("server.mjs")));
    }

    #[test]
    fn node_falls_back_to_path_when_nothing_is_bundled() {
        // No candidate exists under these fake roots, so the caller gets `node`.
        let candidates = node_candidates(None, &PathBuf::from("/nonexistent-zanpos"));
        assert!(candidates.iter().all(|p| !p.exists()));
    }
}
