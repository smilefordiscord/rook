//! The prompt Roblox shows over everything when a client's connection
//! ends (its disconnect prompt): a title, a line under it, a message and
//! one button, centered on the window. Approximation: laid out after the
//! frames Roblox's `ErrorPrompt` CoreScript module builds (title in
//! Source Sans Semibold 25, message in Source Sans 20, a 36-pixel button,
//! 20 pixels of padding), with sizes and colors not measured from Studio
//! (docs/behavior/characters/leaving.md#the-disconnect-prompt).

use glam::Vec2;
use rook_dom::Dom;
use rook_types::{GuiQuad, Paint, font_from_enum};

use crate::Gui;
use crate::draw::Text;
use crate::layout::{Placed, screen_size};
use crate::text::{self, Carets, Case, Source, Style};

/// What a prompt says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub title: String,
    pub message: String,
    pub button: String,
}

/// The prompt shown, and where it was last drawn.
#[derive(Debug)]
pub(crate) struct Shown {
    prompt: Prompt,
    /// The window size it was laid out for, its quads and its button's
    /// rectangle (top left, bottom right).
    drawn: Option<(Vec2, std::sync::Arc<[GuiQuad]>, [Vec2; 2])>,
}

const MAX_WIDTH: f32 = 400.0;
/// Kept clear on each side of a narrower window.
const MARGIN: f32 = 16.0;
const TITLE_HEIGHT: f32 = 50.0;
const PADDING: f32 = 20.0;
const GAP: f32 = 10.0;
const BUTTON_HEIGHT: f32 = 36.0;
const TITLE_SIZE: f32 = 25.0;
const TEXT_SIZE: f32 = 20.0;
/// Roblox's style colors: Flint (the panel), Pumice (the line and the
/// message), Slate (the button's text) and white.
const FLINT: [f32; 4] = [57.0 / 255.0, 59.0 / 255.0, 61.0 / 255.0, 1.0];
const PUMICE: [f32; 3] = [189.0 / 255.0, 190.0 / 255.0, 190.0 / 255.0];
const SLATE: [f32; 3] = [35.0 / 255.0, 37.0 / 255.0, 39.0 / 255.0];
const WHITE: [f32; 3] = [1.0; 3];

impl Gui {
    /// Show `prompt` over everything, from the next drawing on. Input then
    /// reaches only its button ([`Gui::prompt_button_at`]), and the mouse
    /// is free ([`Gui::modal_shown`]).
    pub fn show_prompt(&mut self, prompt: Prompt) {
        self.prompt = Some(Shown {
            prompt,
            drawn: None,
        });
        self.screen.repaint_prompt();
        self.laid_out += 1;
    }

    /// The window's size changed under the shown prompt: it's centered
    /// again on the next drawing, though no layout runs while it shows.
    pub fn resize_prompt(&mut self) {
        if self.prompt.is_some() {
            self.screen.repaint_prompt();
            self.laid_out += 1;
        }
    }

    pub fn prompt_shown(&self) -> bool {
        self.prompt.is_some()
    }

    /// Whether `at` (pixels from the window's top left) is on the shown
    /// prompt's button, as last drawn.
    pub fn prompt_button_at(&self, at: Vec2) -> bool {
        let button = self.prompt.as_ref().and_then(|s| s.drawn.as_ref());
        button.is_some_and(|(_, _, [min, max])| at.cmpge(*min).all() && at.cmple(*max).all())
    }
}

/// The prompt's quads, laid out again when the window's size changed.
pub(in crate::draw) fn emit(
    dom: &Dom,
    shown: &mut Shown,
    text: &mut Text<'_>,
) -> std::sync::Arc<[GuiQuad]> {
    let screen = screen_size(dom);
    if let Some((size, quads, _)) = &shown.drawn
        && *size == screen
    {
        return quads.clone();
    }
    let mut out = Vec::new();
    let button = lay_out(&shown.prompt, screen, text, &mut out);
    let quads: std::sync::Arc<[GuiQuad]> = out.into();
    shown.drawn = Some((screen, quads.clone(), button));
    quads
}

