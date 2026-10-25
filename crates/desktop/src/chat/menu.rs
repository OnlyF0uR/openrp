//! Drop-down menus: model and reasoning (above the composer toolbar),
//! slash commands (above the composer), what to do with a cast member
//! (beside their row's dots in the cast panel) and with a session
//! (duplicate, export or delete, below its sidebar row's dots), the
//! background model (from the settings page) and, on the library pages, the
//! open form's ⋯ (duplicate, export, delete).
//!
//! Pickers are menus shown as a sheet over the window, for choosing among
//! people and worlds by their faces and notes: the library characters to
//! add to the cast (it stays open to add several; its footer creates or
//! generates someone instead), the persona to play as, and the world a
//! character's Play starts a story in.
//!
//! Long lists (characters, worlds, personas, models) are searched: typing
//! while one is open narrows it (names first, then comments, tags and
//! descriptions), the arrows move through what is left and Enter picks; Esc
//! clears the search, then closes.

use arboard::Clipboard;
use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::CursorIcon;

use super::{Chat, Menu, MenuItem, Page, Reasoning, group_digits, price};
use crate::app::Action;
use crate::library::{FormAction, Kind, portrait};
use crate::paint::{Painter, Rect, fade, mix};
use crate::text::TextLayout;
use crate::theme;
use crate::ui::{ButtonStyle, Ui, button, edit_key, id};

/// Height of a menu's row.
const ROW_H: f32 = 30.0;
/// Side of a row's portrait.
const FACE: f32 = 22.0;
/// Height of a picker's row.
const PICK_H: f32 = 70.0;
/// Side of a portrait in a picker.
const PICK_FACE: f32 = 46.0;
/// Widest a picker gets.
const PICKER_W: f32 = 640.0;

/// What an open menu shows.
#[derive(Default)]
struct Content {
    /// What it opens next to.
    anchor: Rect,
    width: f32,
    /// It opens below its anchor, else above.
    below: bool,
    /// Its title (a searched menu's placeholder) and a note on the right.
    header: (String, String),
    items: Vec<MenuItem>,
    /// Shown while it has no rows.
    empty: String,
    /// A picker's title and what it is for.
    title: String,
    blurb: String,
    /// A picker's buttons along its foot; the last is the main one.
    footer: Vec<&'static str>,
}

/// What was clicked in a picker.
enum Picked {
    /// This row (in the rows shown).
    Row(usize),
    /// This footer button.
    Footer(usize),
    /// Outside it: it closes.
    Close,
}

impl Menu {
    /// Whether it lists what may be many (characters, worlds, personas,
    /// models), searched by typing.
    pub(super) fn searchable(self) -> bool {
        matches!(self, Self::Model | Self::UtilityModel | Self::CastLibrary | Self::PlayIn | Self::Personas)
    }

    /// Whether it shows as a picker over the window rather than a drop-down.
    pub(super) fn picker(self) -> bool {
        matches!(self, Self::CastLibrary | Self::PlayIn | Self::Personas)
    }
}

/// The rows of `items` holding every word of `query` (in any case), as
/// indices into it: those whose label does first, then those matching with
/// their detail and search text. All of them, in order, for no query.
fn search(items: &[MenuItem], query: &str) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return (0..items.len()).collect();
    }
    let holds = |text: &str| {
        let text = text.to_lowercase();
        words.iter().all(|w| text.contains(w.as_str()))
    };
    let (mut named, mut other) = (Vec::new(), Vec::new());
    for (index, item) in items.iter().enumerate() {
        if holds(&item.label) {
            named.push(index);
        } else if holds(&format!("{} {} {}", item.label, item.detail, item.search)) {
            other.push(index);
        }
    }
    named.extend(other);
    named
}

