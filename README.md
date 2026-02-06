# UWASIC Documentation

A WASM-based documentation platform built with Rust, Axum, and wasm-bindgen.

## Features
- **WASM-powered**: Interactive features running in the browser
- **Markdown Support**: Write content in standard Markdown with MathJax support
- **Dark/Light Theme**: Toggle between themes with persistent preference
- **Responsive Sidebar**: Collapsible navigation with scroll-spy

## Setup
1. Enter the Nix shell:
    ```sh
    nix-shell
    ```
2. Run the development server:
    ```sh
    cargo run
    ```
    The server will start at http://127.0.0.1:3000
