//! Visual design tokens. Every colour and size in the UI comes from here.
//!
//! Colours live in a [`Palette`] so the scheme can change at runtime; the
//! painter carries the active one (`p.theme`). Sizes and text styles are the
//! same in every scheme. The chrome is Zed-inspired: flat surfaces, hairline
//! borders, small radii and one accent. The story itself is warmer: prose
//! in a serif, and every character in a colour of their own
//! ([`character_color`]), so a busy scene reads at a glance.

use crate::paint::{Color, hex, hexa, mix};
use crate::text::{FontId, Style};

/// Every colour the UI uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// Chat area background.
    pub bg: Color,
    /// Sidebar and other panels.
    pub panel: Color,
    /// Raised surfaces: composer, cards, menus.
    pub surface: Color,
    /// Hovered element.
    pub hover: Color,
    /// Selected or pressed element.
    pub active: Color,
    /// Hairline dividers.
    pub border: Color,
    /// Borders of controls and floating surfaces.
    pub border_strong: Color,
    /// Border of the focused input.
    pub border_focus: Color,
    /// Primary text.
    pub text: Color,
    /// Secondary text.
    pub text_muted: Color,
    /// Hints, placeholders and captions.
    pub text_faint: Color,
    /// The single accent colour.
    pub accent: Color,
    /// Text and icons drawn on the accent colour.
    pub on_accent: Color,
    /// Text selection highlight.
    pub selection: Color,
    /// Errors and destructive actions.
    pub danger: Color,
    /// Drop shadows under floating surfaces.
    pub shadow: Color,
    /// Background of code blocks.
    pub code_bg: Color,
    /// Links in replies.
    pub link: Color,
    /// Laid over a picture so text on it stays legible.
    pub scrim: Color,
    /// Text drawn on [`Palette::scrim`].
    pub on_scrim: Color,
    /// Syntax colours: keyword, string, number, comment, function, type.
    pub syntax: [Color; 6],
}

impl Palette {
    /// Blends every colour towards `other` by `t` (for theme transitions).
    #[must_use]
    pub fn mix(&self, other: &Self, t: f32) -> Self {
        let m = |a, b| mix(a, b, t);
        Self {
            bg: m(self.bg, other.bg),
            panel: m(self.panel, other.panel),
            surface: m(self.surface, other.surface),
            hover: m(self.hover, other.hover),
            active: m(self.active, other.active),
            border: m(self.border, other.border),
            border_strong: m(self.border_strong, other.border_strong),
            border_focus: m(self.border_focus, other.border_focus),
            text: m(self.text, other.text),
            text_muted: m(self.text_muted, other.text_muted),
            text_faint: m(self.text_faint, other.text_faint),
            accent: m(self.accent, other.accent),
            on_accent: m(self.on_accent, other.on_accent),
            selection: m(self.selection, other.selection),
            danger: m(self.danger, other.danger),
            shadow: m(self.shadow, other.shadow),
            code_bg: m(self.code_bg, other.code_bg),
            link: m(self.link, other.link),
            scrim: m(self.scrim, other.scrim),
            on_scrim: m(self.on_scrim, other.on_scrim),
            syntax: std::array::from_fn(|i| m(self.syntax[i], other.syntax[i])),
        }
    }

    /// Whether text is dark on a light background.
    #[must_use]
    pub fn is_light(&self) -> bool {
        self.bg[0] + self.bg[1] + self.bg[2] > 1.5
    }
}

/// Neutral graphite with a white accent. The default.
pub const DARK: Palette = Palette {
    bg: hex(0x161618),
    panel: hex(0x1C1C1F),
    surface: hex(0x212124),
    hover: hex(0x2A2A2E),
    active: hex(0x333338),
    border: hex(0x2A2A2E),
    border_strong: hex(0x3A3A40),
    border_focus: hex(0x5A5A63),
    text: hex(0xEDEDEF),
    text_muted: hex(0xA0A0AB),
    text_faint: hex(0x6B6B76),
    accent: hex(0xEDEDEF),
    on_accent: hex(0x161618),
    selection: hexa(0xFFFFFF, 0.16),
    danger: hex(0xEB6F6F),
    shadow: hexa(0x000000, 0.5),
    code_bg: hex(0x1B1B1E),
    link: hex(0x8AB4F8),
    scrim: hexa(0x0B0B0D, 0.62),
    on_scrim: hex(0xF4F4F6),
    // GitHub Dark.
    syntax: [hex(0xFF7B72), hex(0xA5D6FF), hex(0x79C0FF), hex(0x8B949E), hex(0xD2A8FF), hex(0xFFA657)],
};

