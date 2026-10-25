//! Stories: Play starts one in a world, and its cast is the characters
//! taking part. A cast member is present (in the current scene) or absent
//! (part of the story, elsewhere). Members are copies: what the library
//! does later never changes a story.
//!
//! The cast panel, beside the story (or over it, in a narrow window; the
//! header's Cast button shows and hides it), holds everything the story is
//! made of: the world's banner, the scene (set by the model, or by clicking
//! it), the buttons opening the memories and the author's note, who the user
//! plays, and the cast, in the scene and elsewhere, each with a button to
//! move them and a menu (store in Characters, edit, delete; see
//! `dialog.rs`). Clicking a member edits them. Its footer opens the picker
//! that adds characters from the library, or creates or generates one.
//!
//! A story nobody has written in yet opens with a greeting: the first
//! message of the first character in the scene whose library character has
//! one, the others to swipe to (see [`Chat::greet`]).

use openrp_core::{CastMember, StoredMessage, macros};
use winit::window::CursorIcon;

use super::{Chat, Entry, Load, Menu, MenuItem, Page, tools};
use crate::app::Action;
use crate::library::{Kind, Record, portrait};
use crate::paint::{Painter, Rect, fade, mix};
use crate::text::Style;
use crate::theme;
use crate::ui::{ButtonStyle, Ui, button, id};

/// Width of the cast panel.
const PANEL_W: f32 = 300.0;
/// Narrowest the story gets beside a docked panel; narrower, it floats.
const STAGE_MIN: f32 = 620.0;
/// Height of a cast member's row.
const MEMBER_H: f32 = 56.0;
/// Side of a portrait in the panel.
const FACE: f32 = 38.0;
/// Height of the footer holding the add button.
const FOOTER_H: f32 = 64.0;
/// Height of the world's banner atop the panel.
const BANNER_H: f32 = 132.0;
/// Most lines of the scene the panel shows.
const SCENE_LINES: f32 = 5.0;
/// Faces in the header's Cast button.
const STACK: usize = 4;

/// What was clicked on a cast member's row.
#[derive(Clone, Copy)]
enum RowClick {
    /// The row: edit them.
    Edit,
    /// Move them into the scene or out of it.
    Move,
    /// Their menu, opened from these dots.
    Menu(Rect),
}

impl Chat {
    /// Starts a story in `world` and shows it.
    pub(super) fn play(&mut self, world: String) {
        self.new_conversation();
        self.current().world = Some(world);
    }

    /// Starts a story in `world` with the character Play was pressed for
    /// in the scene; their greeting opens it once the user says who they are.
    pub(super) fn play_character_in(&mut self, world: String, actions: &mut Vec<Action>) {
        let id = std::mem::take(&mut self.play_character);
        let Some(member) = self.library.get(Kind::Character, &id).map(cast_member) else { return };
        self.play(world);
        self.change_cast(actions, |cast| cast.push(member));
    }

