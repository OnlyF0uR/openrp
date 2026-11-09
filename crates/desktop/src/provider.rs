//! The provider form: the providers saved (the one in use, and those used
//! before, switched to with a click), then the address and key of a new one,
//! Anthropic's API or an OpenAI-compatible one, with presets for well-known
//! ones. Sign-in and settings both show it.

use arboard::Clipboard;
use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::app::Action;
use crate::editor::Editor;
use crate::form::{FIELD_PAD, Fields};
use crate::paint::{Painter, Rect};
use crate::text::TextLayout;
use crate::theme;
use crate::ui::{ButtonStyle, Ui, button};

/// Stands in a preset's address for what the user must fill in.
const ACCOUNT: &str = "ACCOUNT_ID";
/// Well-known APIs: name and API root.
const PRESETS: [(&str, &str); 6] = [
    ("Anthropic", "https://api.anthropic.com/v1"),
    ("OpenRouter", "https://openrouter.ai/api/v1"),
    ("Cloudflare", "https://api.cloudflare.com/client/v4/accounts/ACCOUNT_ID/ai/v1"),
    ("Inworld", "https://api.inworld.ai/v1"),
    ("OpenAI", "https://api.openai.com/v1"),
    ("Ollama", "http://localhost:11434/v1"),
];
/// The address field.
const URL: usize = 0;
/// The key field.
const KEY: usize = 1;
/// Height of a preset button.
const PRESET_H: f32 = 26.0;
/// Height of a saved provider's row.
const ROW_H: f32 = 52.0;
/// Space a field's label takes above it.
const LABEL_H: f32 = 22.0;
/// Space between the parts of the form.
const GAP: f32 = 14.0;

/// The providers saved, and the address and key of a new one being entered.
pub struct ProviderForm {
    fields: Fields,
    /// The address in use, if any, and whether a key is saved for it.
    active: Option<(String, bool)>,
    /// Addresses of the providers used before, latest first.
    saved: Vec<String>,
    /// Why the last attempt to connect was refused.
    pub error: Option<String>,
}

impl ProviderForm {
    /// A form listing the provider in use (`active`: its address and whether
    /// a key is saved for it) and those used before (`saved`), with
    /// `base_url` in the address field.
    #[must_use]
    pub fn new(base_url: &str, active: Option<(String, bool)>, saved: Vec<String>) -> Self {
        let mut url = Editor::default();
        url.insert(base_url);
        Self { fields: Fields::new([url, Editor::default()], [false, false]), active, saved, error: None }
    }

    /// Text committed by an input method.
    pub fn insert(&mut self, text: &str) {
        self.fields.insert(text);
    }

    /// The focused field's caret, for the input method's window.
    #[must_use]
    pub fn caret(&self) -> Option<Rect> {
        self.fields.caret()
    }

    /// Handles a key. Returns `true` when Enter in the last field asks to
    /// connect.
    pub fn key(&mut self, event: &KeyEvent, mods: ModifiersState, cb: &mut Option<Clipboard>) -> bool {
        if self.fields.key(event, mods, cb) {
            self.error = None;
            return false;
        }
        event.logical_key == Key::Named(NamedKey::Enter)
    }

    /// The address and key to connect with (`None`: the field was empty),
    /// or `None` with [`ProviderForm::error`] set when the address is not one.
    pub fn submit(&mut self) -> Option<(String, Option<String>)> {
        let url = self.fields.text(URL).trim().trim_end_matches('/');
        // People paste the endpoint as often as the root.
        let url = url.strip_suffix("/chat/completions").unwrap_or(url);
        let host = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or_default();
        if host.is_empty() || url.contains(char::is_whitespace) {
            self.error = Some(
                "Enter the API's address, starting with https:// (or http:// for a server on this computer).".into(),
            );
            return None;
        }
        if url.contains(ACCOUNT) {
            self.error = Some(format!("Replace {ACCOUNT} in the address with your Cloudflare account ID."));
            return None;
        }
        // Keys pasted with a line break would make an invalid header.
        let key: String = self.fields.text(KEY).split_whitespace().collect();
        Some((url.to_owned(), (!key.is_empty()).then_some(key)))
    }

