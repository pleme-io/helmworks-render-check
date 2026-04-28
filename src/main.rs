//! `pleme-io/helmworks-render-check` — pull a helmworks chart, render with
//! operator values, assert expected resources + contract violations.
//!
//! Pre-merge gate for any helmworks consumer. Catches misconfiguration
//! (typed pause contract violations, missing required values, schema
//! mismatches) before the chart reaches a live cluster.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use pleme_actions_shared::{ActionError, Input, Output, StepSummary};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Inputs {
    chart: String,
    version: String,
    values_file: String,
    #[serde(default)]
    expected_resources: serde_json::Value,
    #[serde(default)]
    expected_violations: serde_json::Value,
}

fn main() {
    pleme_actions_shared::log::init();
    if let Err(e) = run() {
        e.emit_to_stdout();
        if e.is_fatal() {
            std::process::exit(1);
        }
    }
}

fn run() -> Result<(), ActionError> {
    let inputs = Input::<Inputs>::from_env()?;
    let expected_resources = parse_string_array(&inputs.expected_resources, "expected-resources")?;
    let expected_violations = parse_string_array(&inputs.expected_violations, "expected-violations")?;

    let render_output = helm_template(&inputs.chart, &inputs.version, &inputs.values_file);

    let (rendered, violations_emitted, pass) = match render_output {
        Ok(stdout) => {
            let resources = enumerate_resources(&stdout);
            let missing: Vec<&String> = expected_resources
                .iter()
                .filter(|r| !resources.contains(r))
                .collect();
            let pass_resources = missing.is_empty() && expected_violations.is_empty();
            (stdout, vec![], pass_resources)
        }
        Err((stderr, _)) => {
            // The chart rendered with errors — could be a contract violation we expected.
            let violations = parse_violations_from_stderr(&stderr);
            let pass = expected_violations.iter().all(|v| violations.contains(v))
                && violations.iter().all(|v| expected_violations.contains(v));
            (String::new(), violations, pass)
        }
    };

    let temp = std::env::temp_dir().join(format!("helmworks-render-{}.yaml", std::process::id()));
    if !rendered.is_empty() {
        std::fs::write(&temp, &rendered)
            .map_err(|e| ActionError::error(format!("failed to write rendered output: {e}")))?;
    }

    let resource_count = enumerate_resources(&rendered).len();

    let output = Output::from_runner_env()?;
    output.set("rendered-yaml-path", temp.to_string_lossy())?;
    output.set("resource-count", resource_count.to_string())?;
    output.set("contract-pass", pass.to_string())?;

    let mut summary = StepSummary::from_runner_env()?;
    summary
        .heading(2, &format!("helm render check — {} {}", inputs.chart, inputs.version))
        .paragraph(&format!(
            "values-file: `{}`",
            inputs.values_file
        ))
        .table(
            &["Metric", "Value"],
            vec![
                vec!["resource count".to_string(), resource_count.to_string()],
                vec!["contract pass".to_string(), pass.to_string()],
                vec!["expected resources".to_string(), format!("{} declared", expected_resources.len())],
                vec!["expected violations".to_string(), format!("{} declared", expected_violations.len())],
                vec!["actual violations".to_string(), format!("{}", violations_emitted.join(", "))],
            ],
        );
    summary.commit()?;

    if !pass {
        return Err(ActionError::error(format!(
            "helm render contract check failed: expected_resources={:?} expected_violations={:?} \
             actual_violations={:?}",
            expected_resources, expected_violations, violations_emitted
        )));
    }

    Ok(())
}

fn parse_string_array(
    value: &serde_json::Value,
    field_name: &str,
) -> Result<Vec<String>, ActionError> {
    if value.is_null() {
        return Ok(vec![]);
    }
    let arr = value.as_array().ok_or_else(|| {
        ActionError::error(format!("input `{field_name}` must be a JSON array of strings"))
    })?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let s = item.as_str().ok_or_else(|| {
            ActionError::error(format!(
                "input `{field_name}` array items must be strings (got {item:?})"
            ))
        })?;
        out.push(s.to_string());
    }
    Ok(out)
}

