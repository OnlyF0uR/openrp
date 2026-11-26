//! Screens drawn offscreen and saved as PNGs, to look at the design in
//! every scheme without opening the app. Run by hand:
//! `OPENRP_SNAPSHOTS=<folder> cargo test -p openrp snapshots -- --ignored`

use std::path::{Path, PathBuf};

use openrp_core::{
    CastMember, Character, Mode, Persona, Player, Portraits, Role, Session, StoredMessage, ToolCall, ToolResult, Usage,
    World,
};
use serde_json::json;

use super::{Chat, Menu, Page, Reasoning, tools};
use crate::gpu::Offscreen;
use crate::image::ImageAtlas;
use crate::library::Kind;
use crate::login::Login;
use crate::paint::{Painter, Rect};
use crate::text::{Fonts, GlyphAtlas};
use crate::theme::Scheme;
use crate::ui::Ui;

/// Logical size of every snapshot, drawn at a scale of one.
const SIZE: (u32, u32) = (1440, 900);

#[test]
#[ignore = "draws on the GPU; run by hand to look at the design"]
fn snapshots() {
    let Some(dir) = std::env::var_os("OPENRP_SNAPSHOTS").map(PathBuf::from) else { return };
    let Some(mut gpu) = Offscreen::new() else { return };
    std::fs::create_dir_all(&dir).expect("snapshot folder");
    let (w, h) = (96, 60);
    write_png(&dir.join("world.png"), (w, h), &dusk(w, h));
    for (name, top, bottom) in PEOPLE {
        write_png(&dir.join(format!("{name}.png")), (60, 80), &gradient((60, 80), top, bottom));
    }
    let fonts = Fonts::load();
    for scheme in Scheme::ALL {
        let mut settings = story(&dir);
        settings.settings_open = true;
        let mut worlds = story(&dir);
        worlds.page = Page::Library(Kind::World);
        // The thinking menu, open from its button in the Direction form.
        let mut menu = story(&dir);
        menu.edit_direction();
        menu.menu = Some(Menu::Reasoning);
        menu.menu_anchor = Rect::new(788.0, 308.0, 168.0, 32.0);
        // A story's menu, open under its row in the sidebar.
        let mut session = story(&dir);
        session.menu = Some(Menu::Session(session.current));
        session.session_menu = Rect::new(8.0, 216.0, 244.0, 46.0);
        let mut picker = story(&dir);
        picker.menu = Some(Menu::CastLibrary);
        let mut spotlight = story(&dir);
        spotlight.open_spotlight();
        let mut dialog = story(&dir);
        dialog.edit_scene();
        let mut direction = story(&dir);
        direction.edit_direction();
        // A chat's direction leads to who the user is.
        let mut steer = chat(&dir);
        steer.edit_direction();
        let mut rename = story(&dir);
        let mut editor = crate::editor::Editor::default();
        editor.insert("The heron letter");
        rename.title_edit = Some((rename.current, editor));
        let mut ooc = story(&dir);
        if let Some(said) = ooc.current().entries.iter_mut().rev().find(|e| e.message.role == Role::User) {
            said.message.content =
                "\"I'm staying.\" (She shouldn't trust me yet.) \"I have a letter for you, Mira.\"\n/ooc Keep it tense."
                    .into();
        }
        ooc.composer.insert("I slide the letter (the heron side up) across the table.");
        ooc.composer.set_mark(Some(ooc.composer.cursor()));
        ooc.composer.insert(" Have Mira notice the broken seal, but say nothing yet.");
        let mut form = story(&dir);
        form.page = Page::Library(Kind::World);
        form.library.open_form(Kind::World, "w");
        let mut characters = home(&dir);
        characters.show(Page::Library(Kind::Character));
        let mut hello = home(&dir);
        hello.chat_with("tobin", true, &mut Vec::new());
        let screens = vec![
            ("empty", Chat::new(None, Reasoning::Auto, Vec::new())),
            ("home", home(&dir)),
            ("characters", characters),
            ("chat", chat(&dir)),
            ("hello", hello),
            ("story", story(&dir)),
            ("settings", settings),
            ("worlds", worlds),
            ("menu", menu),
            ("session", session),
            ("picker", picker),
            ("spotlight", spotlight),
            ("dialog", dialog),
            ("direction", direction),
            ("steer", steer),
            ("form", form),
            ("rename", rename),
            ("ooc", ooc),
        ];
        for (name, mut chat) in screens {
            let pixels = frame(&mut gpu, &fonts, scheme, &mut |p, ui, view| {
                chat.draw(p, ui, view, scheme, &mut Vec::new());
            });
            write_png(&dir.join(format!("{name}-{}.png", scheme.key())), SIZE, &pixels);
        }
        let mut login = Login::new(None, "", vec!["https://api.anthropic.com/v1".into()]);
        let pixels = frame(&mut gpu, &fonts, scheme, &mut |p, ui, view| login.draw(p, ui, view, &mut Vec::new()));
        write_png(&dir.join(format!("login-{}.png", scheme.key())), SIZE, &pixels);
    }
}