    /// Opens the story with a greeting, if nobody has written in it yet:
    /// the first message of the first member in the scene whose library
    /// character has one, as their bubble (their other greetings are kept
    /// to swipe to). `{{user}}` and `{{char}}` are filled in.
    pub(super) fn greet(&mut self, actions: &mut Vec<Action>) {
        let id = self.next_id();
        let library = &self.library;
        let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == self.current) else { return };
        let Some(player) = conversation.player.as_ref().map(|p| p.name.clone()) else { return };
        if !conversation.entries.is_empty() || conversation.world.is_none() || conversation.load != Load::Loaded {
            return;
        }
        let greeter = conversation.cast.iter().filter(|m| m.present).find_map(|m| {
            let greetings = &library.get(Kind::Character, &m.id)?.greetings;
            (!greetings.is_empty()).then(|| (m.name.clone(), greetings.clone()))
        });
        let Some((name, greetings)) = greeter else { return };
        let mut opening: Vec<StoredMessage> =
            greetings.iter().map(|g| tools::greeting(&name, &macros(g, Some(&name), &player))).collect();
        let mut first = opening.remove(0);
        first.swipes = opening.into_iter().map(|m| vec![m]).collect();
        conversation.entries.push(Entry::new(id, first));
        actions.push(Action::SaveSession(conversation.to_session()));
    }

    /// The system prompt for conversation `id`: its world (as the library
    /// has it now), who the user plays, and its cast.
    pub(super) fn instructions(&self, id: u64) -> String {
        let Some(conversation) = self.conversations.iter().find(|c| c.id == id) else {
            return String::new();
        };
        // A story stays a story even before the library has loaded its world.
        let world = conversation.world.as_deref().map(|w| {
            self.library
                .get(Kind::World, w)
                .map_or(("this world", "", &[][..]), |w| (w.name.as_str(), w.description.as_str(), w.lore.as_slice()))
        });
        super::stream::story_prompt(world, conversation.player.as_ref(), &conversation.cast)
    }

    /// The story state sent after the latest message of conversation `id`:
    /// its scene, who is in it, the lore its latest turns mention, memories
    /// and author's note. `None` outside a story.
    pub(super) fn state(&self, id: u64) -> Option<String> {
        let conversation = self.conversations.iter().find(|c| c.id == id)?;
        let world = self.library.get(Kind::World, conversation.world.as_deref()?);
        let world_lore = world.map_or(&[][..], |w| w.lore.as_slice());
        let user = conversation.player.as_ref().map_or("the user", |p| p.name.as_str());
        // Only a story with lore reads its latest turns for it.
        let lore = if world_lore.is_empty() && conversation.cast.iter().all(|m| m.lore.is_empty()) {
            Vec::new()
        } else {
            let from = super::stream::lore_start(&conversation.entries);
            let recent: Vec<StoredMessage> = conversation.entries[from..].iter().map(|e| e.message.clone()).collect();
            let text = format!("{}\n{}", conversation.scene, tools::transcript(&recent, user, usize::MAX));
            super::stream::lore_in_play(world_lore, &conversation.cast, &text, user)
        };
        Some(super::stream::story_state(
            &conversation.cast,
            &lore,
            &conversation.memories,
            &conversation.scene,
            &conversation.note,
        ))
    }

    /// Library characters that can still join the open story, favourites
    /// first.
    pub(super) fn cast_choices(&self) -> Vec<&Record> {
        let Some(conversation) = self.conversations.iter().find(|c| c.id == self.current) else { return Vec::new() };
        let cast = |id: &str| conversation.cast.iter().any(|m| m.id == id);
        let mut choices: Vec<&Record> = self.library.list(Kind::Character).iter().filter(|c| !cast(&c.id)).collect();
        choices.sort_by_key(|c| !c.favorite);
        choices
    }

    /// Rows of the cast picker: the library characters that can join.
    pub(super) fn library_menu(&self) -> Vec<MenuItem> {
        self.cast_choices().into_iter().map(MenuItem::record).collect()
    }

    /// The cast picker's footer buttons; the last closes it.
    pub(super) fn library_menu_footer() -> Vec<&'static str> {
        vec!["Create…", "Generate…", "Library", "Done"]
    }

    /// Applies footer button `index` of the cast picker.
    pub(super) fn library_footer_picked(&mut self, index: usize) {
        self.menu = None;
        match index {
            0 => self.create_member(),
            1 => self.generate_member(),
            2 => self.show(Page::Library(Kind::Character)),
            _ => {}
        }
    }

    /// Adds row `index` of the cast picker to the story. The picker stays
    /// open to add more (whoever joined leaves the list), its search
    /// cleared for the next.
    pub(super) fn library_menu_picked(&mut self, index: usize, actions: &mut Vec<Action>) {
        if let Some(member) = self.cast_choices().into_iter().nth(index).map(cast_member) {
            self.change_cast(actions, |cast| cast.push(member));
            // The first to join a story nobody wrote in yet may open it.
            self.greet(actions);
            self.menu_query.take();
            (self.menu_pick, self.menu_keyed) = (0, false);
        }
        self.menu = Some(Menu::CastLibrary);
    }

    /// Rows of cast member `member`'s menu: storing them updates the library
    /// character they were cast from, if there is one.
    pub(super) fn member_menu(&mut self, member: usize) -> Vec<MenuItem> {
        let id = self.current().cast.get(member).map(|m| m.id.clone()).unwrap_or_default();
        let store = if self.library.get(Kind::Character, &id).is_some() {
            "Update in Characters"
        } else {
            "Store in Characters"
        };
        vec![
            MenuItem::new(store, "For other stories"),
            MenuItem::new("Edit…", ""),
            MenuItem::new("Delete", "From this story"),
        ]
    }

    /// Applies row `index` of cast member `member`'s menu.
    pub(super) fn member_menu_picked(&mut self, member: usize, index: usize, actions: &mut Vec<Action>) {
        match index {
            0 => {
                if let Some(m) = self.current().cast.get(member).cloned() {
                    self.library.store_character(&m, actions);
                }
            }
            1 => self.edit_member(member),
            _ if member < self.current().cast.len() => self.change_cast(actions, |cast| {
                cast.remove(member);
            }),
            _ => {}
        }
    }

    /// Changes the open story's cast, saving it unless nothing was written yet.
    fn change_cast(&mut self, actions: &mut Vec<Action>, change: impl FnOnce(&mut Vec<CastMember>)) {
        let conversation = self.current();
        change(&mut conversation.cast);
        if !conversation.is_fresh() && conversation.load == super::Load::Loaded {
            actions.push(Action::SaveSession(conversation.to_session()));
        }
    }

    /// Where the cast panel goes in `main`, if it shows: beside the story
    /// when there is room for both (`true`), else over it. It shows for a
    /// story once read, when the user opened it, or by itself when docked.
    pub(super) fn cast_panel(&self, main: Rect) -> Option<(Rect, bool)> {
        let conversation = self.conversations.iter().find(|c| c.id == self.current)?;
        if conversation.world.is_none() || conversation.load != Load::Loaded {
            return None;
        }
        let docked = main.w - PANEL_W >= STAGE_MIN;
        let height = main.h - theme::HEADER_HEIGHT;
        self.cast_open
            .unwrap_or(docked)
            .then(|| (Rect::new(main.right() - PANEL_W, theme::HEADER_HEIGHT, PANEL_W, height), docked))
    }

    /// Shows the cast panel if it is hidden in `main`, or hides it.
    pub(super) fn toggle_cast(&mut self, main: Rect) {
        self.cast_open = Some(self.cast_panel(main).is_none());
        if matches!(self.menu, Some(Menu::Member(_))) {
            self.menu = None;
        }
    }

    /// Draws the header's Cast button ending at `right`: the faces of who is
    /// in the scene, overlapping, and how many are cast. `open` while the
    /// panel shows. Returns whether it was clicked; `None` outside a story.
    pub(super) fn draw_cast_button(
        &mut self,
        p: &mut Painter,
        ui: &mut Ui,
        right: f32,
        open: bool,
    ) -> Option<(Rect, bool)> {
        let t = p.theme;
        let conversation = self.conversations.iter().find(|c| c.id == self.current)?;
        if conversation.world.is_none() || conversation.load != Load::Loaded {
            return None;
        }
        let present: Vec<(String, String)> = conversation
            .cast
            .iter()
            .filter(|m| m.present)
            .take(STACK)
            .map(|m| (m.name.clone(), m.portrait.clone()))
            .collect();
        let label = match conversation.cast.len() {
            0 => "Cast".to_owned(),
            n => format!("Cast · {n}"),
        };
        let label = p.layout(&label, theme::LABEL, None);
        let face = 22.0;
        let faces_w = if present.is_empty() { 0.0 } else { face + (present.len() - 1) as f32 * (face - 8.0) + 8.0 };
        let width = faces_w + label.width() + 22.0;
        let rect = Rect::new(right - width, (theme::HEADER_HEIGHT - 30.0) * 0.5, width, 30.0);
        let hovered = ui.hovered(rect);
        let hover = ui.anim(id("cast-button"), f32::from(u8::from(hovered)));
        let fill = if open { mix(t.active, t.hover, hover) } else { fade(t.hover, hover) };
        p.rect(rect, fill, theme::RADIUS);
        let mut x = rect.x + 10.0;
        for (name, image) in &present {
            let disc = Rect::new(x, rect.y + (rect.h - face) * 0.5, face, face);
            // A ring of the header's colour keeps overlapping faces apart.
            p.rect(Rect::new(disc.x - 2.0, disc.y - 2.0, face + 4.0, face + 4.0), t.bg, face * 0.5 + 2.0);
            let path = if image.is_empty() { None } else { self.library.portrait_path(image) };
            portrait(p, path.as_deref(), name, disc, face * 0.5);
            x += face - 8.0;
        }
        if !present.is_empty() {
            x += 8.0 + 8.0;
        }
        p.text(
            &label,
            x,
            rect.y + (rect.h - label.height()) * 0.5,
            mix(t.text_muted, t.text, hover.max(f32::from(u8::from(open)))),
        );
        if hovered {
            ui.cursor = CursorIcon::Pointer;
        }
        Some((rect, ui.clicked(rect)))
    }

    /// Draws the cast panel in `panel` (see [`Chat::cast_panel`]) and
    /// applies what was clicked in it. Returns the anchor of the open
    /// member's menu.
    pub(super) fn draw_cast_panel(
        &mut self,
        p: &mut Painter,
        ui: &mut Ui,
        panel: Rect,
        actions: &mut Vec<Action>,
    ) -> Rect {
        let t = p.theme;
        let conversation = self.current();
        let cast = conversation.cast.clone();
        let player = conversation.player.clone();
        let scene = conversation.scene.split_whitespace().collect::<Vec<_>>().join(" ");
        let (memories, has_note) = (conversation.memories.len(), !conversation.note.trim().is_empty());
        let reviewing = conversation.reviewing.is_some();
        let world = conversation.world.clone();
        let world = world.as_deref().and_then(|w| self.library.get(Kind::World, w));
        let world_name = world.map_or("A deleted world".to_owned(), |w| w.name.clone());
        let banner = world.and_then(|w| self.library.portrait_path(&w.portrait));
        // The persona the user plays, for the note that tells it apart.
        let persona_note = player.as_ref().and_then(|player| {
            let personas = self.library.list(Kind::Persona);
            let same = |r: &&Record| r.name == player.name && r.description == player.description;
            personas.iter().find(same).or_else(|| self.library.persona_named(&player.name)).map(|r| r.comment.clone())
        });

        p.rect(panel, t.panel, 0.0);
        p.rect(Rect::new(panel.x, panel.y, 1.0, panel.h), t.border, 0.0);
        let footer = Rect::new(panel.x, panel.bottom() - FOOTER_H, panel.w, FOOTER_H);
        let body = Rect::new(panel.x + 1.0, panel.y, panel.w - 1.0, footer.y - panel.y);
        if ui.hovered(body) {
            self.cast_scroll += ui.scroll;
            ui.scroll = 0.0;
        }
        self.cast_scroll = self.cast_scroll.clamp(0.0, (self.cast_content_h - body.h).max(0.0));
        let interactive = body.contains(ui.mouse);
        let (x, content_w) = (panel.x + 16.0, panel.w - 32.0);
        let clip = p.push_clip(body);
        let top = body.y + 16.0 - self.cast_scroll.round();
        let mut y = top;

        // The world: its picture with its name over it, or its name alone.
        if let Some(path) = &banner {
            let rect = Rect::new(x, y, content_w, BANNER_H);
            portrait(p, Some(path), &world_name, rect, theme::RADIUS + 2.0);
            let band = Rect::new(x, rect.bottom() - 38.0, content_w, 38.0);
            let saved = p.push_clip(band);
            p.rect(Rect::new(band.x, band.y - 8.0, band.w, band.h + 8.0), t.scrim, theme::RADIUS + 2.0);
            p.set_clip(saved);
            let mut name = p.layout(&world_name, Style { font: crate::text::FontId::SERIF_BOLD, ..theme::LABEL }, None);
            name.truncate(p.fonts, content_w - 24.0);
            p.text(&name, band.x + 12.0, band.y + (band.h - name.height()) * 0.5, t.on_scrim);
            p.bordered(rect, [0.0; 4], theme::RADIUS + 2.0, 1.0, fade(t.border_strong, 0.6));
            y += BANNER_H + 18.0;
        } else {
            let style = Style { font: crate::text::FontId::SERIF_BOLD, size: 19.0, line_height: 1.3 };
            let mut name = p.layout(&world_name, style, None);
            name.truncate(p.fonts, content_w);
            p.text(&name, x, y, t.text);
            y += name.height() + 14.0;
        }

        // The scene, in the story's serif; clicking it edits it.
        y = caption(p, "Scene", x, y);
        let (text, style, color) = if scene.is_empty() {
            ("Not set yet. The AI sets it as the story starts; click to set it yourself.", theme::SMALL, t.text_faint)
        } else {
            (scene.as_str(), theme::SCENE, t.text)
        };
        let layout = p.layout(text, style, Some(content_w - 24.0));
        let text_h = layout.height().min(layout.line_height() * SCENE_LINES);
        let card = Rect::new(x, y, content_w, text_h + 22.0);
        let hovered = interactive && ui.hovered(card);
        let hover = ui.anim(id("cast-scene"), f32::from(u8::from(hovered)));
        p.bordered(
            card,
            mix(t.surface, t.hover, hover * 0.5),
            theme::RADIUS,
            1.0,
            mix(t.border, t.border_strong, hover),
        );
        let saved = p.push_clip(Rect::new(card.x, card.y + 11.0, card.w, text_h));
        p.text(&layout, card.x + 12.0, card.y + 11.0, color);
        p.set_clip(saved);
        let edit_scene = hovered && ui.clicked(card);
        if hovered {
            ui.cursor = CursorIcon::Pointer;
        }
        y = card.bottom() + 10.0;

        // What the story remembers, and the author's note.
        let updating = if reviewing { " …" } else { "" };
        let memory_label =
            if memories == 0 { format!("Memories{updating}") } else { format!("Memories · {memories}{updating}") };
        let note_label = if has_note { "Note ✓" } else { "Author's note" };
        let half = ((content_w - 8.0) * 0.5).floor();
        let open_memories =
            button(p, ui, Rect::new(x, y, half, 30.0), &memory_label, ButtonStyle::Secondary, interactive);
        let open_note = button(
            p,
            ui,
            Rect::new(x + content_w - half, y, half, 30.0),
            note_label,
            ButtonStyle::Secondary,
            interactive,
        );
        y += 30.0 + 24.0;

        // Who the user plays; clicking edits them.
        y = caption(p, "You", x, y);
        let row = Rect::new(x - 8.0, y, content_w + 16.0, MEMBER_H);
        let mut edit_you = false;
        if let Some(player) = &player {
            let hovered = interactive && ui.hovered(row);
            let hover = ui.anim(id("cast-you"), f32::from(u8::from(hovered)));
            p.rect(row, fade(t.hover, hover), theme::RADIUS);
            let note = persona_note.filter(|n| !n.is_empty()).unwrap_or_else(|| "Your character".to_owned());
            let path = if player.portrait.is_empty() { None } else { self.library.portrait_path(&player.portrait) };
            person(p, row, (&player.name, path.as_deref(), &note), (true, row.right() - 8.0));
            if hovered {
                ui.cursor = CursorIcon::Pointer;
                edit_you = ui.clicked(row);
            }
        } else {
            let hint = p.layout("Not chosen yet: say who you are to begin.", theme::SMALL, None);
            p.text(&hint, x, y + (MEMBER_H - hint.height()) * 0.5, t.text_faint);
        }
        y += MEMBER_H + 18.0;

        // The cast: who is here, then who is elsewhere.
        let (mut clicked, mut anchor) = (None, None);
        let present: Vec<usize> = (0..cast.len()).filter(|&i| cast[i].present).collect();
        let absent: Vec<usize> = (0..cast.len()).filter(|&i| !cast[i].present).collect();
        for (title, members, here) in [("In the scene", &present, true), ("Elsewhere", &absent, false)] {
            if !here && members.is_empty() {
                continue;
            }
            y = caption(p, &format!("{title} · {}", members.len()), x, y);
            if members.is_empty() {
                let text = if cast.is_empty() {
                    "No one yet. Add characters from your library, or just say who you meet: the AI creates them."
                } else {
                    "No one is here. Bring someone in, or say who you meet."
                };
                let hint = p.layout(text, theme::SMALL, Some(content_w));
                p.text(&hint, x, y + 2.0, t.text_faint);
                y += hint.height() + 18.0;
                continue;
            }
            for &index in members {
                let member = &cast[index];
                let row = Rect::new(x - 8.0, y, content_w + 16.0, MEMBER_H);
                y += MEMBER_H + 2.0;
                if row.bottom() < body.y || row.y > body.bottom() {
                    continue;
                }
                let menu_open = self.menu == Some(Menu::Member(index));
                let hovered = interactive && ui.hovered(row);
                let hover = ui.anim(id(("cast", &member.id)), f32::from(u8::from(hovered || menu_open)));
                p.rect(row, fade(t.hover, hover), theme::RADIUS);
                // On hover: a button moving them, and their menu.
                let dots = Rect::new(row.right() - 34.0, row.y + (MEMBER_H - 26.0) * 0.5, 26.0, 26.0);
                let move_label = if here { "Away" } else { "Bring in" };
                let move_w = p.layout(move_label, theme::LABEL, None).width() + 18.0;
                let move_rect = Rect::new(dots.x - 4.0 - move_w, dots.y, move_w, 26.0);
                let controls = hovered || menu_open;
                let text_right = if controls { move_rect.x - 6.0 } else { row.right() - 8.0 };
                let summary =
                    member.description.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("No description yet");
                let path = if member.portrait.is_empty() { None } else { self.library.portrait_path(&member.portrait) };
                person(p, row, (&member.name, path.as_deref(), summary), (here, text_right));
                if controls {
                    if button(p, ui, move_rect, move_label, ButtonStyle::Ghost, true) {
                        clicked = Some((index, RowClick::Move));
                    }
                    let on_dots = ui.hovered(dots);
                    p.rect(dots, fade(t.active, if on_dots || menu_open { 1.0 } else { 0.0 }), theme::RADIUS_SM);
                    for i in 0..3 {
                        let dot = Rect::new(dots.x + 7.75 + 4.0 * i as f32, dots.y + 11.75, 2.5, 2.5);
                        p.rect(dot, if on_dots || menu_open { t.text } else { t.text_muted }, 1.25);
                    }
                    if on_dots && ui.clicked(dots) {
                        clicked = Some((index, RowClick::Menu(dots)));
                    }
                }
                if menu_open {
                    anchor = Some(dots);
                }
                if hovered {
                    ui.cursor = CursorIcon::Pointer;
                    let on_control = move_rect.contains(ui.mouse) || dots.contains(ui.mouse);
                    if !on_control && ui.clicked(row) {
                        clicked = Some((index, RowClick::Edit));
                    }
                }
            }
            y += 16.0;
        }
        p.set_clip(clip);
        self.cast_content_h = y - top + 8.0;

        // The footer: adding characters.
        p.rect(Rect::new(footer.x, footer.y, footer.w, 1.0), t.border, 0.0);
        let add = Rect::new(x, footer.y + (FOOTER_H - 36.0) * 0.5, content_w, 36.0);
        if button(p, ui, add, "+  Add characters", ButtonStyle::Primary, true) {
            self.menu = if self.menu == Some(Menu::CastLibrary) { None } else { Some(Menu::CastLibrary) };
            self.menu_scroll = 0.0;
        }

        if edit_scene {
            self.edit_scene();
        } else if open_memories {
            self.edit_memories();
        } else if open_note {
            self.edit_note();
        } else if edit_you {
            self.edit_player();
        } else if let Some((index, click)) = clicked {
            match click {
                RowClick::Edit => self.edit_member(index),
                RowClick::Move => self.change_cast(actions, |cast| cast[index].present = !cast[index].present),
                RowClick::Menu(dots) => {
                    self.menu = if self.menu == Some(Menu::Member(index)) { None } else { Some(Menu::Member(index)) };
                    self.menu_scroll = 0.0;
                    // Anchored at once, so its first click counts as inside it.
                    anchor = Some(dots);
                }
            }
        }
        // A member's menu closes once their row scrolls out of sight.
        if anchor.is_none() && matches!(self.menu, Some(Menu::Member(_))) {
            self.menu = None;
        }
        anchor.unwrap_or_default()
    }
}

