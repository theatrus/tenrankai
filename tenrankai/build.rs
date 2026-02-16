fn main() {
    // This build.rs intentionally does nothing.
    // Frontend assets are served from disk at runtime and are NOT embedded in
    // the binary, so frontend changes should never trigger a Rust recompile.
    // Use `make` (or `npm run build`) to build frontend assets separately.
    println!("cargo:rerun-if-changed=build.rs");
}
