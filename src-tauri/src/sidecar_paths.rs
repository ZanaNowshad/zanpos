//! Starting the WhatsApp sidecar: where its Node runtime and entry script live,
//! and how the process is launched.
//!
//! Startup and the manual restart command both come through here, and the
//! reason is a history of them drifting. They resolved paths separately once,
//! and the restart path looked only next to the executable — right in a dev
//! tree, wrong in an installed build where the installer writes to the Tauri
//! *resource* directory — so restarting killed a healthy sidecar and then could
//! not find anything to respawn.
//!
//! Sharing the resolver fixed that and left the *spawn* duplicated, which drifted
//! the same way and worse. The copy in the restart command discarded the
//! sidecar's output, skipped the session directory, never retried, and — the
//! one that bites hardest — never bound the child to a Job Object. A sidecar
//! spawned that way outlives ZANPOS: an orphaned `node.exe` holding port 3131
//! and a lock on its own directory, which is exactly the process that has to be
//! hunted down before a release build will link.
//!
//! So the whole launch lives here now, once.

use std::ffi::{OsStr, OsString};
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

/// Launch the sidecar.
///
/// `max_attempts` exists because the failure this retries is real and local: a
/// virus scanner or a still-dying previous instance can hold `node.exe` for a
/// moment after the old process was killed, and the restart command hits that
/// window far more often than startup does.
pub async fn spawn(
    node: &OsStr,
    script: &Path,
    session_dir: &Path,
    log_path: &Path,
    max_attempts: u32,
) -> Option<tokio::process::Child> {
    // The session directory is the sidecar's working state — it writes its auth
    // token there before it can answer anything. Startup created it and the
    // restart path did not, so a restart after the directory was cleared handed
    // Node a `--session-dir` pointing at nothing.
    let _ = std::fs::create_dir_all(session_dir);
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Ok(meta) = std::fs::metadata(log_path) {
        if meta.len() > 5 * 1024 * 1024 {
            let rotated = log_path.with_extension("log.1");
            let _ = std::fs::remove_file(&rotated);
            let _ = std::fs::rename(log_path, &rotated);
        }
    }

    tracing::info!(
        "[wa-sidecar] spawn: node={:?} script={:?} session_dir={:?}",
        node,
        script,
        session_dir
    );

    for attempt in 1..=max_attempts {
        let mut cmd = tokio::process::Command::new(node);

        // On Windows, paths under "Program Files" contain spaces. Passing a
        // path-with-spaces as a command-line argument hits Win32 quoting edge
        // cases that leave Node with a bare drive letter ("C:") and an immediate
        // EISDIR crash. Setting CWD to the script's directory and passing only
        // the filename sidesteps it — "server.mjs" needs no quoting, and ESM
        // resolves `import.meta.url` from the file's real location, not CWD.
        if let Some(dir) = script.parent() {
            cmd.current_dir(dir);
            cmd.arg(script.file_name().unwrap_or(script.as_os_str()));
        } else {
            cmd.arg(script);
        }
        cmd.arg(format!("--session-dir={}", session_dir.to_string_lossy()));

        // Both streams to the same appended log. A restart is something someone
        // pressed *because* WhatsApp was misbehaving, so throwing the output
        // away discards the diagnosis at the exact moment it is wanted.
        let out_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path);
        let err_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path);
        match (out_file, err_file) {
            (Ok(out), Ok(err)) => {
                cmd.stdout(out).stderr(err);
            }
            _ => {
                cmd.stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());
            }
        }

        #[cfg(target_os = "windows")]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        match cmd.spawn() {
            Ok(child) => {
                tracing::info!(
                    "WhatsApp sidecar started (pid {:?}, attempt {})",
                    child.id(),
                    attempt
                );
                #[cfg(target_os = "windows")]
                if let Some(pid) = child.id() {
                    bind_to_job_object(pid);
                }
                return Some(child);
            }
            Err(e) if attempt < max_attempts => {
                tracing::warn!(
                    "WhatsApp sidecar start attempt {}/{} failed: {} — retrying in 1 s",
                    attempt,
                    max_attempts,
                    e
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                tracing::error!(
                    "WhatsApp sidecar failed to start after {} attempts: {}",
                    max_attempts,
                    e
                );
            }
        }
    }
    None
}

/// Tie the sidecar's lifetime to ours.
///
/// Windows does not kill children when a parent exits, so without this the
/// sidecar can outlive ZANPOS — however ZANPOS went, clean close or crash. A
/// job object with KILL_ON_JOB_CLOSE makes it a strict subprocess: the OS closes
/// the handle when we go and terminates everything in the job with us.
#[cfg(target_os = "windows")]
pub(crate) fn bind_to_job_object(pid: u32) {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::jobapi2::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    };
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winnt::{
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job.is_null() {
            tracing::warn!("WA sidecar job object: CreateJobObjectW failed");
            return;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            tracing::warn!("WA sidecar job object: SetInformationJobObject failed");
            CloseHandle(job);
            return;
        }
        // PROCESS_SET_QUOTA | PROCESS_TERMINATE — minimum for AssignProcessToJobObject
        let proc = OpenProcess(0x0001 | 0x0100, 0, pid);
        if proc.is_null() {
            tracing::warn!("WA sidecar job object: OpenProcess({}) failed", pid);
            CloseHandle(job);
            return;
        }
        if AssignProcessToJobObject(job, proc) == 0 {
            tracing::warn!("WA sidecar job object: AssignProcessToJobObject({}) failed — sidecar will not be auto-killed on exit", pid);
            CloseHandle(proc);
            CloseHandle(job);
            return;
        }
        CloseHandle(proc);
        // Intentionally keep `job` open — a Windows HANDLE has no Rust Drop. It
        // stays open until ZANPOS exits, at which point the OS closes it and
        // kills every process in the job, the sidecar included.
        tracing::info!(
            "WA sidecar PID {} bound to job object (strict subprocess)",
            pid
        );
    }
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
