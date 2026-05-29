{
  description = "Sprocket — a command line tool for working with Workflow Description Language (WDL) documents";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    pre-commit-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      pre-commit-hooks,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        inherit (pkgs) lib;

        # Single source of truth for the package version: the root Cargo.toml.
        cargoToml = lib.importTOML ./Cargo.toml;

        nativeBuildInputs = with pkgs; [
          pkg-config
          clang
          cmake
        ];

        buildInputs =
          with pkgs;
          [
            openssl
            sqlite
            zlib
          ]
          ++ lib.optionals stdenv.isDarwin (
            with pkgs.darwin.apple_sdk.frameworks;
            [
              Security
              SystemConfiguration
              CoreServices
            ]
          );

        sprocket = pkgs.rustPlatform.buildRustPackage {
          pname = "sprocket";
          version = cargoToml.package.version;

          src = lib.cleanSourceWith {
            src = ./.;
            filter =
              path: type:
              let
                base = baseNameOf (toString path);
              in
              !(lib.hasPrefix "target" base || lib.hasPrefix "result" base || base == ".direnv");
          };

          # We use `cargoHash` (→ `fetchCargoVendor`) rather than `cargoLock`
          # (→ `importCargoLock`) because the latter pulls crates via
          # `fetchurl`/curl, which crates.io now 403s for missing User-Agent
          # headers. `fetchCargoVendor` runs Python+`requests` which sends a
          # proper UA. As a bonus, it handles git-sourced crates (the
          # thirtyfour fork) without per-crate hash entries.
          #
          # Replace fakeHash with the hash Nix prints on the first build.
          # Any change to Cargo.lock will require updating this hash.
          cargoHash = lib.fakeHash;

          inherit nativeBuildInputs buildInputs;

          # Link against system OpenSSL via pkg-config (no in-sandbox fetch).
          # libgit2 is intentionally left to vendor itself via cmake/cc — the
          # version is then guaranteed to match libgit2-sys's bindings.
          OPENSSL_NO_VENDOR = "1";

          # Only build the top-level `sprocket` binary; gauntlet and other helpers
          # are workspace members but not part of the installed package output.
          cargoBuildFlags = [
            "-p"
            "sprocket"
            "--bins"
          ];

          # The full cargo suite needs docker (engine tests), `npx` for pagefind,
          # and an external `shellcheck` PATH — none of which are available inside
          # the Nix build sandbox. We run a binary smoke test under `checks` instead.
          doCheck = false;

          meta = {
            description = cargoToml.package.description;
            homepage = "https://sprocket.bio";
            license = with lib.licenses; [
              mit
              asl20
            ];
            mainProgram = "sprocket";
            platforms = lib.platforms.unix;
          };
        };

        preCommitCheck = pre-commit-hooks.lib.${system}.run {
          src = ./.;
          hooks = {
            nixfmt-rfc-style = {
              enable = true;
              settings.width = 100;
            };
            statix.enable = true;
            deadnix.enable = true;
          };
          excludes = [
            "^Cargo\\.lock$"
            "^target/"
            "^crates/.*/tests/"
          ];
        };
      in
      {
        packages = {
          default = sprocket;
          sprocket = sprocket;
        };

        checks = {
          inherit sprocket;
          pre-commit-check = preCommitCheck;

          # Confirms the produced binary links and runs.
          sprocket-smoke =
            pkgs.runCommand "sprocket-smoke"
              {
                nativeBuildInputs = [ sprocket ];
              }
              ''
                sprocket --version > $out
              '';
        };

        devShells.default = pkgs.mkShell {
          inputsFrom = [ sprocket ];

          packages = with pkgs; [
            # Rust toolchain (matches the version nixpkgs ships, which on
            # nixos-unstable should satisfy the workspace MSRV of 1.91.1).
            rustc
            cargo
            clippy
            rustfmt
            rust-analyzer

            # Cargo tooling used by the upstream cargo CI.
            cargo-nextest
            cargo-llvm-cov
            cargo-deny
            cargo-sort
            cargo-msrv
            taplo

            # Runtime dependency: sprocket invokes `shellcheck` for WDL
            # command-section linting and several tests assume it on PATH.
            shellcheck

            # Nix tooling.
            nixfmt-rfc-style
            deadnix
            statix
          ];

          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          OPENSSL_NO_VENDOR = "1";
          LIBGIT2_NO_VENDOR = "1";

          inherit (preCommitCheck) shellHook;
        };

        formatter = pkgs.nixfmt-rfc-style;
      }
    );
}
