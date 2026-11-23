//! Path2D: a GUI path of cubic Béziers through control points placed in
//! its parent's box (docs/behavior/gui/modern.md#path2d). Scripts set the
//! points with Path2D's methods; they are kept here, beside the layout
//! they're placed by, not in the DOM, where Roblox's only property for
//! them is a binary blob of its own (`PropertiesSerialize`).

use rook_dom::InstanceId;
use rook_types::UDim2;

use crate::Gui;

/// A Path2D's control point (creator-docs Path2DControlPoint): where it is
/// in its path's parent, and its tangents, each from `position` to the
/// Bézier handle on that side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    pub position: UDim2,
    pub left: UDim2,
    pub right: UDim2,
}

impl Gui {
    /// A Path2D's control points; none until a script sets them.
    // TODO: read the points a file saves in `PropertiesSerialize` once a
    // place Studio saved with a Path2D in it shows that blob's layout.
    pub fn path_points(&self, id: InstanceId) -> &[PathPoint] {
        self.paths.get(&id).map_or(&[], Vec::as_slice)
    }

    pub fn set_path_points(&mut self, id: InstanceId, points: Vec<PathPoint>) {
        if points.is_empty() {
            self.paths.remove(&id);
        } else {
            self.paths.insert(id, points);
        }
    }
}
