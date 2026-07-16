use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const POLL_INTERVAL: Duration = Duration::from_millis(450);

fn main() {
    let root = project_root();
    if let Err(error) = env::set_current_dir(&root) {
        eprintln!(
            "failed to enter project directory {}: {error}",
            root.display()
        );
        std::process::exit(1);
    }

    #[cfg(windows)]
    configure_cmake_or_exit();

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

fn project_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

#[cfg(windows)]
fn configure_cmake_or_exit() {
    let Some(cmake) = find_working_cmake() else {
        eprintln!("working CMake executable not found.");
        eprintln!("Install Visual Studio 2022 Desktop development with C++ including C++ CMake tools for Windows, or install standalone CMake.");
        eprintln!("If mise owns cmake shim, install managed CMake with: mise use -g cmake@latest");
        std::process::exit(1);
    };

    env::set_var("CMAKE", &cmake);
    if let Some(directory) = cmake.parent() {
        prepend_path(directory);
    }
    println!("cmake: {}", cmake.display());
}

#[cfg(windows)]
fn find_working_cmake() -> Option<PathBuf> {
    let mut candidates = Vec::<PathBuf>::new();

    if let Some(value) = env::var_os("CMAKE") {
        candidates.push(PathBuf::from(value));
    }

    if let Ok(output) = Command::new("where.exe").arg("cmake.exe").output() {
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                let value = line.trim();
                if !value.is_empty() {
                    candidates.push(PathBuf::from(value));
                }
            }
        }
    }

    if let Some(program_files) = env::var_os("ProgramFiles") {
        let program_files = PathBuf::from(program_files);
        candidates.push(program_files.join("CMake/bin/cmake.exe"));
        add_visual_studio_candidates(&program_files, &mut candidates);
    }

    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        let program_files_x86 = PathBuf::from(program_files_x86);
        candidates.push(program_files_x86.join("CMake/bin/cmake.exe"));
        add_visual_studio_candidates(&program_files_x86, &mut candidates);

        let vswhere = program_files_x86.join("Microsoft Visual Studio/Installer/vswhere.exe");
        if vswhere.is_file() {
            if let Ok(output) = Command::new(vswhere)
                .args([
                    "-latest",
                    "-products",
                    "*",
                    "-requires",
                    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                    "-property",
                    "installationPath",
                ])
                .output()
            {
                if output.status.success() {
                    for line in String::from_utf8_lossy(&output.stdout).lines() {
                        let installation = line.trim();
                        if !installation.is_empty() {
                            candidates.push(PathBuf::from(installation).join(
                                "Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe",
                            ));
                        }
                    }
                }
            }
        }
    }

    let mut checked = Vec::<OsString>::new();
    candidates.into_iter().find(|candidate| {
        let key = candidate.as_os_str().to_os_string();
        if checked.iter().any(|value| value == &key) {
            return false;
        }
        checked.push(key);
        is_working_cmake(candidate)
    })
}

#[cfg(windows)]
fn add_visual_studio_candidates(base: &Path, candidates: &mut Vec<PathBuf>) {
    let visual_studio_root = base.join("Microsoft Visual Studio/2022");
    let Ok(editions) = fs::read_dir(visual_studio_root) else {
        return;
    };

    for edition in editions.flatten() {
        candidates.push(
            edition
                .path()
                .join("Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe"),
        );
    }
}

#[cfg(windows)]
fn is_working_cmake(candidate: &Path) -> bool {
    let output = Command::new(candidate)
        .arg("--version")
        .stdin(Stdio::null())
        .output();

    let Ok(output) = output else {
        return false;
    };
    output.status.success() && String::from_utf8_lossy(&output.stdout).contains("cmake version")
}

#[cfg(windows)]
fn prepend_path(directory: &Path) {
    let current = env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![directory.to_path_buf()];
    paths.extend(env::split_paths(&current));
    if let Ok(value) = env::join_paths(paths) {
        env::set_var("PATH", value);
    }
}

fn cargo_command() -> OsString {
    env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

fn build_app() -> bool {
    println!("building x360ce...");

    let status = Command::new(cargo_command())
        .args(["build", "--package", "x360ce", "--bin", "x360ce"])
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
