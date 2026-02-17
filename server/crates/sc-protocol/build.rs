use std::io::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    // Locate the proto file. Normally it's at the repo root: ../../../proto/
    // relative to this crate (server/crates/sc-protocol/).
    // When cross-compiling with `cross`, only the cargo workspace root (server/)
    // is mounted, so the repo-root path escapes the mount. In that case we fall
    // back to a copy placed at server/proto/ by the CI workflow.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../../proto/messages.proto"), // native / repo-root
        manifest_dir.join("../../proto/messages.proto"),    // server/proto/ (cross)
    ];

    let proto_file = candidates.iter().find(|p| p.exists()).unwrap_or_else(|| {
        panic!(
            "Could not find messages.proto. Searched:\n{}",
            candidates
                .iter()
                .map(|p| format!("  - {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        )
    });

    let proto_dir = proto_file.parent().unwrap();
    println!("cargo:rerun-if-changed={}", proto_file.display());
    prost_build::compile_protos(&[proto_file.as_path()], &[proto_dir])?;
    Ok(())
}
