//! Number properties a script's value is kept within: set outside the
//! range, one reads back at its nearer end, and NaN stays NaN (measured in
//! Studio 0.748 by setting -1e9, 1e9 and ±inf on each). A whole-number
//! property is made whole before it gets here (2.5 reads 2, NaN 0). The
//! audio API's ranges are `crate::audio_api::clamp`, where NaN reads as
//! the highest value instead.

use rook_dom::{DomError, InstanceId};
use rook_types::Value;

use super::set_named;
use crate::runtime::world::World;

/// (class, property, lowest, highest).
const RANGES: &[(&str, &str, f64, f64)] = &[
    // Conformance case `lighting/post_effects_clamps`; InFocusRadius has
    // only a top (-1 reads -1).
    ("DepthOfFieldEffect", "FarIntensity", 0.0, 1.0),
    ("DepthOfFieldEffect", "NearIntensity", 0.0, 1.0),
    ("DepthOfFieldEffect", "FocusDistance", 0.0, 500.0),
    (
        "DepthOfFieldEffect",
        "InFocusRadius",
        f64::NEG_INFINITY,
        500.0,
    ),
];

/// Store `value` within its range, if `name` has one on `id`'s class.
pub(super) fn clamp(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    let class = w.dom.class_name(id);
    let &(_, _, lo, hi) = RANGES
        .iter()
        .find(|(c, p, _, _)| *c == class && *p == name)?;
    // `clamp` keeps NaN, as Studio does.
    let kept = match *value {
        Value::Float32(v) => Value::Float32(v.clamp(lo as f32, hi as f32)),
        Value::Float64(v) => Value::Float64(v.clamp(lo, hi)),
        Value::Int32(n) => Value::Int32(f64::from(n).clamp(lo, hi) as i32),
        _ => return None,
    };
    Some(set_named(w, id, name, kept))
}
