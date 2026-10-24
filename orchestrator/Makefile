default: build

all: test

test: build
	cargo test

build:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets -- -D warnings

check:
	cargo check

doc:
	cargo doc --no-deps

clean:
	cargo clean

.PHONY: default all test build fmt clippy check doc clean