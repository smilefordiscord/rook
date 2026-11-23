//! A Path2D's spline: Studio keeps a path's points twice, as its property
//! and in a spline that places them in its parent, and the two can
//! disagree. Confirmed (Studio 0.748): `SetControlPoints` leaves the
//! spline empty; `InsertControlPoint` and `RemoveControlPoint` change the
//! points, then the spline, and raise when the spline hasn't that place
//! (`Spline inserting control point at index 1 access out of bounds`,
//! counting from 0), keeping the points as changed. Inference: the spline
//! is built from the points again when the path's tree is laid out for a
//! frame or its curve is queried, so only a path changed since falls
//! behind. Rook keeps only how many points such a path's spline holds.

use rook_dom::{Dom, InstanceId};

use crate::Gui;
use crate::layout::gui_parent;

impl Gui {
    /// `SetControlPoints` emptied Path2D `id`'s spline.
    pub fn empty_path_spline(&mut self, id: InstanceId) {
        self.splines.insert(id, 0);
    }

    /// A point inserted at `index` (from 0) in Path2D `id`'s spline: false
    /// when the spline holds fewer points than `index`.
    pub fn insert_into_path_spline(&mut self, id: InstanceId, index: usize) -> bool {
        let Some(held) = self.splines.get_mut(&id) else {
            return true;
        };
        if index > *held {
            return false;
        }
        *held += 1;
        true
    }

    /// The point at `index` (from 0) removed from Path2D `id`'s spline:
    /// false when the spline holds no point there.
    pub fn remove_from_path_spline(&mut self, id: InstanceId, index: usize) -> bool {
        let Some(held) = self.splines.get_mut(&id) else {
            return true;
        };
        if index >= *held {
            return false;
        }
        *held -= 1;
        true
    }

    /// The frame's layout builds every placed path's spline again; one with
    /// no parent to place it in stays as it is.
    pub(crate) fn rebuild_path_splines(&mut self, dom: &Dom) {
        if self.splines.is_empty() {
            return;
        }
        (self.splines).retain(|&id, _| dom.get(id).is_some() && gui_parent(dom, id).is_none());
    }
}
