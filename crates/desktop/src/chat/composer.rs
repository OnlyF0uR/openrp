//! The composer: the text field (with input-method preedit), the toolbar
//! with OOC, Write for me (the AI writes the user's message; see
//! `impersonate.rs`) and send, and the slash commands it completes. The
//! story's model and effort are picked in its Direction.
//!
//! What is out of character (an instruction to the AI rather than something
//! the user's character says) shows grey, as it does in the story once
//! sent. Two ways mark it, which combine. Text typed in parentheses is,
//! anywhere in a message (`tools::asides`). And OOC marks the rest of the
//! message, from the caret (the editor's mark keeps where it starts), or
//! all of it when nothing follows the mark; with text selected, it puts
//! that in parentheses instead. It stays
//! on after sending (the next message is all out of character) until pressed
//! off, so dictating and pressing send is enough.

use winit::window::CursorIcon;

use super::{Chat, tools};
use crate::app::Action;
use crate::paint::{Painter, Rect, fade, mix};
use crate::text::Run;
use crate::theme;
use crate::ui::{Ui, icon, id};

/// Composer grows up to this many lines, then scrolls.
const MAX_LINES: usize = 8;

/// A command typed as `/name` in the composer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Command {
    /// Deletes the open chat and starts over.
    Clear,
    /// Opens the story's author's note.
    Note,
    /// Opens the story's memories.
    Memory,
    /// Has the AI add what the latest turns taught the story to its memories.
    Memorize,
    /// Copies the story exactly.
    Duplicate,
    /// Copies the story's setup without what happened.
    Frame,
    /// Has the AI write the user's next message.
    Impersonate,
    /// Has a chat's character carry on their last reply.
    Continue,
}

impl Command {
    const ALL: [Self; 8] = [
        Self::Continue,
        Self::Impersonate,
        Self::Note,
        Self::Memory,
        Self::Memorize,
        Self::Duplicate,
        Self::Frame,
        Self::Clear,
    ];

    /// What follows the slash.
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Note => "note",
            Self::Memory => "memory",
            Self::Memorize => "memorize",
            Self::Duplicate => "duplicate",
            Self::Frame => "frame",
            Self::Impersonate => "impersonate",
            Self::Continue => "continue",
        }
    }

    /// What it does, shown next to it.
    pub(super) fn detail(self) -> &'static str {
        match self {
            Self::Clear => "Delete this story and start it over",
            Self::Note => "Direction for the whole story: creativity, length and a note",
            Self::Memory => "What the story remembers",
            Self::Memorize => "Add the latest turns to the memories",
            Self::Duplicate => "Copy this story exactly",
            Self::Frame => "New story with this cast and setup",
            Self::Impersonate => "The AI writes your next message",
            Self::Continue => "They carry on their last reply",
        }
    }

    /// Commands completing `text`: a slash and part of a name, nothing else.
    pub(super) fn matching(text: &str) -> Vec<Self> {
        let Some(typed) = text.strip_prefix('/') else {
            return Vec::new();
        };
        if typed.contains(char::is_whitespace) {
            return Vec::new();
        }
        Self::ALL.into_iter().filter(|c| c.name().starts_with(typed)).collect()
    }
}

impl Chat {
    /// The OOC button: turns out of character on from the caret, or off;
    /// with text selected (and it off), puts that in parentheses instead,
    /// which makes it an aside.
    pub(super) fn toggle_ooc(&mut self) {
        let composer = &mut self.composer;
        if composer.mark().is_some() {
            composer.set_mark(None);
        } else if composer.selection().is_empty() {
            composer.set_mark(Some(composer.cursor()));
        } else {
            // Its own word, so the parentheses read as an aside's.
            let (text, range) = (composer.text(), composer.selection());
            let picked = &text[range.clone()];
            let (lead, trail) = (picked.len() - picked.trim_start().len(), picked.trim_end().len());
            let glued = lead == 0 && text[..range.start].chars().next_back().is_some_and(|c| !c.is_whitespace());
            let space = if glued { " " } else { "" };
            let aside = format!("{}{space}({}){}", &picked[..lead], picked.trim(), &picked[trail..]);
            composer.insert(&aside);
        }
    }

