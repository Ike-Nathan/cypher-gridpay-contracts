default: build

all: test

test: build
	cargo test

# Refresh Soroban test snapshots after SDK/diagnostic formatting changes.
# Usage: make update-snapshots  (or UPDATE_SNAPSHOTS=1 make test)
update-snapshots:
	UPDATE_SNAPSHOTS=1 cargo test

build:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm

fmt:
	cargo fmt --all

clean:
	cargo clean
