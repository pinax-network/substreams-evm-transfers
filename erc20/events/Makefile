SPKG := ../../spkg/erc20-events-v0.1.0.spkg
.DEFAULT_GOAL := build
.PHONY: build pack test
# Reproducible WASM paths and hashes: see build.mk at the repository root.
include ../../build.mk
build: export RUSTFLAGS := $(WASM_RUSTFLAGS)
build:
	cargo build --locked -p erc20-events --target wasm32-unknown-unknown --release
# Each live step needs the owner's OK: see "Live use" in AGENTS.md (Evidence).
pack: build
	substreams pack substreams.yaml -o $(SPKG)
test:
	cargo test --locked -p erc20-events
