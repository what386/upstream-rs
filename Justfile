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

integration-tests:
    python3 tests/integration/pkg_build.py
    python3 tests/integration/pkg_upgrade.py
    python3 tests/integration/pkg_rollback.py
    python3 tests/integration/state_mutations.py
    python3 tests/integration/pkg_export_import.py
    python3 tests/integration/pkg_upgrade_failure.py
    python3 tests/integration/pkg_upgrade_interrupt.py
    python3 tests/integration/pkg_install.py
    python3 tests/integration/pkg_remove.py
    python3 tests/integration/pkg_desktop_linux.py

verify-release:
    just lint
    just test
    just install-script-tests
    just integration-tests

install-script-tests:
    python3 -m unittest discover -s tests/install -p 'test_*.py'

run *args:
    cargo run --bin "upstream" -- {{args}}

testbin *args:
    ./tests/fakehome/.upstream/state/symlinks/upstream {{args}}

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
    lazysql ./tests/fakehome/.upstream/metadata/packages.db
