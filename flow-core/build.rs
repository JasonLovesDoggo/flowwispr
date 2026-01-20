use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let package_name = env::var("CARGO_PKG_NAME").unwrap();

    // Output to Sources/CFlow/include/flow.h (where Swift build uses it)
    let output_file = PathBuf::from(&crate_dir)
        .parent()
        .expect("Failed to get parent directory")
        .join("Sources")
        .join("CFlow")
        .join("include")
        .join(format!("{}.h", package_name));

    // Ensure include directory exists
    std::fs::create_dir_all(output_file.parent().unwrap())
        .expect("Failed to create include directory");

    // Run cbindgen CLI
    let status = Command::new("cbindgen")
        .arg("--crate")
        .arg(&package_name)
        .arg("--config")
        .arg(PathBuf::from(&crate_dir).join("cbindgen.toml"))
        .arg("--output")
        .arg(&output_file)
        .current_dir(&crate_dir)
        .status()
        .expect("Failed to run cbindgen");

    if !status.success() {
        panic!("cbindgen generation failed");
    }

    println!("cargo:warning=Generated C header: {}", output_file.display());
}
