{
  description = "A devShell example";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };
  #
  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in {
        devShells.default = with pkgs;
          mkShell {
            buildInputs = [
              openssl
              pkg-config
              cargo-binutils
              cargo-make
              probe-rs
              minicom
              (pkgs.rust-bin.selectLatestNightlyWith (toolchain:
                toolchain.default.override {
                  extensions = ["rust-src" "rust-analyzer" "llvm-tools"];
                  targets = ["thumbv7em-none-eabihf"];
                }))
            ];
            NUM_KEYS = 36;
            NUM_CONFIGS = 3;
            NUM_LAYERS = 6;
            IS_SPLIT = 1;
          };
      }
    );
}
