.DEFAULT_GOAL := pack

# Reproducible WASM paths and hashes: see build.mk at the repository root.
include ../../build.mk
build: export RUSTFLAGS := $(WASM_RUSTFLAGS)
.PHONY: build
build:
	cargo build --locked --target wasm32-unknown-unknown --release -p erc20-tokens

# Local packs go to the ignored out/; committed spkgs come from CI's Linux build.
.PHONY: pack
pack: build
	mkdir -p ../../out
	substreams pack -o ../../out/{spkgDefaultName}
