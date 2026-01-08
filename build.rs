use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/js");
    println!("cargo:rerun-if-changed=src/frontend");
    println!("cargo:rerun-if-changed=src/css");
    println!("cargo:rerun-if-changed=src/assets");
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=tsconfig.json");
    println!("cargo:rerun-if-changed=tsconfig.legacy.json");
    println!("cargo:rerun-if-changed=vite.config.ts");
    println!("cargo:rerun-if-changed=vite.config.js");

    // Control frontend builds based on environment
    let skip_frontend_build = env::var("TENRANKAI_SKIP_FRONTEND").is_ok();

    if skip_frontend_build {
        println!("cargo:warning=Skipping frontend build (TENRANKAI_SKIP_FRONTEND=1)");
        return;
    }

    // Always build frontend
    build_frontend();
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
    let frontend_dir = Path::new(".");

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
    // Check if legacy TypeScript source exists
    if !frontend_dir.join("src/js").exists() {
        println!("cargo:warning=No legacy TypeScript found (src/js). Skipping legacy build.");
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
    let build_command = if std::env::var("PROFILE").unwrap_or_default() == "release" {
        "build:prod"
    } else {
        "build"
    };

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
}

fn check_node_available() -> bool {
    Command::new(node_command())
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
