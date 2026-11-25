//! A chat's character, where a chat shows them. A chat has no side panel:
//! its character is introduced atop the conversation (portrait, name, who
//! they are, and links to edit them and who the user is), and what directs
//! the chat sits in the header: Memories, Direction and a new chat with them.

use winit::window::CursorIcon;

use super::{Chat, Load};
use crate::app::Action;
use crate::library::{Kind, portrait};
use crate::paint::{Painter, Rect, fade, mix};
use crate::text::{Align, FontId, Style};
use crate::theme;
use crate::ui::{Ui, icon, id};

/// Side of the character's portrait in the introduction.
const FACE: f32 = 76.0;
/// Most lines of who they are the introduction shows.
const ABOUT_LINES: f32 = 3.0;
/// Widest the introduction's text runs.
const ABOUT_W: f32 = 520.0;
/// Height of the links under it.
const LINKS_H: f32 = 26.0;
/// Space under the introduction, before the first message.
const BELOW: f32 = 36.0;
/// The character's name in the introduction.
const NAME: Style = Style { font: FontId::SERIF_BOLD, size: 22.0, line_height: 1.25 };

/// Who a chat is with, as its introduction shows them.
pub(super) struct Intro {
    name: String,
    color: String,
    /// Their portrait's path, if they have one.
    path: Option<String>,
    /// Who they are, on one line (the layout wraps it).
    about: String,
    /// Who the user is in the chat.
    player: String,
}

/// What a click on the introduction asks for.
#[derive(Clone, Copy)]
pub(super) enum IntroClick {
    /// Edit this chat's copy of the character.
    Partner,
    /// Edit who the user is.
    Player,
}

impl Chat {
    /// The open chat's introduction; `None` outside a chat.
    pub(super) fn intro(&self) -> Option<Intro> {
        let conversation = self.conversations.iter().find(|c| c.id == self.current)?;
        let partner = conversation.chat_partner()?;
        let path = (!partner.portrait.is_empty()).then(|| self.library.portrait_path(&partner.portrait)).flatten();
        Some(Intro {
            name: partner.name.clone(),
            color: partner.color.clone(),
            path,
            about: partner.description.split_whitespace().collect::<Vec<_>>().join(" "),
            player: conversation.player.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
        })
    }

    /// Does what a click on the introduction asked for.
    pub(super) fn intro_clicked(&mut self, click: IntroClick) {
        match click {
            IntroClick::Partner => self.edit_member(0),
            IntroClick::Player => self.edit_player(),
        }
    }

    /// Draws the header's actions of the open chat, ending at `right` in
    /// `bar`: Memories, Direction and a new chat with its character, and
    /// does what was clicked. Returns where they start; `None` outside a
    /// chat.
    pub(super) fn draw_chat_actions(
        &mut self,
        p: &mut Painter,
        ui: &mut Ui,
        (bar, right): (Rect, f32),
        actions: &mut Vec<Action>,
    ) -> Option<f32> {
        let conversation = self.conversations.iter().find(|c| c.id == self.current)?;
        let partner = conversation.chat_partner().filter(|_| conversation.load == Load::Loaded)?.id.clone();
        let memories = match (conversation.memories.len(), conversation.reviewing.is_some()) {
            (0, false) => "Memories".to_owned(),
            (0, true) => "Memories …".to_owned(),
            (n, false) => format!("Memories · {n}"),
            (n, true) => format!("Memories · {n} …"),
        };
        // Someone no longer in the library can only go on in this chat.
        let kept = self.library.get(Kind::Character, &partner).is_some();
        let mut x = right;
        let mut clicked = None;
        let buttons =
            [(kept, icon::NEW_CHAT, "New chat"), (true, icon::SLIDERS, "Direction"), (true, icon::BRAIN, &memories)];
        for (k, (shown, glyph, label)) in buttons.into_iter().enumerate() {
            if !shown {
                continue;
            }
            let (rect, hit) = header_button(p, ui, (bar, x), glyph, label);
            x = rect.x - 4.0;
            if hit {
                clicked = Some(k);
            }
        }
        match clicked {
            Some(0) => self.chat_with(&partner, true, actions),
            Some(1) => self.edit_direction(),
            Some(_) => self.edit_memories(),
            None => {}
        }
        Some(x)
    }
}

/// Draws a quiet header button, an icon and a label, ending at `right` in
/// `bar`. Returns where it is and whether it was clicked.
fn header_button(p: &mut Painter, ui: &mut Ui, (bar, right): (Rect, f32), glyph: char, label: &str) -> (Rect, bool) {
    let t = p.theme;
    let text = p.layout(label, theme::SMALL, None);
    let rect = Rect::new(right - text.width() - 40.0, bar.y + (bar.h - 30.0) * 0.5, text.width() + 40.0, 30.0);
    let hovered = ui.hovered(rect);
    let hover = ui.anim(id(("chat-action", label.split(' ').next().unwrap_or(label))), f32::from(u8::from(hovered)));
    p.rect(rect, fade(t.hover, hover), theme::RADIUS_SM);
    let color = mix(t.text_muted, t.text, hover);
    icon::draw(p, glyph, (rect.x + 19.0, rect.y + 15.0), 15.0, color);
    p.text(&text, rect.x + 32.0, rect.y + (30.0 - text.height()) * 0.5, color);
    if hovered {
        ui.cursor = CursorIcon::Pointer;
    }
    (rect, hovered && ui.clicked(rect))
}

