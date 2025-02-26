use std::io::Write;

use isr_core::Profile;

/// A codec for encoding and decoding profiles.
pub trait Codec {
    /// The file extension for this codec.
    const EXTENSION: &'static str;

    /// The error type for encoding.
    type EncodeError: std::error::Error + Send + Sync + 'static;

    /// The error type for decoding.
    type DecodeError: std::error::Error + Send + Sync + 'static;

    /// Encodes a profile into the given writer.
    fn encode(writer: impl Write, profile: &Profile) -> Result<(), Self::EncodeError>;

    /// Decodes a profile from the given slice.
    fn decode(slice: &[u8]) -> Result<Profile, Self::DecodeError>;
}

/// A codec for the bincode format.
///
/// Provides a compact binary representation of profiles.
#[cfg(feature = "codec-bincode")]
pub struct BincodeCodec;

#[cfg(feature = "codec-bincode")]
impl Codec for BincodeCodec {
    const EXTENSION: &'static str = "bin";

    type EncodeError = bincode::Error;
    type DecodeError = bincode::Error;

    fn encode(writer: impl Write, profile: &Profile) -> Result<(), Self::EncodeError> {
        bincode::serialize_into(writer, profile)
    }

    fn decode(slice: &[u8]) -> Result<Profile, Self::DecodeError> {
        bincode::deserialize(slice)
    }
}

/// A codec for the JSON format.
///
/// Provides human-readable profiles.
#[cfg(feature = "codec-json")]
pub struct JsonCodec;

#[cfg(feature = "codec-json")]
impl Codec for JsonCodec {
    const EXTENSION: &'static str = "json";

    type EncodeError = serde_json::Error;
    type DecodeError = serde_json::Error;

    fn encode(writer: impl Write, profile: &Profile) -> Result<(), Self::EncodeError> {
        serde_json::to_writer_pretty(writer, profile)
    }

    fn decode(slice: &[u8]) -> Result<Profile, Self::DecodeError> {
        serde_json::from_slice(slice)
    }
}

/// A codec for the MessagePack format.
///
/// Provides a compact binary representation of profiles.
#[cfg(feature = "codec-msgpack")]
pub struct MsgpackCodec;

#[cfg(feature = "codec-msgpack")]
impl Codec for MsgpackCodec {
    const EXTENSION: &'static str = "msgpack";

    type EncodeError = rmp_serde::encode::Error;
    type DecodeError = rmp_serde::decode::Error;

    fn encode(mut writer: impl Write, profile: &Profile) -> Result<(), Self::EncodeError> {
        rmp_serde::encode::write(&mut writer, profile)
    }

    fn decode(slice: &[u8]) -> Result<Profile, Self::DecodeError> {
        rmp_serde::from_slice(slice)
    }
}
