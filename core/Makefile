default: build

all: test

test: build
	cargo test

# Refresh Soroban test snapshots after an SDK upgrade.
# Usage: make update-snapshots  (equivalent to UPDATE_SNAPSHOTS=1 cargo test)
update-snapshots:
	UPDATE_SNAPSHOTS=1 cargo test

build:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm
	@$(MAKE) check-size

check-size:
	@MAX_SIZE=262144; \
	for wasm in target/wasm32v1-none/release/*.wasm; do \
		size=$$(wc -c < "$$wasm"); \
		if [ "$$size" -gt "$$MAX_SIZE" ]; then \
			echo "$$wasm is too large: $$size bytes (limit: $$MAX_SIZE)"; \
			exit 1; \
		fi; \
	done

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

# CPU/memory budget regression gate (issue #252): prints a markdown table and
# fails if any benchmarked entry point exceeds its ceiling.
budget-bench:
	cargo test -p payments budget_bench -- --nocapture --test-threads=1

.PHONY: default all test build check-size fmt clippy check doc clean budget-bench