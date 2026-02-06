{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc
    cargo
    rustfmt
    rust-analyzer
    clippy
    
    # CSS tools
    dart-sass
    
    # WASM tools
    wasm-pack
    trunk
    
    # System deps
    lld
    clang
    pkg-config
    openssl
  ];

  shellHook = ''
    echo "Rust + WASM Environment Loaded"
    echo "Run 'trunk serve' to start data server"
  '';
}
