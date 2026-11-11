//! The settings, a modal with a page per section down its left:
//! appearance, models, usage, and the provider and data.

use arboard::Clipboard;
use winit::event::KeyEvent;
use winit::keyboard::ModifiersState;
use winit::window::CursorIcon;

use crate::app::Action;
use crate::chat::{ReasoningView, format_cost, group_digits};
use crate::paint::{Painter, Rect, fade, mix};
use crate::provider::ProviderForm;
use crate::text::Style;
use crate::theme::{self, Palette, Scheme};
use crate::ui::{ButtonStyle, Scroll, Ui, button, chevron, icon, icon_button, id, modal, row, segmented};

/// The modal's size, where the window has it to give.
const SIZE: (f32, f32) = (940.0, 660.0);
/// Width of the list of pages at its left.
const NAV: f32 = 196.0;
/// Height of a settings row.
const ROW_H: f32 = 58.0;

/// Usage summed over every saved session.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Totals {
    /// USD spent.
    pub cost: f64,
    /// Tokens billed.
    pub tokens: u64,
    /// Sessions with at least one message.
    pub sessions: usize,
}

/// The settings pages, in list order.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Tab {
    #[default]
    Appearance,
    Models,
    Usage,
    Provider,
}

impl Tab {
    const ALL: [Self; 4] = [Self::Appearance, Self::Models, Self::Usage, Self::Provider];

    /// Its name, its icon, and what the page is for.
    fn label(self) -> (&'static str, char, &'static str) {
        match self {
            Self::Appearance => ("Appearance", icon::PALETTE, "How OpenRP looks, and how replies are shown."),
            Self::Models => ("Models", icon::SPARKLES, "The story's model is picked under the message box."),
            Self::Usage => ("Usage", icon::CHART, "Totals across every story saved on this device."),
            Self::Provider => ("Provider", icon::PLUG, "Where replies come from. Your stories stay on this device."),
        }
    }
}

/// What the provider tab knows of the saved provider.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Provider {
    /// The provider's API root; empty when none was saved.
    pub base_url: String,
    /// A key is saved for the provider.
    pub has_key: bool,
    /// Addresses of the providers used before, latest first.
    pub saved: Vec<String>,
}

/// Open page and scroll state of the settings.
pub struct SettingsView {
    tab: Tab,
    scroll: Scroll,
    /// The providers saved, and a new one being entered.
    form: ProviderForm,
}

impl Default for SettingsView {
    fn default() -> Self {
        Self { tab: Tab::default(), scroll: Scroll::default(), form: ProviderForm::new("", None, Vec::new()) }
    }
}

impl SettingsView {
    /// Shows `provider`, starting the provider form afresh from it.
    pub fn set_provider(&mut self, provider: Provider) {
        let active = Some((provider.base_url, provider.has_key)).filter(|(url, _)| !url.is_empty());
        self.form = ProviderForm::new("", active, provider.saved);
    }

    /// Keyboard input for the provider form, on its tab.
    pub fn key(
        &mut self,
        event: &KeyEvent,
        mods: ModifiersState,
        cb: &mut Option<Clipboard>,
        actions: &mut Vec<Action>,
    ) {
        if self.tab == Tab::Provider && self.form.key(event, mods, cb) {
            self.connect(actions);
        }
    }

    /// Text committed by an input method goes to the provider form, on its tab.
    pub fn insert(&mut self, text: &str) {
        if self.tab == Tab::Provider {
            self.form.insert(text);
        }
    }

    /// The provider form's caret, for the input method's window.
    #[must_use]
    pub fn caret(&self) -> Option<Rect> {
        (self.tab == Tab::Provider).then(|| self.form.caret()).flatten()
    }

    /// Asks to use the provider entered, if the form accepts it.
    fn connect(&mut self, actions: &mut Vec<Action>) {
        if let Some((base_url, api_key)) = self.form.submit() {
            actions.push(Action::Connect { base_url, api_key });
        }
    }