/// Draws a small caption (`Scene`, `You`, `In the scene · 3`) at `(x, y)`;
/// returns where what it heads starts.
fn caption(p: &mut Painter, text: &str, x: f32, y: f32) -> f32 {
    let t = p.theme;
    p.label(&text.to_uppercase(), Style { size: 10.5, ..theme::CAPTION }, x, y, t.text_faint);
    y + 22.0
}

/// Draws a person's row inside `row`: their portrait ringed in their colour
/// (faded while they are not `here`), their name and a line about them,
/// which ends at `right`.
fn person(p: &mut Painter, row: Rect, (name, path, about): (&str, Option<&str>, &str), (here, right): (bool, f32)) {
    let t = p.theme;
    let ink = theme::character_color(&t, name);
    let face = Rect::new(row.x + 8.0, row.y + (row.h - FACE) * 0.5, FACE, FACE);
    portrait(p, path, name, face, FACE * 0.5);
    let ring = Rect::new(face.x - 2.5, face.y - 2.5, FACE + 5.0, FACE + 5.0);
    if here {
        p.bordered(ring, [0.0; 4], ring.w * 0.5, 1.5, fade(ink, 0.8));
    } else {
        // Elsewhere: the face fades into the panel.
        p.rect(face, fade(t.panel, 0.55), FACE * 0.5);
        p.bordered(ring, [0.0; 4], ring.w * 0.5, 1.0, t.border_strong);
    }
    let text_x = face.right() + 12.0;
    let width = (right - text_x).max(24.0);
    let mut label = p.layout(name, theme::LABEL, None);
    label.truncate(p.fonts, width);
    let mut line = p.layout(about, theme::SMALL, None);
    line.truncate(p.fonts, width);
    let block = label.height() + 2.0 + line.height();
    let top = row.y + (row.h - block) * 0.5;
    p.text(&label, text_x, top, if here { t.text } else { t.text_muted });
    p.text(&line, text_x, top + label.height() + 2.0, t.text_faint);
}