/// Draws a screen with `draw` until its pictures have decoded and its
/// animations settled, then renders the last frame.
fn frame(
    gpu: &mut Offscreen,
    fonts: &Fonts,
    scheme: Scheme,
    draw: &mut dyn FnMut(&mut Painter, &mut Ui, Rect),
) -> Vec<u8> {
    let (mut atlas, mut images) = (GlyphAtlas::default(), ImageAtlas::default());
    let mut ui = Ui::default();
    (ui.focused, ui.mouse) = (true, (-1.0, -1.0));
    let view = Rect::new(0.0, 0.0, SIZE.0 as f32, SIZE.1 as f32);
    let palette = scheme.palette();
    let mut instances = Vec::new();
    for _ in 0..4 {
        // A long frame: every ease lands at once.
        ui.begin(1.0, 10.0);
        let mut p = Painter::new(fonts, (&mut atlas, &mut images), &mut instances, 1.0, view, palette);
        draw(&mut p, &mut ui, view);
        crate::titlebar::draw(&mut p, &mut ui, view, false);
        ui.end();
        // Pictures asked for decode here, as the app's workers would.
        for key in images.take_wanted() {
            let pixels = crate::image::thumbnail(&key);
            images.insert(key, pixels);
        }
    }
    gpu.render(SIZE, &instances, (&mut atlas.uploads, &mut images.uploads), 1.0, palette.bg)
}

/// A story two turns in: a world with a picture, a cast of three (one
/// elsewhere), the user's character and a scene.
fn story(dir: &Path) -> Chat {
    let speak = |id: &str, messages: serde_json::Value| ToolResult {
        call: ToolCall {
            call_id: id.into(),
            name: tools::SPEAK.into(),
            arguments: json!({ "scene": "", "introduce": [], "messages": messages }).to_string(),
        },
        output: String::new(),
    };
    let reply = |call: ToolResult| {
        let mut message = StoredMessage::new(Role::Assistant, String::new());
        message.tool_calls = vec![call];
        message.model = Some("claude-opus-5-5".into());
        message.usage = Usage::new(1_240, 312);
        message
    };
    let member = |id: &str, name: &str, description: &str, present| CastMember {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        present,
        ..CastMember::default()
    };
    let session = Session {
        version: Session::VERSION,
        id: "s".into(),
        title: "The heron letter".into(),
        created: 1,
        updated: 2,
        mode: Mode::Story,
        world: Some("w".into()),
        cast: vec![
            member("m", "Mira Vale", "A lantern-maker with ink-stained hands.", true),
            member("t", "Tobin Ashe", "A retired duellist who keeps the tea stall.", true),
            member("o", "Oren", "The harbourmaster.", false),
        ],
        player: Some(Player {
            name: "Wren".into(),
            description: "A courier new to the city.".into(),
            ..Player::default()
        }),
        scene: "The night market under the old aqueduct; paper lanterns, rain on canvas awnings.".into(),
        memories: vec!["Wren carries a letter sealed with a heron.".into()],
        model: "claude-opus-5-5".into(),
        effort: "high".into(),
        note: String::new(),
        direction: openrp_core::Direction::default(),
        reviewed: 0,
        messages: vec![
            StoredMessage::new(
                Role::User,
                "I shake the rain off my coat and step under the awning. \"Is this where I find the lantern-maker?\""
                    .into(),
            ),
            reply(speak(
                "a",
                json!([
                    { "character": "Mira Vale", "action": "looks up from a half-folded lantern",
                      "text": "Depends who's asking. *She sets the paper down.* You've got courier's boots." },
                    { "character": "Tobin Ashe", "action": "",
                      "text": "She means welcome. Tea's on me if you're staying." }
                ]),
            )),
            StoredMessage::new(
                Role::User,
                "\"I'm staying. I have a letter for you, Mira. It's sealed with a heron.\"".into(),
            ),
            reply(speak(
                "b",
                json!([{ "character": "Mira Vale", "action": "goes very still",
                         "text": "A heron. *Her voice drops.* Close the flap behind you, Wren." }]),
            )),
        ],
    };
    let mut chat = Chat::new(Some("claude-opus-5-5".into()), Reasoning::Auto, vec![session.summary()]);
    let world = World { id: "w".into(), name: "Lanternfall".into(), portrait: "world.png".into(), ..World::default() };
    chat.library_loaded(vec![world], Vec::new(), Vec::new(), Portraits::at(dir.to_path_buf()));
    let id = chat.conversations.iter().find(|c| c.session_id == "s").map(|c| c.id).expect("the story is listed");
    chat.open(id, &mut Vec::new());
    chat.session_loaded(id, Ok(session), &mut Vec::new());
    chat
}

