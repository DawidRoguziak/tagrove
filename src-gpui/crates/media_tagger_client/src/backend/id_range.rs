//! The ordered asset IDs of a query session range.
//!
//! The core answers `get_asset_query_id_range` in the binary format of the Tauri
//! command: little-endian f64 `[count, ids...]`, or no bytes at all when the session
//! is stale. The port hands models typed IDs; this is the decoder its core adapter
//! uses (the counterpart of `decodeAssetIdRange` in `src/platform/tauri/assets.ts`).

use std::fmt::{Display, Formatter};

/// Largest asset ID the binary format carries exactly (2^53).
pub const MAX_EXACT_ID: i64 = 1 << 53;

/// Ordered snapshot IDs of an inclusive session range, clamped by the backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetIdRange {
    Ready(Vec<i64>),
    /// The session changed or is gone.
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdRangeError {
    /// The length is not a multiple of eight bytes, or a value is not an asset ID.
    Malformed,
    /// The leading count does not match the number of IDs that follow.
    Incomplete,
}

impl Display for IdRangeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Malformed => "Asset ID range response is malformed",
            Self::Incomplete => "Asset ID range response is incomplete",
        })
    }
}

impl std::error::Error for IdRangeError {}

/// Decodes a binary ID range. An empty body means `Stale`.
pub fn decode_id_range(bytes: &[u8]) -> Result<AssetIdRange, IdRangeError> {
    if bytes.is_empty() {
        return Ok(AssetIdRange::Stale);
    }
    let (chunks, rest) = bytes.as_chunks::<8>();
    if !rest.is_empty() {
        return Err(IdRangeError::Malformed);
    }
    let mut values = chunks.iter().map(|chunk| f64::from_le_bytes(*chunk));
    let count = values.next().ok_or(IdRangeError::Malformed)?;
    if count != (chunks.len() - 1) as f64 {
        return Err(IdRangeError::Incomplete);
    }
    values
        .map(|value| {
            let exact = value.fract() == 0.0 && (1.0..=MAX_EXACT_ID as f64).contains(&value);
            exact.then_some(value as i64).ok_or(IdRangeError::Malformed)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(AssetIdRange::Ready)
}

#[cfg(test)]
mod tests {
    use super::{decode_id_range, AssetIdRange, IdRangeError, MAX_EXACT_ID};

    fn body(values: &[f64]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn decodes_little_endian_f64_ids_after_the_count() {
        let bytes = body(&[3.0, 7.0, MAX_EXACT_ID as f64, 1.0]);

        assert_eq!(
            decode_id_range(&bytes),
            Ok(AssetIdRange::Ready(vec![7, MAX_EXACT_ID, 1]))
        );
    }

    #[test]
    fn an_empty_body_is_stale_and_a_zero_count_is_an_empty_range() {
        assert_eq!(decode_id_range(&[]), Ok(AssetIdRange::Stale));
        assert_eq!(
            decode_id_range(&body(&[0.0])),
            Ok(AssetIdRange::Ready(Vec::new()))
        );
    }

    #[test]
    fn rejects_malformed_and_incomplete_bodies() {
        assert_eq!(decode_id_range(&[0; 12]), Err(IdRangeError::Malformed));
        assert_eq!(
            decode_id_range(&body(&[3.0, 1.0])),
            Err(IdRangeError::Incomplete)
        );
        assert_eq!(
            decode_id_range(&body(&[1.0, 1.5])),
            Err(IdRangeError::Malformed)
        );
        assert_eq!(
            decode_id_range(&body(&[1.0, 0.0])),
            Err(IdRangeError::Malformed)
        );
        assert_eq!(
            decode_id_range(&body(&[1.0, -4.0])),
            Err(IdRangeError::Malformed)
        );
        assert_eq!(
            decode_id_range(&body(&[1.0, f64::NAN])),
            Err(IdRangeError::Malformed)
        );
        assert_eq!(
            decode_id_range(&body(&[1.0, MAX_EXACT_ID as f64 * 2.0])),
            Err(IdRangeError::Malformed)
        );
        assert_eq!(
            IdRangeError::Malformed.to_string(),
            "Asset ID range response is malformed"
        );
        assert_eq!(
            IdRangeError::Incomplete.to_string(),
            "Asset ID range response is incomplete"
        );
    }
}
