use std::io::Result;

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=../../../proto/messages.proto");
    prost_build::compile_protos(&["../../../proto/messages.proto"], &["../../../proto/"])?;
    Ok(())
}
