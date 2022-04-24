{
  description = "flake for fetch-followers";

  inputs = { mozilla.url = "github:mozilla/nixpkgs-mozilla"; };

  outputs = { self, nixpkgs, mozilla }: {
    packages.x86_64-linux.fetch-followers =
      let pkgs = import nixpkgs { overlays = [ mozilla.overlays.rust ]; };
      in pkgs.rustPlatform.buildRustPackage rec {
        pname = "fetch-followers";
        version = "1.0";
        src = ./.;
        cargoSha256 = "sha256-UJAqefyKg4kv2DZPHbqj94cmmcYoYn5Y18cBwgBDG1k=";
        cargoDepsName = pname;
        buildInputs = [ pkgs.openssl ];
        nativeBuildInputs = with pkgs; [
          protobuf
          pkg-config
          latest.rustChannels.nightly.rust
        ];
      };
  };
}
