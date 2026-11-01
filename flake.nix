{
  description = "Lead Build System: declarative build descriptions for Ninja";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      cargoToml = lib.importTOML ./Cargo.toml;
    in
    {
      packages = forAllSystems (pkgs: {
        lead-build = pkgs.callPackage (
          {
            lib,
            stdenv,
            rustPlatform,
            installShellFiles,
          }:
          rustPlatform.buildRustPackage {
            pname = cargoToml.package.name;
            version = cargoToml.package.version;

            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./build.rs
                ./src
                ./examples
                ./tests
              ];
            };

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [ installShellFiles ];

            # The mangen example writes the man pages; build it along with
            # the binaries so it needs no second compile.
            cargoBuildFlags = [
              "--bins"
              "--examples"
            ];

            postInstall = lib.optionalString (stdenv.buildPlatform.canExecute stdenv.hostPlatform) ''
              mangen=$(find target -path '*/release/examples/mangen' -type f | head -n 1)
              "$mangen" man
              installManPage man/*.1
            '';

            meta = {
              description = cargoToml.package.description;
              homepage = cargoToml.package.repository;
              license = lib.licenses.gpl2Only;
              mainProgram = "pb";
              platforms = lib.platforms.unix;
            };
          }
        ) { };
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.lead-build;
      });

      overlays.default = final: prev: {
        lead-build = self.packages.${final.stdenv.hostPlatform.system}.lead-build;
      };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.lead-build ];
          packages = [
            pkgs.clippy
            pkgs.rustfmt
            pkgs.rust-analyzer
            pkgs.ninja
          ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
