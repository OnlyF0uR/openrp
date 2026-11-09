//! Screens drawn offscreen and saved as PNGs, to look at the design in
//! every scheme without opening the app. Run by hand:
//! `OPENRP_SNAPSHOTS=<folder> cargo test -p openrp snapshots -- --ignored`

use std::path::{Path, PathBuf};

use openrp_core::{CastMember, Player, Portraits, Role, Session, StoredMessage, ToolCall, ToolResult, Usage, World};
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
    let fonts = Fonts::load();
    for scheme in Scheme::ALL {
        let mut settings = story(&dir);
        settings.page = Page::Settings;
        let mut worlds = story(&dir);
        worlds.page = Page::Library(Kind::World);
        let mut menu = story(&dir);
        menu.menu = Some(Menu::Reasoning);
        let mut picker = story(&dir);
        picker.menu = Some(Menu::CastLibrary);
        let mut spotlight = story(&dir);
        spotlight.open_spotlight();
        let mut dialog = story(&dir);
        dialog.edit_scene();
        let mut form = story(&dir);
        form.page = Page::Library(Kind::World);
        form.library.open_form(Kind::World, "w");
        let screens = vec![
            ("empty", Chat::new(None, Reasoning::Auto, Vec::new())),
            ("story", story(&dir)),
            ("settings", settings),
            ("worlds", worlds),
            ("menu", menu),
            ("picker", picker),
            ("spotlight", spotlight),
            ("dialog", dialog),
            ("form", form),
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
        note: String::new(),
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