/// A library character as a story's cast member, in the scene: a copy, so
/// the story keeps them whatever the library does later.
fn cast_member(character: &Record) -> CastMember {
    CastMember {
        id: character.id.clone(),
        name: character.name.clone(),
        description: character.description.clone(),
        portrait: character.portrait.clone(),
        examples: character.examples.clone(),
        lore: character.lore.clone(),
        present: true,
        ..CastMember::default()
    }
}

#[cfg(test)]
mod tests {
    use openrp_core::{Character, Portraits, World};

    use super::*;
    use crate::chat::Reasoning;

    #[test]
    fn play_casts_and_prompts() {
        let mut chat = Chat::new(None, Reasoning::Auto, Vec::new());
        let world =
            World { id: "w".into(), name: "Panem".into(), description: "Twelve districts.".into(), ..World::default() };
        let katniss = Character {
            id: "k".into(),
            name: "Katniss".into(),
            description: "A hunter.".into(),
            ..Character::default()
        };
        let peeta =
            Character { id: "p".into(), name: "Peeta".into(), description: "A baker.".into(), ..Character::default() };
        chat.library_loaded(vec![world], vec![katniss, peeta], Vec::new(), Portraits::at("portraits".into()));
        assert!(!chat.current().playable(), "nothing to write in before Play");

        chat.play("w".into());
        assert!(chat.page == Page::Chat && !chat.current().playable(), "first: who does the user play");
        chat.current().player = Some(openrp_core::Player {
            name: "Gale".into(),
            description: "A hunter too.".into(),
            ..openrp_core::Player::default()
        });
        assert!(chat.current().playable());
        let mut actions = Vec::new();
        chat.menu = Some(Menu::CastLibrary);
        chat.menu_query.insert("kat");
        chat.library_menu_picked(0, &mut actions);
        assert!(chat.menu == Some(Menu::CastLibrary), "stays open to add more");
        assert!(chat.menu_query.text().is_empty(), "with its search cleared for the next");
        chat.library_menu_picked(0, &mut actions);
        assert_eq!(actions.len(), 2, "a begun story is saved with every change");
        assert_eq!(chat.cast_choices().len(), 0, "everyone is cast");
        chat.library_menu_picked(0, &mut actions);
        assert_eq!(chat.current().cast.len(), 2, "no one left to add: nothing happens");
        // The footer creates someone, or leads to the characters page.
        chat.library_footer_picked(0);
        assert!(chat.menu.is_none() && chat.character_form.is_some(), "Create opens the dialog");
        chat.character_form = None;
        chat.library_footer_picked(2);
        assert!(chat.page == Page::Library(Kind::Character));
        chat.page = Page::Chat;
        chat.change_cast(&mut actions, |cast| cast[1].present = false);

        // The cast holds copies: the library losing Katniss changes nothing.
        let world =
            World { id: "w".into(), name: "Panem".into(), description: "Twelve districts.".into(), ..World::default() };
        chat.library_loaded(vec![world], Vec::new(), Vec::new(), Portraits::at("portraits".into()));
        let id = chat.current().id;
        let prompt = chat.instructions(id);
        assert!(
            prompt.contains("# World: Panem\n\nTwelve districts.")
                && prompt.contains("# The user's character: Gale\n\nA hunter too.")
        );
        assert!(
            prompt.contains("## Katniss\n\nA hunter.") && prompt.contains("## Peeta\n\nA baker."),
            "the whole cast, described"
        );
        // Who is where is the story state's.
        let state = chat.state(id).unwrap();
        assert!(state.contains("# In the scene\n\nKatniss") && state.contains("Peeta") && !state.contains("A baker"));

        // Sending saves the story with its world, cast and player.
        actions.clear();
        chat.composer.insert("I step into the arena.");
        chat.send(&mut actions);
        let Some(Action::SaveSession(session)) = actions.first() else { panic!("not saved") };
        assert_eq!(session.world.as_deref(), Some("w"));
        assert_eq!(session.cast.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(), ["Katniss", "Peeta"]);
        assert!(session.player.is_some());
        let Some(Action::Send(job)) = actions.last() else { panic!("not sent") };
        assert_eq!(job.instructions, prompt);
        assert!(job.tools, "stories offer the story's tools");
    }

