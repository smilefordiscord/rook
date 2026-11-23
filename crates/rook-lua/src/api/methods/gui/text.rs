//! TextService's measuring: `GetTextSize` at once in what fonts are
//! loaded, and `GetTextBoundsAsync`, which waits for the fonts its text
//! needs (docs/behavior/gui/text.md#measuring-text).

use mlua::{Lua, Thread};
use rook_dom::InstanceId;
use rook_gui::TextQuery;
use rook_types::Vector2;

use super::RawMethod;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};
use crate::types::LVector2;
use crate::types::enums::item_name;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("TextService", "GetTextSize") => get_text_size,
        ("TextService", "GetTextSizeOffsetAsync") => get_text_size_offset,
        ("TextService", "GetTextBoundsAsync") => get_text_bounds,
        _ => return None,
    })
}

/// `GetTextBoundsAsync` calls waiting for a font, with what each measures
/// (read when called, so later changes to its params don't count).
pub(crate) type BoundsWaiting = Vec<(Thread, TextQuery)>;

/// `GetTextSize(text, size, font, frameSize)`: the bounds of `text` in an
/// `Enum.Font` wrapped to the frame's width (creator-docs
/// TextService:GetTextSize).
fn get_text_size(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let size = a.number(1)? as f32;
    let font = item_name(lua, a.get(2), "Font")
        .and_then(rook_types::font_from_enum)
        .ok_or_else(|| a.wrong(2, "Font"))?;
    let frame = a.userdata::<LVector2>(3, "Vector2")?.0;
    let bounds = with_world(lua, |w| w.gui.text_size(&text, font, size, frame.0));
    Outcome::ret(lua, LVector2(Vector2::new(bounds.x, bounds.y)))
}

/// How far a font's text sits from where its line starts; Rook's fonts
/// place text at the line's start.
fn get_text_size_offset(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 0.0)
}

/// `GetTextBoundsAsync(params)`: the bounds of a GetTextBoundsParams'
/// `Text` in its `Font` at `Size`, as rich text when `RichText` is on
/// (creator-docs TextService:GetTextBoundsAsync). Answered at once when its
/// fonts are loaded; otherwise the thread waits for them.
fn get_text_bounds(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    // Inference: nil and other classes are turned away as missing.
    let params = a
        .object_of(lua, 0, "GetTextBoundsParams")?
        .ok_or_else(|| a.missing(0))?;
    let query = with_world(lua, |w| {
        let dom = &w.dom;
        TextQuery {
            text: dom.string_property(params, "Text").unwrap_or_default(),
            font: dom.font_property(params, "Font").unwrap_or_default(),
            size: dom.float_property(params, "Size").unwrap_or_default(),
            // Inference: a `Width` of 0 or less (the default) doesn't wrap.
            width: dom.float_property(params, "Width").filter(|&w| w > 0.0),
            rich: dom.bool_property(params, "RichText") == Some(true),
        }
    });
    let thread = lua.current_thread();
    let bounds = with_world(lua, |w| {
        let bounds = w.gui.text_bounds(&query);
        if bounds.is_none() {
            w.text_bounds_waiting.push((thread, query));
        }
        bounds
    });
    match bounds {
        Some(b) => Outcome::ret(lua, LVector2(Vector2::new(b.x, b.y))),
        None => Ok(Outcome::Yield),
    }
}

/// A font arrived: the `GetTextBoundsAsync` calls it was the last font for
/// return, and the rest keep waiting.
pub(crate) fn deliver(lua: &Lua) {
    let done: Vec<_> = with_world(lua, |w| {
        let waiting = std::mem::take(&mut w.text_bounds_waiting);
        let mut done = Vec::new();
        for (thread, query) in waiting {
            match w.gui.text_bounds(&query) {
                Some(b) => done.push((thread, b)),
                None => w.text_bounds_waiting.push((thread, query)),
            }
        }
        done
    });
    for (thread, b) in done {
        crate::runtime::threads::resume(lua, &thread, LVector2(Vector2::new(b.x, b.y)));
    }
}
