//! Measuring text for TextService (docs/behavior/gui/text.md#measuring-text):
//! the measurements need the fonts of the user's Roblox install and are
//! skipped without one.

use super::*;

fn installed() -> Option<Gui> {
    use rook_content::resolver::install::{Places, newest_from};
    let install = newest_from(None, &Places::from_env(|n| std::env::var_os(n)))?;
    let mut config = rook_content::Config::empty();
    config.install = Some(install);
    config.network = false;
    let mut gui = Gui::default();
    gui.set_content(std::sync::Arc::new(rook_content::Resolver::new(config)));
    Some(gui)
}

fn query(text: &str, width: Option<f32>, rich: bool) -> TextQuery {
    TextQuery {
        text: text.into(),
        font: rook_types::font_from_enum("SourceSans").unwrap(),
        size: 24.0,
        width,
        rich,
    }
}

/// The bounds a TextLabel of the same text reports: "Hello, world!" in
/// Source Sans 24 is 107 × 24 in Studio.
#[test]
fn text_bounds_measure_as_a_label_lays_out() {
    let Some(mut gui) = installed() else {
        eprintln!("skipped: no Roblox install found");
        return;
    };
    let hello = "Hello, world!";
    assert_eq!(
        gui.text_bounds(&query(hello, None, false)),
        Some(Vec2::new(107.0, 24.0))
    );
    let wrapped = gui.text_bounds(&query(hello, Some(60.0), false)).unwrap();
    assert!(wrapped.x <= 60.0 && wrapped.y == 48.0, "{wrapped}");
    assert_eq!(
        gui.text_bounds(&query("A\nB", None, false)).unwrap().y,
        48.0
    );
    let bold = gui.text_bounds(&query("<b>Hello</b>, world!", None, true));
    let raw = gui.text_bounds(&query("<b>Hello</b>, world!", None, false));
    let (bold, raw) = (bold.unwrap().x, raw.unwrap().x);
    assert!(bold > 107.0 && bold < raw, "{bold} vs {raw}");
}

/// Text in a font still on its way has no bounds yet; once the font has
/// come (or failed, leaving the default family), it has.
#[test]
fn text_bounds_wait_for_their_font() {
    let mut gui = Gui::default();
    let mut q = query("Ab", None, false);
    q.font = rook_types::Font {
        family: "rbxassetid://1".into(),
        ..Default::default()
    };
    assert_eq!(gui.text_bounds(&q), None);
    let requests = gui.take_font_requests();
    assert_eq!(requests, ["rbxassetid://1"]);
    assert_eq!(gui.text_bounds(&q), None);
    gui.font_arrived(&requests[0], None);
    assert!(gui.text_bounds(&q).is_some());
}
