//! Build script for `wdl-doc`.

use std::path::Path;

fn main() {
    // In `build.rs`, this is always expected to exist.
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("assets.rs");

    let mut code = String::new();

    let files = std::fs::read_dir("theme/assets")
        .expect("theme assets to exist")
        .filter_map(Result::ok)
        .filter(|entry|
        // SAFETY: we expect that every entry in that directory has a file type.
        entry.file_type().unwrap().is_file())
        .collect::<Vec<_>>();

    code.push_str("use std::collections::HashMap;\n\n");
    code.push_str("/// Gets the assets from the `wdl-doc` bundle.\n");
    code.push_str("pub fn get_assets() -> HashMap<&'static str, &'static [u8]> {\n");
    code.push_str("    let mut map = HashMap::new();\n");

    for file in files {
        let file_name = file.file_name().into_string().expect("UTF-8 filename");
        let bytes = std::fs::read(file.path()).expect("to read assets file");
        code.push_str(&format!(
            "    map.insert(\"{file_name}\", &{bytes:?}[..]);\n"
        ));
    }

    code.push_str("    map\n");
    code.push_str("}\n");

    std::fs::write(&dest_path, code).expect("file to write");

    println!("cargo:rerun-if-changed=theme/assets");
}
