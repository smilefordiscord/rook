//! TextService's measuring: `GetTextSize` at once in what fonts are
//! loaded, and `GetTextBoundsAsync`, which waits for the fonts its text
//! needs (docs/behavior/gui/text.md#measuring-text).

use mlua::{Function, Lua, MultiValue, Thread};
use rook_dom::InstanceId;
use rook_gui::{TextFailure, TextQuery};
use rook_types::Vector2;

use super::RawMethod;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};
use crate::types::LVector2;

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
/// `Enum.Font` (creator-docs TextService:GetTextSize). Measured (Studio
/// 0.748): the arguments are cast from the last to the first, so with none
/// it's `Argument 4 missing or nil`; `size` is an `int`, rounded.
fn get_text_size(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let frame = a.datatype::<LVector2>(3, "Vector2")?.0;
    let font = a.enum_value(lua, 2, "Font")?;
    let size = a.int(1)?;
    let text = a.string(0)?;
    let font = rook_reflection::database()
        .enum_by_name("Font")
        .and_then(|e| e.item_name(font))
        .and_then(rook_types::font_from_enum)
        .unwrap_or_default();
    // Measured: a size of 0 or less is 0 tall.
    let size = size.max(0) as f32;
    let bounds = with_world(lua, |w| w.gui.text_size(&text, font, size, frame.0));
    Outcome::ret(lua, LVector2(Vector2::new(bounds.x, bounds.y)))
}

/// How far a font's text sits from where its line starts; Rook's fonts
/// place text at the line's start.
fn get_text_size_offset(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 0.0)
}

/// Measured (Studio 0.748): what `GetTextBoundsAsync` raises when it can't
/// measure.
fn failure_message(failure: TextFailure) -> &'static str {
    match failure {
        TextFailure::RichText => "Error parsing rich text",
        TextFailure::MissingFile => "Temp read failed.",
        TextFailure::MissingAsset => "Request asset was not found",
    }
}

/// `GetTextBoundsAsync(params)`: the bounds of a GetTextBoundsParams'
/// `Text` in its `Font` at `Size`, as rich text when `RichText` is on
/// (creator-docs TextService:GetTextBoundsAsync). Answered at once when its
/// fonts are loaded; otherwise the thread waits for them.
fn get_text_bounds(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    // Measured (Studio 0.748): nil and a missing argument are `Argument 1
    // missing or nil`, an instance of another class names the class it
    // wanted.
    let params = a.object(0)?.ok_or_else(|| a.missing(0))?;
    if !with_world(lua, |w| w.dom.is_a(params, "GetTextBoundsParams")) {
        return Err(ScriptError(
            "Argument 1 is invalid: expected 'GetTextBoundsParams' instance type".into(),
        ));
    }
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
    match with_world(lua, |w| w.gui.text_bounds(&query)) {
        Some(Ok(b)) => Outcome::ret(lua, LVector2(Vector2::new(b.x, b.y))),
        Some(Err(failure)) => Err(ScriptError(failure_message(failure).into())),
        None => {
            with_world(lua, |w| {
                w.text_bounds_waiting.push((lua.current_thread(), query))
            });
            let helper: Function = lua.named_registry_value("rook.text_bounds_wait")?;
            Ok(Outcome::Call(helper, MultiValue::new()))
        }
    }
}

/// A font arrived: the `GetTextBoundsAsync` calls it was the last font for
/// return or raise, and the rest keep waiting.
pub(crate) fn deliver(lua: &Lua) {
    let done: Vec<_> = with_world(lua, |w| {
        let waiting = std::mem::take(&mut w.text_bounds_waiting);
        let mut done = Vec::new();
        for (thread, query) in waiting {
            match w.gui.text_bounds(&query) {
                Some(result) => done.push((thread, result)),
                None => w.text_bounds_waiting.push((thread, query)),
            }
        }
        done
    });
    for (thread, result) in done {
        use crate::runtime::threads::resume;
        match result {
            Ok(b) => resume(lua, &thread, (true, LVector2(Vector2::new(b.x, b.y)))),
            Err(failure) => resume(lua, &thread, (false, failure_message(failure))),
        }
    }
}

/// The Luau side of a `GetTextBoundsAsync` that waits: it yields until
/// [`deliver`] resumes it with the bounds, or with what to raise.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let helper: Function = lua
        .load(
            r#"
            local wait, error = ...
            return function()
                local ok, result = wait()
                if not ok then
                    error(result, 0)
                end
                return result
            end
            "#,
        )
        .set_name("=[rook] text bounds")
        .call((
            api_fn(lua, |_, _| Ok(Outcome::Yield))?,
            lua.globals().get::<Function>("error")?,
        ))?;
    lua.set_named_registry_value("rook.text_bounds_wait", helper)
}
