SPKG := ../../spkg/erc20-events-v0.1.0.spkg
.DEFAULT_GOAL := build
.PHONY: build pack test
# Remap the checkout path in panic locations, so the WASM sha256 and module
# hash do not depend on where the repository is cloned.
build: export RUSTFLAGS += --remap-path-prefix=$(abspath ../..)=.
build:
	cargo build --locked -p erc20-events --target wasm32-unknown-unknown --release
# Packaging and live streaming stay paused until explicitly resumed.
pack: build
	substreams pack substreams.yaml -o $(SPKG)
test:
	cargo test --locked -p erc20-events
