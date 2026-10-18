//! Embeds everything in `examples/boxes` and `examples/compositions` into
//! the binary, so the bundled examples are read-only for the user: they can
//! only be copied onto the canvas, never edited in place.

use std::{env, fs, path::Path};

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let mut out = String::from("pub static EMBEDDED: &[(&str, &str, &str)] = &[\n");
    for group in ["boxes", "compositions"] {
        let dir = Path::new(&manifest).join("examples").join(group);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut files: Vec<_> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .collect();
        files.sort();
        for path in files {
            let stem = path.file_stem().unwrap().to_string_lossy().to_string();
            out.push_str(&format!("    ({group:?}, {stem:?}, include_str!({:?})),\n", path.display().to_string()));
        }
    }
    out.push_str("];\n");
    let dest = Path::new(&env::var("OUT_DIR").unwrap()).join("examples_gen.rs");
    fs::write(dest, out).unwrap();
}
