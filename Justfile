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

gen-completions:
    cargo run --bin completions --features="shell-completions" -- "bash" > ./completions/completions.bash
    cargo run --bin completions --features="shell-completions" -- "fish" > ./completions/completions.fish
    cargo run --bin completions --features="shell-completions" -- "powershell" > ./completions/completions.ps1
    cargo run --bin completions --features="shell-completions" -- "zsh" > ./completions/completions.zsh
    cargo run --bin completions --features="shell-completions" -- "elvish" > ./completions/completions.elvish

