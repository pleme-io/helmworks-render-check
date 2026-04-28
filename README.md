# pleme-io/helmworks-render-check

Pre-merge gate for any helmworks consumer. Pulls a chart from `oci://ghcr.io/pleme-io/charts`, renders it with operator-supplied values, asserts expected resources + contract violations.

## Usage

```yaml
- uses: pleme-io/helmworks-render-check@v1
  with:
    chart: pleme-arc-runner-pool
    version: "0.2.0"
    values-file: clusters/rio/.../runner-pool-values.yaml
    expected-resources: |
      ["ServiceAccount/arc-rio-default-sa", "ConfigMap/arc-rio-default-pause-state"]
```

## Inputs

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `chart` | string | yes | — | Chart name |
| `version` | string | yes | — | Version constraint |
| `values-file` | string | yes | — | Path to values.yaml |
| `expected-resources` | json | no | `[]` | Array of `Kind/name` strings the render must contain |
| `expected-violations` | json | no | `[]` | Array of contract names this values file MUST trigger |

## Outputs

| Name | Type | Description |
|---|---|---|
| `rendered-yaml-path` | string | Path to rendered manifests |
| `resource-count` | number | K8s resources rendered |
| `contract-pass` | bool | true iff all expectations met |

## v1 stability guarantees

Inputs guaranteed within `v1`: `chart`, `version`, `values-file`.
Outputs guaranteed within `v1`: `rendered-yaml-path`, `resource-count`, `contract-pass`.

## Part of the pleme-io action library

This action is one of 11 in [`pleme-io/pleme-actions`](https://github.com/pleme-io/pleme-actions) — discovery hub, version compat matrix, contributing guide, and reusable SDLC workflows shared across the library.
