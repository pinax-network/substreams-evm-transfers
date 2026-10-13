//! Protobuf types for the packages of this repository, generated from
//! `proto/v1` by the `buf.build/anthropics/buffa` plugin (v0.9.2, see
//! `proto/buf.gen.yaml`); run `buf generate` in `proto/` to regenerate. This
//! file is written by hand.
//!
//! `erc20.transfers.v1`, `erc20.tokens.v1` and `native.transfers.v1` are
//! substreams-evm's protos, byte for byte: `v1/erc20-transfers.proto` and
//! `v1/erc20-tokens.proto` at `d8a731d` (tag `evm-transfers-v0.3.3`), and
//! `v1/native-transfers.proto` at `aaae9dd`, the version of the native
//! transfers package that `evm-transfers-v0.3.3` runs.

macro_rules! include_buffa {
    ($package:literal) => {
        include!(concat!($package, ".mod.rs"));
    };
}

pub mod erc20 {
    pub mod events {
        pub mod v1 {
            include_buffa!("erc20.events.v1");
        }
    }
    pub mod tokens {
        pub mod v1 {
            include_buffa!("erc20.tokens.v1");
        }
    }
    pub mod transfers {
        pub mod v1 {
            include_buffa!("erc20.transfers.v1");
        }
    }
}

pub mod native {
    pub mod transfers {
        pub mod v1 {
            include_buffa!("native.transfers.v1");
        }
    }
}