/// Height of `intro` laid out `width` wide, the space under it included.
pub(super) fn intro_height(p: &Painter, width: f32, intro: &Intro) -> f32 {
    let wrap = width.min(ABOUT_W);
    let name = p.layout(&intro.name, NAME, Some(wrap));
    let about = if intro.about.is_empty() { 0.0 } else { about_height(p, &intro.about, wrap) + 8.0 };
    FACE + 16.0 + name.height() + about + 12.0 + LINKS_H + BELOW
}

/// Height of who they are, at most [`ABOUT_LINES`].
fn about_height(p: &Painter, about: &str, wrap: f32) -> f32 {
    let layout = p.layout(about, theme::SMALL, Some(wrap));
    layout.height().min(layout.line_height() * ABOUT_LINES)
}

/// Draws `intro` at `y`, centred in the column from `x`, `width` wide:
/// their portrait ringed in their colour, their name, who they are, and
/// links to edit them and who the user is. Returns what was clicked.
pub(super) fn draw_intro(
    p: &mut Painter,
    ui: &mut Ui,
    (x, y, width): (f32, f32, f32),
    intro: &Intro,
    interactive: bool,
) -> Option<IntroClick> {
    let t = p.theme;
    let cx = (x + width * 0.5).round();
    let wrap = width.min(ABOUT_W);
    let ink = theme::character_color(&t, &intro.name, &intro.color);
    let face = Rect::new(cx - FACE * 0.5, y, FACE, FACE);
    p.shadow(Rect::new(face.x, face.y + 6.0, FACE, FACE), fade(ink, 0.22), FACE * 0.5, 22.0);
    portrait(p, intro.path.as_deref(), (&intro.name, &intro.color), face, FACE * 0.5);
    let ring = Rect::new(face.x - 3.0, face.y - 3.0, FACE + 6.0, FACE + 6.0);
    p.bordered(ring, [0.0; 4], ring.w * 0.5, 1.5, fade(ink, 0.8));
    let mut y = face.bottom() + 16.0;
    let name = p.layout(&intro.name, NAME, Some(wrap));
    p.text_aligned(&name, cx - name.width() * 0.5, y, Align::Center, name.width(), t.text);
    y += name.height();
    if !intro.about.is_empty() {
        y += 8.0;
        let about = p.layout(&intro.about, theme::SMALL, Some(wrap));
        let shown = about_height(p, &intro.about, wrap);
        let saved = p.push_clip(Rect::new(cx - wrap * 0.5, y, wrap, shown));
        p.text_aligned(&about, cx - about.width() * 0.5, y, Align::Center, about.width(), t.text_muted);
        p.set_clip(saved);
        y += shown;
    }
    y += 12.0;

    // Edit them · who the user is.
    let first = intro.name.split_whitespace().next().unwrap_or(&intro.name);
    let edit = format!("Edit {first}");
    let you = if intro.player.is_empty() { "Who are you?".to_owned() } else { format!("You are {}", intro.player) };
    // Two links, a dot between them, centred.
    let (edit_w, you_w) =
        (p.layout(&edit, theme::SMALL, None).width() + 12.0, p.layout(&you, theme::SMALL, None).width() + 12.0);
    let left = (cx - (edit_w + 16.0 + you_w) * 0.5).round();
    p.rect(Rect::new(left + edit_w + 6.5, y + LINKS_H * 0.5 - 1.5, 3.0, 3.0), t.text_faint, 1.5);
    let links = [
        (Rect::new(left, y, edit_w, LINKS_H), edit, IntroClick::Partner),
        (Rect::new(left + edit_w + 16.0, y, you_w, LINKS_H), you, IntroClick::Player),
    ];
    let mut clicked = None;
    for (rect, label, click) in links {
        let hovered = interactive && ui.hovered(rect);
        let hover = ui.anim(id(("intro-link", rect.x.to_bits())), f32::from(u8::from(hovered)));
        p.rect(rect, fade(t.hover, hover), theme::RADIUS_SM);
        let text = p.layout(&label, theme::SMALL, None);
        p.text(&text, rect.x + 6.0, rect.y + (LINKS_H - text.height()) * 0.5, mix(t.text_faint, t.text, hover));
        if hovered {
            ui.cursor = CursorIcon::Pointer;
            if ui.clicked(rect) {
                clicked = Some(click);
            }
        }
    }
    clicked
}
