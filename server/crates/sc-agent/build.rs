fn main() {
    // On macOS, the `screencapturekit` crate depends on Swift's concurrency
    // runtime (libswift_Concurrency.dylib). We need to tell the linker where
    // to find Swift libraries at runtime via rpath.
    #[cfg(target_os = "macos")]
    {
        // System Swift runtime (always present on macOS 12.3+)
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

        // Xcode / Command Line Tools Swift runtime (fallback)
        if std::path::Path::new("/Library/Developer/CommandLineTools/usr/lib/swift/macosx").exists()
        {
            println!("cargo:rustc-link-arg=-Wl,-rpath,/Library/Developer/CommandLineTools/usr/lib/swift/macosx");
        }

        // If Xcode.app is installed
        let xcode_swift = "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx";
        if std::path::Path::new(xcode_swift).exists() {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", xcode_swift);
        }
    }
}