/// Zed's "One Dark": slate greys with a blue accent.
pub const BLUE: Palette = Palette {
    bg: hex(0x282C33),
    panel: hex(0x2F343E),
    surface: hex(0x2F343E),
    hover: hex(0x363C46),
    active: hex(0x454A56),
    border: hex(0x363C46),
    border_strong: hex(0x464B57),
    border_focus: hex(0x47679E),
    text: hex(0xDCE0E5),
    text_muted: hex(0xA9AFBC),
    text_faint: hex(0x6F7581),
    accent: hex(0x74ADE8),
    on_accent: hex(0x1B1F25),
    selection: hexa(0x74ADE8, 0.24),
    danger: hex(0xD07277),
    shadow: hexa(0x000000, 0.35),
    code_bg: hex(0x23272E),
    link: hex(0x74ADE8),
    scrim: hexa(0x16191E, 0.62),
    on_scrim: hex(0xF0F2F5),
    // Zed One Dark.
    syntax: [hex(0xB477CF), hex(0xA1C181), hex(0xBF956A), hex(0x5D636F), hex(0x73ADE9), hex(0x6EB4BF)],
};

/// Paper white with an ink accent.
pub const LIGHT: Palette = Palette {
    bg: hex(0xFFFFFF),
    panel: hex(0xF7F7F8),
    surface: hex(0xFFFFFF),
    hover: hex(0xEDEDF0),
    active: hex(0xE3E3E8),
    border: hex(0xE8E8EC),
    border_strong: hex(0xD6D6DC),
    border_focus: hex(0xA0A0AB),
    text: hex(0x18181B),
    text_muted: hex(0x5B5B66),
    text_faint: hex(0x9A9AA5),
    accent: hex(0x18181B),
    on_accent: hex(0xFFFFFF),
    selection: hexa(0x2563EB, 0.18),
    danger: hex(0xD93636),
    shadow: hexa(0x000000, 0.1),
    code_bg: hex(0xF6F6F8),
    link: hex(0x2563EB),
    scrim: hexa(0x0F0F12, 0.55),
    on_scrim: hex(0xFFFFFF),
    // GitHub Light.
    syntax: [hex(0xCF222E), hex(0x0A3069), hex(0x0550AE), hex(0x6E7781), hex(0x8250DF), hex(0x953800)],
};

/// Warm charcoal lit by an amber accent, like a page by lamplight.
pub const EMBER: Palette = Palette {
    bg: hex(0x16130F),
    panel: hex(0x1C1814),
    surface: hex(0x221D18),
    hover: hex(0x2C261F),
    active: hex(0x362F27),
    border: hex(0x2C261F),
    border_strong: hex(0x3E362C),
    border_focus: hex(0x7A6650),
    text: hex(0xEFE6DA),
    text_muted: hex(0xB3A794),
    text_faint: hex(0x7C7061),
    accent: hex(0xE8A55E),
    on_accent: hex(0x1A140E),
    selection: hexa(0xE8A55E, 0.22),
    danger: hex(0xE5736A),
    shadow: hexa(0x000000, 0.5),
    code_bg: hex(0x1B1713),
    link: hex(0xF2BA74),
    scrim: hexa(0x0E0B08, 0.62),
    on_scrim: hex(0xF6EEE3),
    // Gruvbox Dark.
    syntax: [hex(0xF07A62), hex(0xB8BB26), hex(0xD3869B), hex(0x928374), hex(0x83A598), hex(0xFABD2F)],
};

