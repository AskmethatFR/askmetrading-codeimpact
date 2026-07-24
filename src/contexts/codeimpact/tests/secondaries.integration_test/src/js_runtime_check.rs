/// Node/jsdom runtime-security-check harness (#28, ADR-8.10) — runs the REAL
/// emitted report JS through a genuine HTML parser + DOM implementation
/// (`runtime-security-check/check.mjs`), which the static gates
/// (`rendering_gate`) structurally cannot do: they can prove a banned sink
/// is absent from the source text, never that a payload landing as
/// `textContent` truly never becomes markup under a spec-conformant parser.
///
/// Node.js is a genuinely optional dev-time vehicle — a bare Rust toolchain
/// with no Node.js installed must not see a red suite for a security
/// regression test it cannot run. Per the #45 lesson (a guard that silently
/// no-ops is worthless: the ONLY guard against an indefinite hang used to
/// skip silently whenever `python3` was absent), the absence check here
/// prints a banner loud enough that `cargo test` shows it even without
/// `--nocapture` — every skip path returns from inside a `#[test]` fn while
/// EXPLICITLY FAILING the test (`panic!`) rather than returning quietly, so
/// a Node-less CI runner gets a visible red with a clear reason instead of a
/// silent green.
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::support::workspace_root;

pub fn runtime_check_dir() -> PathBuf {
    workspace_root()
        .join("src/contexts/codeimpact/tests/secondaries.integration_test/runtime-security-check")
}

/// jsdom 29's actual `engines` floor — checked directly against the
/// installed `node_modules/jsdom/package.json` — is the odd union
/// `^20.19.0 || ^22.13.0 || >=24.0.0`. An old-but-present Node (verified:
/// Node 20.10.0 on PATH crashes deep inside a transitive ESM-only
/// dependency with a cryptic `ERR_REQUIRE_ESM`, not a clean message) is a
/// worse failure mode than "not found" — the loud-skip banner needs to name
/// this case specifically, not just "Node.js not found".
enum NodeStatus {
    Missing,
    TooOld(String),
    Ready,
}

/// Approximates jsdom's engines range as "major > 20, or major == 20 and
/// minor >= 19" — a precise semver-range parser is more machinery than this
/// diagnostic warrants (YAGNI), and this is intentionally permissive on the
/// rare non-LTS odd major (21.x, 23.x) nobody runs in CI anyway while being
/// precise on the two floors that matter in practice: an old Node 18/20.x
/// machine, and the common >=22/24 case.
fn node_version_supported(version: &str) -> Option<bool> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    Some(major > 20 || (major == 20 && minor >= 19))
}

fn detect_node() -> NodeStatus {
    let output = match Command::new("node").arg("--version").output() {
        Ok(o) if o.status.success() => o,
        _ => return NodeStatus::Missing,
    };
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    match node_version_supported(&version) {
        Some(true) => NodeStatus::Ready,
        _ => NodeStatus::TooOld(version),
    }
}

/// Call at the top of every test that needs Node.js. Panics with a loud,
/// unmistakable banner (visible in default `cargo test` failure output, no
/// `--nocapture` needed) rather than returning quietly — see the module
/// doc's #45 rationale.
pub fn require_node_or_fail_loudly(test_name: &str) {
    match detect_node() {
        NodeStatus::Ready => {}
        NodeStatus::Missing => panic!(
            "\n\n\
             ================================================================\n\
             SKIPPED-LOUDLY: {test_name}\n\
             Node.js was not found on PATH.\n\
             This test executes the REAL emitted report JS against jsdom to\n\
             assert runtime security invariants (no code execution, no markup\n\
             parsing, Object.prototype hygiene, numeric clamping) that a\n\
             static Rust gate cannot see (ADR-8.10, #28).\n\
             Install Node.js (^20.19.0 || ^22.13.0 || >=24.0.0) and re-run.\n\
             ================================================================\n\n"
        ),
        NodeStatus::TooOld(version) => panic!(
            "\n\n\
             ================================================================\n\
             SKIPPED-LOUDLY: {test_name}\n\
             Found Node.js {version}, but jsdom (the runtime-security-check\n\
             harness's own dependency) requires ^20.19.0 || ^22.13.0 || >=24.0.0.\n\
             An older Node crashes deep inside a transitive ESM-only\n\
             dependency with a cryptic ERR_REQUIRE_ESM — this check fails\n\
             fast instead, before that happens.\n\
             Upgrade Node.js and re-run.\n\
             ================================================================\n\n"
        ),
    }
}

/// UNCONDITIONALLY runs `npm ci` (mirrors `support::ensure_bin_built`'s no
/// exists-check rule, #63) so a stale `node_modules` from a different jsdom
/// version can never validate silently.
pub fn ensure_npm_install() {
    let dir = runtime_check_dir();
    let status = Command::new("npm")
        .args(["ci", "--no-audit", "--no-fund"])
        .current_dir(&dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn npm ci in {:?}: {}", dir, e));
    assert!(status.success(), "npm ci failed in {:?}", dir);
}

/// Runs `check.mjs` against `html_path`, returns its parsed stdout JSON.
/// `payload_probe`, if set, is passed through `CODEIMPACT_PAYLOAD_PROBE` so
/// the script can report whether that literal text reached `document.body`.
pub fn run_check(html_path: &Path, payload_probe: Option<&str>) -> serde_json::Value {
    let dir = runtime_check_dir();
    let mut cmd = Command::new("node");
    cmd.arg("check.mjs").arg(html_path).current_dir(&dir);
    if let Some(probe) = payload_probe {
        cmd.env("CODEIMPACT_PAYLOAD_PROBE", probe);
    }
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn node check.mjs: {}", e));
    assert!(
        output.status.success(),
        "check.mjs exited non-zero: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "check.mjs did not print valid JSON: {} (stdout={})",
            e,
            String::from_utf8_lossy(&output.stdout)
        )
    })
}