    /// Draws the settings over `view`. `utility` names the background
    /// model. Returns where that model's button is when it was clicked, to
    /// open the model menu there, and whether the settings were left (the
    /// close button, or a click outside them).
    #[allow(clippy::too_many_arguments, reason = "one argument per setting shown; a struct would only rename them")]
    pub fn draw(
        &mut self,
        p: &mut Painter,
        ui: &mut Ui,
        view: Rect,
        scheme: Scheme,
        reasoning: ReasoningView,
        utility: &str,
        totals: Totals,
        actions: &mut Vec<Action>,
    ) -> (Option<Rect>, bool) {
        let t = p.theme;
        let (card, outside) = modal(p, ui, view, SIZE, "settings");
        let mut leave = outside;

        // The pages, down the left.
        p.label("Settings", theme::HEADING, card.x + 24.0, card.y + 20.0, t.text);
        let mut y = card.y + 62.0;
        for tab in Tab::ALL {
            let (label, glyph, _) = tab.label();
            let item = Rect::new(card.x + 12.0, y, NAV - 24.0, 34.0);
            let hovered = ui.hovered(item);
            let hover = ui.anim(id(("settings-tab", label)), f32::from(u8::from(hovered)));
            let selected = self.tab == tab;
            p.rect(item, if selected { t.active } else { fade(t.hover, hover) }, theme::RADIUS_SM);
            let ink = mix(t.text_muted, t.text, if selected { 1.0 } else { hover });
            icon::draw(p, glyph, (item.x + 18.0, item.y + 17.0), 15.0, ink);
            p.label(label, theme::SMALL, item.x + 34.0, item.y + 7.0, ink);
            if hovered {
                ui.cursor = CursorIcon::Pointer;
                if ui.clicked(item) && !selected {
                    self.tab = tab;
                    self.scroll.reset();
                }
            }
            y += 38.0;
        }
        let version = concat!("OpenRP ", env!("CARGO_PKG_VERSION"));
        p.label(version, theme::TINY, card.x + 24.0, card.bottom() - 36.0, t.text_faint);
        p.rect(Rect::new(card.x + NAV, card.y + 1.0, 1.0, card.h - 2.0), t.border, 0.0);

        // The page, scrolling beside them.
        let area = Rect::new(card.x + NAV + 1.0, card.y + 1.0, card.w - NAV - 2.0, card.h - 2.0);
        let (x, width) = (area.x + 32.0, area.w - 64.0);
        let offset = self.scroll.update(ui, ui.hovered(area));
        let clip = p.push_clip(area);
        let (name, _, about) = self.tab.label();
        p.label(name, theme::TITLE, x, area.y + 22.0 - offset, t.text);
        let about = p.layout(about, theme::SMALL, Some(width - 40.0));
        p.text(&about, x, area.y + 58.0 - offset, t.text_muted);
        let mut y = area.y + 58.0 - offset + about.height() + 26.0;
        let mut pick_utility = None;
        // Theme cards and usage stats share a three-column grid.
        let card_w = ((width - 24.0) / 3.0).floor();

        match self.tab {
            Tab::Appearance => {
                y = heading(p, x, y, "Theme");
                let preview_h = (card_w * 0.58).round();
                let card_h = preview_h + 46.0;
                for (i, choice) in Scheme::ALL.into_iter().enumerate() {
                    let tile = Rect::new(x + i as f32 * (card_w + 12.0), y, card_w, card_h);
                    if theme_card(p, ui, tile, preview_h, choice, choice == scheme) && choice != scheme {
                        actions.push(Action::SetTheme(choice));
                    }
                }
                y += card_h + 34.0;

                y = heading(p, x, y, "Replies");
                let line = Rect::new(x, y, width, ROW_H);
                row(p, line, icon::BRAIN, "Model reasoning", "What the model considered before answering.", 290.0);
                let labels = ReasoningView::ALL.map(ReasoningView::label);
                let selected = ReasoningView::ALL.iter().position(|v| *v == reasoning).unwrap_or(0);
                if let Some(choice) = segmented(p, ui, control(line, 264.0), &labels, selected) {
                    actions.push(Action::SetReasoningView(ReasoningView::ALL[choice]));
                }
                y += ROW_H;
            }
            Tab::Models => {
                let line = Rect::new(x, y, width, ROW_H);
                let detail = "Writes memory reviews, summaries and generated characters.";
                row(p, line, icon::SPARKLES, "Background model", detail, 246.0);
                let picker = control(line, 220.0);
                if dropdown(p, ui, picker, utility) {
                    pick_utility = Some(picker);
                }
                y += ROW_H + 14.0;
                let tip = p.layout(
                    "A cheaper model here saves money: it never writes the story itself. Summaries use the story's model when this \
                     one holds less context.",
                    theme::TINY,
                    Some(width),
                );
                p.text(&tip, x, y, t.text_faint);
                y += tip.height();
            }
            Tab::Usage => {
                // Providers rarely list prices: costs show only once something cost money.
                let unpriced = totals.cost <= 0.0;
                let stats = [
                    ("Total spent", format_cost(totals.cost)),
                    ("Tokens", group_digits(totals.tokens)),
                    ("Stories", group_digits(totals.sessions as u64)),
                ];
                for (i, (label, value)) in stats.iter().skip(usize::from(unpriced)).enumerate() {
                    let stat = Rect::new(x + i as f32 * (card_w + 12.0), y, card_w, 84.0);
                    p.bordered(stat, t.surface, theme::RADIUS, 1.0, t.border);
                    p.label(label, theme::CAPTION, stat.x + 16.0, stat.y + 16.0, t.text_faint);
                    p.label(value, Style::semibold(24.0), stat.x + 16.0, stat.y + 38.0, t.text);
                }
                y += 84.0;
            }
            Tab::Provider => {
                let inner = width - 32.0;
                let form_h = self.form.height(p, inner);
                let form = Rect::new(x, y, width, 16.0 + form_h + 20.0 + 30.0 + 16.0);
                p.bordered(form, t.surface, theme::RADIUS, 1.0, t.border);
                self.form.draw(p, ui, form.x + 16.0, form.y + 16.0, inner, actions);
                let connect = Rect::new(form.right() - 16.0 - 120.0, form.y + 16.0 + form_h + 20.0, 120.0, 30.0);
                if button(p, ui, connect, "Connect", ButtonStyle::Primary, true) {
                    self.connect(actions);
                }
                y += form.h + 34.0;

                y = heading(p, x, y, "On this computer");
                let line = Rect::new(x, y, width, ROW_H);
                row(p, line, icon::FOLDER, "Your data", "Stories, worlds and characters are files in ~/.openrp", 142.0);
                if button(p, ui, control(line, 116.0), "Open folder", ButtonStyle::Secondary, true) {
                    actions.push(Action::OpenDataDir);
                }
                y += ROW_H;
            }
        }
        p.set_clip(clip);
        let content = y + offset - area.y + 24.0;
        self.scroll.finish(ui, content, area.h);
        self.scroll.bar(p, area, content);

        // Over the page, so what scrolls under it cannot take its click.
        if icon_button(p, ui, Rect::new(card.right() - 44.0, card.y + 14.0, 30.0, 30.0), icon::X, true) {
            leave = true;
        }
        (pick_utility, leave)
    }
}

