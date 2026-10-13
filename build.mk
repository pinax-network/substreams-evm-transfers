# Included by the package Makefiles. WASM panic locations embed source paths,
# so remap the checkout, CARGO_HOME and the Rust sources: the WASM sha256 and
# the module hashes then depend only on the source and the pinned toolchain,
# not on the checkout directory, the user or whether rust-src is installed.
# `:=` in the Makefiles keeps RUSTFLAGS set in the shell out of the build.
REPO_ROOT := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
RUST_SYSROOT := $(shell rustc --print sysroot)
RUST_COMMIT := $(shell rustc -vV | sed -n 's/^commit-hash: //p')
WASM_RUSTFLAGS := --remap-path-prefix=$(REPO_ROOT)=. --remap-path-prefix=$(or $(CARGO_HOME),$(HOME)/.cargo)=/cargo --remap-path-prefix=$(RUST_SYSROOT)/lib/rustlib/src/rust=/rustc/$(RUST_COMMIT)
