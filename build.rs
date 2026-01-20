//! Build script for whatsapp-translator.
//!
//! This script optionally compiles the Go wa-bridge binary during `cargo build`.
//! If Go is not installed or the build fails, it will print a warning but not fail the build.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Only rebuild if Go source files change
    println!("cargo:rerun-if-changed=wa-bridge/main.go");
    println!("cargo:rerun-if-changed=wa-bridge/client.go");
    println!("cargo:rerun-if-changed=wa-bridge/protocol.go");
    println!("cargo:rerun-if-changed=wa-bridge/go.mod");
    println!("cargo:rerun-if-changed=wa-bridge/go.sum");

    // Skip Go build if explicitly disabled
    if env::var("SKIP_GO_BUILD").is_ok() {
        println!("cargo:warning=Skipping Go bridge build (SKIP_GO_BUILD is set)");
        return;
    }

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let wa_bridge_dir = PathBuf::from(&manifest_dir).join("wa-bridge");
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = PathBuf::from(&out_dir)
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();

    // Determine output binary name based on target OS
    let binary_name = if cfg!(target_os = "windows") {
        "wa-bridge.exe"
    } else {
        "wa-bridge"
    };

    let output_path = target_dir.join(binary_name);

    // Check if Go is available
    let go_check = Command::new("go").arg("version").output();

    if go_check.is_err() {
        println!("cargo:warning=Go is not installed. Please build wa-bridge manually:");
        println!("cargo:warning=  cd wa-bridge && go build -o wa-bridge .");
        return;
    }

    // Build the Go binary
    println!("cargo:warning=Building wa-bridge...");

    let status = Command::new("go")
        .args(["build", "-o", output_path.to_str().unwrap(), "."])
        .current_dir(&wa_bridge_dir)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!(
                "cargo:warning=wa-bridge built successfully at {:?}",
                output_path
            );
        }
        Ok(s) => {
            println!(
                "cargo:warning=Failed to build wa-bridge (exit code: {:?})",
                s.code()
            );
            println!("cargo:warning=Please build wa-bridge manually:");
            println!("cargo:warning=  cd wa-bridge && go build -o wa-bridge .");
        }
        Err(e) => {
            println!("cargo:warning=Failed to run go build: {}", e);
            println!("cargo:warning=Please build wa-bridge manually:");
            println!("cargo:warning=  cd wa-bridge && go build -o wa-bridge .");
        }
    }
}
