//! The `db_out` param: how addresses are written. Ported from substreams-evm
//! `common/src/lib.rs` (`d8a731d`): `Encoding`, `handle_encoding_param`,
//! `bytes_to_hex`, `bytes_to_string` and `tron_base58_from_bytes`.

use sha2::{Digest, Sha256};
use substreams::{errors::Error, Hex};

/// How `db_out` writes addresses, selected by the `db_out` param.
///
/// Only addresses follow it: transaction hashes, log topics and log data are
/// `0x` + lowercase hex in either encoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    /// `0x` + lowercase hex: param `hex`, or an empty param.
    #[default]
    Hex,
    /// Tron Base58Check: param `tron_base58`. A 20-byte value gets the `0x41`
    /// version byte, a 21-byte one must start with it; any other length, an
    /// empty value included, is written as an empty string.
    TronBase58,
}

impl Encoding {
    /// Parses the `db_out` param as v0.3.3 did: `hex` or an empty param is
    /// [`Encoding::Hex`], `tron_base58` is [`Encoding::TronBase58`].
    ///
    /// # Errors
    ///
    /// Any other param, where v0.3.3 panicked.
    pub fn from_param(param: &str) -> Result<Self, Error> {
        match param {
            "" | "hex" => Ok(Self::Hex),
            "tron_base58" => Ok(Self::TronBase58),
            _ => Err(Error::msg(format!("invalid encoding parameter {param:?}, supported: hex, tron_base58"))),
        }
    }

    /// An address in this encoding (v0.3.3's `bytes_to_string`).
    pub(crate) fn address(self, bytes: &[u8]) -> String {
        match self {
            Self::Hex => hex(bytes),
            Self::TronBase58 => tron_base58(bytes).unwrap_or_default(),
        }
    }
}

/// `0x` + lowercase hex (v0.3.3's `bytes_to_hex`); `0x` for no bytes.
pub(crate) fn hex(bytes: &[u8]) -> String {
    format!("0x{}", Hex::encode(bytes))
}

/// The version byte of Tron addresses (Base58 `T...`).
const TRON_VERSION_BYTE: u8 = 0x41;

/// Base58Check of the version byte and the 20-byte address body: the payload,
/// then the first 4 bytes of its double SHA-256 (v0.3.3's
/// `tron_base58_from_bytes`). `None` for any length but 20, or 21 starting
/// with the version byte.
fn tron_base58(bytes: &[u8]) -> Option<String> {
    let mut data = match bytes.len() {
        20 => [&[TRON_VERSION_BYTE], bytes].concat(),
        21 if bytes[0] == TRON_VERSION_BYTE => bytes.to_vec(),
        _ => return None,
    };
    let checksum = Sha256::digest(Sha256::digest(&data));
    data.extend_from_slice(&checksum[..4]);
    Some(bs58::encode(data).into_string())
}

#[cfg(test)]
mod tests {
    //! The Base58 vectors are those of substreams-evm
    //! `common/tests/address_tests.rs` (`d8a731d`), plus Tron USDT.
    use super::*;

    fn bytes(hex: &str) -> Vec<u8> {
        Hex::decode(hex).unwrap()
    }

    #[test]
    fn params_select_the_encoding_like_v0_3_3() {
        assert_eq!(Encoding::from_param("").unwrap(), Encoding::Hex);
        assert_eq!(Encoding::from_param("hex").unwrap(), Encoding::Hex);
        assert_eq!(Encoding::from_param("tron_base58").unwrap(), Encoding::TronBase58);
        assert_eq!(Encoding::default(), Encoding::Hex);
        // v0.3.3 panicked on every other param; the port fails the block.
        for param in ["HEX", " hex", "hex ", "base58", "tron", "tron_base58\n"] {
            assert!(Encoding::from_param(param).is_err(), "{param:?}");
        }
    }

    #[test]
    fn hex_is_0x_and_lowercase_at_any_length() {
        assert_eq!(hex(&[]), "0x");
        assert_eq!(hex(&[0xAB, 0x01]), "0xab01");
        let address = bytes("d002c2f5b91353c1320e4d194d2d10e6b31dbe33");
        assert_eq!(Encoding::Hex.address(&address), "0xd002c2f5b91353c1320e4d194d2d10e6b31dbe33");
        assert_eq!(Encoding::Hex.address(&[]), "0x");
        assert_eq!(Encoding::Hex.address(&[0x11; 32]), format!("0x{}", "11".repeat(32)));
    }

    #[test]
    fn tron_base58_encodes_20_and_21_byte_addresses() {
        for (body, base58) in [
            ("0000000000000000000000000000000000000000", "T9yD14Nj9j7xAB4dbGeiX9h8unkKHxuWwb"),
            ("d002c2f5b91353c1320e4d194d2d10e6b31dbe33", "TUw4rqTx1tBvj5RosPfRW2dv38HoaC7yaR"),
            ("5e5602e054d1da6a43c83d9f8774daf15d79d210", "TJa1ZFq2UzaXZqfw7F7hjfRmiQxWWBkCzS"),
            ("72c28b75b5e60469ac187174a296683680ae8a58", "TLS168n4rPdVbZQDoDCjj57axRZTyZ9vJh"),
            // USDT (TRC-20).
            ("a614f803b6fd780986a42c78ec9c7f77e6ded13c", "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"),
        ] {
            assert_eq!(Encoding::TronBase58.address(&bytes(body)), base58);
            assert_eq!(Encoding::TronBase58.address(&bytes(&format!("41{body}"))), base58);
        }
    }

    #[test]
    fn tron_base58_writes_other_lengths_as_empty_strings() {
        let wrong_version = bytes("01d002c2f5b91353c1320e4d194d2d10e6b31dbe33");
        for address in [vec![], vec![0x41], vec![0; 19], wrong_version, vec![0x41; 22], vec![0x11; 32]] {
            assert_eq!(Encoding::TronBase58.address(&address), "", "{}", hex(&address));
        }
    }
}
