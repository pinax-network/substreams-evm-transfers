//! Protobuf types for the packages of this repository, generated from
//! `proto/v1` by the `buf.build/anthropics/buffa` plugin (v0.9.2, see
//! `proto/buf.gen.yaml`); run `buf generate` in `proto/` to regenerate. This
//! file is written by hand.

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
}
