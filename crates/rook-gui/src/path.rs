//! Path2D: a GUI path of lines and Béziers through control points placed
//! in its parent's box (docs/behavior/gui/modern.md#path2d). Its points are
//! a property like any other, `PropertiesSerialize`, which Roblox keeps
//! them in: `Clone`, saving and replication carry them, and a change to
//! them reaches the GUI through the change journal, as a change to its
//! `Color3` does. Their bytes are read and written only by [`codec`]. The
//! curve they make is [`PathCurve`]; drawing it is `draw/objects/path.rs`.
//! Studio keeps a second copy of the points, its spline, which can fall
//! behind them; what that does to the setters is in [`spline`].

mod codec;
mod curve;
mod spline;

use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{UDim2, Value};

use crate::Gui;
use crate::layout::{Placed, gui_parent};
pub use curve::PathCurve;

/// A Path2D's control point (creator-docs Path2DControlPoint): where it is
/// in its path's parent, and its tangents, each from `position` to the
/// Bézier handle on that side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    pub position: UDim2,
    pub left: UDim2,
    pub right: UDim2,
}

/// The property a Path2D keeps its points in.
const POINTS: &str = "PropertiesSerialize";

/// Path2D `id`'s control points; none until a script sets them, or when
/// they're saved in Roblox's own layout, which is logged.
// TODO: read the points Studio saves in `PropertiesSerialize`, in
// `codec`, once a place Studio saved with a Path2D in it shows that
// blob's layout.
pub fn path_points(dom: &Dom, id: InstanceId) -> Vec<PathPoint> {
    let decoded = dom.read_by_name(id, POINTS, |v| match v {
        Value::BinaryString(b) => Some(codec::decode(b.as_ref())),
        _ => None,
    });
    match decoded {
        Some(Ok(points)) => points,
        Some(Err(e)) => {
            tracing::debug!("Path2D {} has no points Rook reads: {e}", dom.full_name(id));
            Vec::new()
        }
        None => Vec::new(),
    }
}

/// Make `points` Path2D `id`'s: its `PropertiesSerialize` changes, which
/// draws it again and replicates.
pub fn set_path_points(
    dom: &mut Dom,
    id: InstanceId,
    points: &[PathPoint],
) -> Result<(), DomError> {
    let bytes = codec::encode(points);
    dom.set_property_by_name(id, POINTS, Value::BinaryString(bytes.into()))
}

/// Whether Path2D `id` has a parent its points are placed in: a GUI
/// object or LayerCollector, past Folders. Confirmed (Studio 0.748):
/// without one, the setters that place a point and the length and bounds
/// queries raise `Attempting to use Path2D with invalid parent`.
pub fn path_is_placed(dom: &Dom, id: InstanceId) -> bool {
    gui_parent(dom, id).is_some()
}

/// How many segments Path2D `id`'s points make, wherever it is.
pub fn path_segments(dom: &Dom, id: InstanceId) -> usize {
    let closed = dom.bool_property(id, "Closed") == Some(true);
    curve::segment_count(path_points(dom, id).len(), closed)
}

impl Gui {
    /// Path2D `id`'s curve where its parent is laid out now, laying the
    /// parent's tree out first if it changed, and its spline built again.
    /// Confirmed (creator-docs 2D paths): points are placed in the path's
    /// parent, not the screen. `None` when it has no such parent.
    pub fn path_curve(&mut self, dom: &mut Dom, id: InstanceId) -> Option<PathCurve> {
        let parent = gui_parent(dom, id)?;
        self.sync_for(dom, parent);
        let frame = (self.layout.placed.get(&parent).copied()).unwrap_or(Placed::NOWHERE);
        self.splines.remove(&id);
        let closed = dom.bool_property(id, "Closed") == Some(true);
        Some(PathCurve::new(&path_points(dom, id), closed, frame))
    }
}
