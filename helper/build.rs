fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // HIServices is a subframework inside the ApplicationServices umbrella;
        // it is not discoverable by bare name on the default framework search
        // path, and the macOS 15 SDK umbrella stubs stopped re-exporting its
        // data symbols (the kAX* attribute constants) in release links.
        println!(
            "cargo:rustc-link-search=framework=/System/Library/Frameworks/ApplicationServices.framework/Frameworks"
        );
    }
}