    #[test]
    fn a_characters_greeting_opens_the_story() {
        let mut chat = Chat::new(None, Reasoning::Auto, Vec::new());
        let world = World { id: "w".into(), name: "Ard".into(), ..World::default() };
        let greetings = vec!["Hi, {{user}}.".into(), "Back, {{user}}?".into()];
        let mira = Character {
            id: "m".into(),
            name: "Mira".into(),
            greetings,
            examples: "Hm.".into(),
            ..Character::default()
        };
        chat.library_loaded(vec![world], vec![mira], Vec::new(), Portraits::at("portraits".into()));

        // Play on a character: a story in the world picked, with her in the scene.
        chat.play_character = "m".into();
        let mut actions = Vec::new();
        chat.play_character_in("w".into(), &mut actions);
        assert!(
            chat.current().cast.iter().any(|m| m.name == "Mira" && m.examples == "Hm." && m.present),
            "cast with what the prompt needs"
        );
        chat.greet(&mut actions);
        assert!(chat.current().entries.is_empty(), "not before the user says who they are");

        // Once they do, she opens it; her other greeting can be swiped to.
        chat.current().player = Some(openrp_core::Player { name: "Gale".into(), ..openrp_core::Player::default() });
        chat.greet(&mut actions);
        let said = |chat: &mut Chat| {
            let entry = chat.current().entries.first_mut().unwrap();
            entry.refresh_display(false, true);
            entry.display.clone()
        };
        assert_eq!(said(&mut chat), "**Mira**: Hi, Gale.");
        assert_eq!(chat.current().greetings(), Some((0, 2)));
        assert!(
            matches!(actions.last(), Some(Action::SaveSession(s)) if s.messages.len() == 1 && s.messages[0].swipes.len() == 1)
        );
        chat.greet(&mut actions);
        assert_eq!(chat.current().entries.len(), 1, "only once");
        chat.swipe_greeting(1, &mut actions);
        assert_eq!((said(&mut chat), chat.current().greetings()), ("**Mira**: Back, Gale?".to_owned(), Some((1, 2))));
        chat.swipe_greeting(0, &mut actions);
        assert_eq!(said(&mut chat), "**Mira**: Hi, Gale.");

        // The model reads her greeting as her turn; once the user writes, it stays.
        chat.composer.insert("Hello.");
        chat.send(&mut actions);
        assert!(chat.current().greetings().is_none());
        let Some(Action::Send(job)) = actions.pop() else { panic!("not sent") };
        assert!(job.instructions.contains("### How Mira talks"));
        assert!(
            matches!(&job.input()[1], openrp_core::InputItem::ToolCall(call) if call.arguments.contains("Hi, Gale."))
        );
    }

