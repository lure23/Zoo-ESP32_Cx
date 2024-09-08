/*
* build.rs
*
* Gets run by:
*   - IDE on host; WRONG FEATURES!!
*   - 'cargo build' (CLI); correct features
*/

use itertools::Itertools;
use esp_build::assert_unique_used_features;
#[allow(unused_imports)]
use std::{
    env,
    fs::{self, OpenOptions, File},
    fmt::format,
    process::exit as _exit
};

const FN: &str = "tmp/config.h";
const MAKEFILE_INNER: &str = "Makefile.inner";

/*
* Note: 'build.rs' is supposedly run only once, for any 'examples', 'lib' etc. build.
*
* References:
*   - Environment variables set
*       -> https://doc.rust-lang.org/cargo/reference/environment-variables.html#environment-variables-cargo-sets-for-build-scripts
*/
fn main() {
    // Detect when IDE is running us:
    //  - Rust Rover:
    //      __CFBundleIdentifier=com.jetbrains.rustrover-EAP
    //
    #[allow(non_snake_case)]
    let IDE_RUN = std::env::var("__CFBundleIdentifier").is_ok();

    // If IDE runs, terminate early.
    if IDE_RUN { return };

    /***
    // DEBUG: Show what we know about the compilation.
    //
    // Potentially useful env.vars.
    //   CARGO_CFG_TARGET_FEATURE=c,m
    //   CARGO_FEATURE_{..feature..}=1
    //   LD_LIBRARY_PATH=/home/ubuntu/VL53L5CX_rs.cifs/vl53l5cx_uld/target/release/deps:/home/ubuntu/VL53L5CX_rs.cifs/vl53l5cx_uld/target/release:/home/ubuntu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/lib:/home/ubuntu/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/lib
    //   RUSTUP_TOOLCHAIN=stable-x86_64-unknown-linux-gnu
    //   TARGET=riscv32imc-unknown-none-elf
    //
    {
        env::vars().for_each(|(a, b)| {
            eprintln!("{a}={b}");
        });
        _exit(1);
    }
    ***/

    //---
    // Config sanity checks
    {
        // Pick 1
        assert_unique_used_features!(
            "targets_per_zone_1",
            "targets_per_zone_2",
            "targets_per_zone_3",
            "targets_per_zone_4",
        );

        // "range_sigma_mm" relates to "distance_mm"
        #[cfg(all(feature = "range_sigma_mm", not(feature = "distance_mm")))]
        println!("cargo:warning=Feature 'range_sigma_mm' does not make sense without feature 'distance_mm' (which is not enabled)");
    }

    //---
    // Create a C config header, based on the features from 'Cargo.toml'.
    //
    // Note: Since the IDE runs don't really (in Sep'24, Rust Rover, yet?) have a clue on the right
    //      set of features, update the 'config.h' *only* on actual builds.
    //
    {
        let mut defs: Vec<&str> = vec!();

        // Output-enabling features (in Rust, we have them enabling; in C they are disable flags). Same thing.
        #[cfg(not(feature = "target_status"))]
        defs.push("VL53L5CX_DISABLE_TARGET_STATUS");
        #[cfg(not(feature = "nb_targets_detected"))]
        defs.push("VL53L5CX_DISABLE_NB_TARGET_DETECTED");

        #[cfg(not(feature = "ambient_per_spad"))]
        defs.push("VL53L5CX_DISABLE_AMBIENT_PER_SPAD");
        #[cfg(not(feature = "nb_spads_enabled"))]
        defs.push("VL53L5CX_DISABLE_NB_SPADS_ENABLED");
        #[cfg(not(feature = "signal_per_spad"))]
        defs.push("VL53L5CX_DISABLE_SIGNAL_PER_SPAD");
        #[cfg(not(feature = "range_sigma_mm"))]
        defs.push("VL53L5CX_DISABLE_RANGE_SIGMA_MM");
        #[cfg(not(feature = "distance_mm"))]
        defs.push("VL53L5CX_DISABLE_DISTANCE_MM");
        #[cfg(not(feature = "reflectance_percent"))]
        defs.push("VL53L5CX_DISABLE_REFLECTANCE_PERCENT");

        // 'motion_indicator' feature & support is not implemented; always disable in C
        // #[cfg(not(feature = "motion_indicator"))]
        defs.push("VL53L5CX_DISABLE_MOTION_INDICATOR");

        // Vendor docs:
        //      "the number of target[s] per zone sent through I2C. [...] a lower number [...] means
        //      a lower RAM [consumption]."
        //
        #[cfg(feature = "targets_per_zone_1")]
        defs.push("VL53L5CX_NB_TARGET_PER_ZONE 1U");
        #[cfg(feature = "targets_per_zone_2")]
        defs.push("VL53L5CX_NB_TARGET_PER_ZONE 2U");
        #[cfg(feature = "targets_per_zone_3")]
        defs.push("VL53L5CX_NB_TARGET_PER_ZONE 3U");
        #[cfg(feature = "targets_per_zone_4")]
        defs.push("VL53L5CX_NB_TARGET_PER_ZONE 4U");

        // Write the file. This way the last 'cargo build' state remains available, even if
        // 'make' were run manually (compared to passing individual defines to 'make');
        // also, it keeps the 'Makefile' simple.
        //
        let contents = defs.iter()
            .map(|s| format!("#define {s}"))
            .join("\n");

        fs::write(FN, contents)
            .expect("Unable to write a file");  // note: cannot pass 'FN' here; tbd.
    }

    // make stuff
    //
    let st = std::process::Command::new("make")
        .arg("-f").arg(MAKEFILE_INNER)
        .arg("tmp/libvendor_uld.a")    // ULD C library
        .arg("src/uld_raw.rs")      // generate the ULD Rust bindings
        .output()
        .expect("could not spawn `make`")   // shown if 'make' not found on PATH
        .status;

    assert!(st.success(), "[ERROR]: Running 'make' failed");    // shown if 'make' returns a non-zero

    // Link arguments
    //
    // Note: Is it okay to do this in a lib crate?  We want it to affect at least the 'examples'.
    {
        #[allow(unused_mut)]
        let mut link_args: Vec<&str> = vec!(    // 'mut' in case we wish to conditionally add stuff
            "-Tlinkall.x",
            "-Tdefmt.x"     // required by 'defmt'
        );

        link_args.iter().for_each(|s| {
            println!("cargo::rustc-link-arg={}", s);
        });
    }

    println!("cargo:rustc-link-search=tmp");
    println!("cargo:rustc-link-lib=static=vendor_uld");

    // Allow using '#[cfg(disabled)]' for block-disabling code
    println!("cargo::rustc-check-cfg=cfg(disabled)");
}
