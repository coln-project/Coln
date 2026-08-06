// SPDX-FileCopyrightText: 2026 Coln contributors
//
// SPDX-License-Identifier: Apache-2.0 OR MIT

// TODO avoid requiring Cabal bundled library naming convention - see Cargo.toml
#![allow(non_snake_case)]

use std::ffi::{CStr, CString, c_char};

use coln_flir_rs::ir::FlatRealm;
use coln_store::store::Store;

// TODO should these all be marked unsafe?
// https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-extern.html
// adds some documentation overhead if we want to be lint-clean (`# Safety` section), but then it's no bad thing really
// and sounds like it's becoming compulsory anyway
// indeed if we move to nightlies we already trigger the warning...

// TODO const not working to make this a pure function? that sort of thing did work in test project
/// Attempt to parse a string as a JSON encoding of a `FlatRealm`.
/// The caller is responsible for freeing the result with `free_theory` (success) or `free_string` (failure).
/// The input must be a valid, null-terminated UTF-8 string.
/// cbindgen:prefix=__attribute__((const))
#[unsafe(no_mangle)]
pub extern "C" fn theory_from_json(s: *const c_char) -> Result<*const FlatRealm, *const c_char> {
    match serde_json::from_str((unsafe { CStr::from_ptr(s) }.to_str()).unwrap()) {
        Ok(res) => Result::Ok(Box::into_raw(Box::new(res))),
        Err(err) => Result::Err(CString::new(err.to_string()).unwrap().into_raw()),
    }
}

/// Serialise a `FlatRealm` to JSON.
/// The caller is responsible for freeing the result with `free_string`.
#[unsafe(no_mangle)]
pub extern "C" fn theory_to_json(t: &FlatRealm) -> *const c_char {
    CString::new(serde_json::to_string(t).unwrap())
        .unwrap()
        .into_raw()
}

/// Debug-format a `FlatRealm`.
/// The caller is responsible for freeing the result with `free_string`.
#[unsafe(no_mangle)]
pub extern "C" fn theory_debug(t: &FlatRealm) -> *const c_char {
    CString::new(format!("{t:#?}")).unwrap().into_raw()
}

/// Attempt to create a `Store` from a `FlatRealm`.
/// The caller is responsible for freeing the result with `free_store` (success) or `free_string` (failure).
#[unsafe(no_mangle)]
pub extern "C" fn store_from_theory(t: &FlatRealm) -> Result<*const Store, *const c_char> {
    match Store::try_from_theory(t.clone()) {
        Ok(res) => Result::Ok(Box::into_raw(Box::new(res))),
        Err(err) => Result::Err(CString::new(err.to_string()).unwrap().into_raw()),
    }
}

/// Dump the store contents as a string.
/// The caller is responsible for freeing the result with `free_string`.
#[unsafe(no_mangle)]
pub extern "C" fn store_dump(s: &Store) -> *const c_char {
    CString::new(s.dump()).unwrap().into_raw()
}

/// Equivalent to Rust's standard Result, but with a C memory layout for FFI.
/// TODO this tag doesn't work yet, but I'd like to propose it - see the Yaml file
/// @hs_bindgen_enum closed
#[repr(C)]
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}

/// Free a Rust-allocated C string.
/// The pointer must not be used after calling this.
#[unsafe(no_mangle)]
pub extern "C" fn free_string(s: *mut c_char) {
    unsafe { drop(CString::from_raw(s)) }
}

macro_rules! free_box {
    ($Type:ty, $free:ident) => {
        #[doc = concat!("Free a `", stringify!($Type), "` on the Rust heap.")]
        #[doc = "The pointer must not be used after calling this."]
        #[unsafe(no_mangle)]
        pub extern "C" fn $free(p: *mut $Type) {
            unsafe { drop(Box::from_raw(p)) }
        }
    };
}

// TODO the whole reason we're using nightlies of the Rust compiler is so that `cbindgen` can see through these macros
// i.e. so we can use `cargo-expand`
// idk if that's truly worth it, given the issues...
// e.g. workspace-wide `cargo test` and `cargo clippy --all-targets` fail due to issues with `dbsp`
free_box!(FlatRealm, free_theory);
free_box!(Store, free_store);
