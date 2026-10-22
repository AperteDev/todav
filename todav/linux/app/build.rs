//! Compiles the Blueprint views in `ui/` to GtkBuilder XML, the translations in `po/` (for a build
//! run; installs compile their own, see the Makefile), and the icons in `icons/` with the
//! translated metainfo to a resource bundle, in OUT_DIR.

use std::process::Command;

const PO: &str = "../../../po";
const METAINFO: &str = "../data/io.github.selfref.Todav.metainfo.xml";

fn msgfmt(args: &[&str]) {
    let ok = Command::new("msgfmt")
        .args(args)
        // po/its: what of the metainfo is translated.
        .env("GETTEXTDATADIRS", PO)
        .status()
        .expect("msgfmt not found; install gettext to build the Linux app")
        .success();
    assert!(ok, "msgfmt {args:?} failed");
}

fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    println!("cargo::rerun-if-changed={PO}");
    let linguas = std::fs::read_to_string(format!("{PO}/LINGUAS")).unwrap_or_default();
    for lang in linguas
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let dir = format!("{out}/locale/{lang}/LC_MESSAGES");
        std::fs::create_dir_all(&dir).unwrap();
        msgfmt(&[
            "--check",
            "-o",
            &format!("{dir}/todav.mo"),
            &format!("{PO}/{lang}.po"),
        ]);
    }
    msgfmt(&[
        "--xml",
        "--template",
        METAINFO,
        "-d",
        PO,
        "-o",
        &format!("{out}/io.github.selfref.Todav.metainfo.xml"),
    ]);

    println!("cargo::rerun-if-changed=ui");
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

    // Fallback icons, the app icon and the translated metainfo (for About, from OUT_DIR),
    // embedded in the binary.
    println!("cargo::rerun-if-changed=icons");
    println!("cargo::rerun-if-changed=../data/io.github.selfref.Todav.svg");
    println!("cargo::rerun-if-changed={METAINFO}");
    let ok = Command::new("glib-compile-resources")
        .args(["--sourcedir", &out, "--sourcedir", "icons", "--target"])
        .arg(format!("{out}/icons.gresource"))
        .arg("icons/icons.gresource.xml")
        .status()
        .expect("glib-compile-resources not found")
        .success();
    assert!(ok, "glib-compile-resources failed");
}