    /// Draws the composer. Returns its top edge and its card, which the
    /// command menu opens over.
    pub(super) fn draw_composer(
        &mut self,
        p: &mut Painter,
        ui: &mut Ui,
        main: Rect,
        actions: &mut Vec<Action>,
    ) -> (f32, Rect) {
        let t = p.theme;
        let (x, width) = Self::column(main);
        let pad = 16.0;
        let top_pad = 16.0;
        let toolbar_h = 50.0;
        let text_w = width - 2.0 * pad;

        // With an input method composing, show its text at the caret.
        let cursor = self.composer.cursor();
        let (shown, preedit) = match &self.preedit {
            Some((text, _)) => {
                let mut shown = self.composer.text().to_owned();
                shown.insert_str(cursor, text);
                (shown, Some(cursor..cursor + text.len()))
            }
            None => (self.composer.text().to_owned(), None),
        };
        // Out of character: from the OOC mark on (all of it when nothing
        // follows), and what is in parentheses before that.
        let ooc = self.composer.mark().map(|mark| {
            // The input method's text shifts what follows the caret.
            let mark = match &preedit {
                Some(range) if mark > cursor => mark + range.len(),
                _ => mark,
            };
            let rest = shown.get(mark..).unwrap_or_default();
            if rest.trim().is_empty() { 0..shown.len() } else { mark..shown.len() }
        });
        let said = ooc.as_ref().map_or(shown.as_str(), |rest| shown.get(..rest.start).unwrap_or_default());
        let spans: Vec<_> = tools::asides(said)
            .into_iter()
            .chain(ooc.clone().filter(|rest| !rest.is_empty()))
            .map(|range| (range, Run { ink: 1, ..Run::plain(theme::BODY.font) }))
            .collect();
        let layout = p.layout_rich(&shown, &spans, theme::BODY, Some(text_w));
        let line_h = layout.line_height();
        let visible_h = layout.line_count().min(MAX_LINES) as f32 * line_h;

        let card_h = top_pad + visible_h + toolbar_h;
        let card = Rect::new(x, main.bottom() - 16.0 - card_h, width, card_h);
        let text_area = Rect::new(card.x + pad, card.y + top_pad, text_w, visible_h);

        // Keep the caret inside the visible part of a tall draft.
        let caret_byte = preedit.as_ref().map_or(cursor, |r| {
            let ime_cursor = self.preedit.as_ref().and_then(|(_, c)| *c).map_or(r.end - r.start, |(_, end)| end);
            r.start + ime_cursor
        });
        let (caret_x, caret_y) = layout.caret(caret_byte);
        self.composer_scroll = self
            .composer_scroll
            .clamp(caret_y + line_h - visible_h, caret_y)
            .clamp(0.0, (layout.height() - visible_h).max(0.0));
        let origin = (text_area.x, text_area.y - self.composer_scroll);
        self.caret_rect = Some(Rect::new(origin.0 + caret_x, origin.1 + caret_y, 2.0, line_h));

        // Mouse: click to place the caret, drag to select.
        let hit = |ui: &Ui| layout.hit(ui.mouse.0 - origin.0, ui.mouse.1 - origin.1);
        let text_zone = Rect::new(card.x, text_area.y - 4.0, card.w, text_area.h + 8.0);
        if ui.hovered(text_zone) {
            ui.cursor = CursorIcon::Text;
            if ui.pressed && preedit.is_none() {
                self.composer.set_cursor(hit(ui), ui.mods.shift_key());
                if ui.clicks == 2 {
                    self.composer.select_word();
                }
                self.selecting = true;
                ui.last_edit = ui.time;
            }
        }
        if self.selecting {
            if ui.down && ui.clicks < 2 {
                self.composer.set_cursor(hit(ui), true);
            } else if !ui.down {
                self.selecting = false;
            }
        }

        // Spotlight, or the story's title being typed, has the keys instead.
        let typing = ui.focused && self.spotlight.is_none() && self.title_edit.is_none();
        let focus = ui.anim(id("composer-focus"), f32::from(u8::from(typing)));
        let lift = Rect::new(card.x + 8.0, card.y + 10.0, card.w - 16.0, card.h);
        p.shadow(lift, fade(t.shadow, 0.8), theme::RADIUS_LG, 28.0);
        p.bordered(card, t.surface, theme::RADIUS_LG, 1.0, mix(t.border_strong, t.border_focus, focus * 0.6));

        let clip = p.push_clip(Rect::new(text_area.x - 2.0, text_area.y, text_area.w + 4.0, text_area.h));
        let selection = self.composer.selection();
        if !selection.is_empty() && preedit.is_none() {
            // A selected line break shows as a small tail.
            for (x0, x1, y) in layout.selection_spans(selection.start, selection.end, false) {
                p.rect(Rect::new(origin.0 + x0, origin.1 + y, x1 - x0, line_h), t.selection, 2.0);
            }
        }
        if shown.is_empty() && ooc.is_some() {
            let invite = "Out of character: tell the AI what you want";
            p.label(invite, theme::BODY, origin.0, origin.1, t.text_faint);
        } else if shown.is_empty() {
            let conversation = self.current();
            let invite = match conversation.chat_partner() {
                Some(partner) => format!("Message {}", partner.name),
                None if conversation.world.is_some() => "What do you do?".to_owned(),
                None => "Message OpenRP".to_owned(),
            };
            let width = p.label(&invite, theme::BODY, origin.0, origin.1, t.text_faint);
            let commands = p.layout("/ for commands", theme::BODY, None);
            p.text(&commands, origin.0 + width + 12.0, origin.1, fade(t.text_faint, 0.55));
        } else {
            p.rich(&layout, origin.0, origin.1, &[t.text, t.text_muted], t.hover);
        }
        // The input method's text is underlined until committed.
        if let Some(range) = &preedit {
            for (start, end, y) in layout.line_spans() {
                let (from, to) = (range.start.max(start), range.end.min(end));
                if from < to {
                    let (x0, x1) = (layout.caret(from).0, layout.caret(to).0);
                    p.rect(Rect::new(origin.0 + x0, origin.1 + y + line_h - 5.0, x1 - x0, 1.5), t.accent, 0.0);
                }
            }
        }
        if ui.caret_visible() && (selection.is_empty() || preedit.is_some()) && typing {
            p.rect(Rect::new(origin.0 + caret_x - 1.0, origin.1 + caret_y + 3.0, 2.0, line_h - 6.0), t.accent, 0.0);
        }
        p.set_clip(clip);

        // Toolbar: OOC on the left; Write for me and send/stop on the right.
        let item_y = card.bottom() - 10.0 - 30.0;
        let busy = self.current().busy();
        let ready = !self.composer.text().trim().is_empty();
        let send = Rect::new(card.right() - 10.0 - 30.0, item_y, 30.0, 30.0);

        // OOC: what follows the caret (or all of it) is for the AI.
        let marked = self.composer.mark().is_some();
        let text = p.layout("OOC", theme::SMALL, None);
        let ooc_button = Rect::new(card.x + 8.0, item_y, text.width() + 40.0, 30.0);
        {
            let hovered = ui.hovered(ooc_button);
            let hover = ui.anim(id("ooc"), f32::from(u8::from(hovered)));
            let on = ui.anim(id("ooc-on"), f32::from(u8::from(marked)));
            p.rect(ooc_button, mix(fade(t.hover, hover), fade(t.accent, 0.16 + 0.06 * hover), on), theme::RADIUS_SM);
            let color = mix(mix(t.text_muted, t.text, hover), t.accent, on);
            icon::draw(p, icon::PARENTHESES, (ooc_button.x + 18.0, item_y + 15.0), 15.0, color);
            p.text(&text, ooc_button.x + 31.0, item_y + (30.0 - text.height()) * 0.5, color);
            if hovered {
                ui.cursor = CursorIcon::Pointer;
                if ui.clicked(ooc_button) {
                    self.toggle_ooc();
                    ui.last_edit = ui.time;
                }
            }
        }

        // Write for me: the AI writes the user's message, from their draft.
        let writing = self.impersonating();
        if writing || self.can_impersonate() {
            let label = if writing { "Writing…" } else { "Write for me" };
            let text = p.layout(label, theme::SMALL, None);
            let rect = Rect::new(send.x - 6.0 - text.width() - 42.0, item_y, text.width() + 42.0, 30.0);
            // Only where the toolbar has room for it.
            if rect.x > ooc_button.right() + 8.0 {
                let hovered = ui.hovered(rect);
                let hover = ui.anim(id("impersonate"), f32::from(u8::from(hovered)));
                p.rect(rect, fade(t.hover, hover), theme::RADIUS_SM);
                let color = if writing {
                    ui.animating = true;
                    mix(t.text_faint, t.text, ((ui.time * 2.6).sin() * 0.5 + 0.5) * 0.75)
                } else {
                    mix(t.text_muted, t.text, hover)
                };
                icon::draw(p, icon::WAND, (rect.x + 19.0, item_y + 15.0), 15.0, color);
                p.text(&text, rect.x + 32.0, item_y + (30.0 - text.height()) * 0.5, color);
                if hovered {
                    ui.cursor = CursorIcon::Pointer;
                    if ui.clicked(rect) {
                        self.impersonate(actions);
                    }
                }
            }
        }
        let hovered = ui.hovered(send) && (busy || ready);
        let hover = ui.anim(id("send"), f32::from(u8::from(hovered)));
        let center = (send.x + 15.0, send.y + 15.0);
        if busy {
            p.rect(send, mix(t.hover, t.active, hover), 15.0);
            p.rect(Rect::new(send.x + 10.0, send.y + 10.0, 10.0, 10.0), t.text, 2.5);
        } else if ready {
            p.rect(send, fade(t.accent, 1.0 - 0.12 * hover), 15.0);
            icon::draw(p, icon::ARROW_UP, center, 17.0, t.on_accent);
        } else {
            p.rect(send, t.hover, 15.0);
            icon::draw(p, icon::ARROW_UP, center, 17.0, t.text_faint);
        }
        if hovered {
            ui.cursor = CursorIcon::Pointer;
            if ui.clicked(send) {
                if busy {
                    self.stop(actions);
                } else {
                    self.send(actions);
                }
            }
        }

        self.composer_layout = if preedit.is_none() { Some((layout, origin)) } else { None };
        (card.y, card)
    }
}

#[cfg(test)]
mod tests {
    use super::Command;

    #[test]
    fn commands_complete_a_bare_slash_word() {
        assert_eq!(Command::matching("/").len(), Command::ALL.len());
        assert_eq!(Command::matching("/d"), [Command::Duplicate]);
        assert_eq!(Command::matching("/im"), [Command::Impersonate]);
        assert_eq!(Command::matching("/cl"), [Command::Clear]);
        assert_eq!(Command::matching("/clear"), [Command::Clear]);
        assert!(Command::matching("/clearer").is_empty());
        assert!(Command::matching("/clear it").is_empty(), "text after a command makes it a message");
        assert!(Command::matching("/etc/hosts\n").is_empty());
        assert!(Command::matching("clear").is_empty());
        assert!(Command::matching("").is_empty());
    }
}
