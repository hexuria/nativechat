fn main() {
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=AppKit"); // Keep AppKit linkage for other parts if needed
    println!("cargo:rustc-link-lib=framework=LocalAuthentication"); // Touch ID for saved logins
}
