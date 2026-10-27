default: ci

# Full fast gate — pre-commit and CI both invoke this
ci: fmt-check lint test

install:
    cargo fetch --locked
    pre-commit install --install-hooks --hook-type pre-commit --hook-type commit-msg

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets -- -D warnings
    cargo clippy --all-targets --features send -- -D warnings

test:
    cargo test
    cargo test --features send

run:
    cargo run -- --help

clean:
    cargo clean