    /// Height of the form drawn `width` wide.
    #[must_use]
    pub fn height(&self, p: &Painter, width: f32) -> f32 {
        let fields: f32 = self.layouts(p, width).iter().map(|(_, h)| LABEL_H + h + GAP).sum();
        let error = self.error.as_deref().map_or(0.0, |e| p.layout(e, theme::SMALL, Some(width)).height() + GAP);
        self.list_height() + chips(p, width, PRESETS.map(|(name, _)| name)).1 + GAP + fields + error - GAP
    }

    /// Rows of the providers saved.
    fn rows(&self) -> usize {
        usize::from(self.active.is_some()) + self.saved.len()
    }

    /// Height of the saved providers and the label under them; none without them.
    fn list_height(&self) -> f32 {
        match self.rows() {
            0 => 0.0,
            rows => rows as f32 * ROW_H + GAP + LABEL_H,
        }
    }

    /// Draws the form at `(x, y)`, `width` wide: Use on a saved provider
    /// connects to it, × forgets it, and Forget key the key in use.
    pub fn draw(&mut self, p: &mut Painter, ui: &mut Ui, x: f32, mut y: f32, width: f32, actions: &mut Vec<Action>) {
        let t = p.theme;
        let rows = self.rows();
        if rows > 0 {
            p.bordered(Rect::new(x, y, width, rows as f32 * ROW_H), t.bg, theme::RADIUS, 1.0, t.border);
            let active = self.active.as_ref().map(|(url, key)| (url.as_str(), Some(*key)));
            let saved = self.saved.iter().map(|url| (url.as_str(), None));
            let mut forgotten = None;
            for (i, (url, in_use)) in active.into_iter().chain(saved).enumerate() {
                let row = Rect::new(x, y + i as f32 * ROW_H, width, ROW_H);
                if i > 0 {
                    p.rect(Rect::new(row.x, row.y, row.w, 1.0), t.border, 0.0);
                }
                let control = |w: f32, right: f32| Rect::new(right - w, row.y + (ROW_H - 28.0) * 0.5, w, 28.0);
                let mut right = row.right() - 12.0;
                if let Some(has_key) = in_use {
                    if has_key {
                        let forget = control(96.0, right);
                        if button(p, ui, forget, "Forget key", ButtonStyle::Danger, true) {
                            actions.push(Action::ForgetKey);
                        }
                        right = forget.x - 12.0;
                    }
                    let tag = p.layout("In use", theme::LABEL, None);
                    right -= tag.width();
                    p.text(&tag, right, row.y + (ROW_H - tag.height()) * 0.5, t.accent);
                } else {
                    let remove = control(28.0, right);
                    if button(p, ui, remove, "×", ButtonStyle::Ghost, true) {
                        actions.push(Action::ForgetProvider(url.to_owned()));
                        forgotten = Some(i - usize::from(self.active.is_some()));
                    }
                    let switch = control(56.0, remove.x - 6.0);
                    if button(p, ui, switch, "Use", ButtonStyle::Secondary, true) {
                        actions.push(Action::Connect { base_url: url.to_owned(), api_key: None });
                    }
                    right = switch.x;
                }
                let text_w = right - 12.0 - (row.x + 14.0);
                let mut title = p.layout(&name(url), theme::LABEL, None);
                title.truncate(p.fonts, text_w);
                p.text(&title, row.x + 14.0, row.y + 8.0, t.text);
                let mut address = p.layout(url.split_once("://").map_or(url, |(_, rest)| rest), theme::SMALL, None);
                address.truncate(p.fonts, text_w);
                p.text(&address, row.x + 14.0, row.y + 28.0, t.text_muted);
            }
            if let Some(index) = forgotten {
                self.saved.remove(index);
            }
            y += rows as f32 * ROW_H + GAP;
            p.label("Add a provider", theme::LABEL, x, y, t.text);
            y += LABEL_H;
        }
        let (buttons, presets_h) = chips(p, width, PRESETS.map(|(name, _)| name));
        for ((name, url), button_at) in PRESETS.into_iter().zip(buttons) {
            let at = Rect::new(x + button_at.x, y + button_at.y, button_at.w, button_at.h);
            if button(p, ui, at, name, ButtonStyle::Secondary, true) {
                self.fields.replace(URL, url);
                // What comes next: the account to fill in, else the key.
                match url.find(ACCOUNT) {
                    Some(at) => self.fields.select(URL, at..at + ACCOUNT.len()),
                    None => self.fields.focus(KEY),
                }
                self.error = None;
            }
        }
        y += presets_h + GAP;
        let [url, key] = self.layouts(p, width);
        for (index, label, hint, (layout, h)) in
            [(URL, "Base URL", "https://…/v1", url), (KEY, "API key", "Leave empty if the server needs none", key)]
        {
            p.label(label, theme::LABEL, x, y, t.text);
            y += LABEL_H;
            self.fields.draw(index, p, ui, Rect::new(x, y, width, h), layout, hint, true);
            y += h + GAP;
        }
        if let Some(error) = &self.error {
            let text = p.layout(error, theme::SMALL, Some(width));
            p.text(&text, x, y, t.danger);
        }
    }

