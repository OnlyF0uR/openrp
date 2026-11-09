//! The sidebar: a new story, search, worlds, characters and personas, every
//! saved session, and settings.

use openrp_core::unix_now;
use winit::window::CursorIcon;

use super::{Chat, Menu, PRIMARY_KEY, Page, ago};
use crate::app::Action;
use crate::library::Kind;
use crate::paint::{Painter, Rect, fade, mix};
use crate::theme;
use crate::ui::{Ui, icon, id};

/// Height of a row: navigation and sessions alike.
const ROW_H: f32 = 32.0;
/// Inset of rows from the sidebar's edges.
const INSET: f32 = 10.0;

impl Chat {
    pub(super) fn draw_sidebar(&mut self, p: &mut Painter, ui: &mut Ui, area: Rect, actions: &mut Vec<Action>) {
        let t = p.theme;
        let width = area.w - 2.0 * INSET;

        // A story starts from a world: New story (Ctrl/Cmd+N) opens Worlds.
        let new = Rect::new(INSET, area.y + 2.0, width, 36.0);
        if new_story(p, ui, new) {
            self.show(Page::Library(Kind::World));
        }

        let mut row_y = new.bottom() + 10.0;
        let row = |y: f32| Rect::new(INSET, y, width, ROW_H);
        let search = (icon::SEARCH, "Search", Some(format!("{PRIMARY_KEY}+K")));
        if list_row(p, ui, row(row_y), id("search"), search, false) {
            self.open_spotlight();
            // The click that opened it is spent: Spotlight would take it
            // for one outside it, and close.
            ui.released = false;
        }
        row_y += ROW_H + 2.0;
        let on_chat = self.page == Page::Chat;
        for kind in Kind::ALL {
            let page = Page::Library(kind);
            let glyph = match kind {
                Kind::World => icon::GLOBE,
                Kind::Character => icon::USERS,
                Kind::Persona => icon::DRAMA,
            };
            if list_row(
                p,
                ui,
                row(row_y),
                id(("library", kind.plural())),
                (glyph, kind.plural(), None),
                self.page == page,
            ) {
                self.show(page);
            }
            row_y += ROW_H + 2.0;
        }

        let list_top = row_y + 18.0;
        p.label("Recent", theme::CAPTION, INSET + 10.0, list_top, t.text_faint);
        let settings = Rect::new(INSET, area.bottom() - 12.0 - ROW_H, width, ROW_H);
        let list = Rect::new(0.0, list_top + 22.0, area.w, settings.y - 8.0 - list_top - 22.0);
        let shown = |c: &&super::Conversation| !c.is_fresh();
        let content_h = self.conversations.iter().filter(shown).count() as f32 * (ROW_H + 2.0);
        if ui.hovered(list) {
            self.sidebar_scroll += ui.scroll;
        }
        self.sidebar_scroll = self.sidebar_scroll.clamp(0.0, (content_h - list.h).max(0.0));
        if content_h == 0.0 {
            p.label("No stories yet", theme::SMALL, INSET + 10.0, list.y + 6.0, t.text_faint);
        }

        let clip = p.push_clip(list);
        let now = unix_now();
        let mut open = None;
        let menu_open = match self.menu {
            Some(Menu::Session(id)) => Some(id),
            _ => None,
        };
        let mut toggle_menu = None;
        let mut y = list.y - self.sidebar_scroll;
        for conversation in self.conversations.iter().filter(shown) {
            let item = Rect::new(INSET, y, width, ROW_H);
            y += ROW_H + 2.0;
            if item.bottom() < list.y || item.y > list.bottom() {
                continue;
            }
            // Rows scrolled under the list edges must not react.
            let hovered = ui.hovered(item) && list.contains(ui.mouse);
            let hover = ui.anim(id(("conversation", conversation.id)), f32::from(u8::from(hovered)));
            let selected = on_chat && conversation.id == self.current;
            p.rect(item, if selected { t.active } else { fade(t.hover, hover) }, theme::RADIUS_SM);

            // Right side: age, or the session's menu (duplicate, delete) while hovered.
            let mut on_control = false;
            let right_w = if hovered || menu_open == Some(conversation.id) {
                let dots = Rect::new(item.right() - 28.0, item.y + 4.0, 24.0, 24.0);
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
                32.0
            } else if conversation.busy() {
                let pulse = 0.5 + 0.5 * (ui.time * 4.0).sin();
                p.rect(
                    Rect::new(item.right() - 16.0, item.y + ROW_H * 0.5 - 3.0, 6.0, 6.0),
                    fade(t.accent, 0.4 + 0.6 * pulse),
                    3.0,
                );
                24.0
            } else {
                let age = p.layout(&ago(now, conversation.updated), theme::TINY, None);
                p.text(&age, item.right() - 10.0 - age.width(), item.y + (ROW_H - age.height()) * 0.5, t.text_faint);
                age.width() + 18.0
            };

            // A story nobody wrote in yet is named after its world.
            let world =
                conversation.world.as_deref().and_then(|w| self.library.get(Kind::World, w)).map(|w| w.name.as_str());
            let name = if conversation.title.is_empty() { world.unwrap_or("New story") } else { &conversation.title };
            let mut title = p.layout(name, theme::SMALL, None);
            title.truncate(p.fonts, item.w - 12.0 - right_w);
            let color = if selected { t.text } else { mix(t.text_muted, t.text, hover) };
            p.text(&title, item.x + 10.0, item.y + (ROW_H - title.height()) * 0.5, color);
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
            if self.sidebar_scroll > 0.5 {
                p.gradient(Rect::new(0.0, list.y, area.w, edge), t.panel, fade(t.panel, 0.0), 0.0);
            }
            p.gradient(Rect::new(0.0, list.bottom() - edge, area.w, edge), fade(t.panel, 0.0), t.panel, 0.0);
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

        let entry = (icon::SETTINGS, "Settings", Some(format!("{PRIMARY_KEY}+,")));
        if list_row(p, ui, settings, id("settings"), entry, self.page == Page::Settings) {
            self.toggle_settings();
        }
    }
}

/// The sidebar's first button, New story with its shortcut. Returns
/// whether it was clicked.
fn new_story(p: &mut Painter, ui: &mut Ui, rect: Rect) -> bool {
    let t = p.theme;
    let hovered = ui.hovered(rect);
    let hover = ui.anim(id("new-story"), f32::from(u8::from(hovered)));
    p.bordered(rect, mix(t.surface, t.hover, hover), theme::RADIUS, 1.0, t.border_strong);
    let mid = rect.y + rect.h * 0.5;
    icon::draw(p, icon::PLUS, (rect.x + 20.0, mid), 16.0, t.text);
    let label = p.layout("New story", theme::LABEL, None);
    p.text(&label, rect.x + 36.0, mid - label.height() * 0.5, t.text);
    let hint = p.layout(&format!("{PRIMARY_KEY}+N"), theme::TINY, None);
    p.text(&hint, rect.right() - 12.0 - hint.width(), mid - hint.height() * 0.5, t.text_faint);
    if hovered {
        ui.cursor = CursorIcon::Pointer;
    }
    ui.clicked(rect)
}

/// Draws a list row: an icon, a label and an optional right-hand hint.
/// Returns whether it was clicked.
fn list_row(
    p: &mut Painter,
    ui: &mut Ui,
    rect: Rect,
    key: u64,
    (glyph, label, hint): (char, &str, Option<String>),
    selected: bool,
) -> bool {
    let t = p.theme;
    let hovered = ui.hovered(rect);
    let hover = ui.anim(key, f32::from(u8::from(hovered)));
    p.rect(rect, if selected { t.active } else { fade(t.hover, hover) }, theme::RADIUS_SM);
    let mut reserved = 12.0;
    if let Some(hint) = hint {
        // Shortcuts show only on hover: the sidebar stays quiet.
        let hint = p.layout(&hint, theme::TINY, None);
        reserved = hint.width() + 24.0;
        let y = rect.y + (rect.h - hint.height()) * 0.5;
        p.text(&hint, rect.right() - 10.0 - hint.width(), y, fade(t.text_faint, hover));
    }
    let color = if selected { t.text } else { mix(t.text_muted, t.text, hover) };
    let mid = rect.y + rect.h * 0.5;
    icon::draw(p, glyph, (rect.x + 20.0, mid), 16.0, color);
    let mut layout = p.layout(label, theme::SMALL, None);
    layout.truncate(p.fonts, rect.w - 36.0 - reserved);
    p.text(&layout, rect.x + 36.0, mid - layout.height() * 0.5, color);
    if hovered {
        ui.cursor = CursorIcon::Pointer;
    }
    ui.clicked(rect)
}