/// Portraits drawn for the snapshots: a file name and its gradient.
const PEOPLE: [(&str, [f32; 3], [f32; 3]); 4] = [
    ("mira", [92.0, 54.0, 70.0], [214.0, 160.0, 128.0]),
    ("tobin", [40.0, 64.0, 70.0], [150.0, 190.0, 170.0]),
    ("sefa", [30.0, 30.0, 60.0], [120.0, 130.0, 220.0]),
    ("wren", [70.0, 60.0, 40.0], [210.0, 190.0, 140.0]),
];

/// The library the chat snapshots draw from: characters with portraits
/// (one starred), a world and a starred persona.
fn library(chat: &mut Chat, dir: &Path) {
    let character = |id: &str, name: &str, description: &str, portrait: &str, favorite| Character {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        portrait: portrait.into(),
        favorite,
        ..Character::default()
    };
    let characters = vec![
        character(
            "mira",
            "Mira Vale",
            "A lantern-maker with ink-stained hands and a sharp tongue, who keeps the city's secrets folded into paper.",
            "mira.png",
            true,
        ),
        character(
            "tobin",
            "Tobin Ashe",
            "A retired duellist who keeps the tea stall and pretends not to listen.",
            "tobin.png",
            false,
        ),
        character("sefa", "Sefa Lind", "A star-chart thief with a conscience she resents.", "sefa.png", false),
        character("oren", "Oren", "The harbourmaster.", "", false),
    ];
    let worlds = vec![
        World { id: "w".into(), name: "Lanternfall".into(), portrait: "world.png".into(), ..World::default() },
        World { id: "v".into(), name: "The Glass Steppe".into(), ..World::default() },
    ];
    let wren = Persona {
        id: "p".into(),
        name: "Wren".into(),
        description: "A courier new to the city.".into(),
        portrait: "wren.png".into(),
        favorite: true,
        ..Persona::default()
    };
    chat.library_loaded(worlds, characters, vec![wren], Portraits::at(dir.to_path_buf()));
}

/// The home screen, with a library to choose from and a chat behind it.
fn home(dir: &Path) -> Chat {
    let mut chat = Chat::new(Some("claude-opus-5-5".into()), Reasoning::Auto, vec![chat_session().summary()]);
    library(&mut chat, dir);
    chat
}

/// A chat with Mira, a few messages in.
fn chat_session() -> Session {
    let reply = |text: &str| {
        let mut message = StoredMessage::new(Role::Assistant, text.into());
        message.model = Some("gemma-4".into());
        message.usage = Usage::new(980, 140);
        message
    };
    Session {
        version: Session::VERSION,
        id: "c".into(),
        title: "You fixed the blue one?".into(),
        created: 1,
        updated: 3,
        mode: Mode::Chat,
        cast: vec![CastMember {
            id: "mira".into(),
            name: "Mira Vale".into(),
            description: "A lantern-maker with ink-stained hands and a sharp tongue, who keeps the city's secrets folded into paper.".into(),
            portrait: "mira.png".into(),
            present: true,
            ..CastMember::default()
        }],
        player: Some(Player { name: "Wren".into(), portrait: "wren.png".into(), ..Player::default() }),
        memories: vec!["Wren promised to bring Mira a heron feather.".into()],
        model: "gemma-4".into(),
        messages: vec![
            reply("*glances up from a half-folded lantern, a smudge of ink on her cheek* Back again, Wren? The rain usually keeps couriers away."),
            StoredMessage::new(Role::User, "\"You fixed the blue one?\" *I nod at the lantern by the door.* (keep her a little guarded)".into()),
            reply("*follows your gaze and her mouth twitches* Fixed is a strong word. I convinced it to stop sulking.\n\n*She sets the paper down.* You didn't come out in this weather to admire my repairs. What do you need?"),
        ],
        ..Session::default()
    }
}

