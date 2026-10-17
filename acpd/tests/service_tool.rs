//! `acpd service` status parsing against a fake `systemctl`.
//!
//! The test copies itself to `systemctl.exe` (or `systemctl`). That copy exits as
//! systemctl when `ACPD_FAKE_SYSTEMCTL` is set. The test process keeps its own name.

use std::{fs, path::Path};

fn main() {
    if fake_systemctl_process() {
        fake_systemctl();
    }
    let scratch = tempfile::tempdir().expect("scratch");
    let log = scratch.path().join("systemctl-log.txt");
    let mode_file = scratch.path().join("mode.txt");
    fs::write(&mode_file, "active").unwrap();
    install_fake(scratch.path(), &log, &mode_file);
    let active = acpd::service::systemctl_show().expect("active show");
    assert_eq!(active.load_state, "loaded");
    assert_eq!(active.active_state, "active");
    assert_eq!(active.unit_file_state, "enabled");
    let logged = fs::read_to_string(&log).unwrap();
    let tokens: Vec<&str> = logged.split_whitespace().collect();
    assert!(tokens.contains(&"--user"));
    assert!(tokens.contains(&"show"));
    assert!(tokens.contains(&"acpd.service"));
    assert!(!tokens.contains(&"sudo"));
    assert!(!tokens.contains(&"sh"));
    assert!(!logged.contains("sh -c"));

    fs::write(&mode_file, "inactive").unwrap();
    let inactive = acpd::service::systemctl_show().expect("inactive show");
    assert_eq!(inactive.active_state, "inactive");
    assert_eq!(inactive.unit_file_state, "disabled");

    fs::write(&mode_file, "missing").unwrap();
    let missing = acpd::service::systemctl_show().expect("missing show");
    assert_eq!(missing.load_state, "not-found");
    assert!(missing.unit_file_state.is_empty());
}

fn fake_systemctl_process() -> bool {
    std::env::var_os("ACPD_FAKE_SYSTEMCTL").is_some()
        && std::env::current_exe()
            .ok()
            .and_then(|path| {
                path.file_name().map(|name| {
                    name.to_string_lossy().eq_ignore_ascii_case("systemctl.exe")
                        || name.to_string_lossy().eq_ignore_ascii_case("systemctl")
                })
            })
            .unwrap_or(false)
}

fn fake_systemctl() -> ! {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(path) = std::env::var("FAKE_SYSTEMCTL_LOG") {
        let _ = fs::write(path, args.join(" "));
    }
    let mode = std::env::var("FAKE_SYSTEMCTL_MODE_FILE")
        .ok()
        .and_then(|path| fs::read_to_string(path).ok())
        .unwrap_or_default();
    match mode.trim() {
        "inactive" => {
            println!("LoadState=loaded\nActiveState=inactive\nUnitFileState=disabled");
        }
        "missing" => {
            println!("LoadState=not-found\nActiveState=inactive\nUnitFileState=");
        }
        _ => {
            println!("LoadState=loaded\nActiveState=active\nUnitFileState=enabled");
        }
    }
    std::process::exit(0);
}

fn install_fake(dir: &Path, log: &Path, mode_file: &Path) {
    let src = std::env::current_exe().expect("current exe");
    let dest = dir.join(if cfg!(windows) {
        "systemctl.exe"
    } else {
        "systemctl"
    });
    fs::copy(&src, &dest).expect("copy fake systemctl");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).unwrap();
    }
    let mut paths =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect::<Vec<_>>();
    paths.insert(0, dir.to_path_buf());
    // SAFETY: this process is still the only thread. The fake systemctl inherits the variables.
    unsafe {
        std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
        std::env::set_var("ACPD_FAKE_SYSTEMCTL", "1");
        std::env::set_var("FAKE_SYSTEMCTL_LOG", log);
        std::env::set_var("FAKE_SYSTEMCTL_MODE_FILE", mode_file);
    }
}