    #[test]
    fn stories_are_exported_once_read() {
        let session = openrp_core::Session {
            id: "s".into(),
            title: "Night".into(),
            world: Some("w".into()),
            messages: vec![openrp_core::StoredMessage::new(openrp_core::Role::User, "Hi.".into())],
            ..openrp_core::Session::default()
        };
        let mut chat = Chat::new(None, Reasoning::Auto, vec![session.summary()]);
        let world = World { id: "w".into(), name: "Ard".into(), ..World::default() };
        chat.library_loaded(vec![world], Vec::new(), Vec::new(), Portraits::at("portraits".into()));
        let id = chat.conversations.iter().find(|c| c.session_id == "s").unwrap().id;
        let mut actions = Vec::new();
        chat.export(id, true, &mut actions);
        assert!(matches!(&actions[..], [Action::LoadSession { .. }]), "read first");
        let mut actions = Vec::new();
        chat.session_loaded(id, Ok(session), &mut actions);
        assert!(
            matches!(&actions[..], [Action::ExportStory { session, world, jsonl: true }] if session.messages.len() == 1 && world == "Ard")
        );
    }

    #[test]
    fn members_are_stored_in_characters() {
        let mut chat = Chat::new(None, Reasoning::Auto, Vec::new());
        chat.play("w".into());
        let rue = CastMember {
            id: "r".into(),
            name: "Rue".into(),
            description: "Small.".into(),
            portrait: "rue.png".into(),
            present: true,
            ..CastMember::default()
        };
        chat.current().cast.push(rue);
        assert_eq!(chat.member_menu(0)[0].label, "Store in Characters");
        let mut actions = Vec::new();
        chat.member_menu_picked(0, 0, &mut actions);
        assert!(
            matches!(&actions[..], [Action::SaveCharacter(c)] if c.id == "r" && c.description == "Small." && c.portrait == "rue.png")
        );
        assert_eq!(chat.member_menu(0)[0].label, "Update in Characters", "now cast from the library");

        // Storing again updates that character; the story itself is untouched.
        chat.current().cast[0].description = "Quick.".into();
        chat.member_menu_picked(0, 0, &mut actions);
        let records = chat.library.list(Kind::Character);
        assert!(records.len() == 1 && records[0].description == "Quick.");
        assert!(actions.iter().all(|a| matches!(a, Action::SaveCharacter(_))));
    }

