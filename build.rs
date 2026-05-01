fn main() {
    // Fetch the short commit hash at compile time
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    let hash = match output {
        Ok(o) if o.status.success() => String::from_utf8(o.stdout)
            .unwrap_or_default()
            .trim()
            .to_string(),
        _ => "unknown".to_string(),
    };

    // Expose it as an env var to the main crate
    println!("cargo:rustc-env=GIT_HASH={}", hash);
    // Re-run if HEAD changes (e.g. new commit)
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
}
