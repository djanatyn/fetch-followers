{
  description = "flake for fetch-followers";

  inputs = { rust-overlay.url = "github:oxalica/rust-overlay"; };

  outputs = { self, nixpkgs, rust-overlay }: {
    packages.x86_64-linux.fetch-followers =
      let pkgs = import nixpkgs { overlays = [ rust-overlay.overlay ]; };

      in pkgs.rustPlatform.buildRustPackage rec {
        pname = "fetch-followers";
        version = "1.0";
        buildInputs = with pkgs; [ openssl sqlite ];
        nativeBuildInputs = with pkgs; [
          pkg-config
          latest.rustChannels.nightly.rust
        ];

        cargoSha256 = "sha256-EByARHVCJR8H8gtVHtNsRpcT1AyEf37KfKyGmYpUSgM=";

        src = ./.;
      };
  };
}
