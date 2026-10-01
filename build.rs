fn main() {
    println!("cargo:rerun-if-changed=native/ios/AudioBridge.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        cc::Build::new()
            .file("native/ios/AudioBridge.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("musictalk_ios_audio");
        for framework in ["UIKit", "WebKit", "ReplayKit", "AVFoundation", "Security"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