    /// Each field's layout and box height for `width`. Both wrap, so a long
    /// key stays inside its box, yet Enter moves on as in one-line fields.
    // ponytail: the key shows as typed; mask it once `Fields` can draw a
    // stand-in layout with mapped offsets.
    fn layouts(&self, p: &Painter, width: f32) -> [(TextLayout, f32); 2] {
        [URL, KEY].map(|index| {
            let layout = p.layout(self.fields.text(index), theme::BODY, Some(width - 2.0 * FIELD_PAD.0));
            let h = layout.height().max(layout.line_height()) + 2.0 * FIELD_PAD.1;
            (layout, h)
        })
    }
}

/// Where a button for each of `names` sits from the form's corner, its rows
/// wrapped to `width`, and their height.
fn chips<'a>(p: &Painter, width: f32, names: impl IntoIterator<Item = &'a str>) -> (Vec<Rect>, f32) {
    let (mut x, mut y) = (0.0, 0.0);
    let buttons = names.into_iter().map(|name| {
        let w = (p.layout(name, theme::LABEL, None).width() + 20.0).round();
        if x > 0.0 && x + w > width {
            (x, y) = (0.0, y + PRESET_H + 6.0);
        }
        x += w + 6.0;
        Rect::new(x - w - 6.0, y, w, PRESET_H)
    });
    let buttons = buttons.collect();
    (buttons, y + PRESET_H)
}

/// What a saved provider is called: the preset on its host, else the host,
/// with Messages for an address that speaks it (Cloudflare's `/messages`).
fn name(url: &str) -> String {
    let host =
        |url: &str| url.split_once("://").map_or("", |(_, rest)| rest.split('/').next().unwrap_or(rest)).to_owned();
    let name = PRESETS
        .iter()
        .find(|(_, preset)| host(preset) == host(url))
        .map_or_else(|| host(url), |(n, _)| (*n).to_owned());
    if url.ends_with("/messages") { format!("{name} · Messages") } else { name }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_checked_and_tidied() {
        let mut form = ProviderForm::new(" https://openrouter.ai/api/v1/chat/completions/ ", None, Vec::new());
        form.fields.focus(KEY);
        form.insert(" sk-or\n-v1 ");
        assert_eq!(form.submit(), Some(("https://openrouter.ai/api/v1".into(), Some("sk-or-v1".into()))));
        assert_eq!(
            ProviderForm::new("http://localhost:11434/v1", None, Vec::new()).submit(),
            Some(("http://localhost:11434/v1".into(), None))
        );
        for bad in ["", "openrouter.ai", "https://", "ftp://x", "https://a b", PRESETS[2].1] {
            let mut form = ProviderForm::new(bad, None, Vec::new());
            assert!(form.submit().is_none() && form.error.is_some(), "{bad}");
        }
    }

    #[test]
    fn the_cloudflare_preset_selects_the_account_to_fill_in() {
        let mut form = ProviderForm::new("", None, Vec::new());
        let url = PRESETS[2].1;
        form.fields.replace(URL, url);
        let at = url.find(ACCOUNT).unwrap();
        form.fields.select(URL, at..at + ACCOUNT.len());
        form.insert("abc");
        assert_eq!(form.submit().unwrap().0, "https://api.cloudflare.com/client/v4/accounts/abc/ai/v1");
    }

    #[test]
    fn saved_providers_are_named() {
        assert_eq!(name("https://api.anthropic.com/v1"), "Anthropic");
        assert_eq!(name("https://api.cloudflare.com/client/v4/accounts/abc/ai/v1"), "Cloudflare");
        assert_eq!(name("https://api.cloudflare.com/client/v4/accounts/abc/ai/v1/messages"), "Cloudflare · Messages");
        assert_eq!(name("http://localhost:8080"), "localhost:8080");
    }
}
