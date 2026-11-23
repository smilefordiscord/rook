//! Undoing edits: while the DOM records, each change made through its
//! setters keeps what it replaced, so the change can be put back. What
//! ChangeHistoryService's waypoints are built from
//! (docs/behavior/studio/history.md).
//!
//! Only edits are recorded: a value written, a rename, a move, an
//! attribute or a tag. `Destroy` can't be undone (the instance is locked
//! and freed once nothing holds it), so editors remove things by moving
//! them to nil, as Studio's own delete can be undone.

use super::*;

/// What one change replaced.
#[derive(Debug, Clone, PartialEq)]
pub enum Undo {
    /// A stored property's value before the change (`None`: it read its
    /// class default).
    Property {
        id: InstanceId,
        property: PropertyId,
        old: Option<Value>,
    },
    Name {
        id: InstanceId,
        old: String,
    },
    Parent {
        id: InstanceId,
        old: Option<InstanceId>,
    },
    Attribute {
        id: InstanceId,
        name: String,
        old: Option<Value>,
    },
    /// A tag added (`had` false) or removed (`had` true).
    Tag {
        id: InstanceId,
        tag: String,
        had: bool,
    },
}

impl Dom {
    /// Start keeping what each change replaces (see [`Undo`]), dropping
    /// anything kept and not taken.
    pub fn start_recording(&mut self) {
        self.recording = Some(Vec::new());
    }

    pub fn is_recording(&self) -> bool {
        self.recording.is_some()
    }

    /// Stop recording and take what was kept, oldest first.
    pub fn stop_recording(&mut self) -> Vec<Undo> {
        self.recording.take().unwrap_or_default()
    }

    /// What was kept so far, oldest first, recording on.
    pub fn take_recorded(&mut self) -> Vec<Undo> {
        self.recording
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Put back what `undo` replaced, newest first, through the same
    /// setters, so the journal records each change as any other. Returns
    /// what puts it back again (the redo), oldest first. Changes to
    /// instances that are gone, or that the tree's rules now refuse (a
    /// parent that was destroyed), are skipped.
    pub fn revert(&mut self, undo: &[Undo]) -> Vec<Undo> {
        let outer = self.recording.replace(Vec::new());
        for u in undo.iter().rev() {
            let result = match u.clone() {
                Undo::Property { id, property, old } => match old {
                    Some(v) => self.set_property(id, property, v),
                    None => self.reset_property(id, property),
                },
                Undo::Name { id, old } => self.set_name(id, old),
                Undo::Parent { id, old } => {
                    if old.is_some_and(|p| self.is_destroyed(p)) {
                        Ok(())
                    } else {
                        self.set_parent(id, old)
                    }
                }
                Undo::Attribute { id, name, old } => self.set_attribute(id, &name, old),
                Undo::Tag { id, tag, had } => {
                    if had {
                        self.add_tag(id, &tag)
                    } else {
                        self.remove_tag(id, &tag)
                    }
                }
            };
            // A refused change (a parent now locked) has nothing to put back.
            let _ = result;
        }
        std::mem::replace(&mut self.recording, outer).unwrap_or_default()
    }

    /// Run `f` without recording: what the engine itself writes (the edit
    /// camera, input, GUI layout) isn't anyone's edit.
    pub fn unrecorded<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let kept = self.recording.take();
        let result = f(self);
        self.recording = kept;
        result
    }

    /// Leave out changes under `container`, a child of the DataModel (the
    /// GUI Studio and its plugins draw: CoreGui, PluginGuiService).
    pub fn leave_unrecorded(&mut self, container: InstanceId) {
        self.unrecorded.insert(container);
    }

    /// Whether a change to `id` is kept: recording is on and `id` is in
    /// the DataModel, outside the containers left out. What isn't in the
    /// DataModel is kept as it is, so moving it in (or out) is enough to
    /// undo.
    pub(crate) fn records(&self, id: InstanceId) -> bool {
        self.recording.is_some() && self.recorded_place(id)
    }

    /// Whether `id` is in the DataModel, outside the containers left out.
    fn recorded_place(&self, id: InstanceId) -> bool {
        let mut at = id;
        loop {
            match self.parent(at) {
                Some(p) if p == self.root => return !self.unrecorded.contains(&at),
                Some(p) => at = p,
                None => return at == self.root,
            }
        }
    }

    /// Whether moving `id` from `old` to `new` is kept: either end is in
    /// the recorded part of the DataModel. Services coming and going in
    /// the DataModel itself (one made by its first `GetService`) aren't
    /// edits.
    pub(crate) fn records_move(&self, old: Option<InstanceId>, new: Option<InstanceId>) -> bool {
        self.recording.is_some()
            && [old, new]
                .into_iter()
                .flatten()
                .any(|p| p != self.root && self.recorded_place(p))
    }

    /// Keep what a change replaced (see [`Dom::records`]).
    pub(crate) fn note_undo(&mut self, undo: Undo) {
        if let Some(r) = &mut self.recording {
            r.push(undo);
        }
    }
}