fn helm_template(
    chart: &str,
    version: &str,
    values_file: &str,
) -> Result<String, (String, String)> {
    // Use registry-aware OCI helm pull semantics — caller supplies the chart by
    // name (e.g. `pleme-arc-runner-pool`) + version constraint.
    let chart_ref = format!("oci://ghcr.io/pleme-io/charts/{chart}");
    let output = Command::new("helm")
        .args([
            "template",
            "render-check",
            &chart_ref,
            "--version",
            version,
            "-f",
            values_file,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| (format!("failed to spawn helm: {e}"), String::new()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if output.status.success() {
        Ok(stdout)
    } else {
        Err((stderr, stdout))
    }
}

/// Walk the multi-doc YAML output, return Vec of `Kind/name` strings.
fn enumerate_resources(rendered: &str) -> Vec<String> {
    let mut out = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(rendered) {
        let value = match serde_yaml::Value::deserialize(doc) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let Some(kind) = value.get("kind").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(name) = value
            .get("metadata")
            .and_then(|m| m.get("name"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        out.push(format!("{kind}/{name}"));
    }
    out
}

/// Heuristic: extract chart-side contract violation names from helm's stderr.
/// pleme-lib chart errors typically include `validatePause:` /
/// `validateAttestation:` / etc. as the helper name.
fn parse_violations_from_stderr(stderr: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for line in stderr.lines() {
        // Match `pleme-arc-runner-pool: paused=true requires …` style errors
        if let Some(start) = line.find("validatePause") {
            let _ = start;
            violations.push("validatePause".into());
        }
        if line.contains("validateAttestation") {
            violations.push("validateAttestation".into());
        }
        // Generic chart fail
        if line.contains("Error: execution error") {
            // Extract template name when present
            if let Some(idx) = line.find("templates/") {
                if let Some(end) = line[idx + 10..].find(':') {
                    let template = &line[idx + 10..idx + 10 + end];
                    violations.push(template.to_string());
                }
            }
        }
    }
    violations.sort();
    violations.dedup();
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_string_array_null_returns_empty() {
        let v = parse_string_array(&serde_json::Value::Null, "x").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn parse_string_array_happy_path() {
        let v = serde_json::json!(["a", "b", "c"]);
        assert_eq!(parse_string_array(&v, "x").unwrap(), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_string_array_rejects_non_array() {
        let v = serde_json::json!({"not": "array"});
        let err = parse_string_array(&v, "expected-resources").unwrap_err();
        assert!(err.as_workflow_command().contains("must be a JSON array"));
    }

    #[test]
    fn parse_string_array_rejects_non_string_items() {
        let v = serde_json::json!(["ok", 42]);
        let err = parse_string_array(&v, "expected-resources").unwrap_err();
        assert!(err.as_workflow_command().contains("array items must be strings"));
    }

    #[test]
    fn enumerate_resources_extracts_kind_name() {
        let yaml = "\
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: my-config
data:
  foo: bar
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-deploy
";
        let mut got = enumerate_resources(yaml);
        got.sort();
        assert_eq!(got, vec!["ConfigMap/my-config", "Deployment/my-deploy"]);
    }

    #[test]
    fn enumerate_resources_skips_documents_missing_kind_or_name() {
        let yaml = "\
---
apiVersion: v1
kind: ConfigMap
metadata: {}
---
apiVersion: v1
metadata:
  name: orphan
---
apiVersion: v1
kind: Pod
metadata:
  name: pod-x
";
        assert_eq!(enumerate_resources(yaml), vec!["Pod/pod-x"]);
    }

    #[test]
    fn parse_violations_recognizes_validate_pause() {
        let stderr = "\
Error: execution error at (pleme-arc-runner-pool/templates/pause-state.yaml:11:4): \
pleme-arc-runner-pool: paused=true requires gha-runner-scale-set.minRunners=0 \
AND gha-runner-scale-set.maxRunners=0 (got minRunners=0 maxRunners=5). … \
The validatePause helper enforces this.
";
        let violations = parse_violations_from_stderr(stderr);
        assert!(violations.contains(&"validatePause".to_string()));
    }
}
