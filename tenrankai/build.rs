use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

fn main() {
    // Tell Cargo when to re-run build.rs
    // Note: For directories, Cargo only checks if the directory itself changed,
    // not the files within. We handle file-level checking in needs_rebuild().
    // Paths are relative to workspace root (one level up from this crate)
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../package.json");
    println!("cargo:rerun-if-changed=../package-lock.json");
    println!("cargo:rerun-if-changed=../tsconfig.json");
    println!("cargo:rerun-if-changed=../tsconfig.legacy.json");
    println!("cargo:rerun-if-changed=../vite.config.ts");
    println!("cargo:rerun-if-changed=../vite.config.js");
    println!("cargo:rerun-if-changed=../frontend/legacy");
    println!("cargo:rerun-if-changed=../frontend/react");
    println!("cargo:rerun-if-changed=../src/css");
    println!("cargo:rerun-if-changed=../src/assets");
    println!("cargo:rerun-if-changed=../static");
    println!("cargo:rerun-if-changed=../scripts");

    // Control frontend builds based on environment
    let skip_frontend_build = env::var("TENRANKAI_SKIP_FRONTEND").is_ok();

    if skip_frontend_build {
        println!("cargo:warning=Skipping frontend build (TENRANKAI_SKIP_FRONTEND=1)");
        return;
    }

    // Check if we need to rebuild by comparing source and output timestamps
    if !needs_rebuild() {
        // No rebuild needed - sources haven't changed
        return;
    }

    // Build frontend
    build_frontend();
}

/// Check if frontend rebuild is needed by comparing source and output timestamps
fn needs_rebuild() -> bool {
    // Paths relative to workspace root (parent of this crate)
    let output_dirs = ["../static/dist", "../static/js"];
    let source_dirs = [
        "../frontend/legacy",
        "../frontend/react",
        "../src/css",
        "../src/assets",
    ];
    let config_files = [
        "../package.json",
        "../tsconfig.json",
        "../tsconfig.legacy.json",
        "../vite.config.ts",
        "../vite.config.js",
    ];

    // Get the newest source file timestamp
    let mut newest_source: Option<SystemTime> = None;

    // Check config files
    for file in &config_files {
        if let Some(mtime) = get_mtime(Path::new(file)) {
            newest_source = Some(match newest_source {
                Some(current) => current.max(mtime),
                None => mtime,
            });
        }
    }

    // Check source directories
    for dir in &source_dirs {
        if let Some(mtime) = get_newest_mtime_recursive(Path::new(dir)) {
            newest_source = Some(match newest_source {
                Some(current) => current.max(mtime),
                None => mtime,
            });
        }
    }

    let newest_source = match newest_source {
        Some(t) => t,
        None => {
            // No source files found, skip build
            return false;
        }
    };

    // Get the oldest output file timestamp (if any output exists)
    let mut oldest_output: Option<SystemTime> = None;
    let mut any_output_exists = false;

    for dir in &output_dirs {
        let dir_path = Path::new(dir);
        if dir_path.exists() {
            any_output_exists = true;
            if let Some(mtime) = get_oldest_mtime_recursive(dir_path) {
                oldest_output = Some(match oldest_output {
                    Some(current) => current.min(mtime),
                    None => mtime,
                });
            }
        }
    }

    // If no output exists, we need to build
    if !any_output_exists {
        return true;
    }

    // If we have outputs, check if sources are newer
    match oldest_output {
        Some(output_time) => newest_source > output_time,
        None => true, // Output dir exists but is empty
    }
}

/// Get modification time of a file
fn get_mtime(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

/// Get the newest modification time of any file in a directory (recursive)
fn get_newest_mtime_recursive(dir: &Path) -> Option<SystemTime> {
    if !dir.exists() {
        return None;
    }

    let mut newest: Option<SystemTime> = None;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            let mtime = if path.is_dir() {
                get_newest_mtime_recursive(&path)
            } else {
                get_mtime(&path)
            };

            if let Some(t) = mtime {
                newest = Some(match newest {
                    Some(current) => current.max(t),
                    None => t,
                });
            }
        }
    }

    newest
}

