default:
    just --list

fmt:
    cargo fmt --all

lint:
    cargo fmt -- --check
    cargo clippy --all-targets -- -D warnings
    cargo xwin clippy --all-targets -- -D warnings

test:
    cargo test --all
    cargo xwin test --all

run *args:
    cargo run -- {{args}}

prepare version:
    lash run scripts/release/prepare.lash {{version}}

promote:
    just lint
    just test
    lash run scripts/release/promote.lash

release version:
    just lint
    just test
    lash run scripts/release/publish.lash {{version}}

gen-completions:
    #!/usr/bin/env bash
    for shell in bash fish powershell zsh elvish; do
        ext=$([ "$shell" = "powershell" ] && echo "ps1" || echo "$shell")
        cargo run --bin completions --features="shell-completions" -- "$shell" \
            > "./completions/completions.$ext"
    done