/// A chat with one character, open, with their profile in the panel.
fn chat(dir: &Path) -> Chat {
    let mut chat = home(dir);
    let id = chat.conversations.iter().find(|c| c.session_id == "c").map(|c| c.id).expect("the chat is listed");
    chat.open(id, &mut Vec::new());
    chat.session_loaded(id, Ok(chat_session()), &mut Vec::new());
    chat
}

/// A vertical gradient from `top` to `bottom`, `w` by `h`.
fn gradient((w, h): (u32, u32), top: [f32; 3], bottom: [f32; 3]) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let t = y as f32 / h as f32;
        for _ in 0..w {
            pixels.extend(top.iter().zip(bottom).map(|(a, b)| (a + (b - a) * t) as u8));
            pixels.push(255);
        }
    }
    pixels
}

/// A dusk sky, violet above and amber at the horizon: a world's picture.
fn dusk(w: u32, h: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let t = y as f32 / h as f32;
        let (top, bottom) = ([58.0, 42.0, 110.0], [236.0, 146.0, 72.0]);
        for _ in 0..w {
            pixels.extend(top.iter().zip(bottom).map(|(a, b)| (a + (b - a) * t) as u8));
            pixels.push(255);
        }
    }
    pixels
}

fn write_png(path: &Path, (w, h): (u32, u32), rgba: &[u8]) {
    let file = std::io::BufWriter::new(std::fs::File::create(path).expect("snapshot file"));
    let mut encoder = png::Encoder::new(file, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().and_then(|mut writer| writer.write_image_data(rgba)).expect("snapshot written");
}

/// Draws the app's icon (its mark on a dark tile) at every size the
/// platforms want, into the folder `OPENRP_ICON` names (the test runs in
/// `crates/desktop`, so a relative path starts there):
/// `OPENRP_ICON=assets/icon cargo test -p openrp icon -- --ignored`
/// It also draws the logo for use outside the app into `branding/` at the
/// workspace's root: square to its corners, as whoever shows it rounds it.
#[test]
#[ignore = "draws on the GPU; run by hand to make the icon again"]
fn icon() {
    let Some(dir) = std::env::var_os("OPENRP_ICON").map(PathBuf::from) else { return };
    let Some(mut gpu) = Offscreen::new() else { return };
    let fonts = Fonts::load();
    let branding = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../branding");
    std::fs::create_dir_all(&branding).expect("branding folder");
    let sizes = [16u32, 20, 24, 32, 40, 48, 64, 256, 1024].map(|size| (size, false));
    for (size, square) in sizes.into_iter().chain([(500, true), (2000, true)]) {
        let (mut atlas, mut images) = (GlyphAtlas::default(), ImageAtlas::default());
        let side = size as f32;
        let view = Rect::new(0.0, 0.0, side, side);
        let mut instances = Vec::new();
        let palette = Scheme::Black.palette();
        let mut p = Painter::new(&fonts, (&mut atlas, &mut images), &mut instances, 1.0, view, palette);
        // macOS wants air around the tile; the others fill the square.
        let inset = if size == 1024 { side * 0.1 } else { 0.0 };
        let tile = Rect::new(inset, inset, side - 2.0 * inset, side - 2.0 * inset);
        // The square one keeps its mark clear of a round crop.
        let (radius, mark) = if square { (0.0, tile.w * 0.85) } else { (tile.w * 0.225, tile.w) };
        p.gradient(tile, crate::paint::hex(0x24_2428), crate::paint::hex(0x0C_0C0E), radius);
        p.logo((side * 0.5, side * 0.5), mark, crate::paint::hex(0xF2_EEE6));
        let pixels = gpu.render((size, size), &instances, (&mut atlas.uploads, &mut images.uploads), 1.0, [0.0; 4]);
        let path = match size {
            _ if square => branding.join(format!("openrp-{size}.png")),
            1024 => dir.join("macos.png"),
            _ => dir.join(format!("{size}.png")),
        };
        write_png(&path, (size, size), &pixels);
    }
}