/// Old paper and sepia ink, with a burnt-sienna accent.
pub const PARCHMENT: Palette = Palette {
    bg: hex(0xF7F1E6),
    panel: hex(0xEFE7D8),
    surface: hex(0xFCF8F0),
    hover: hex(0xE9DFCC),
    active: hex(0xE0D4BE),
    border: hex(0xE3D8C4),
    border_strong: hex(0xD2C4AB),
    border_focus: hex(0xA8916F),
    text: hex(0x2A2118),
    text_muted: hex(0x6B5B4B),
    text_faint: hex(0x9C8C78),
    accent: hex(0x9A4F1C),
    on_accent: hex(0xFFF8EE),
    selection: hexa(0x9A4F1C, 0.18),
    danger: hex(0xC0392B),
    shadow: hexa(0x3B2A14, 0.12),
    code_bg: hex(0xF1E9DA),
    link: hex(0x2F6690),
    scrim: hexa(0x1E150C, 0.55),
    on_scrim: hex(0xFFF9F0),
    // Gruvbox Light.
    syntax: [hex(0x9D0006), hex(0x79740E), hex(0x8F3F71), hex(0x928374), hex(0x076678), hex(0xB57614)],
};

/// The colour schemes offered in settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scheme {
    /// [`DARK`].
    #[default]
    Dark,
    /// [`EMBER`].
    Ember,
    /// [`BLUE`].
    Blue,
    /// [`LIGHT`].
    Light,
    /// [`PARCHMENT`].
    Parchment,
}

impl Scheme {
    /// Every scheme, in settings order.
    pub const ALL: [Self; 5] = [Self::Dark, Self::Ember, Self::Blue, Self::Light, Self::Parchment];

    /// Its colours.
    #[must_use]
    pub fn palette(self) -> &'static Palette {
        match self {
            Self::Dark => &DARK,
            Self::Ember => &EMBER,
            Self::Blue => &BLUE,
            Self::Light => &LIGHT,
            Self::Parchment => &PARCHMENT,
        }
    }

    /// Name shown in settings.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Ember => "Ember",
            Self::Blue => "One Dark",
            Self::Light => "Light",
            Self::Parchment => "Parchment",
        }
    }

    /// Value stored in the config file.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Ember => "ember",
            Self::Blue => "blue",
            Self::Light => "light",
            Self::Parchment => "parchment",
        }
    }

    /// Parses a config value; unknown values fall back to the default.
    #[must_use]
    pub fn from_key(key: Option<&str>) -> Self {
        Self::ALL.into_iter().find(|s| Some(s.key()) == key).unwrap_or_default()
    }

    /// Whether the OS chrome (title bar) should be light.
    #[must_use]
    pub fn is_light(self) -> bool {
        self.palette().is_light()
    }
}

/// The colour of the character called `name` in `palette`, the same
/// wherever they appear (their name, bubble, portrait ring and initial):
/// the one the user `chose` (`#rrggbb`), made light or deep enough to read
/// on this scheme, or, with none, one picked from the name, so the same
/// name always gets the same colour.
#[must_use]
pub fn character_color(palette: &Palette, name: &str, chose: &str) -> Color {
    if let Some(rgb) = parse_hex(chose) {
        return legible(palette, rgb);
    }
    // FNV-1a over the name as names are compared: case and marks ignored.
    let mut hash: u32 = 0x811C_9DC5;
    for c in name.chars().flat_map(char::to_lowercase).filter(|c| c.is_alphanumeric()) {
        hash = (hash ^ c as u32).wrapping_mul(0x0100_0193);
    }
    let hue = (hash % 360) as f32;
    let (lightness, chroma) = if palette.is_light() { (0.5, 0.13) } else { (0.78, 0.11) };
    oklch(lightness, chroma, hue)
}

/// Perceptual lightness a character's colour keeps on `palette`, so names
/// in it stay readable: light on dark schemes, deep on light ones.
fn readable(palette: &Palette) -> (f32, f32) {
    if palette.is_light() { (0.3, 0.55) } else { (0.62, 0.95) }
}