    #[test]
    fn the_cast_panel_docks_where_there_is_room() {
        let mut chat = Chat::new(None, Reasoning::Auto, Vec::new());
        let wide = Rect::new(256.0, 0.0, 1200.0, 800.0);
        let narrow = Rect::new(256.0, 0.0, 700.0, 800.0);
        assert!(chat.cast_panel(wide).is_none(), "no story, no panel");
        chat.play("w".into());
        let (rect, docked) = chat.cast_panel(wide).unwrap();
        assert!(docked && (rect.right() - wide.right()).abs() < 0.5 && (rect.y - theme::HEADER_HEIGHT).abs() < 0.5);
        assert!(chat.cast_panel(narrow).is_none(), "too narrow to dock: hidden until asked for");
        chat.toggle_cast(narrow);
        assert_eq!(chat.cast_panel(narrow).map(|(_, docked)| docked), Some(false), "then it floats");
        chat.toggle_cast(wide);
        assert!(chat.cast_panel(wide).is_none(), "hidden by the user, it stays hidden");
    }

    #[test]
    fn deleting_a_world_deletes_its_stories() {
        let mut chat = Chat::new(None, Reasoning::Auto, Vec::new());
        for world in ["a", "b"] {
            chat.play(world.into());
            chat.current().player = Some(openrp_core::Player { name: "Gale".into(), ..openrp_core::Player::default() });
            chat.composer.insert("Hello.");
            chat.send(&mut Vec::new());
        }
        // A world's page lists its stories.
        let world = |id: &str| openrp_core::World { id: id.into(), name: id.into(), ..openrp_core::World::default() };
        chat.library_loaded(vec![world("a"), world("b")], Vec::new(), Vec::new(), Portraits::at("portraits".into()));
        assert!(chat.world_stories().is_empty(), "no world open");
        chat.library.open_form(Kind::World, "a");
        let stories = chat.world_stories();
        assert!(
            stories.len() == 1
                && chat.conversations.iter().any(|c| c.id == stories[0].id && c.world.as_deref() == Some("a"))
        );

        let mut actions = Vec::new();
        chat.delete_world_stories("a", &mut actions);
        assert_eq!(actions.iter().filter(|a| matches!(a, Action::DeleteSession(_))).count(), 1);
        assert!(chat.conversations.iter().all(|c| c.world.as_deref() != Some("a")));
        assert!(chat.conversations.iter().any(|c| c.world.as_deref() == Some("b")));
    }
}
