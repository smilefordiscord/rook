use mlua::Lua;
use rook_dom::InstanceId;
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
/// `Text` in its `Font` at `Size`, wrapped to `Width` when that's above
/// zero (creator-docs TextService:GetTextBoundsAsync,
/// GetTextBoundsParams). Rook's fonts are local, so it answers at once.
/// Approximation: rich text is measured as written.
fn get_text_bounds(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let params = a.instance(0)?;
    let bounds = with_world(lua, |w| {
        let dom = &w.dom;
        let text = dom.string_property(params, "Text").unwrap_or_default();
        let size = dom.float_property(params, "Size").unwrap_or(20.0);
        let width = dom.float_property(params, "Width").unwrap_or(0.0);
        let font = dom
            .font_property(params, "Font")
            .or_else(|| rook_types::font_from_enum("SourceSans"))
            .unwrap_or_default();
        let wrap = if width > 0.0 { width } else { 1.0e7 };
        w.gui
            .text_size(&text, font, size, glam::Vec2::new(wrap, 1.0e7))
    });
    Outcome::ret(lua, LVector2(Vector2::new(bounds.x, bounds.y)))
}
