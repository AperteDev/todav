//! Compiles the Blueprint views in `ui/` to GtkBuilder XML, and the icons in `icons/` to a
//! resource bundle, in OUT_DIR.

use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=ui");
    let out = std::env::var("OUT_DIR").unwrap();
    let blps: Vec<_> = std::fs::read_dir("ui")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "blp"))
        .collect();
    let ok = Command::new("blueprint-compiler")
        .args(["batch-compile", &out, "ui"])
        .args(&blps)
        .status()
        .expect("blueprint-compiler not found; install it to build the Linux app")
        .success();
    assert!(ok, "blueprint-compiler failed");

    // Fallback icons, the app icon and metainfo (for About), embedded in the binary.
    println!("cargo::rerun-if-changed=icons");
    println!("cargo::rerun-if-changed=../data/io.github.selfref.Todav.svg");
    println!("cargo::rerun-if-changed=../data/io.github.selfref.Todav.metainfo.xml");
    let ok = Command::new("glib-compile-resources")
        .args(["--sourcedir", "icons", "--target"])
        .arg(format!("{out}/icons.gresource"))
        .arg("icons/icons.gresource.xml")
        .status()
        .expect("glib-compile-resources not found")
        .success();
    assert!(ok, "glib-compile-resources failed");
}