impl Chat {
    /// What `menu` shows now; `anchors` holds the model and reasoning
    /// buttons, the composer card and the cast strip's add button or open
    /// member's dots.
    fn menu_content(&mut self, menu: Menu, anchors: [Rect; 4]) -> Content {
        let content = |anchor, width, below, header: (&str, &str), items| Content {
            anchor,
            width,
            below,
            header: (header.0.to_owned(), header.1.to_owned()),
            items,
            empty: "Loading…".to_owned(),
            ..Content::default()
        };
        // Some providers list no models: their names are typed.
        let models = |count: usize| match count {
            0 => "Type a model's name".to_owned(),
            n => format!("Search {} models", group_digits(n as u64)),
        };
        let typed = self.typed_model().map(|id| MenuItem::new(&format!("Use “{id}”"), "Typed model"));
        match menu {
            Menu::Model => {
                let items = self
                    .models
                    .iter()
                    .map(|m| MenuItem {
                        selected: m.id == self.model,
                        search: m.id.clone(),
                        ..MenuItem::new(model_label(m), &price(m))
                    })
                    .chain(typed)
                    .collect();
                let mut content =
                    content(anchors[0], 360.0, false, (&models(self.models.len()), "Input / output per 1M"), items);
                if let Some(error) = &self.models_error {
                    content.empty = format!("{error} Trying again soon.");
                }
                content
            }
            Menu::Reasoning => {
                let in_use = self.reasoning_in_use();
                let items = Reasoning::choices(self.selected_model())
                    .into_iter()
                    .map(|r| MenuItem { selected: r == in_use, ..MenuItem::new(r.label(), r.detail()) })
                    .collect();
                content(anchors[1], 280.0, false, ("Reasoning effort", ""), items)
            }
            Menu::Commands => {
                let items =
                    self.commands().into_iter().map(|c| MenuItem::new(&format!("/{}", c.name()), c.detail())).collect();
                content(anchors[2], 320.0, false, ("Commands", "Tab to complete"), items)
            }
            Menu::CastLibrary => {
                let items = self.library_menu();
                let placeholder = format!("Search {} characters", group_digits(items.len() as u64));
                let mut content = content(anchors[3], PICKER_W, true, (&placeholder, ""), items);
                "Add to the cast".clone_into(&mut content.title);
                "Each character you pick joins the scene at once; pick as many as you like."
                    .clone_into(&mut content.blurb);
                content.empty = if self.library.list(Kind::Character).is_empty() {
                    "Your library has no characters yet. Create one here, have the AI generate one, or import cards on the \
                     Characters page."
                        .to_owned()
                } else {
                    "Everyone in your library is in this story.".to_owned()
                };
                content.footer = Self::library_menu_footer();
                content
            }
            Menu::Member(index) => {
                let title = self.current().cast.get(index).map(|m| m.name.clone()).unwrap_or_default();
                let items = self.member_menu(index);
                content(anchors[3], 260.0, true, (&title, ""), items)
            }
            Menu::Session(_) => {
                let items = vec![
                    MenuItem::new("Duplicate", "Exact copy"),
                    MenuItem::new("Duplicate frame", "Cast and setup only"),
                    MenuItem::new("Export as text", "Readable transcript"),
                    MenuItem::new("Export as JSONL", "SillyTavern chat"),
                    MenuItem::new("Delete", ""),
                ];
                content(self.session_menu, 280.0, true, ("Session", ""), items)
            }
            Menu::Record => {
                let stories = self.world_stories().len();
                let items = self
                    .library
                    .form_menu()
                    .into_iter()
                    .map(|action| match action {
                        FormAction::Duplicate => MenuItem::new("Duplicate", "Saves this first"),
                        FormAction::Export => MenuItem::new("Export card…", "PNG or JSON"),
                        FormAction::Delete => match stories {
                            0 => MenuItem::new("Delete", ""),
                            1 => MenuItem::new("Delete", "With its story"),
                            n => MenuItem::new("Delete", &format!("With its {n} stories")),
                        },
                    })
                    .collect();
                content(self.menu_anchor, 260.0, true, ("Options", ""), items)
            }
            Menu::PlayIn => {
                let worlds = self.library.list(Kind::World);
                let placeholder = format!("Search {} worlds", group_digits(worlds.len() as u64));
                let items: Vec<MenuItem> = worlds.iter().map(MenuItem::record).collect();
                let mut content = content(self.menu_anchor, PICKER_W, true, (&placeholder, ""), items);
                let who = self.library.get(Kind::Character, &self.play_character).map_or("them", |c| c.name.as_str());
                content.title = format!("Play {who} in…");
                "The world the story happens in. They start in its first scene.".clone_into(&mut content.blurb);
                "No worlds yet: make one first.".clone_into(&mut content.empty);
                content.footer = vec!["New world…", "Cancel"];
                content
            }
            Menu::Personas => {
                let chosen = self.player_choice();
                let items: Vec<MenuItem> = self
                    .persona_choices()
                    .into_iter()
                    .map(|r| MenuItem { selected: chosen.as_deref() == Some(r.id.as_str()), ..MenuItem::record(r) })
                    .collect();
                let placeholder = format!("Search {} personas", group_digits(items.len() as u64));
                let mut content = content(self.menu_anchor, PICKER_W, true, (&placeholder, ""), items);
                "Play as".clone_into(&mut content.title);
                "Your personas. The note under each tells apart those with the same name."
                    .clone_into(&mut content.blurb);
                "No personas yet.".clone_into(&mut content.empty);
                content.footer = vec!["Cancel"];
                content
            }
            Menu::UtilityModel => {
                let same =
                    MenuItem { selected: self.utility_model.is_none(), ..MenuItem::new("Same as the story", "") };
                let items = std::iter::once(same)
                    .chain(self.models.iter().map(|m| MenuItem {
                        selected: self.utility_model.as_deref() == Some(m.id.as_str()),
                        search: m.id.clone(),
                        ..MenuItem::new(model_label(m), &price(m))
                    }))
                    .chain(typed)
                    .collect();
                content(self.utility_anchor, 360.0, true, (&models(self.models.len()), "Background model"), items)
            }
        }
    }

