//! Text objects' legacy views: `Font` of `FontFace` and `FontSize` of
//! `TextSize`, and `TextSize`'s limits; and GetTextBoundsParams' `Size`.

use rook_types::{enum_of_font, font_from_enum, font_size_of, text_size_of};

use super::*;

fn item(enum_name: &str, value: u32) -> Option<&'static str> {
    rook_reflection::database()
        .enum_by_name(enum_name)?
        .item_name(value)
}

fn value(enum_name: &str, item: &str) -> Option<Value> {
    let e = rook_reflection::database().enum_by_name(enum_name)?;
    e.item_value(item).map(Value::Enum)
}

pub(super) fn get(w: &World, id: InstanceId, name: &str) -> Option<Value> {
    match name {
        "Font" => value("Font", enum_of_font(&w.dom.font_property(id, "FontFace")?)),
        "FontSize" => value(
            "FontSize",
            font_size_of(w.dom.float_property(id, "TextSize")?),
        ),
        _ => None,
    }
}

pub(super) fn set(
    w: &mut World,
    id: InstanceId,
    name: &str,
    v: &Value,
) -> Option<Result<(), DomError>> {
    match (name, v) {
        ("Font", Value::Enum(n)) => {
            let font = font_from_enum(item("Font", *n)?)?;
            Some(set_named(w, id, "FontFace", Value::Font(font)))
        }
        ("FontSize", Value::Enum(n)) => {
            let size = text_size_of(item("FontSize", *n)?)?;
            Some(set_named(w, id, "TextSize", Value::Float32(size)))
        }
        // Measured in Studio 0.743: 500 reads back 100, -3 reads 1.
        ("TextSize", v) => {
            let size = v.as_number()?.clamp(1.0, 100.0) as f32;
            Some(set_named(w, id, "TextSize", Value::Float32(size)))
        }
        _ => None,
    }
}

/// What a GetTextBoundsParams' `Size` keeps of `v`. Measured (Studio
/// 0.748): anything that isn't a number, nil included, is 0; it's kept
/// within 1 to 100 and cut to a whole number (1.5 is 1), and NaN is
/// -2147483648.
pub(crate) fn params_size(v: &mlua::Value) -> Value {
    let n = crate::runtime::ctx::args::number_of(v).unwrap_or(0.0);
    let size = if n.is_nan() {
        f64::from(i32::MIN)
    } else {
        n.clamp(1.0, 100.0).trunc()
    };
    Value::Float32(size as f32)
}
