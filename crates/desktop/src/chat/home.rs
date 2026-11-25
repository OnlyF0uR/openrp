//! Home: what a new conversation shows before it is anything. Two ways in,
//! side by side: chat one-on-one with a character (their portraits, the
//! favourites and those chatted with lately first; a click opens the latest
//! chat with them or starts one), or play a story in a world with a whole
//! cast (its pictures; a click starts one). Each row ends in its library
//! page, and an empty library offers to create (or import) the first.

use std::collections::HashMap;

use winit::window::CursorIcon;

use super::{Chat, PRIMARY_KEY, Page};
use crate::app::Action;
use crate::library::{Kind, Record, portrait};
use crate::paint::{Painter, Rect, fade, mix};
use crate::text::{Align, Style};
use crate::theme;
use crate::ui::{Ui, icon, id, keycap, logo};

/// Widest the home column gets.
const WIDTH: f32 = 820.0;
/// Space between tiles.
const TILE_GAP: f32 = 12.0;
/// Narrowest a character's tile gets; they stand tall.
const PERSON_W: f32 = 118.0;
/// A character tile's height for its width.
const PERSON_ASPECT: f32 = 1.32;
/// Narrowest a world's tile gets; they lie wide.
const WORLD_W: f32 = 200.0;
/// A world tile's height for its width.
const WORLD_ASPECT: f32 = 0.56;
/// Height of the name band at a tile's foot.
const BAND: f32 = 34.0;

/// What a click on the home screen asks for.
enum Click {
    /// Chat with this library character.
    Chat(String),
    /// Start a story in this world.
    Play(String),
    /// Show this library page.
    Library(Kind),
    /// Open a new form on this library page.
    Create(Kind),
    /// Read character cards from other apps.
    Import,
}

/// One tile of a row: a record, or the way to its whole page.
enum Tile<'a> {
    Record(&'a Record, Kind),
    More(Kind, usize),
}

/// The tiles of a row of `kind` with room for `columns`: the `records` that
/// fit, and when some do not (or there are none) one leading to the rest.
fn row(records: Vec<&Record>, kind: Kind, columns: usize) -> Vec<Tile<'_>> {
    let total = records.len();
    let shown = if total > columns { columns - 1 } else { total };
    let mut tiles: Vec<Tile<'_>> = records.into_iter().take(shown).map(|r| Tile::Record(r, kind)).collect();
    if total > shown || total == 0 {
        tiles.push(Tile::More(kind, total));
    }
    tiles
}

