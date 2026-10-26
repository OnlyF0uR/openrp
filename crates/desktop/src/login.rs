//! The connect screen, shown while no provider is set: the base URL and API
//! key of an OpenAI-compatible API (see `ProviderForm`).

use arboard::Clipboard;
use winit::event::KeyEvent;
use winit::keyboard::ModifiersState;

use crate::app::Action;
use crate::paint::{Painter, Rect};
use crate::provider::ProviderForm;
use crate::text::{Align, Style};
use crate::theme;
use crate::ui::{ButtonStyle, Ui, button, logo};

/// State of the connect screen.
pub struct Login {
    form: ProviderForm,
    /// Something that went wrong out of sight (a failed save), told once
    /// the chat opens.
    pub problem: Option<String>,
    /// How far the card is scrolled, when it is taller than the window.
    scroll: f32,
}

impl Login {
    /// The screen holding `base_url`, optionally explaining why the user
    /// landed here.
    #[must_use]
    pub fn new(error: Option<String>, base_url: &str) -> Self {
        let mut form = ProviderForm::new(base_url, false);
        form.error = error;
        Self { form, problem: None, scroll: 0.0 }
    }

    /// Where the input method's candidate window should appear.
    #[must_use]
    pub fn ime_area(&self) -> Option<Rect> {
        self.form.caret()
    }

    /// Asks to use the provider entered, if the form accepts it.
    fn connect(&mut self, actions: &mut Vec<Action>) {
        if let Some((base_url, api_key)) = self.form.submit() {
            actions.push(Action::Connect { base_url, api_key });
        }
    }

    /// Text committed by an input method.
    pub fn commit(&mut self, text: &str) {
        self.form.insert(text);
    }

    /// Keyboard input.
    pub fn key(
        &mut self,
        event: &KeyEvent,
        mods: ModifiersState,
        cb: &mut Option<Clipboard>,
        actions: &mut Vec<Action>,
    ) {
        if self.form.key(event, mods, cb) {
            self.connect(actions);
        }
    }

    /// Draws the screen.
    pub fn draw(&mut self, p: &mut Painter, ui: &mut Ui, view: Rect, actions: &mut Vec<Action>) {
        let t = p.theme;
        let width = 400.0;
        let inner = width - 64.0;
        let subtitle = "Enter the base URL and API key of any OpenAI-compatible API, such as OpenRouter or Inworld. Your key stays on this device.";
        let subtitle = p.layout(subtitle, Style { line_height: 1.5, ..theme::SMALL }, Some(inner));
        let form_h = self.form.height(p, inner);

        // Logo, title, subtitle, the form and the button under it.
        let height = 32.0 + 40.0 + 20.0 + 30.0 + 6.0 + subtitle.height() + 24.0 + form_h + 24.0 + 36.0 + 32.0;
        // Taller than the window (a high zoom), the card scrolls instead.
        self.scroll = (self.scroll + ui.scroll).clamp(0.0, (height + 32.0 - view.h).max(0.0));
        let top = ((view.h - height) * 0.5).max(16.0).round() - self.scroll.round();
        let card = Rect::new(((view.w - width) * 0.5).round(), top, width, height);
        p.shadow(Rect::new(card.x, card.y + 8.0, card.w, card.h), t.shadow, theme::RADIUS, 24.0);
        p.bordered(card, t.panel, theme::RADIUS, 1.0, t.border_strong);

        let x = card.x + 32.0;
        let mut y = card.y + 32.0;
        logo(p, Rect::new(card.x + (card.w - 40.0) * 0.5, y, 40.0, 40.0));
        y += 40.0 + 20.0;
        let title = p.layout("Connect a provider", theme::TITLE, None);
        p.text_aligned(&title, x, y, Align::Center, inner, t.text);
        y += 30.0 + 6.0;
        p.text_aligned(&subtitle, x, y, Align::Center, inner, t.text_muted);
        y += subtitle.height() + 24.0;

        self.form.draw(p, ui, x, y, inner);
        y += form_h + 24.0;
        if button(p, ui, Rect::new(x, y, inner, 36.0), "Connect", ButtonStyle::Primary, true) {
            self.connect(actions);
        }
    }
}
