/// Build script: expose the compilation target triple to the crate.
///
/// Cargo sets the `TARGET` environment variable for build scripts but not for
/// the crate itself, so we forward it via `cargo:rustc-env` and read it back
/// with `env!("BUILD_TARGET")` (e.g. `x86_64-pc-windows-gnu`).
fn main() {
    let target = std::env::var("TARGET").expect("TARGET env var not set by Cargo");
    println!("cargo:rustc-env=BUILD_TARGET={target}");
    println!("cargo:rerun-if-changed=build.rs");
}