    /// Draws the open menu (if any) and applies a choice. `anchors`: see
    /// [`Chat::menu_content`].
    pub(super) fn draw_open_menu(
        &mut self,
        p: &mut Painter,
        ui: &mut Ui,
        anchors: [Rect; 4],
        actions: &mut Vec<Action>,
    ) {
        let Some(menu) = self.menu else {
            (self.menu_rect, self.menu_seen) = (None, None);
            return;
        };
        // A menu opens with nothing searched.
        let fresh = self.menu_seen != Some(menu);
        if fresh {
            self.menu_seen = Some(menu);
            self.menu_query.take();
            (self.menu_pick, self.menu_keyed) = (0, false);
        }
        let content = self.menu_content(menu, anchors);
        let rows = if menu.searchable() {
            search(&content.items, self.menu_query.text())
        } else {
            (0..content.items.len()).collect()
        };
        if menu.picker() {
            match self.draw_picker(p, ui, &content, &rows, fresh) {
                Some(Picked::Row(row)) => {
                    ui.released = false;
                    self.choose(menu, rows[row], actions);
                }
                Some(Picked::Footer(button)) => {
                    ui.released = false;
                    self.footer_picked(menu, button);
                }
                Some(Picked::Close) => self.close_menu(),
                None => {}
            }
            return;
        }
        if let Some(row) = self.draw_menu(p, ui, &content, &rows, menu.searchable()) {
            ui.released = false;
            self.choose(menu, rows[row], actions);
            return;
        }
        let inside = self.menu_rect.is_some_and(|r| r.contains(ui.press_pos)) || content.anchor.contains(ui.press_pos);
        if ui.released && !inside {
            self.close_menu();
        }
    }

