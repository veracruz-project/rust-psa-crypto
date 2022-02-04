// Copyright 2020 Contributors to the Parsec project.
// SPDX-License-Identifier: Apache-2.0

#![deny(
    nonstandard_style,
    const_err,
    dead_code,
    improper_ctypes,
    non_shorthand_field_patterns,
    no_mangle_generic_items,
    overflowing_literals,
    path_statements,
    patterns_in_fns_without_body,
    private_in_public,
    unconditional_recursion,
    unused,
    unused_allocation,
    unused_comparisons,
    unused_parens,
    while_true,
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unused_extern_crates,
    unused_import_braces,
    unused_qualifications,
    unused_results,
    missing_copy_implementations
)]
// This one is hard to avoid.
#![allow(clippy::multiple_crate_versions)]

fn main() -> std::io::Result<()> {
    #[cfg(feature = "operations")]
    return operations::script_operations();

    #[cfg(all(feature = "interface", not(feature = "operations")))]
    return interface::script_interface();

    #[cfg(not(any(feature = "interface", feature = "operations")))]
    Ok(())
}

#[cfg(any(feature = "interface", feature = "operations"))]
mod common {
    use std::env;
    use std::io::{Error, ErrorKind, Result};
    use std::path::{Path, PathBuf};

    pub fn configure_mbed_crypto() -> Result<()> {
        let mbedtls_dir = String::from("./vendor");
        let mbedtls_config = mbedtls_dir + "/scripts/config.py";

        println!("cargo:rerun-if-changed=src/c/mbedtls-config/user_config.h");
        println!("cargo:rerun-if-changed=src/c/mbedtls_hardware_poll.c");
        println!("cargo:rerun-if-changed=src/c/shim.c");
        println!("cargo:rerun-if-changed=src/c/shim.h");

        let out_dir = env::var("OUT_DIR").unwrap();

        //  Check for Mbed TLS sources
        if !Path::new(&mbedtls_config).exists() {
            return Err(Error::new(
                ErrorKind::Other,
                "MbedTLS config.py is missing. Have you run 'git submodule update --init'?",
            ));
        }

        // Configure the MbedTLS build for making Mbed Crypto
        if !::std::process::Command::new(mbedtls_config)
            .arg("--write")
            .arg(&(out_dir + "/config.h"))
            .arg("crypto_baremetal")
            .status()
            .map_err(|_| Error::new(ErrorKind::Other, "configuring mbedtls failed"))?
            .success()
        {
            return Err(Error::new(
                ErrorKind::Other,
                "config.py returned an error status",
            ));
        }

        Ok(())
    }

    pub fn generate_mbed_crypto_bindings(mbed_include_dir: String) -> Result<()> {
        let header = mbed_include_dir.clone() + "/psa/crypto.h";

        println!("cargo:rerun-if-changed={}", header);

        let out_dir = env::var("OUT_DIR").unwrap();

        let shim_bindings = bindgen::Builder::default()
            .clang_arg(format!("-I{}", out_dir))
            .clang_arg("-DMBEDTLS_CONFIG_FILE=<config.h>")
            .clang_arg(format!("-I{}/src/c/mbedtls-config",
                               env::current_dir()?.to_str().unwrap()))
            .clang_arg("-DMBEDTLS_USER_CONFIG_FILE=<user_config.h>")
            .clang_arg(format!("-I{}", mbed_include_dir))
            .rustfmt_bindings(true)
            .header("src/c/shim.h")
            .blacklist_type("max_align_t")
            .generate_comments(false)
            .size_t_is_usize(true)
            .generate()
            .map_err(|_| {
                Error::new(
                    ErrorKind::Other,
                    "Unable to generate bindings to mbed crypto",
                )
            })?;
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        shim_bindings.write_to_file(out_path.join("shim_bindings.rs"))?;

        Ok(())
    }

