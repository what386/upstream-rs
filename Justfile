default:
    just --list

fmt:
    cargo clippy --fix --bin "upstream"
    cargo fmt --all

lint:
    cargo fmt -- --check
    cargo clippy --all-targets -- -D warnings
    cargo xwin clippy --all-targets -- -D warnings

test:
    cargo nextest run --all
    cargo xwin test --all --target x86_64-pc-windows-msvc


run *args:
    cargo run --bin "upstream" -- {{args}}

prepare version:
    scripts/release/prepare.sh {{version}}

promote:
    just lint
    just test
    scripts/release/promote.sh

publish version:
    scripts/release/publish.sh {{version}}
    git switch dev

gen-completions:
    #!/usr/bin/env bash
    for shell in bash fish powershell zsh elvish; do
        ext=$([ "$shell" = "powershell" ] && echo "ps1" || echo "$shell")
        cargo run --bin completions --features="shell-completions" -- "$shell" \
            > "./completions/completions.$ext"
    done