    /// Keys for an open searched menu: typing searches, the arrows move,
    /// Enter picks, Esc clears the search and then closes. Returns `false`
    /// for keys it does not use.
    pub(super) fn menu_key(
        &mut self,
        event: &KeyEvent,
        mods: ModifiersState,
        cb: &mut Option<Clipboard>,
        actions: &mut Vec<Action>,
    ) -> bool {
        let Some(menu) = self.menu.filter(|m| m.searchable()) else { return false };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) if !self.menu_query.text().is_empty() => {
                self.menu_query.take();
                (self.menu_pick, self.menu_keyed, self.menu_scroll) = (0, false, 0.0);
            }
            Key::Named(NamedKey::Escape) => self.close_menu(),
            Key::Named(NamedKey::Enter) => {
                let content = self.menu_content(menu, [Rect::default(); 4]);
                let rows = search(&content.items, self.menu_query.text());
                if let Some(&index) = rows.get(self.menu_pick) {
                    self.choose(menu, index, actions);
                }
            }
            Key::Named(key @ (NamedKey::ArrowUp | NamedKey::ArrowDown)) => {
                let content = self.menu_content(menu, [Rect::default(); 4]);
                let count = search(&content.items, self.menu_query.text()).len();
                if count > 0 {
                    let step = if *key == NamedKey::ArrowUp { count - 1 } else { 1 };
                    // The first press lands on the first row.
                    self.menu_pick = if self.menu_keyed { (self.menu_pick.min(count - 1) + step) % count } else { 0 };
                    (self.menu_keyed, self.menu_follow) = (true, true);
                }
            }
            _ => {
                let before = self.menu_query.text().to_owned();
                if !edit_key(&mut self.menu_query, event, mods, cb) {
                    return false;
                }
                if self.menu_query.text() != before {
                    (self.menu_pick, self.menu_scroll) = (0, 0.0);
                    self.menu_keyed = !self.menu_query.text().trim().is_empty();
                }
            }
        }
        true
    }

    /// What is typed into a model menu's search, unless a listed model has
    /// that id: the last row uses it, for models the provider does not list.
    fn typed_model(&self) -> Option<String> {
        let typed = self.menu_query.text().trim();
        (!typed.is_empty() && !self.models.iter().any(|m| m.id == typed)).then(|| typed.to_owned())
    }

    /// Picks row `index` of `menu` (before searching).
    fn choose(&mut self, menu: Menu, index: usize, actions: &mut Vec<Action>) {
        self.menu = None;
        match menu {
            Menu::Model => {
                if let Some(model) = self.models.get(index).map(|m| m.id.clone()).or_else(|| self.typed_model()) {
                    actions.push(Action::SelectModel(model.clone()));
                    self.model = model;
                }
            }
            Menu::Reasoning => {
                if let Some(choice) = Reasoning::choices(self.selected_model()).get(index) {
                    self.reasoning = *choice;
                    actions.push(Action::SetReasoning(self.reasoning));
                }
            }
            Menu::Commands => {
                if let Some(command) = self.commands().get(index) {
                    self.run_command(*command, actions);
                }
            }
            Menu::CastLibrary => self.library_menu_picked(index, actions),
            Menu::Member(member) => self.member_menu_picked(member, index, actions),
            Menu::Session(id) => match index {
                0 | 1 => self.duplicate(id, index == 1, actions),
                2 | 3 => self.export(id, index == 3, actions),
                _ => self.delete_conversation(id, actions),
            },
            Menu::Record => {
                let picked = self.library.form_menu().get(index).copied();
                if let Some(world) = picked.and_then(|action| self.library.form_menu_picked(action, actions)) {
                    self.delete_world_stories(&world, actions);
                }
            }
            Menu::PlayIn => {
                if let Some(world) = self.library.list(Kind::World).get(index).map(|w| w.id.clone()) {
                    self.play_character_in(world, actions);
                }
            }
            Menu::Personas => self.play_as(index),
            Menu::UtilityModel => {
                let model = match index {
                    0 => None,
                    i => self.models.get(i - 1).map(|m| m.id.clone()).or_else(|| self.typed_model()),
                };
                self.utility_model.clone_from(&model);
                actions.push(Action::SetUtilityModel(model));
            }
        }
    }

    /// Applies footer button `button` of the picker `menu`.
    fn footer_picked(&mut self, menu: Menu, button: usize) {
        match menu {
            Menu::CastLibrary => self.library_footer_picked(button),
            Menu::PlayIn if button == 0 => {
                self.menu = None;
                self.show(Page::Library(Kind::World));
                self.library.new_form(Kind::World);
            }
            _ => self.close_menu(),
        }
    }

    /// Draws `content` as a picker over the whole window, `rows` of its
    /// items under a search field: each a face, a name, the note telling it
    /// apart and what it is about. A click outside it closes it, unless it
    /// opened (`fresh`) with that click.
    fn draw_picker(
        &mut self,
        p: &mut Painter,
        ui: &mut Ui,
        content: &Content,
        rows: &[usize],
        fresh: bool,
    ) -> Option<Picked> {
        let t = p.theme;
        let view = p.clip();
        // Everything beneath is covered: nothing there reacts.
        self.menu_rect = Some(view);
        p.rect(view, fade(t.scrim, 0.7), 0.0);
        let width = content.width.min(view.w - 48.0);
        let height = (view.h - 96.0).clamp(320.0, 720.0);
        let card = Rect::new(
            view.x + ((view.w - width) * 0.5).round(),
            view.y + ((view.h - height) * 0.42).round().max(24.0),
            width,
            height,
        );
        p.shadow(Rect::new(card.x, card.y + 12.0, card.w, card.h), t.shadow, theme::RADIUS + 4.0, 32.0);
        p.bordered(card, t.panel, theme::RADIUS + 4.0, 1.0, t.border_strong);

        // Title, what it is for, and the search.
        let (x, inner) = (card.x + 24.0, card.w - 48.0);
        let mut y = card.y + 22.0;
        let mut title = p.layout(&content.title, theme::TITLE, None);
        title.truncate(p.fonts, inner);
        p.text(&title, x, y, t.text);
        y += title.height() + 4.0;
        let blurb = p.layout(&content.blurb, theme::SMALL, Some(inner));
        p.text(&blurb, x, y, t.text_muted);
        y += blurb.height() + 16.0;
        let field = Rect::new(x, y, inner, 40.0);
        p.bordered(field, t.surface, theme::RADIUS, 1.0, t.border_focus);
        let query = self.menu_query.text();
        let count = if query.is_empty() {
            String::new()
        } else {
            format!("{} of {}", group_digits(rows.len() as u64), group_digits(content.items.len() as u64))
        };
        let count = p.layout(&count, theme::SMALL, None);
        p.text(&count, field.right() - 12.0 - count.width(), field.y + (field.h - count.height()) * 0.5, t.text_faint);
        let field_w = field.w - 36.0 - count.width();
        let (text, color) = if query.is_empty() { (content.header.0.as_str(), t.text_faint) } else { (query, t.text) };
        let mut layout = p.layout(text, theme::BODY, None);
        let caret_x = if query.is_empty() { 0.0 } else { layout.caret(self.menu_query.cursor()).0 };
        layout.truncate(p.fonts, field_w);
        let origin = (field.x + 12.0, field.y + (field.h - layout.height()) * 0.5);
        p.text(&layout, origin.0, origin.1, color);
        if ui.caret_visible() && caret_x <= field_w {
            let caret = Rect::new(origin.0 + caret_x - 1.0, origin.1 + 2.0, 2.0, layout.line_height() - 4.0);
            p.rect(caret, t.accent, 0.0);
        }
        y = field.bottom() + 12.0;

        // The footer's buttons, right to left, the main one last.
        let footer_y = card.bottom() - 60.0;
        p.rect(Rect::new(card.x, footer_y, card.w, 1.0), t.border, 0.0);
        let mut picked = None;
        let mut right = card.right() - 24.0;
        for (k, label) in content.footer.iter().enumerate().rev() {
            let main = k + 1 == content.footer.len();
            let button_w = (p.layout(label, theme::LABEL, None).width() + 28.0).max(if main { 96.0 } else { 0.0 });
            right -= button_w;
            let style = if main { ButtonStyle::Primary } else { ButtonStyle::Ghost };
            if button(p, ui, Rect::new(right, footer_y + 13.0, button_w, 34.0), label, style, true) {
                picked = Some(Picked::Footer(k));
            }
            right -= 8.0;
        }
        let shortcut = p.layout("↑↓ to move, Enter to pick, Esc to close", theme::TINY, None);
        if shortcut.width() < right - x {
            p.text(&shortcut, x, footer_y + 30.0 - shortcut.height() * 0.5, t.text_faint);
        }

        // The rows, scrolled.
        let list = Rect::new(card.x + 12.0, y, card.w - 24.0, footer_y - 8.0 - y);
        if rows.is_empty() {
            let empty = if content.items.is_empty() { content.empty.as_str() } else { "Nothing matches." };
            let text = p.layout(empty, theme::SMALL, Some(inner));
            p.text(&text, x, list.y + 12.0, t.text_muted);
        }
        let content_h = rows.len() as f32 * PICK_H;
        if ui.hovered(list) {
            self.menu_scroll += ui.scroll;
            ui.scroll = 0.0;
        }
        if std::mem::take(&mut self.menu_follow) {
            let top = self.menu_pick as f32 * PICK_H;
            self.menu_scroll = self.menu_scroll.min(top).max(top + PICK_H - list.h);
        }
        self.menu_scroll = self.menu_scroll.clamp(0.0, (content_h - list.h).max(0.0));
        let clip = p.push_clip(list);
        for (i, &index) in rows.iter().enumerate() {
            let row = Rect::new(list.x, list.y + i as f32 * PICK_H - self.menu_scroll, list.w, PICK_H - 4.0);
            if row.bottom() < list.y || row.y > list.bottom() {
                continue;
            }
            let item = &content.items[index];
            let hovered = ui.hovered(row) && list.contains(ui.mouse);
            let keyed = self.menu_keyed && i == self.menu_pick;
            let hover = ui.anim(
                id(("picker", &content.title, &item.image, &item.label, i)),
                f32::from(u8::from(hovered || keyed)),
            );
            p.rect(row, fade(t.hover, hover), theme::RADIUS);
            if item.selected {
                p.bordered(row, fade(t.accent, 0.08), theme::RADIUS, 1.0, fade(t.accent, 0.6));
            }
            let face = Rect::new(row.x + 12.0, row.y + (row.h - PICK_FACE) * 0.5, PICK_FACE, PICK_FACE);
            let path = self.library.portrait_path(&item.image);
            portrait(p, path.as_deref(), (&item.label, &item.color), face, PICK_FACE * 0.5);
            let ink = theme::character_color(&t, &item.label, &item.color);
            let ring = Rect::new(face.x - 3.0, face.y - 3.0, face.w + 6.0, face.h + 6.0);
            p.bordered(ring, [0.0; 4], ring.w * 0.5, 1.5, fade(ink, 0.35 + 0.5 * hover));

            // Name (starred favourites), the note telling it apart, what it is about.
            let text_x = face.right() + 16.0;
            let right = row.right() - if item.selected { 40.0 } else { 14.0 };
            let width = (right - text_x).max(24.0);
            let mut name = p.layout(&item.label, theme::LABEL, None);
            name.truncate(p.fonts, width - if item.favorite { 20.0 } else { 0.0 });
            let mut note = p.layout(&item.detail, theme::SMALL, None);
            note.truncate(p.fonts, width);
            let mut about = p.layout(&item.summary, theme::TINY, None);
            about.truncate(p.fonts, width);
            let lines = [(&name, 0.0), (&note, 1.0), (&about, 1.0)];
            let block: f32 = lines.iter().filter(|(l, _)| l.width() > 0.0).map(|(l, gap)| l.height() + gap).sum();
            let mut line_y = row.y + ((row.h - block) * 0.5).round();
            p.text(&name, text_x, line_y, t.text);
            if item.favorite {
                p.label("★", theme::SMALL, text_x + name.width() + 6.0, line_y - 1.0, t.accent);
            }
            line_y += name.height();
            if note.width() > 0.0 {
                p.text(&note, text_x, line_y + 1.0, mix(t.text_muted, t.text, hover * 0.4));
                line_y += note.height() + 1.0;
            }
            if about.width() > 0.0 {
                p.text(&about, text_x, line_y + 1.0, t.text_faint);
            }
            if item.selected {
                p.label_centered("✓", theme::LABEL, Rect::new(row.right() - 36.0, row.y, 24.0, row.h), t.accent);
            }
            if hovered {
                ui.cursor = CursorIcon::Pointer;
                if ui.clicked(row) {
                    picked = Some(Picked::Row(i));
                }
            }
        }
        p.set_clip(clip);
        if picked.is_none() && !fresh && ui.released && !card.contains(ui.press_pos) && !card.contains(ui.mouse) {
            picked = Some(Picked::Close);
        }
        picked
    }

    /// Draws `content` as a menu next to its anchor (below it, or above),
    /// showing `rows` of its items, and a search field above them when
    /// `searched`. Returns the clicked row (in `rows`).
    fn draw_menu(
        &mut self,
        p: &mut Painter,
        ui: &mut Ui,
        content: &Content,
        rows: &[usize],
        searched: bool,
    ) -> Option<usize> {
        let t = p.theme;
        let (header_h, pad) = (if searched { 38.0 } else { 30.0 }, 4.0);
        let content_h = rows.len().max(1) as f32 * ROW_H;
        let view_h = p.clip().bottom();
        let anchor = content.anchor;
        // It opens on the side asked for unless the other has more room for
        // it, and scrolls when the window is too short for every row.
        let needed = header_h + content_h + 2.0 * pad;
        let (room_below, room_above) = (view_h - anchor.bottom() - 16.0, anchor.y - 16.0);
        let below = if content.below {
            room_below >= needed || room_below >= room_above
        } else {
            room_above < needed && room_below > room_above
        };
        let room = if below { room_below } else { room_above };
        let height = needed.min(room.max(header_h + ROW_H * 3.0));
        let y = if below { anchor.bottom() + 6.0 } else { anchor.y - 6.0 - height };
        // Kept inside the window when the anchor sits near its right edge.
        let x = anchor.x.min(p.clip().right() - content.width - 8.0).max(8.0);
        let area = Rect::new(x, y, content.width, height);
        self.menu_rect = Some(area);

        p.shadow(Rect::new(area.x, area.y + 6.0, area.w, area.h), t.shadow, theme::RADIUS, 18.0);
        p.bordered(area, t.surface, theme::RADIUS, 1.0, t.border_strong);
        if searched {
            // The search: what is typed while the menu is open lands here.
            let query = self.menu_query.text();
            let note = if query.is_empty() { content.header.1.clone() } else { group_digits(rows.len() as u64) };
            let note = p.layout(&note, theme::CAPTION, None);
            p.text(&note, area.right() - 12.0 - note.width(), area.y + (header_h - note.height()) * 0.5, t.text_faint);
            let field_w = area.w - 36.0 - note.width();
            let (text, color) =
                if query.is_empty() { (content.header.0.as_str(), t.text_faint) } else { (query, t.text) };
            let mut layout = p.layout(text, theme::SMALL, None);
            let caret_x = if query.is_empty() { 0.0 } else { layout.caret(self.menu_query.cursor()).0 };
            layout.truncate(p.fonts, field_w);
            let origin = (area.x + 12.0, area.y + (header_h - layout.height()) * 0.5);
            p.text(&layout, origin.0, origin.1, color);
            if ui.caret_visible() && caret_x <= field_w {
                p.rect(
                    Rect::new(origin.0 + caret_x - 1.0, origin.1 + 2.0, 2.0, layout.line_height() - 4.0),
                    t.accent,
                    0.0,
                );
            }
        } else {
            p.label(&content.header.0, theme::CAPTION, area.x + 12.0, area.y + 9.0, t.text_faint);
            let right = p.layout(&content.header.1, theme::CAPTION, None);
            p.text(&right, area.right() - 12.0 - right.width(), area.y + 9.0, t.text_faint);
        }
        p.rect(Rect::new(area.x, area.y + header_h, area.w, 1.0), t.border, 0.0);

        let list = Rect::new(area.x, area.y + header_h + pad, area.w, area.h - header_h - 2.0 * pad);
        if rows.is_empty() {
            let empty = if content.items.is_empty() { content.empty.clone() } else { "Nothing matches.".to_owned() };
            let mut text = p.layout(&empty, theme::SMALL, None);
            text.truncate(p.fonts, list.w - 24.0);
            p.text(&text, list.x + 12.0, list.y + 7.0, t.text_muted);
            return None;
        }
        if ui.hovered(area) {
            self.menu_scroll += ui.scroll;
            ui.scroll = 0.0;
        }
        // The row picked with the keyboard stays in view.
        if std::mem::take(&mut self.menu_follow) {
            let top = self.menu_pick as f32 * ROW_H;
            self.menu_scroll = self.menu_scroll.min(top).max(top + ROW_H - list.h);
        }
        self.menu_scroll = self.menu_scroll.clamp(0.0, (content_h - list.h).max(0.0));

        let faces = content.items.iter().any(|item| item.face);
        let clip = p.push_clip(list);
        let mut chosen = None;
        for (i, &index) in rows.iter().enumerate() {
            let row = Rect::new(area.x + 4.0, list.y + i as f32 * ROW_H - self.menu_scroll, area.w - 8.0, ROW_H);
            if row.bottom() < list.y || row.y > list.bottom() {
                continue;
            }
            let item = &content.items[index];
            let hovered = ui.hovered(row) && list.contains(ui.mouse);
            // The row Enter would pick is highlighted.
            let picked = (self.menu == Some(Menu::Commands) && i == self.command_pick)
                || (searched && self.menu_keyed && i == self.menu_pick);
            let hover = ui.anim(id(("menu", &content.header.0, i)), f32::from(u8::from(hovered || picked)));
            p.rect(row, fade(t.hover, hover), theme::RADIUS_SM);

            let centre = |layout: &TextLayout| row.y + (row.h - layout.height()) * 0.5;
            // People and worlds show their portrait (or initial); every
            // label in such a list lines up after it.
            let face = Rect::new(row.x + 6.0, row.y + (ROW_H - FACE) * 0.5, FACE, FACE);
            let label_x = if faces { face.right() + 10.0 } else { row.x + 24.0 };
            if item.face {
                let path = self.library.portrait_path(&item.image);
                portrait(p, path.as_deref(), (&item.label, &item.color), face, FACE * 0.5);
            }
            if item.selected {
                p.label_centered("✓", theme::SMALL, Rect::new(row.x + 4.0, row.y, 16.0, row.h), t.accent);
            }
            let mut detail = p.layout(&item.detail, theme::SMALL, None);
            detail.truncate(p.fonts, (row.w * 0.5).max(60.0));
            p.text(&detail, row.right() - 10.0 - detail.width(), centre(&detail), t.text_faint);
            let mut label = p.layout(&item.label, theme::SMALL, None);
            label.truncate(p.fonts, row.right() - label_x - 26.0 - detail.width());
            p.text(&label, label_x, centre(&label), t.text);

            if hovered {
                ui.cursor = CursorIcon::Pointer;
                if ui.clicked(row) {
                    chosen = Some(i);
                }
            }
        }
        p.set_clip(clip);
        chosen
    }
}

