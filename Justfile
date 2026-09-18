set positional-arguments

default:
    just --list

fmt:
    cargo clippy --fix --bin "upstream"
    cargo fmt --all
    cargo spaced

lint:
    cargo fmt -- --check
    cargo spaced --check
    cargo clippy --all-targets -- -D warnings
    cargo xwin clippy --all-targets -- -D warnings


test:
    cargo nextest run --all
    cargo xwin test --all --target x86_64-pc-windows-msvc

start-artifact-server:
    ruby test-server/main.rb

test-artifact-server:
    ruby test-server/test.rb

generate-artifacts:
    scripts/test/generate-artifacts.py

test-all:
    just run-tests integration
    just run-tests end2end
    just run-tests live

run-test test:
    python3 -m unittest {{test}}

[arg('type', pattern='integration|end2end|live')]
run-tests *type:
    python3 -m unittest discover -s tests/{{type}} -p 'test_*.py'

verify-release:
    just lint
    just test
    just test-all

run *args:
    cargo run --bin "upstream" -- {{args}}

testbin *args:
    "$(scripts/test/testhome.sh)/.upstream/state/symlinks/upstream" {{args}}

prepare version:
    scripts/release/prepare.sh {{version}}

promote:
    scripts/release/promote.sh

publish version:
    scripts/release/publish.sh {{version}}
    git switch dev
    printf "ready" > .release-state

resync:
    scripts/release/resync.sh

gen-completions:
    #!/usr/bin/env bash
    for shell in bash fish powershell zsh elvish; do
        ext=$([ "$shell" = "powershell" ] && echo "ps1" || echo "$shell")
        cargo run --bin completions --features="shell-completions" -- "$shell" \
            > "./completions/completions.$ext"
    done

inspect-db:
    lazysql "$(scripts/test/testhome.sh)/.upstream/metadata/packages.db"