/// Push the prompt's quads, centered on a `screen`-sized window; returns
/// its button's rectangle.
fn lay_out(
    prompt: &Prompt,
    screen: Vec2,
    text: &mut Text<'_>,
    out: &mut Vec<GuiQuad>,
) -> [Vec2; 2] {
    let width = MAX_WIDTH
        .min(screen.x - 2.0 * MARGIN)
        .max(2.0 * PADDING + 1.0);
    let inner = width - 2.0 * PADDING;
    let message = source(&prompt.message, "SourceSans", TEXT_SIZE, PUMICE, true);
    let message_block = text::lay_out(text.fonts, &message, Vec2::new(inner, f32::MAX), None);
    let message_height = message_block.bounds.y;
    let height = TITLE_HEIGHT + 1.0 + PADDING + message_height + GAP + BUTTON_HEIGHT + PADDING;
    let top_left = ((screen - Vec2::new(width, height)) * 0.5).round();
    let rect = |min: Vec2, size: Vec2, color: [f32; 4], out: &mut Vec<GuiQuad>| {
        out.push(GuiQuad {
            center: top_left + min + size * 0.5,
            size,
            rotation: 0.0,
            color,
            clip: GuiQuad::NO_CLIP,
            paint: Paint::Solid,
            shape: None,
            gradient: None,
        });
    };
    let label = |block: &text::TextBlock,
                 min: Vec2,
                 size: Vec2,
                 text: &mut Text<'_>,
                 out: &mut Vec<GuiQuad>| {
        let placed = Placed {
            center: top_left + min + size * 0.5,
            local: size,
            scale: 1.0,
            rotation: 0.0,
        };
        crate::draw::objects::text::emit(block, (placed, GuiQuad::NO_CLIP), false, None, text, out);
    };
    rect(Vec2::ZERO, Vec2::new(width, height), FLINT, out);
    let title_size = Vec2::new(width, TITLE_HEIGHT);
    let title = source(
        &prompt.title,
        "SourceSansSemibold",
        TITLE_SIZE,
        WHITE,
        false,
    );
    let title_block = text::lay_out(text.fonts, &title, title_size, None);
    label(&title_block, Vec2::ZERO, title_size, text, out);
    let line = [PUMICE[0], PUMICE[1], PUMICE[2], 1.0];
    rect(
        Vec2::new(PADDING, TITLE_HEIGHT),
        Vec2::new(inner, 1.0),
        line,
        out,
    );
    let message_at = Vec2::new(PADDING, TITLE_HEIGHT + 1.0 + PADDING);
    let message_size = Vec2::new(inner, message_height);
    let message_block = text::lay_out(text.fonts, &message, message_size, None);
    label(&message_block, message_at, message_size, text, out);
    let button_at = message_at + Vec2::new(0.0, message_height + GAP);
    let button_size = Vec2::new(inner, BUTTON_HEIGHT);
    rect(button_at, button_size, [1.0; 4], out);
    let caption = source(&prompt.button, "SourceSans", TEXT_SIZE, SLATE, false);
    let caption_block = text::lay_out(text.fonts, &caption, button_size, None);
    label(&caption_block, button_at, button_size, text, out);
    [top_left + button_at, top_left + button_at + button_size]
}

/// Centered text in one of Roblox's legacy fonts.
fn source(text: &str, font: &str, size: f32, color: [f32; 3], wrapped: bool) -> Source {
    Source {
        text: text.to_owned(),
        rich: false,
        style: Style {
            font: font_from_enum(font).unwrap_or_default(),
            size,
            color,
            transparency: 0.0,
            underline: false,
            strike: false,
            stroke: None,
            mark: None,
            case: Case::AsWritten,
        },
        wrapped,
        scaled: false,
        size_limits: None,
        align: Vec2::splat(0.5),
        line_height: 1.0,
        visible: None,
        carets: Carets::None,
    }
}
