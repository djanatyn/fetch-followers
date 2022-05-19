{
  description = "flake for fetch-followers";

  inputs = { mozilla.url = "github:mozilla/nixpkgs-mozilla"; };

  outputs = { self, nixpkgs, mozilla }: {
    packages.x86_64-linux.fetch-followers = let
      pkgs = import nixpkgs { overlays = [ mozilla.overlays.rust ]; };

      nightly = (pkgs.rustChannelOf {
        date = "2022-04-12";
        channel = "nightly";
      }).rust;

      platform = pkgs.makeRustPlatform {
        cargo = nightly;
        rustc = nightly;
      };
    in platform.buildRustPackage rec {
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
