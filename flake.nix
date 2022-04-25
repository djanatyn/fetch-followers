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
        cargoSha256 = "sha256-vw4YeBub9DwXRN0YEfIelzc2NEC+nr3cZano+7guzj0=";
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