/// `rgb` (sRGB, 0–1) with its lightness moved into what `palette` reads
/// well, hue and chroma kept: a colour already readable is unchanged.
#[must_use]
pub fn legible(palette: &Palette, rgb: [f32; 3]) -> Color {
    let (lightness, chroma, hue) = to_oklch(rgb);
    let (low, high) = readable(palette);
    if (low..=high).contains(&lightness) {
        return [rgb[0], rgb[1], rgb[2], 1.0];
    }
    oklch(lightness.clamp(low, high), chroma, hue)
}

/// Whether [`legible`] changes `rgb` on `palette`.
#[must_use]
pub fn adjusted(palette: &Palette, rgb: [f32; 3]) -> bool {
    !(readable(palette).0..=readable(palette).1).contains(&to_oklch(rgb).0)
}

/// An sRGB colour (0–1 each) from `#rrggbb`, `rrggbb` or `#rgb`.
#[must_use]
pub fn parse_hex(text: &str) -> Option<[f32; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize, width: usize| {
        let value = u8::from_str_radix(&digits[at * width..(at + 1) * width], 16).ok()?;
        Some(f32::from(if width == 1 { value * 17 } else { value }) / 255.0)
    };
    let width = match digits.len() {
        3 => 1,
        6 => 2,
        _ => return None,
    };
    Some([channel(0, width)?, channel(1, width)?, channel(2, width)?])
}

/// `rgb` (sRGB, 0–1 each) as `#rrggbb`.
#[must_use]
pub fn to_hex(rgb: [f32; 3]) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(rgb[0]), byte(rgb[1]), byte(rgb[2]))
}

