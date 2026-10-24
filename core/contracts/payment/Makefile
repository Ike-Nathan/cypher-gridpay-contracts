# Payment contract — build & test tooling
#
# Snapshot tests: Soroban diagnostic snapshots (test_snapshots/) are sensitive to
# SDK version bumps (diagnostic logging / error formatting changes). To keep the
# suite stable across Soroban upgrades we:
#   * refresh snapshots explicitly via `make update-snapshots` (UPDATE_SNAPSHOTS=1)
#   * run the suite with `make test` which fails loudly on drift so it is caught
#     in CI rather than silently masked.

CARGO ?= cargo
CONTRACT ?= payment

.PHONY: build test test-semantic update-snapshots clean

build:
	$(CARGO) build --target wasm32-unknown-unknown --release

test:
	$(CARGO) test

# Semantic value assertions: run the suite while ignoring raw diagnostic
# snapshot equality, so behavior is validated independently of SDK formatting.
test-semantic:
	$(CARGO) test -- --skip snapshot

# Refresh Soroban diagnostic snapshots after an SDK upgrade.
# Usage: make update-snapshots
update-snapshots:
	UPDATE_SNAPSHOTS=1 $(CARGO) test

clean:
	$(CARGO) clean
