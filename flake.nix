{
  description = "pleme-io/helmworks-render-check — pull a helmworks chart, render with values, assert contract";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    crate2nix = {
      url = "github:nix-community/crate2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    substrate = {
      url = "github:pleme-io/substrate";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ { self, nixpkgs, crate2nix, flake-utils, substrate, ... }:
    (import "${substrate}/lib/rust-action-release-flake.nix" {
      inherit nixpkgs crate2nix flake-utils;
    }) {
      toolName = "helmworks-render-check";
      src = self;
      repo = "pleme-io/helmworks-render-check";
      action = {
        description = "Pull a published helmworks chart from oci://ghcr.io/pleme-io/charts at a given version, render it with operator-supplied values, and fail on contract violations (pause, attestation, schema). Pre-merge gate for any helmworks consumer.";
        inputs = [
          { name = "chart"; description = "Chart name (e.g. pleme-arc-runner-pool)"; required = true; }
          { name = "version"; description = "Chart version constraint (e.g. ~> 0.2.0 or 0.2.1)"; required = true; }
          { name = "values-file"; description = "Path to the values.yaml file (relative to repo root)"; required = true; }
          { name = "expected-resources"; description = "JSON array of Kind/name strings the rendered output must contain"; default = "[]"; }
          { name = "expected-violations"; description = "JSON array of contract names that the chart MUST fail with for this values file (negative tests)"; default = "[]"; }
        ];
        outputs = [
          { name = "rendered-yaml-path"; description = "Path to the rendered manifests file"; }
          { name = "resource-count"; description = "Number of K8s resources rendered"; }
          { name = "contract-pass"; description = "true iff all expected resources are present and no unexpected violations fired"; }
        ];
      };
    };
}
