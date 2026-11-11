//! The sidebar: the library (worlds, characters, personas), then every
//! saved story, newest first, each with its world's picture; the search and
//! a new story are icons over them, the settings an icon at the foot.

use openrp_core::unix_now;
use winit::window::CursorIcon;

use super::{Chat, Menu, PRIMARY_KEY, Page, ago};
use crate::app::Action;
use crate::library::{Kind, portrait};
use crate::paint::{Painter, Rect, fade, mix};
use crate::theme;
use crate::ui::{Ui, icon, icon_button, id};

/// Height of a library row.
const ROW_H: f32 = 32.0;
/// Height of a story's row.
const STORY_H: f32 = 46.0;
/// Inset of rows from the sidebar's edges.
const INSET: f32 = theme::GAP;

impl Chat {
    pub(super) fn draw_sidebar(&mut self, p: &mut Painter, ui: &mut Ui, area: Rect, actions: &mut Vec<Action>) {
        let t = p.theme;
        let width = area.w - 2.0 * INSET;

        p.label("LIBRARY", theme::CAPTION, INSET + 10.0, area.y + 9.0, t.text_faint);
        let mut y = area.y + 34.0;
        for kind in Kind::ALL {
            let page = Page::Library(kind);
            let glyph = match kind {
                Kind::World => icon::GLOBE,
                Kind::Character => icon::USERS,
                Kind::Persona => icon::DRAMA,
            };
            let row = Rect::new(INSET, y, width, ROW_H);
            if list_row(p, ui, row, (glyph, kind.plural()), self.page == page) {
                self.show(page);
            }
            y += ROW_H + 2.0;
        }

        // The stories' heading, with the search and a new story at its right.
        let top = y + 12.0;
        p.label("STORIES", theme::CAPTION, INSET + 10.0, top + 7.0, t.text_faint);
        let add = Rect::new(area.right() - INSET - 29.0, top, 28.0, 28.0);
        let search = Rect::new(add.x - 30.0, top, 28.0, 28.0);
        // A story starts from a world: a new one (Ctrl/Cmd+N) opens Worlds.
        if tool(p, ui, add, icon::PLUS, &format!("New story  {PRIMARY_KEY}+N"), search.x - 8.0) {
            self.show(Page::Library(Kind::World));
        }
        if tool(p, ui, search, icon::SEARCH, &format!("Search  {PRIMARY_KEY}+K"), search.x - 8.0) {
            self.open_spotlight();
            // The click that opened it is spent: Spotlight would take it
            // for one outside it, and close.
            ui.released = false;
        }
        // The app's own things, as icons at the foot.
        let foot = area.bottom() - INSET - 30.0;
        let settings = Rect::new(INSET, foot, 30.0, 30.0);
        if self.settings_open {
            p.rect(settings, t.active, theme::RADIUS_SM);
        }
        if tool(p, ui, settings, icon::SETTINGS, &format!("Settings  {PRIMARY_KEY}+,"), -(settings.right() + 8.0)) {
            self.toggle_settings();
        }
        let list = Rect::new(0.0, top + 32.0, area.w, foot - 6.0 - top - 32.0);
        self.draw_stories(p, ui, list, actions);
    }

