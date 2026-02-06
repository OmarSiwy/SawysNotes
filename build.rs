use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=templates/layout.html");
    println!("cargo:rerun-if-changed=templates/sidebar.html");
    println!("cargo:rerun-if-changed=style/main.scss");

    // Prevent recursion: Don't run wasm-pack if we are already building for WASM
    if let Ok(target) = std::env::var("TARGET") {
        if target.contains("wasm32") {
            return;
        }
    }

    // Allow skipping WASM build explicitly (e.g. in CI where it's built separately)
    if std::env::var("SKIP_WASM_BUILD").is_ok() {
        println!("cargo:warning=Skipping WASM build via SKIP_WASM_BUILD env var");
        return;
    }

    // Check if wasm-pack is installed
    let status = Command::new("wasm-pack")
        .arg("--version")
        .status();

    if let Ok(s) = status {
        if s.success() {
             println!("cargo:warning=Building WASM...");
             let build_status = Command::new("wasm-pack")
                .args(&["build", "--target", "web", "--no-typescript", "--release"])
                // .current_dir(".") // Default
                .status()
                .expect("Failed to run wasm-pack");
            
            if build_status.success() {
                // Move pkg to dist
                let _ = Command::new("rm")
                    .args(&["-rf", "dist"])
                    .status();
                let _ = Command::new("mv")
                    .args(&["pkg", "dist"])
                    .status();
            } else {
                 println!("cargo:warning=WASM build failed!");
            }
        } else {
             println!("cargo:warning=wasm-pack not found or failed.");
        }
    } else {
         println!("cargo:warning=wasm-pack command not found. Skipping WASM build.");
    }

    // Compile Sass
    let sass_status = Command::new("sass")
        .args(&["style/main.scss", "style/main.css"])
        .status();
        
    if let Ok(s) = sass_status {
        if !s.success() {
             println!("cargo:warning=Sass compilation failed.");
        }
    }
}
