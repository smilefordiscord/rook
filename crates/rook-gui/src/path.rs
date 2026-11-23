//! Path2D: a GUI path of cubic Béziers through control points placed in
//! its parent's box (docs/behavior/gui/modern.md#path2d). Scripts set the
//! points with Path2D's methods; they are kept here, beside the layout
//! they're placed by, not in the DOM, where Roblox's only property for
//! them is a binary blob of its own (`PropertiesSerialize`). The curve
//! they make is [`PathCurve`]; drawing it is `draw/objects/path.rs`.

mod curve;

use rook_dom::{Dom, InstanceId};
use rook_types::UDim2;
use rustc_hash::FxHashMap;

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

/// Each Path2D's control points.
pub(crate) type Paths = FxHashMap<InstanceId, Vec<PathPoint>>;

impl Gui {
    /// A Path2D's control points; none until a script sets them.
    // TODO: read the points a file saves in `PropertiesSerialize` once a
    // place Studio saved with a Path2D in it shows that blob's layout.
    pub fn path_points(&self, id: InstanceId) -> &[PathPoint] {
        self.paths.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Set a Path2D's control points; it draws again.
    pub fn set_path_points(&mut self, id: InstanceId, points: Vec<PathPoint>) {
        if points.is_empty() {
            self.paths.remove(&id);
        } else {
            self.paths.insert(id, points);
        }
        self.restyled.insert(id);
    }

    /// Path2D `id`'s curve where its parent is laid out now, laying the
    /// parent's tree out first if it changed. Confirmed (creator-docs 2D
    /// paths): points are placed in the path's parent, not the screen.
    /// Inference: a path with no GUI parent is placed as an unparented
    /// object's children are, by offsets alone.
    pub fn path_curve(&mut self, dom: &mut Dom, id: InstanceId) -> PathCurve {
        let frame = match gui_parent(dom, id) {
            Some(parent) => {
                self.sync_for(dom, parent);
                let placed = self.layout.placed.get(&parent);
                placed.copied().unwrap_or(Placed::NOWHERE)
            }
            None => Placed::NOWHERE,
        };
        let closed = dom.bool_property(id, "Closed") == Some(true);
        PathCurve::new(self.path_points(id), closed, frame)
    }
}
