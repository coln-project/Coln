// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

// TODO we'd prefer to just have Cabal drive the build
// see comment elsewhere about "Cabal Hooks", which are not yet really mature enough

use cbindgen::*;
use std::env;
use std::path::PathBuf;

fn main() {
    // this doesn't make _much_ difference really, since this is our only Rust source file
    // but it seems it's probably better than not having it
    // what we really want is to tell Rust to only regenerate the header file if the Rust code actually compiles
    // but we don't have that flexibility
    // and it's an issue because cbindgen tries to be fault-tolerant in some ways that don't even seem to make sense
    //
    // e.g. mis-spell "Option" as "Option" and you get
    // void print_optional(Optio<const int8_t*> x);
    // instead of
    // void print_optional(const int8_t *x);
    // and that's only an issue because in HLS TH dependent-file watching gives up after an error
    // i.e. once the containing splice has thrown an exception once, the containing file needs a manual edit to kick it
    // and that's really not helped by Rust Analyzer mostly only showing diagnostics on save
    // P.S. strings to stdout?! what a terrible API
    // don't get me started in the discoverability of actually then using the terminal for debugging:
    // println!("cargo::warning={:?}", env::var("OUT_DIR"));
    println!("cargo::rerun-if-changed=src/lib.rs");
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let header_file_name = "coln_store.h";
    let profile = env::var("PROFILE").unwrap();
    let target_dir = PathBuf::from(&crate_dir)
        .join("..")
        .join("..")
        .join("target")
        .join(&profile);
    let hs_include_dir = PathBuf::from(&crate_dir)
        .join("..")
        .join("coln-store-hs")
        .join("include");
    let mut config = Config::default();
    // TODO from test project - I don't think anything here yet needs this mangling config
    config.export.mangle.remove_underscores = true;
    config.export.mangle.rename_types = RenameRule::PascalCase;
    // TODO do we actually need this line?
    config.parse.expand.crates = vec!["coln-store-ffi".to_string()];
    match Builder::new()
        .with_config(config)
        .with_crate(&crate_dir)
        .with_language(Language::C)
        .with_style(Style::Tag)
        .with_parse_deps(true)
        .with_parse_include(&["coln-store", "coln-flir-rs"])
        .generate()
    {
        // TODO is generating in both locations overkill?
        // would it be better if it were configurable, or if our Just/Nix code had a copy step?
        Ok(bindings) => {
            bindings.write_to_file(target_dir.join(header_file_name));
            if std::fs::create_dir_all(&hs_include_dir).is_ok() {
                bindings.write_to_file(hs_include_dir.join(header_file_name));
            }
        }
        // transient error - best to just let it slide
        // we'll see it from build or LSP anyway
        // and we don't want to generate a malformed header - see above about needing to "kick" the TH
        // TODO are there more of these sorts of errors that we should ignore?
        Err(cbindgen::Error::ParseSyntaxError { .. }) => {}
        Err(err) => panic!("Unable to generate bindings: {err}"),
    }
}
