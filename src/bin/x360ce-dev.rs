use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const POLL_INTERVAL: Duration = Duration::from_millis(450);

fn main() {
    let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let watch_paths = [
        root.join("src"),
        root.join("assets"),
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        root.join("build.rs"),
    ];

    println!("x360ce dev runner");
    println!("watching: {}", root.display());
    println!("mode: polling, no cargo-watch dependency");

    let mut last_fingerprint = FileFingerprint::default();
    let mut child: Option<Child> = None;

    loop {
        let fingerprint = fingerprint_paths(&watch_paths);
        if fingerprint != last_fingerprint {
            last_fingerprint = fingerprint;

            stop_child(&mut child);

            if build_app() {
                child = start_app(&root);
            }
        }

        if let Some(app) = child.as_mut() {
            match app.try_wait() {
                Ok(Some(status)) => {
                    println!("x360ce exited with {status}.");
                    child = None;
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!("failed to poll x360ce process: {error}");
                    child = None;
                }
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn build_app() -> bool {
    println!("building x360ce...");

    let status = Command::new("cargo")
        .args(["build", "--bin", "x360ce"])
        .stdin(Stdio::null())
        .status();

    match status {
        Ok(status) if status.success() => {
            println!("build finished; starting app.");
            true
        }
        Ok(status) => {
            eprintln!("build failed with {status}; waiting for next file change.");
            false
        }
        Err(error) => {
            eprintln!("failed to start cargo build: {error}");
            false
        }
    }
}

fn start_app(root: &Path) -> Option<Child> {
    let exe = app_executable_path(root);
    if !exe.exists() {
        eprintln!("built executable was not found: {}", exe.display());
        return None;
    }

    match Command::new(&exe)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(child) => {
            println!("running: {}", exe.display());
            Some(child)
        }
        Err(error) => {
            eprintln!("failed to start x360ce: {error}");
            None
        }
    }
}

fn stop_child(child: &mut Option<Child>) {
    let Some(mut app) = child.take() else {
        return;
    };

    match app.try_wait() {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(error) => {
            eprintln!("failed to poll previous x360ce process: {error}");
            return;
        }
    }

    println!("stopping previous x360ce instance...");
    if let Err(error) = app.kill() {
        eprintln!("failed to stop previous x360ce instance: {error}");
    }
    let _ = app.wait();
}

fn app_executable_path(root: &Path) -> PathBuf {
    let target_dir = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target"));

    let exe_name = if cfg!(windows) {
        "x360ce.exe"
    } else {
        "x360ce"
    };
    target_dir.join("debug").join(exe_name)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct FileFingerprint {
    files: u64,
    modified_nanos: u128,
    bytes: u64,
}

fn fingerprint_paths(paths: &[PathBuf]) -> FileFingerprint {
    let mut fingerprint = FileFingerprint::default();
    for path in paths {
        collect_fingerprint(path, &mut fingerprint);
    }
    fingerprint
}

fn collect_fingerprint(path: &Path, fingerprint: &mut FileFingerprint) {
    if should_ignore(path) || !path.exists() {
        return;
    }

    let Ok(metadata) = fs::metadata(path) else {
        return;
    };

    if metadata.is_file() {
        fingerprint.files = fingerprint.files.saturating_add(1);
        fingerprint.bytes = fingerprint.bytes.saturating_add(metadata.len());
        fingerprint.modified_nanos = fingerprint.modified_nanos.max(system_time_to_nanos(
            metadata.modified().unwrap_or(UNIX_EPOCH),
        ));
        return;
    }

    if !metadata.is_dir() {
        return;
    }

    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    for entry in entries.flatten() {
        collect_fingerprint(&entry.path(), fingerprint);
    }
}

fn system_time_to_nanos(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn should_ignore(path: &Path) -> bool {
    path.components().any(|component| {
        let value = component.as_os_str();
        matches!(
            value.to_str(),
            Some("target") | Some(".git") | Some(".cache") | Some(".x360ce-data")
        ) || is_temporary_file(value)
    })
}

fn is_temporary_file(value: &OsStr) -> bool {
    let Some(name) = value.to_str() else {
        return false;
    };

    name.ends_with('~')
        || name.ends_with(".tmp")
        || name.ends_with(".swp")
        || name.ends_with(".swx")
        || name.starts_with(".#")
}