impl Chat {
    /// Draws the home screen in `view`, and does what was clicked.
    pub(super) fn draw_home(&mut self, p: &mut Painter, ui: &mut Ui, view: Rect, actions: &mut Vec<Action>) {
        let t = p.theme;
        let width = (view.w - 64.0).min(WIDTH);
        let x = view.x + ((view.w - width) * 0.5).round();
        let cx = x + width * 0.5;
        let characters = self.chat_candidates();
        let mut worlds: Vec<&Record> = self.library.list(Kind::World).iter().collect();
        worlds.sort_by_key(|w| !w.favorite);

        // One row of each, as many tiles as fit (the last leads to the rest).
        let fit = |min: f32| ((width + TILE_GAP) / (min + TILE_GAP)).floor().max(1.0) as usize;
        let (person_cols, world_cols) = (fit(PERSON_W).min(6), fit(WORLD_W).min(4));
        let people = row(characters, Kind::Character, person_cols);
        let places = row(worlds, Kind::World, world_cols);
        let person_w = ((width - TILE_GAP * (person_cols as f32 - 1.0)) / person_cols as f32).floor();
        let world_w = ((width - TILE_GAP * (world_cols as f32 - 1.0)) / world_cols as f32).floor();
        let person_h = (person_w * PERSON_ASPECT).round();
        let world_h = (world_w * WORLD_ASPECT).round();

        // Centred in the view while it fits; from the top when it does not.
        let content_h = 36.0 + 20.0 + 34.0 + 26.0 + 2.0 * (44.0 + 30.0) + person_h + world_h + 56.0;
        let mut y = view.y + ((view.h - content_h) * 0.45).max(24.0).round();
        logo(p, Rect::new(cx - 18.0, y, 36.0, 36.0));
        y += 36.0 + 20.0;
        let title = p.layout("Who's it going to be?", theme::DISPLAY, None);
        p.text_aligned(&title, cx - title.width() * 0.5, y, Align::Center, title.width(), t.text);
        y += title.height() + 6.0;
        let blurb = "Chat one-on-one with a character, or play a story in a world with a whole cast.";
        let blurb = p.layout(blurb, theme::SMALL, Some(width));
        p.text_aligned(&blurb, cx - blurb.width() * 0.5, y, Align::Center, blurb.width(), t.text_muted);
        y += blurb.height() + 34.0;

        let mut click = None;
        y = heading(p, (icon::MESSAGE, "Chat with someone"), "Just the two of you, in their own voice", (x, y, width));
        for (k, tile) in people.iter().enumerate() {
            let rect = Rect::new(x + k as f32 * (person_w + TILE_GAP), y, person_w, person_h);
            click = click.or(self.tile(p, ui, rect, tile));
        }
        y += person_h + 30.0;
        y = heading(p, (icon::BOOK, "Play a story"), "A world, a cast, and you in the middle of it", (x, y, width));
        for (k, tile) in places.iter().enumerate() {
            let rect = Rect::new(x + k as f32 * (world_w + TILE_GAP), y, world_w, world_h);
            click = click.or(self.tile(p, ui, rect, tile));
        }
        y += world_h + 36.0;

        // The shortcuts, in a line, for whoever is new here.
        let shortcuts = [
            ("Search", format!("{PRIMARY_KEY}+K")),
            ("Home", format!("{PRIMARY_KEY}+N")),
            ("Settings", format!("{PRIMARY_KEY}+,")),
        ];
        // Each: its label, then its keys in a cap 12 wider than their text.
        let widths: Vec<(f32, f32)> = shortcuts
            .iter()
            .map(|(label, keys)| {
                (p.layout(label, theme::SMALL, None).width(), p.layout(keys, theme::TINY, None).width() + 12.0)
            })
            .collect();
        let total: f32 = widths.iter().map(|(label, cap)| label + 8.0 + cap).sum::<f32>() + 28.0 * 2.0;
        let mut sx = (cx - total * 0.5).round();
        for ((label, keys), (label_w, cap_w)) in shortcuts.iter().zip(widths) {
            let text = p.layout(label, theme::SMALL, None);
            p.text(&text, sx, y + (20.0 - text.height()) * 0.5, t.text_faint);
            keycap(p, keys, sx + label_w + 8.0 + cap_w, y);
            sx += label_w + 8.0 + cap_w + 28.0;
        }

        match click {
            Some(Click::Chat(character)) => self.chat_with(&character, false, actions),
            Some(Click::Play(world)) => self.play(world),
            Some(Click::Library(kind)) => self.show(Page::Library(kind)),
            Some(Click::Create(kind)) => {
                self.show(Page::Library(kind));
                self.library.new_form(kind);
            }
            Some(Click::Import) => {
                self.show(Page::Library(Kind::Character));
                actions.push(Action::ImportCards);
            }
            None => {}
        }
    }

    /// The library's characters as the home screen offers them: favourites,
    /// then those chatted with most lately, then the rest as listed.
    fn chat_candidates(&self) -> Vec<&Record> {
        let mut last: HashMap<String, u64> = HashMap::new();
        for conversation in self.conversations.iter().filter(|c| !c.is_fresh()) {
            if let Some(partner) = conversation.face() {
                let at = last.entry(partner.id).or_default();
                *at = (*at).max(conversation.updated);
            }
        }
        let mut characters: Vec<&Record> = self.library.list(Kind::Character).iter().collect();
        characters.sort_by_key(|c| (!c.favorite, std::cmp::Reverse(last.get(&c.id).copied().unwrap_or(0))));
        characters
    }