/// OKLCH (lightness 0–1, chroma, hue in degrees) of an sRGB colour.
fn to_oklch(rgb: [f32; 3]) -> (f32, f32, f32) {
    let linear = rgb.map(|c| if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
    let [r, g, b] = linear;
    let long = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let medium = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let short = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let lightness = 0.210_454_26 * long + 0.793_617_8 * medium - 0.004_072_047 * short;
    let green_red = 1.977_998_5 * long - 2.428_592_2 * medium + 0.450_593_7 * short;
    let blue_yellow = 0.025_904_037 * long + 0.782_771_77 * medium - 0.808_675_77 * short;
    let hue = blue_yellow.atan2(green_red).to_degrees().rem_euclid(360.0);
    (lightness, green_red.hypot(blue_yellow), hue)
}

/// An opaque sRGB colour from OKLCH (perceptual lightness 0–1, chroma,
/// hue in degrees), its chroma reduced until it fits sRGB.
fn oklch(lightness: f32, chroma: f32, hue: f32) -> Color {
    let (sin, cos) = hue.to_radians().sin_cos();
    let mut chroma = chroma;
    loop {
        // OKLab's axes, then the cone responses (long, medium, short).
        let (green_red, blue_yellow) = (chroma * cos, chroma * sin);
        let long = (lightness + 0.396_337_8 * green_red + 0.215_803_76 * blue_yellow).powi(3);
        let medium = (lightness - 0.105_561_35 * green_red - 0.063_854_17 * blue_yellow).powi(3);
        let short = (lightness - 0.089_484_18 * green_red - 1.291_485_5 * blue_yellow).powi(3);
        let linear = [
            4.076_741_7 * long - 3.307_711_6 * medium + 0.230_969_94 * short,
            -1.268_438 * long + 2.609_757_4 * medium - 0.341_319_38 * short,
            -0.004_196_086 * long - 0.703_418_6 * medium + 1.707_614_7 * short,
        ];
        if chroma < 0.005 || linear.iter().all(|c| (0.0..=1.0).contains(c)) {
            let encode = |c: f32| {
                let c = c.clamp(0.0, 1.0);
                if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
            };
            return [encode(linear[0]), encode(linear[1]), encode(linear[2]), 1.0];
        }
        chroma *= 0.85;
    }
}

/// Corner radius of small elements: list rows, buttons.
pub const RADIUS_SM: f32 = 4.0;
/// Corner radius of containers: composer, menus, cards.
pub const RADIUS: f32 = 6.0;
/// Sidebar width.
pub const SIDEBAR_WIDTH: f32 = 256.0;
/// Height of the header bar above the main area.
pub const HEADER_HEIGHT: f32 = 44.0;
/// Maximum width of the message column.
pub const COLUMN_WIDTH: f32 = 760.0;

/// Message body text.
pub const BODY: Style = Style::regular(14.5);
/// Secondary UI text.
pub const SMALL: Style = Style::regular(13.0);
/// Captions and hints.
pub const TINY: Style = Style::regular(11.5);
/// Buttons and list items.
pub const LABEL: Style = Style::semibold(13.0);
/// Small headings and captions that need weight.
pub const CAPTION: Style = Style::semibold(11.5);
/// Page and section titles.
pub const TITLE: Style = Style::semibold(22.0);
/// What characters say and do, and the user's part of a story: a serif
/// made for long reading.
pub const PROSE: Style = Style { font: FontId::SERIF, size: 15.5, line_height: 1.6 };
/// The title a story opens under, in the prose's serif.
pub const DISPLAY: Style = Style { font: FontId::SERIF_BOLD, size: 26.0, line_height: 1.25 };
/// A scene card: where the story is, between the turns.
pub const SCENE: Style = Style { font: FontId::SERIF_ITALIC, size: 15.0, line_height: 1.55 };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_get_their_own_legible_colour() {
        for palette in Scheme::ALL.map(Scheme::palette) {
            let colors: Vec<Color> = ["Katniss", "Peeta", "Haymitch", "Effie", "Rue"]
                .iter()
                .map(|n| character_color(palette, n, ""))
                .collect();
            for c in &colors {
                assert!(c.iter().all(|v| (0.0..=1.0).contains(v)) && (c[3] - 1.0).abs() < f32::EPSILON, "{c:?}");
                // Light enough on dark schemes, dark enough on light ones.
                let luma = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
                assert!(if palette.is_light() { luma < 0.55 } else { luma > 0.5 }, "{luma}");
            }
            let apart = |a: &Color, b: &Color| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 0.01);
            assert!(colors.windows(2).all(|w| apart(&w[0], &w[1])), "different names, different colours");
            assert_eq!(character_color(palette, "Katniss", ""), character_color(palette, "katniss!", ""), "folded");
        }
        // Grey has no hue to lose; white stays white.
        let white = oklch(1.0, 0.0, 0.0);
        assert!(white[..3].iter().all(|v| (v - 1.0).abs() < 0.01), "{white:?}");
    }

    #[test]
    fn chosen_colours_are_kept_where_readable() {
        assert_eq!(parse_hex("#E8A55E").map(to_hex).as_deref(), Some("#e8a55e"));
        assert_eq!(parse_hex(" f00 ").map(to_hex).as_deref(), Some("#ff0000"), "short and bare");
        for bad in ["", "#", "#12345", "#gggggg", "#1234567", "#é12345"] {
            assert!(parse_hex(bad).is_none(), "{bad:?}");
        }
        // OKLCH there and back.
        for hex in ["#e8a55e", "#3e63dd", "#12a594", "#808080"] {
            let rgb = parse_hex(hex).unwrap();
            let (l, c, h) = to_oklch(rgb);
            assert_eq!(to_hex(oklch(l, c, h)[..3].try_into().unwrap()), hex);
        }
        let amber = parse_hex("#e8a55e").unwrap();
        assert_eq!(character_color(&DARK, "Mira", "#e8a55e")[..3], amber, "readable on dark: as chosen");
        assert!(!adjusted(&DARK, amber) && adjusted(&LIGHT, amber));
        // Black on a dark scheme, white on a light one: moved to where they read.
        let luma = |c: Color| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        assert!(luma(character_color(&DARK, "Shade", "#000000")) > 0.25);
        assert!(luma(character_color(&LIGHT, "Ghost", "#ffffff")) < 0.5);
        // Nothing chosen (or nonsense): from the name.
        assert_eq!(character_color(&DARK, "Mira", "nope"), character_color(&DARK, "Mira", ""));
    }

    #[test]
    fn scheme_keys_round_trip() {
        for scheme in Scheme::ALL {
            assert_eq!(Scheme::from_key(Some(scheme.key())), scheme);
        }
        assert_eq!(Scheme::from_key(Some("neon")), Scheme::Dark);
        assert_eq!(Scheme::from_key(None), Scheme::Dark);
    }
}