/// A model's name in a menu: its name, or its id without one.
fn model_label(model: &openrp_core::Model) -> &str {
    if model.name.is_empty() { &model.id } else { &model.name }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::Reasoning;

    #[test]
    fn menus_are_searched_by_name_first() {
        let item = |label: &str, detail: &str, search: &str| MenuItem {
            search: search.to_owned(),
            ..MenuItem::new(label, detail)
        };
        let items = [
            item("Mira", "barkeep, v2", "fantasy"),
            item("Rex", "", "clone army"),
            item("Old Mira", "", ""),
            item("Kira", "mira's sister", ""),
        ];
        assert_eq!(search(&items, ""), [0, 1, 2, 3]);
        assert_eq!(search(&items, "mira"), [0, 2, 3], "names first, then comments");
        assert_eq!(search(&items, "MIRA fantasy"), [0], "every word, across name and tags");
        assert_eq!(search(&items, "army"), [1]);
        assert!(search(&items, "nobody").is_empty());
    }

    #[test]
    fn a_model_can_be_typed_when_none_is_listed() {
        let mut chat = Chat::new(None, Reasoning::Auto, Vec::new());
        chat.menu = Some(Menu::Model);
        chat.menu_query.insert(" inworld-llm ");
        let content = chat.menu_content(Menu::Model, [Rect::default(); 4]);
        let rows = search(&content.items, chat.menu_query.text());
        assert_eq!(rows.len(), 1, "the typed name is offered");
        let mut actions = Vec::new();
        chat.choose(Menu::Model, rows[0], &mut actions);
        assert_eq!(chat.model, "inworld-llm");
        assert!(matches!(actions.as_slice(), [Action::SelectModel(m)] if m == "inworld-llm"));
    }

    #[test]
    fn pickers_tell_apart_namesakes_and_their_footers_act() {
        let persona = |id: &str, comment: &str, favorite: bool| openrp_core::Persona {
            id: id.into(),
            name: "Gale".into(),
            description: format!("Gale, {comment}.\nMore."),
            comment: comment.into(),
            favorite,
            ..Default::default()
        };
        let mut chat = Chat::new(None, Reasoning::Auto, Vec::new());
        let personas = vec![persona("a", "the hunter", false), persona("b", "the miner", true)];
        chat.library_loaded(Vec::new(), Vec::new(), personas, openrp_core::Portraits::at("p".into()));
        chat.play("w".into());
        chat.sync_character_form();
        let content = chat.menu_content(Menu::Personas, [Rect::default(); 4]);
        let shown: Vec<(&str, &str, bool)> =
            content.items.iter().map(|i| (i.detail.as_str(), i.summary.as_str(), i.selected)).collect();
        assert_eq!(
            shown,
            [("the miner", "Gale, the miner.", true), ("the hunter", "Gale, the hunter.", false)],
            "starred first, filled in and ticked; each with its note and what it is about"
        );
        assert_eq!(search(&content.items, "hunter"), [1], "notes are searched");

        // A world is made from Play's picker.
        chat.menu = Some(Menu::PlayIn);
        chat.footer_picked(Menu::PlayIn, 0);
        assert!(chat.menu.is_none() && chat.page == Page::Library(Kind::World));
    }

    #[test]
    fn the_cast_is_picked_from_a_search() {
        let character = |id: &str, name: &str, comment: &str| openrp_core::Character {
            id: id.into(),
            name: name.into(),
            comment: comment.into(),
            ..Default::default()
        };
        let mut chat = Chat::new(None, Reasoning::Auto, Vec::new());
        let library = vec![
            character("a", "Mira", "barkeep v1"),
            character("b", "Mira", "barkeep v2, darker"),
            character("c", "Rex", ""),
        ];
        chat.library_loaded(Vec::new(), library, Vec::new(), openrp_core::Portraits::at("p".into()));
        chat.play("w".into());
        chat.current().player = Some(openrp_core::Player { name: "Gale".into(), ..Default::default() });
        chat.menu = Some(Menu::CastLibrary);
        chat.menu_query.insert("mira DARKER");
        let content = chat.menu_content(Menu::CastLibrary, [Rect::default(); 4]);
        let rows = search(&content.items, chat.menu_query.text());
        assert_eq!(rows.len(), 1);
        assert_eq!(content.items[rows[0]].detail, "barkeep v2, darker", "the comment tells the two apart");
        let mut actions = Vec::new();
        chat.choose(Menu::CastLibrary, rows[0], &mut actions);
        assert!(chat.current().cast.iter().any(|m| m.id == "b"), "the one searched for joins");
        assert!(chat.menu == Some(Menu::CastLibrary), "and the menu stays open for more");
    }
}