/// Get the oldest modification time of any file in a directory (recursive)
fn get_oldest_mtime_recursive(dir: &Path) -> Option<SystemTime> {
    if !dir.exists() {
        return None;
    }

    let mut oldest: Option<SystemTime> = None;

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();

            let mtime = if path.is_dir() {
                get_oldest_mtime_recursive(&path)
            } else {
                get_mtime(&path)
            };

            if let Some(t) = mtime {
                oldest = Some(match oldest {
                    Some(current) => current.min(t),
                    None => t,
                });
            }
        }
    }

    oldest
}

fn npm_command() -> &'static str {
    if cfg!(target_os = "windows") {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn node_command() -> &'static str {
    if cfg!(target_os = "windows") {
        "node.exe"
    } else {
        "node"
    }
}

fn build_frontend() {
    // Frontend directory is at workspace root (parent of this crate)
    let frontend_dir = Path::new("..");

    // Check if Node.js and npm are available
    if !check_node_available() {
        println!("cargo:warning=Node.js not found. Skipping frontend build.");
        println!("cargo:warning=Install Node.js to enable TypeScript compilation.");
        return;
    }

    // Check if package.json exists (frontend setup completed)
    if !frontend_dir.join("package.json").exists() {
        println!("cargo:warning=Frontend not set up (no package.json). Skipping frontend build.");
        return;
    }

    // Install dependencies if node_modules doesn't exist
    if !frontend_dir.join("node_modules").exists() {
        println!("cargo:warning=Installing frontend dependencies...");
        let output = Command::new(npm_command())
            .arg("install")
            .current_dir(frontend_dir)
            .output()
            .expect("Failed to run npm install");

        if !output.status.success() {
            panic!(
                "Frontend dependency installation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    // Build legacy TypeScript first
    build_legacy_typescript(frontend_dir);

    // Then build Vite/React
    if !frontend_dir.join("vite.config.js").exists()
        && !frontend_dir.join("vite.config.ts").exists()
    {
        println!("cargo:warning=No Vite configuration found. Skipping React build.");
        return;
    }

    build_with_vite(frontend_dir);
}

fn build_legacy_typescript(frontend_dir: &Path) {
    // Check if legacy TypeScript source exists (now at frontend/legacy)
    if !frontend_dir.join("frontend/legacy").exists() {
        println!(
            "cargo:warning=No legacy TypeScript found (frontend/legacy). Skipping legacy build."
        );
        return;
    }

    println!("cargo:warning=Building legacy TypeScript...");

    let output = Command::new(npm_command())
        .arg("run")
        .arg("build:legacy")
        .current_dir(frontend_dir)
        .output()
        .expect("Failed to run legacy TypeScript build");

    if !output.status.success() {
        panic!(
            "Legacy TypeScript build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    println!("cargo:warning=Legacy TypeScript build completed successfully.");
}

fn build_with_vite(frontend_dir: &Path) {
    println!("cargo:warning=Building frontend with Vite (React)...");

    // Use production build for release
    let is_release = std::env::var("PROFILE").unwrap_or_default() == "release";
    let build_command = if is_release { "build:prod" } else { "build" };

    let output = Command::new(npm_command())
        .arg("run")
        .arg(build_command)
        .current_dir(frontend_dir)
        .output()
        .expect("Failed to run Vite build");

    if !output.status.success() {
        panic!(
            "Vite build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    println!("cargo:warning=Vite build completed successfully.");

    // Run linting for release builds
    if is_release {
        run_frontend_linting(frontend_dir);
    }
}

fn run_frontend_linting(frontend_dir: &Path) {
    println!("cargo:warning=Running frontend linting (release build)...");

    // Run the full lint suite (TypeScript + CSS + CSS variables)
    let output = Command::new(npm_command())
        .arg("run")
        .arg("lint")
        .current_dir(frontend_dir)
        .output()
        .expect("Failed to run frontend linting");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("Frontend linting failed:\n{}\n{}", stdout, stderr);
    }

    println!("cargo:warning=Frontend linting passed.");
}

fn check_node_available() -> bool {
    Command::new(node_command())
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