    pub fn compile_shim_library(include_dir: String) -> Result<()> {
        let out_dir = env::var("OUT_DIR").unwrap();

        // Compile and package the shim library
        cc::Build::new()
            .include(&out_dir)
            .define("MBEDTLS_CONFIG_FILE", "<config.h>")
            .include("src/c/mbedtls-config")
            .define("MBEDTLS_USER_CONFIG_FILE", "<user_config.h>")
            .include(include_dir)
            .file("./src/c/mbedtls_hardware_poll.c")
            .file("./src/c/shim.c")
            .warnings(true)
            .flag("-Werror")
            .opt_level(2)
            .try_compile("libshim.a")
            .map_err(|_| Error::new(ErrorKind::Other, "compiling shim.c failed"))?;

        // Also link shim library
        println!("cargo:rustc-link-search=native={}", out_dir);
        println!("cargo:rustc-link-lib=static=shim");

        Ok(())
    }
}

#[cfg(all(feature = "interface", not(feature = "operations")))]
mod interface {
    use super::common;
    use std::env;
    use std::io::{Error, ErrorKind, Result};

    // Build script when the interface feature is on and not the operations one
    pub fn script_interface() -> Result<()> {
        if let Ok(include_dir) = env::var("MBEDTLS_INCLUDE_DIR") {
            common::configure_mbed_crypto()?;
            common::generate_mbed_crypto_bindings(include_dir.clone())?;
            common::compile_shim_library(include_dir)
        } else {
            Err(Error::new(
                ErrorKind::Other,
                "interface feature necessitates MBEDTLS_INCLUDE_DIR environment variable",
            ))
        }
    }
}

#[cfg(feature = "operations")]
mod operations {
    use super::common;
    use cmake::Config;
    use std::env;
    use std::io::{Error, ErrorKind, Result};
    use std::path::PathBuf;
    use walkdir::WalkDir;

    fn compile_mbed_crypto() -> Result<PathBuf> {
        let mbedtls_dir = String::from("./vendor");
        let out_dir = env::var("OUT_DIR").unwrap();

        // Rerun build if any file under the vendor directory has changed.
        for entry in WalkDir::new(&mbedtls_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    println!("cargo:rerun-if-changed={}", entry.path().display());
                }
            }
        }

        // Build the MbedTLS libraries
        let mbed_build_path = Config::new(&mbedtls_dir)
            .cflag(format!("-I{}", out_dir))
            .cflag("-DMBEDTLS_CONFIG_FILE='<config.h>'")
            .cflag(format!("-I{}/src/c/mbedtls-config",
                           env::current_dir()?.to_str().unwrap()))
            .cflag("-DMBEDTLS_USER_CONFIG_FILE='<user_config.h>'")
            .define("ENABLE_PROGRAMS", "OFF")
            .define("ENABLE_TESTING", "OFF")
            .build();

        Ok(mbed_build_path)
    }

    fn link_to_lib(lib_path: String, link_statically: bool) {
        let link_type = if link_statically { "static" } else { "dylib" };

        // Request rustc to link the Mbed Crypto library
        println!("cargo:rustc-link-search=native={}", lib_path,);
        println!("cargo:rustc-link-lib={}=mbedcrypto", link_type);
    }

    // Build script when the operations feature is on
    pub fn script_operations() -> Result<()> {
        let lib;
        let statically;
        let include;

        if env::var("MBEDTLS_LIB_DIR").is_err() ^ env::var("MBEDTLS_INCLUDE_DIR").is_err() {
            return Err(Error::new(
                ErrorKind::Other,
                "both environment variables MBEDTLS_LIB_DIR and MBEDTLS_INCLUDE_DIR need to be set for operations feature",
            ));
        }

        common::configure_mbed_crypto()?;

        if let (Ok(lib_dir), Ok(include_dir)) =
            (env::var("MBEDTLS_LIB_DIR"), env::var("MBEDTLS_INCLUDE_DIR"))
        {
            lib = lib_dir;
            include = include_dir;
            statically = cfg!(feature = "static") || env::var("MBEDCRYPTO_STATIC").is_ok();
        } else {
            println!("Did not find environment variables, building MbedTLS!");
            let mut mbed_lib_dir = compile_mbed_crypto()?;
            let mut mbed_include_dir = mbed_lib_dir.clone();
            mbed_lib_dir.push("lib");
            mbed_include_dir.push("include");

            lib = mbed_lib_dir.to_str().unwrap().to_owned();
            include = mbed_include_dir.to_str().unwrap().to_owned();
            statically = true;
        }

        // Linking to PSA Crypto library is only needed for the operations.
        link_to_lib(lib, statically);
        common::generate_mbed_crypto_bindings(include.clone())?;
        common::compile_shim_library(include)
    }
}
