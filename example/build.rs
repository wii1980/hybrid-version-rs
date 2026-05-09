use hybrid_version::error::VResult;
use hybrid_version::version::Version;
use std::path::PathBuf;

fn main() -> VResult<()> {
    println!("cargo:rerun-if-changed=Buildlog.txt");
    println!("cargo:rerun-if-changed=Cargo.toml");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let out_dir = std::env::var("OUT_DIR").unwrap_or_else(|_| ".".to_string());

    let version_rs = PathBuf::from(&out_dir).join("version.rs");
    let buildlog = PathBuf::from(&manifest_dir).join("Buildlog.txt");
    let build_id = std::env::var("BUILD_ID").ok();

    Version::new_for(&manifest_dir, build_id)?
        .modified_cannot_build_release()
        .write_version(version_rs)?
        .set_output_env("EXAMPLE_VERSION")?
        .write_buildlog(buildlog)?;
    Ok(())
}
