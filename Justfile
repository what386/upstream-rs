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

integration-tests:
    python3 tests/integration/pkg_build.py
    python3 tests/integration/pkg_upgrade.py
    python3 tests/integration/pkg_rollback.py
    python3 tests/integration/state_mutations.py
    python3 tests/integration/pkg_export_import.py
    python3 tests/integration/pkg_install.py
    python3 tests/integration/pkg_remove.py

registry-validate:
    python3 scripts/registry/validate.py
    python3 -m unittest discover -s tests/registry -p 'test_*.py'

registry-validate-revisions base_ref:
    python3 scripts/registry/validate_revisions.py {{base_ref}}

registry-index:
    python3 scripts/registry/build_index.py


run *args:
    cargo run --bin "upstream" -- {{args}}

testbin *args:
    ./tests/fakehome/.upstream/state/symlinks/upstream {{args}}

prepare version:
    scripts/release/prepare.sh {{version}}

promote:
    just lint
    just test
    scripts/release/promote.sh

publish version:
    scripts/release/publish.sh {{version}}
    git switch dev
    printf "ready" > .release-state

gen-completions:
    #!/usr/bin/env bash
    for shell in bash fish powershell zsh elvish; do
        ext=$([ "$shell" = "powershell" ] && echo "ps1" || echo "$shell")
        cargo run --bin completions --features="shell-completions" -- "$shell" \
            > "./completions/completions.$ext"
    done

inspect-db:
    lazysql ~/.upstream/metadata/packages.db