    /// Draws `tile` in `rect`; returns what clicking it asks for.
    fn tile(&self, p: &mut Painter, ui: &mut Ui, rect: Rect, tile: &Tile<'_>) -> Option<Click> {
        let t = p.theme;
        let radius = theme::RADIUS + 2.0;
        let key = match tile {
            Tile::Record(record, _) => id(("home", &record.id)),
            Tile::More(kind, _) => id(("home-more", kind.plural())),
        };
        let hovered = ui.hovered(rect);
        let hover = ui.anim(key, f32::from(u8::from(hovered)));
        // Hovered, a tile lifts off the page.
        let card = Rect::new(rect.x, rect.y - (2.0 * hover).round(), rect.w, rect.h);
        if hover > 0.0 {
            p.shadow(Rect::new(card.x, card.y + 6.0, card.w, card.h), fade(t.shadow, hover), radius, 16.0);
        }
        let click = match tile {
            Tile::Record(record, kind) => {
                let path = self.library.portrait_path(&record.portrait).filter(|_| !record.portrait.is_empty());
                portrait(p, path.as_deref(), (&record.name, &record.color), card, radius);
                let band = Rect::new(card.x, card.bottom() - BAND, card.w, BAND);
                let saved = p.push_clip(band);
                p.rect(Rect::new(band.x, band.y - radius, band.w, band.h + radius), t.scrim, radius);
                p.set_clip(saved);
                let mut name = p.layout(&record.name, theme::LABEL, None);
                name.truncate(p.fonts, card.w - 20.0);
                p.text(&name, band.x + 10.0, band.y + (BAND - name.height()) * 0.5, t.on_scrim);
                let ink =
                    if record.favorite { t.accent } else { theme::character_color(&t, &record.name, &record.color) };
                let border = mix(t.border, ink, 0.25 + 0.5 * hover);
                p.bordered(card, [0.0; 4], radius, 1.0, border);
                if record.favorite {
                    p.label("★", Style::regular(13.0), card.right() - 22.0, card.y + 8.0, t.accent);
                }
                if *kind == Kind::World { Click::Play(record.id.clone()) } else { Click::Chat(record.id.clone()) }
            }
            Tile::More(kind, total) => {
                p.bordered(
                    card,
                    mix(t.surface, t.hover, hover * 0.6),
                    radius,
                    1.0,
                    mix(t.border, t.border_strong, hover),
                );
                let glyph = if *total == 0 { icon::PLUS } else { icon::ELLIPSIS };
                let mid = card.y + card.h * 0.5;
                icon::draw(p, glyph, (card.x + card.w * 0.5, mid - 12.0), 18.0, mix(t.text_muted, t.text, hover));
                let label = match (kind, total) {
                    (Kind::Character, 0) => "New character".to_owned(),
                    (Kind::World, 0) => "New world".to_owned(),
                    (_, total) => format!("All {total}"),
                };
                let text = p.layout(&label, theme::SMALL, None);
                p.text(&text, card.x + (card.w - text.width()) * 0.5, mid + 4.0, t.text_muted);
                match (kind, total) {
                    (_, 0) => Click::Create(*kind),
                    _ => Click::Library(*kind),
                }
            }
        };
        if hovered {
            ui.cursor = CursorIcon::Pointer;
            if ui.clicked(rect) {
                return Some(click);
            }
        }
        // With no characters yet, cards from other apps are one click away.
        if let Tile::More(Kind::Character, 0) = tile {
            let link = p.layout("or import a card", theme::TINY, None);
            let at =
                Rect::new(rect.x + (rect.w - link.width()) * 0.5 - 4.0, rect.bottom() + 6.0, link.width() + 8.0, 18.0);
            let over = ui.hovered(at);
            p.text(&link, at.x + 4.0, at.y + (at.h - link.height()) * 0.5, if over { t.text } else { t.text_faint });
            if over {
                ui.cursor = CursorIcon::Pointer;
                if ui.clicked(at) {
                    return Some(Click::Import);
                }
            }
        }
        None
    }
}

/// Draws a row's heading at `(x, y)` across `width`: an icon and a title,
/// and what it is for, faint, at the right. Returns where the row starts.
fn heading(p: &mut Painter, (glyph, title): (char, &str), about: &str, (x, y, width): (f32, f32, f32)) -> f32 {
    let t = p.theme;
    icon::draw(p, glyph, (x + 8.0, y + 10.0), 16.0, t.text_muted);
    let label = p.layout(title, theme::HEADING, None);
    p.text(&label, x + 26.0, y + (20.0 - label.height()) * 0.5, t.text);
    let mut about = p.layout(about, theme::SMALL, None);
    about.truncate(p.fonts, (width - label.width() - 48.0).max(0.0));
    p.text(&about, x + width - about.width(), y + (20.0 - about.height()) * 0.5, t.text_faint);
    y + 20.0 + 14.0
}
