//! Build script for `sprocketup`.

fn main() {
    println!("cargo::rerun-if-env-changed=TARGET");

    // `TARGET` is only set for build scripts, so we need to propagate it to the
    // crate compilation.
    let target = std::env::var("TARGET").unwrap();
    println!("cargo::rustc-env=TARGET={target}");
}
