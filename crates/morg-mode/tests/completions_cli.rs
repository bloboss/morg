//! CLI test for `morg completions`: the script generation must succeed and
//! mention the binary plus a known subcommand. The full script is not
//! snapshotted — clap_complete's output shifts between versions.

use std::process::Command;

fn run_completions(shell: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_morg"))
        .args(["completions", shell])
        .output()
        .expect("run morg completions")
}

#[test]
fn completions_zsh_emits_script() {
    let out = run_completions("zsh");
    assert!(out.status.success(), "exit: {:?}", out.status);

    let script = String::from_utf8(out.stdout).unwrap();
    assert!(script.contains("#compdef morg"), "missing #compdef header");
    assert!(script.contains("morg"), "missing binary name");
    assert!(script.contains("tangle"), "missing known subcommand");
}

#[test]
fn completions_rejects_unknown_shell() {
    let out = run_completions("notashell");
    assert!(!out.status.success());
}
