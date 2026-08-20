use std::{fs, process::Command};

use tempfile::tempdir;

#[test]
fn lists_and_verifies_the_embedded_signed_registry_without_activation() {
    let listed = pinset(&["provider", "list", "--json"]);
    assert!(listed.status.success(), "{}", stderr(&listed));
    let listed_json: serde_json::Value =
        serde_json::from_slice(&listed.stdout).expect("provider list JSON");
    assert_eq!(listed_json["schema"], 1);
    assert_eq!(listed_json["command"], "provider.list");
    assert_eq!(listed_json["ok"], true);
    assert_eq!(
        listed_json["data"]["document"]["providers"]
            .as_array()
            .map(Vec::len),
        Some(9)
    );
    assert_eq!(
        listed_json["data"]["signer_fingerprint"],
        "344588BBBFCC111E8FA61D82D63D8DE4D3B15A4B"
    );

    let verified = pinset(&["provider", "verify", "--json"]);
    assert!(verified.status.success(), "{}", stderr(&verified));
    let verified_json: serde_json::Value =
        serde_json::from_slice(&verified.stdout).expect("provider verify JSON");
    assert_eq!(verified_json["command"], "provider.verify");
    assert_eq!(verified_json["data"], listed_json["data"]);
}

#[test]
fn rejects_a_tampered_local_registry_with_a_stable_json_error() {
    let root = tempdir().expect("temporary registry directory");
    let registry = root.path().join("providers.json.asc");
    let tampered = include_str!("../../../registry/providers.json.asc").replacen(
        "pinset/node",
        "pinset/n0de",
        1,
    );
    fs::write(&registry, tampered).expect("write tampered registry");

    let output = pinset(&[
        "provider",
        "verify",
        registry.to_str().expect("UTF-8 registry path"),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("provider error JSON");
    assert_eq!(json["schema"], 1);
    assert_eq!(json["command"], "provider.verify");
    assert_eq!(json["ok"], false);
    assert_eq!(json["error"]["code"], "signature_invalid");
}

fn pinset(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pinset"))
        .args(arguments)
        .output()
        .expect("run pinset")
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
