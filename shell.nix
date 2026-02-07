{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = with pkgs; [
    rustc cargo
    dart-sass       # For SCSS compilation
    wasm-pack       # For WASM build (run separately)
    pkg-config openssl  # Required by some crates
  ];

  shellHook = ''
    echo "Dev shell ready. Run 'cargo run' for server, 'wasm-pack build --target web' for WASM."
  '';
}
