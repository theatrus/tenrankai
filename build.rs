use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/js");
    println!("cargo:rerun-if-changed=src/css");
    println!("cargo:rerun-if-changed=src/assets");
    println!("cargo:rerun-if-changed=package.json");
    println!("cargo:rerun-if-changed=tsconfig.json");

    // Always build frontend if setup is present
    let profile = env::var("PROFILE").unwrap_or_default();

    if profile == "release" {
        build_frontend();
    } else {
        println!(
            "cargo:warning=Skipping frontend build in debug mode for faster development iteration."
        );
        println!("cargo:warning=Frontend will be built automatically in release mode.");
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

    println!("cargo:warning=Building frontend assets...");

    // Install dependencies if node_modules doesn't exist
    if !frontend_dir.join("node_modules").exists() {
        println!("cargo:warning=Installing frontend dependencies...");
        let output = Command::new("npm")
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

    // Run the build
    println!("cargo:warning=Compiling TypeScript...");
    let output = Command::new("npm")
        .arg("run")
        .arg("build")
        .current_dir(frontend_dir)
        .output()
        .expect("Failed to run npm run build");

    if !output.status.success() {
        panic!(
            "Frontend build failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    println!("cargo:warning=Frontend build completed successfully.");
}

fn check_node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