/// A button showing `label` with a chevron, for opening a menu. Returns
/// whether it was clicked.
fn dropdown(p: &mut Painter, ui: &mut Ui, rect: Rect, label: &str) -> bool {
    let t = p.theme;
    let hovered = ui.hovered(rect);
    let hover = ui.anim(id(("dropdown", rect.y.to_bits())), f32::from(u8::from(hovered)));
    p.bordered(rect, mix(t.surface, t.hover, hover), theme::RADIUS_SM, 1.0, t.border_strong);
    let mut text = p.layout(label, theme::LABEL, None);
    text.truncate(p.fonts, rect.w - 40.0);
    p.text(&text, rect.x + 12.0, rect.y + (rect.h - text.height()) * 0.5, t.text);
    chevron(p, rect.right() - 20.0, rect.y + rect.h * 0.5 - 2.0, true, t.text_muted);
    if hovered {
        ui.cursor = CursorIcon::Pointer;
    }
    ui.clicked(rect)
}

/// A heading over a group of rows; returns where they start.
fn heading(p: &mut Painter, x: f32, y: f32, title: &str) -> f32 {
    p.label(title, theme::LABEL, x, y, p.theme.text);
    y + 30.0
}

/// A control of `width` right-aligned in `row`.
fn control(row: Rect, width: f32) -> Rect {
    Rect::new(row.right() - 12.0 - width, row.y + (row.h - 32.0) * 0.5, width, 32.0)
}

