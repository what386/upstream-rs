set unstable

default:
    just --list

check:
    cargo check --all-targets

fmt:
    cargo fmt --all

lint:
    cargo fmt -- --check
    cargo clippy --all-targets -- -D warnings

test:
    cargo test --all

run *args:
    cargo run -- {{args}}

completions *args:
    cargo run --bin completions --features="shell-completions" -- {{args}}


clean:
    cargo clean

release target:
    mkdir -p dist
    cross build --release --target {{target}}
    cp "target/{{target}}/release/upstream-rs{{ext(target)}}" "dist/upstream-rs-{{target}}{{ext(target)}}"

release_win-x86:
    cargo xwin build --target x86_64-pc-windows-msvc

matrix:
    # Linux
    cargo build --release --target x86_64-unknown-linux-gnu
    cargo build --release --target aarch64-unknown-linux-gnu
    # Windows
    cargo xwin build --release --target x86_64-pc-windows-msvc
    cargo xwin build --release --target aarch64-pc-windows-msvc
    # MacOS
    cargo build --release --target x86_64-apple-darwin
    cargo build --release --target aarch64-apple-darwin
    # Generate checksums
    find dist -type f ! -name 'SHA256SUMS.txt' -print0 | sort -z | xargs -0 sha256sum > dist/SHA256SUMS.txt
