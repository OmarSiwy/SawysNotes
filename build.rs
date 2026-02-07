fn main() {
    println!("cargo:rerun-if-changed=templates/layout.html");
    println!("cargo:rerun-if-changed=templates/sidebar.html");
    println!("cargo:rerun-if-changed=style/main.scss");

    // Compile Sass if available
    if let Ok(status) = std::process::Command::new("sass")
        .args(["style/main.scss", "style/main.css"])
        .status()
    {
        if !status.success() {
            println!("cargo:warning=Sass compilation failed.");
        }
    }

    // NOTE: WASM is built separately via `wasm-pack build` to avoid Cargo.lock deadlock.
    // Do NOT call wasm-pack from build.rs - it causes a deadlock on the lock file.
}