/// A selectable card previewing `scheme`. Returns whether it was clicked.
fn theme_card(p: &mut Painter, ui: &mut Ui, card: Rect, preview_h: f32, scheme: Scheme, selected: bool) -> bool {
    let t = p.theme;
    let hovered = ui.hovered(card);
    let hover = ui.anim(id(("theme-card", scheme.key())), f32::from(u8::from(hovered)));
    let border = if selected { t.accent } else { mix(t.border, t.border_strong, hover) };
    p.bordered(card, t.surface, theme::RADIUS + 2.0, if selected { 2.0 } else { 1.0 }, border);
    preview(p, Rect::new(card.x + 8.0, card.y + 8.0, card.w - 16.0, preview_h), scheme.palette());

    // Its name, ticked when it is the one in use.
    let label = Rect::new(card.x + 14.0, card.y + preview_h + 8.0, card.w - 28.0, 38.0);
    let name = p.layout(scheme.label(), theme::LABEL, None);
    p.text(&name, label.x, label.y + (label.h - name.height()) * 0.5, t.text);
    if selected {
        icon::draw(p, icon::CHECK, (label.right() - 7.0, label.y + label.h * 0.5), 15.0, t.text);
    }

    if hovered {
        ui.cursor = CursorIcon::Pointer;
    }
    ui.clicked(card)
}

/// A miniature of the app drawn in `c`'s colours: the sidebar on the
/// window, the story on its canvas.
fn preview(p: &mut Painter, r: Rect, c: &Palette) {
    p.bordered(r, c.panel, theme::RADIUS_SM, 1.0, c.border_strong);

    // Sidebar: the selected row, then a few more.
    let side = Rect::new(r.x + 1.0, r.y + 1.0, (r.w * 0.28).round(), r.h - 2.0);
    p.rect(Rect::new(side.x + 5.0, side.y + 7.0, side.w - 10.0, 9.0), c.border_strong, 4.0);
    p.rect(Rect::new(side.x + 5.0, side.y + 21.0, side.w - 10.0, 9.0), c.active, 3.0);
    for (i, w) in [0.6, 0.5, 0.7, 0.45].into_iter().enumerate() {
        let row = Rect::new(side.x + 8.0, side.y + 36.0 + i as f32 * 9.0, (side.w - 16.0) * w, 3.0);
        p.rect(row, fade(c.text_muted, 0.45), 1.5);
    }

    // The canvas: bubbles and the composer with its send button.
    let canvas = Rect::new(side.right(), r.y + 5.0, r.right() - 5.0 - side.right(), r.h - 10.0);
    p.bordered(canvas, c.bg, 5.0, 1.0, c.border);
    let pad = (canvas.w * 0.1).round();
    let col = Rect::new(canvas.x + pad, canvas.y, canvas.w - 2.0 * pad, canvas.h);
    for (i, w) in [0.9, 0.7].into_iter().enumerate() {
        let y = col.y + 9.0 + i as f32 * 20.0;
        p.rect(Rect::new(col.x, y + 1.0, 7.0, 7.0), fade(c.text_muted, 0.5), 3.5);
        p.bordered(Rect::new(col.x + 11.0, y, (col.w - 11.0) * w, 14.0), c.surface, 4.0, 1.0, c.border);
        p.rect(Rect::new(col.x + 15.0, y + 5.5, (col.w - 11.0) * w * 0.6, 3.0), fade(c.text, 0.55), 1.5);
    }
    let composer = Rect::new(col.x, canvas.bottom() - 22.0, col.w, 16.0);
    p.bordered(composer, c.surface, 6.0, 1.0, c.border_strong);
    p.rect(Rect::new(composer.right() - 13.0, composer.y + 3.0, 10.0, 10.0), c.accent, 5.0);
}