    /// Every saved story in `list`, the open one lit.
    fn draw_stories(&mut self, p: &mut Painter, ui: &mut Ui, list: Rect, actions: &mut Vec<Action>) {
        let t = p.theme;
        let width = list.w - 2.0 * INSET;
        let shown = |c: &&super::Conversation| !c.is_fresh();
        let content_h = self.conversations.iter().filter(shown).count() as f32 * (STORY_H + 2.0);
        let offset = self.sidebar_scroll.update(ui, ui.hovered(list));
        self.sidebar_scroll.finish(ui, content_h, list.h);
        if content_h == 0.0 {
            p.label("No stories yet", theme::SMALL, INSET + 10.0, list.y + 6.0, t.text_faint);
        }

        let clip = p.push_clip(list);
        let now = unix_now();
        let on_chat = self.page == Page::Chat;
        let mut open = None;
        let menu_open = match self.menu {
            Some(Menu::Session(id)) => Some(id),
            _ => None,
        };
        let mut toggle_menu = None;
        let mut y = list.y - offset;
        for conversation in self.conversations.iter().filter(shown) {
            let item = Rect::new(INSET, y, width, STORY_H);
            y += STORY_H + 2.0;
            if item.bottom() < list.y || item.y > list.bottom() {
                continue;
            }
            // Rows scrolled under the list edges must not react.
            let hovered = ui.hovered(item) && list.contains(ui.mouse);
            let hover = ui.anim(id(("conversation", conversation.id)), f32::from(u8::from(hovered)));
            let selected = on_chat && conversation.id == self.current;
            p.rect(item, if selected { t.active } else { fade(t.hover, hover) }, theme::RADIUS);

            // A story nobody named yet is called after its world.
            let world = conversation.world.as_deref().and_then(|w| self.library.get(Kind::World, w));
            let place = world.map(|w| w.name.as_str());
            let (name, about) = match (conversation.title.as_str(), place) {
                ("", Some(place)) => (place, "New story"),
                ("", None) => ("New story", ""),
                (title, place) => (title, place.unwrap_or("")),
            };
            // Its world's picture, as its face.
            let picture = world.and_then(|w| self.library.portrait_path(&w.portrait));
            let face = Rect::new(item.x + 8.0, item.y + 8.0, 30.0, 30.0);
            portrait(p, picture.as_deref(), (place.unwrap_or(name), ""), face, 8.0);

            // Right side: age, or the session's menu (duplicate, delete) while hovered.
            let mut on_control = false;
            let right_w = if hovered || menu_open == Some(conversation.id) {
                let dots = Rect::new(item.right() - 30.0, item.y + 11.0, 24.0, 24.0);
                let over = hovered && dots.contains(ui.mouse);
                if over || menu_open == Some(conversation.id) {
                    p.rect(dots, if selected { t.hover } else { t.active }, theme::RADIUS_SM - 2.0);
                }
                let center = (dots.x + 12.0, dots.y + 12.0);
                icon::draw(p, icon::ELLIPSIS, center, 16.0, if over { t.text } else { t.text_muted });
                on_control = over;
                if over && ui.clicked(dots) {
                    toggle_menu = Some((conversation.id, dots));
                }
                34.0
            } else if conversation.busy() {
                let pulse = 0.5 + 0.5 * (ui.time * 4.0).sin();
                p.rect(Rect::new(item.right() - 17.0, item.y + 12.0, 6.0, 6.0), fade(t.accent, 0.4 + 0.6 * pulse), 3.0);
                24.0
            } else {
                let age = p.layout(&ago(now, conversation.updated), theme::TINY, None);
                p.text(&age, item.right() - 10.0 - age.width(), item.y + 8.0, t.text_faint);
                age.width() + 16.0
            };

            let mut title = p.layout(name, theme::LABEL, None);
            title.truncate(p.fonts, item.w - 48.0 - right_w);
            let bright = if selected { 1.0 } else { 0.4 + 0.6 * hover };
            p.text(&title, item.x + 48.0, item.y + 6.0, mix(t.text_muted, t.text, bright));
            let mut about = p.layout(about, theme::TINY, None);
            about.truncate(p.fonts, item.w - 58.0);
            p.text(&about, item.x + 48.0, item.y + 25.0, t.text_faint);
            if hovered {
                ui.cursor = CursorIcon::Pointer;
                if !on_control && ui.clicked(item) {
                    open = Some(conversation.id);
                }
            }
        }
        // The list fades out at its edges rather than being cut.
        if content_h > list.h {
            let edge = 16.0;
            if offset > 0.5 {
                p.gradient(Rect::new(0.0, list.y, list.w, edge), t.panel, fade(t.panel, 0.0), 0.0);
            }
            p.gradient(Rect::new(0.0, list.bottom() - edge, list.w, edge), fade(t.panel, 0.0), t.panel, 0.0);
        }
        p.set_clip(clip);

        if let Some(id) = open {
            self.open(id, actions);
        }
        if let Some((id, anchor)) = toggle_menu {
            self.menu = if menu_open == Some(id) { None } else { Some(Menu::Session(id)) };
            self.menu_scroll = 0.0;
            self.session_menu = anchor;
        }
    }
}

/// An icon button that says what it does (and its shortcut) beside it
/// while the mouse is over it: `hint` ending at `beside`, or, when that is
/// negative, starting at its absolute value. Returns whether it was clicked.
fn tool(p: &mut Painter, ui: &mut Ui, rect: Rect, glyph: char, hint: &str, beside: f32) -> bool {
    if ui.hovered(rect) {
        let hint = p.layout(hint, theme::TINY, None);
        let x = if beside < 0.0 { -beside } else { beside - hint.width() };
        p.text(&hint, x, rect.y + (rect.h - hint.height()) * 0.5, p.theme.text_faint);
    }
    icon_button(p, ui, rect, glyph, true)
}

/// Draws a library row: an icon and a label. Returns whether it was clicked.
fn list_row(p: &mut Painter, ui: &mut Ui, rect: Rect, (glyph, label): (char, &str), selected: bool) -> bool {
    let t = p.theme;
    let hovered = ui.hovered(rect);
    let hover = ui.anim(id(("library", label)), f32::from(u8::from(hovered)));
    p.rect(rect, if selected { t.active } else { fade(t.hover, hover) }, theme::RADIUS_SM);
    let color = if selected { t.text } else { mix(t.text_muted, t.text, hover) };
    let mid = rect.y + rect.h * 0.5;
    icon::draw(p, glyph, (rect.x + 20.0, mid), 16.0, color);
    let layout = p.layout(label, theme::SMALL, None);
    p.text(&layout, rect.x + 36.0, mid - layout.height() * 0.5, color);
    if hovered {
        ui.cursor = CursorIcon::Pointer;
    }
    ui.clicked(rect)
}
