//! A Path2D's control points as the bytes of its `PropertiesSerialize`,
//! the hidden, saved and replicated BinaryString Roblox keeps them in, so
//! `Clone`, saving, replication and the change journal carry them like
//! any property. Roblox's layout of the blob isn't documented, so Rook
//! writes its own, which only this module reads and writes: the 4-byte
//! tag [`TAG`], the number of points as a little-endian `u32`, then each
//! point's `Position`, `LeftTangent` and `RightTangent`, each a UDim2 as
//! X scale (`f32`), X offset (`i32`), Y scale and Y offset, little-endian.
//! No points is the class default, so such a path saves as Studio's does.

use rook_types::{UDim, UDim2};

use super::PathPoint;

/// Rook's tag, which Roblox's layout is taken not to start with.
const TAG: &[u8; 4] = b"RKP1";

/// The class default: a path with no points (the reflection database's
/// default for `PropertiesSerialize`).
const EMPTY: [u8; 4] = [0; 4];

/// Bytes a UDim2 takes, and a point (its position and two tangents).
const UDIM2_BYTES: usize = 16;
const POINT_BYTES: usize = 3 * UDIM2_BYTES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DecodeError {
    /// Not Rook's layout: Roblox's own, saved by Studio.
    Unknown,
    /// Rook's tag, but not as many bytes as its count of points needs.
    Length { count: usize, bytes: usize },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown => f.write_str("not in Rook's layout (Roblox's isn't read)"),
            Self::Length { count, bytes } => {
                write!(f, "{bytes} bytes don't hold {count} control points")
            }
        }
    }
}

/// `points` as `PropertiesSerialize`'s bytes.
pub(crate) fn encode(points: &[PathPoint]) -> Vec<u8> {
    if points.is_empty() {
        return EMPTY.to_vec();
    }
    let mut out = Vec::with_capacity(8 + points.len() * POINT_BYTES);
    out.extend_from_slice(TAG);
    let count = u32::try_from(points.len()).expect("a path's points fit a u32");
    out.extend_from_slice(&count.to_le_bytes());
    for p in points {
        for u in [p.position, p.left, p.right] {
            for d in [u.x, u.y] {
                out.extend_from_slice(&d.scale.to_le_bytes());
                out.extend_from_slice(&d.offset.to_le_bytes());
            }
        }
    }
    out
}

/// The points `bytes` hold.
pub(crate) fn decode(bytes: &[u8]) -> Result<Vec<PathPoint>, DecodeError> {
    if bytes.is_empty() || bytes == EMPTY {
        return Ok(Vec::new());
    }
    let Some(rest) = bytes.strip_prefix(TAG) else {
        return Err(DecodeError::Unknown);
    };
    let length = DecodeError::Length {
        count: 0,
        bytes: bytes.len(),
    };
    let (count, body) = rest.split_first_chunk::<4>().ok_or(length)?;
    let count = u32::from_le_bytes(*count) as usize;
    if Some(body.len()) != count.checked_mul(POINT_BYTES) {
        return Err(DecodeError::Length {
            count,
            bytes: bytes.len(),
        });
    }
    Ok(body
        .as_chunks::<POINT_BYTES>()
        .0
        .iter()
        .map(point)
        .collect())
}

/// One point from its [`POINT_BYTES`] bytes.
fn point(bytes: &[u8; POINT_BYTES]) -> PathPoint {
    let word = |at: usize| -> [u8; 4] { bytes[at..at + 4].try_into().expect("4 bytes") };
    let udim = |at: usize| UDim {
        scale: f32::from_le_bytes(word(at)),
        offset: i32::from_le_bytes(word(at + 4)),
    };
    let udim2 = |at: usize| UDim2 {
        x: udim(at),
        y: udim(at + 8),
    };
    PathPoint {
        position: udim2(0),
        left: udim2(UDIM2_BYTES),
        right: udim2(2 * UDIM2_BYTES),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_come_back_as_they_went() {
        let points = vec![
            PathPoint {
                position: UDim2::new(0.25, -3, 1.0, 7),
                left: UDim2::from_offset(-10, 0),
                right: UDim2::new(f32::INFINITY, 0, 0.5, i32::MIN),
            },
            PathPoint {
                position: UDim2::new(0.0, 0, 0.0, 0),
                left: UDim2::default(),
                right: UDim2::from_offset(i32::MAX, 1),
            },
        ];
        let bytes = encode(&points);
        assert_eq!(bytes.len(), 8 + 2 * POINT_BYTES);
        assert_eq!(decode(&bytes), Ok(points));
    }

    /// No points is the class default, so a path without them saves and
    /// replicates as Studio's does.
    #[test]
    fn no_points_is_the_class_default() {
        assert_eq!(encode(&[]), EMPTY);
        assert_eq!(decode(&EMPTY), Ok(Vec::new()));
        assert_eq!(decode(&[]), Ok(Vec::new()));
    }

    #[test]
    fn other_layouts_and_short_blobs_are_refused() {
        assert_eq!(decode(&[1, 0, 0, 0, 9]), Err(DecodeError::Unknown));
        let mut bytes = encode(&[PathPoint {
            position: UDim2::default(),
            left: UDim2::default(),
            right: UDim2::default(),
        }]);
        bytes.pop();
        let short = DecodeError::Length {
            count: 1,
            bytes: 55,
        };
        assert_eq!(decode(&bytes), Err(short));
        assert!(matches!(
            decode(b"RKP1\x01"),
            Err(DecodeError::Length { .. })
        ));
    }
}
