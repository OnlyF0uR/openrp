//! A story as one file (`.openrp`), to back up or take to another computer.
//!
//! It holds the [`Session`] with every message, the [`World`] the story is
//! played in (the prompt reads the world and its lore from the library, so a
//! story without its world could not go on) and the portraits both use, in
//! base64. A world's private comment and star stay behind.
//!
//! Reading one takes nothing on trust: the session and world are upgraded
//! or refused as files from disk are, ids that are not ids are replaced,
//! and portraits are checked as images before they are saved (or dropped).

use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::card::{base64, base64_decode};
use crate::error::{Error, Result};
use crate::library::{Portraits, World, invalid};
use crate::session::{Session, StoredMessage, new_id, valid_id};

/// File extension of a story file.
pub const STORY_EXTENSION: &str = "openrp";

/// What [`StoryFile::format`] says, to tell a story file from other JSON.
const FORMAT: &str = "openrp-story";
/// The story file format this version of the app writes.
const VERSION: u32 = 1;

/// On-disk shape of a story file.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct StoryFile {
    /// [`FORMAT`].
    format: String,
    /// The [`VERSION`] that wrote it.
    version: u32,
    session: Session,
    #[serde(skip_serializing_if = "Option::is_none")]
    world: Option<World>,
    /// The portraits the session and world name, in base64, by file name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    portraits: BTreeMap<String, String>,
}

/// A story read from a file: ready to save, its portraits already in.
#[derive(Debug, Clone, PartialEq)]
pub struct Story {
    /// The story, with every message.
    pub session: Session,
    /// The world it is played in, as it was when the file was written.
    pub world: Option<World>,
}

/// `session`, played in `world`, as a story file. Portraits are read from
/// `portraits`; one that is gone is left out.
///
/// # Errors
/// The story cannot be written as JSON.
pub fn write_story(session: &Session, world: Option<&World>, portraits: &Portraits) -> Result<Vec<u8>> {
    let world = world.map(|w| World { comment: String::new(), favorite: false, ..w.clone() });
    let names = world
        .iter()
        .map(|w| &w.portrait)
        .chain(session.player.iter().map(|p| &p.portrait))
        .chain(session.cast.iter().map(|m| &m.portrait));
    let mut images = BTreeMap::new();
    for name in names {
        if !images.contains_key(name)
            && let Some(bytes) = portraits.path(name).and_then(|path| fs::read(path).ok())
        {
            images.insert(name.clone(), base64(&bytes));
        }
    }
    let file =
        StoryFile { format: FORMAT.to_owned(), version: VERSION, session: session.clone(), world, portraits: images };
    Ok(serde_json::to_vec(&file)?)
}

/// Reads a story file, saving its portraits into `portraits`: under the
/// names they had, so stories that share one bring it once (one this
/// computer already has is not copied again), or a fresh name where that
/// cannot be. One that is missing or not an image is dropped.
///
/// # Errors
/// `bytes` are not a story file, or [`Error::Newer`] for one written by a
/// newer version of the app.
pub fn read_story(bytes: &[u8], portraits: &Portraits) -> Result<Story> {
    let not_one = || invalid("This is not an OpenRP story file.");
    let file: StoryFile = serde_json::from_slice(bytes).map_err(|_| not_one())?;
    if file.format != FORMAT {
        return Err(not_one());
    }
    let StoryFile { version, mut session, mut world, portraits: images, .. } = file;
    if version > VERSION || session.version > Session::VERSION || world.as_ref().is_some_and(World::is_newer) {
        return Err(Error::Newer { what: "story" });
    }
    session.upgrade();
    if !valid_id(&session.id) {
        session.id = new_id();
    }
    match &mut world {
        Some(world) => {
            if !valid_id(&world.id) {
                world.id = new_id();
            }
            session.world = Some(world.id.clone());
        }
        None => session.world = session.world.filter(|id| valid_id(id)),
    }

    // Each portrait once, whoever shares it.
    let mut placed: BTreeMap<String, String> = BTreeMap::new();
    let mut place = |name: &mut String| {
        if name.is_empty() {
            return;
        }
        let new = placed.entry(name.clone()).or_insert_with(|| {
            if portraits.path(name).is_some_and(|path| path.exists()) {
                return name.clone();
            }
            let image = images.get(name.as_str()).and_then(|text| base64_decode(text.as_bytes()));
            image.and_then(|bytes| portraits.keep(name, &bytes).ok()).unwrap_or_default()
        });
        name.clone_from(new);
    };
    if let Some(world) = &mut world {
        place(&mut world.portrait);
    }
    if let Some(player) = &mut session.player {
        place(&mut player.portrait);
    }
    session.cast.iter_mut().for_each(|member| place(&mut member.portrait));
    // What replies changed remembers the cast as it was: swipes hold
    // messages of their own, so walk them without recursing.
    let mut messages: Vec<&mut StoredMessage> = session.messages.iter_mut().collect();
    while let Some(message) = messages.pop() {
        for (before, after) in message.change.iter_mut().flat_map(|change| &mut change.cast) {
            if let Some(before) = before {
                place(&mut before.portrait);
            }
            place(&mut after.portrait);
        }
        messages.extend(message.swipes.iter_mut().flatten());
    }
    Ok(Story { session, world })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::responses::Role;
    use crate::session::{CastMember, Player, StoryChange};

    /// The smallest thing [`Portraits::add`] takes for a PNG.
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nnot really";

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openrp-story-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_story_travels_with_its_world_and_portraits() {
        let (here, there) = (temp("here"), temp("there"));
        let portraits = Portraits::at(here.clone());
        let (face, sky) = (portraits.add(PNG).unwrap(), portraits.add(PNG).unwrap());
        let mira = CastMember { id: "m".into(), name: "Mira".into(), portrait: face.clone(), ..CastMember::default() };
        let mut reply = StoredMessage::new(Role::Assistant, String::new());
        reply.change = Some(StoryChange { cast: vec![(None, mira.clone())], ..StoryChange::default() });
        let session = Session {
            version: Session::VERSION,
            id: "s1".into(),
            title: "The heron letter".into(),
            world: Some("w1".into()),
            player: Some(Player { name: "Wren".into(), portrait: face.clone(), ..Player::default() }),
            cast: vec![mira, CastMember { portrait: "gone.png".into(), ..CastMember::default() }],
            messages: vec![StoredMessage::new(Role::User, "Hi.".into()), reply],
            ..Session::default()
        };
        let world = World {
            id: "w1".into(),
            name: "Lanternfall".into(),
            portrait: sky.clone(),
            comment: "my private note".into(),
            favorite: true,
            ..World::default()
        };
        let file = write_story(&session, Some(&world), &portraits).unwrap();
        assert!(!String::from_utf8_lossy(&file).contains("my private note"), "the comment stays behind");

        // On this computer the portraits are here already: nothing is copied.
        let same = read_story(&file, &portraits).unwrap();
        assert_eq!(same.session.cast[0].portrait, face);
        assert_eq!(fs::read_dir(&here).unwrap().count(), 2);

        // On another they come in, shared ones once, and a second story
        // that uses them brings nothing new.
        let elsewhere = Portraits::at(there.clone());
        let Story { session: read, world: read_world } = read_story(&file, &elsewhere).unwrap();
        let read_world = read_world.unwrap();
        assert!(elsewhere.path(&face).is_some_and(|p| fs::read(p).unwrap() == PNG));
        assert_eq!(read.cast[0].portrait, face);
        assert_eq!(read.player.as_ref().map(|p| p.portrait.as_str()), Some(face.as_str()));
        assert_eq!(read.messages[1].change.as_ref().map(|c| c.cast[0].1.portrait.as_str()), Some(face.as_str()));
        assert_eq!(read.cast[1].portrait, "", "one that was gone is dropped");
        assert_eq!(read_world.portrait, sky);
        read_story(&file, &elsewhere).unwrap();
        assert_eq!(fs::read_dir(&there).unwrap().count(), 2);
        assert_eq!((read_world.comment.as_str(), read_world.favorite), ("", false));
        assert_eq!((read.id.as_str(), read.world.as_deref(), read.messages.len()), ("s1", Some("w1"), 2));
        let _ = (fs::remove_dir_all(here), fs::remove_dir_all(there));
    }

    #[test]
    fn other_files_and_newer_stories_are_refused() {
        let portraits = Portraits::at(temp("refused"));
        for bytes in [&b"not json"[..], b"{}", b"[1]", br#"{"format":"something-else","session":{}}"#] {
            assert!(matches!(read_story(bytes, &portraits), Err(Error::Io(_))));
        }
        let newer = br#"{"format":"openrp-story","version":99,"session":{}}"#;
        assert!(matches!(read_story(newer, &portraits), Err(Error::Newer { .. })));
        let newer =
            format!(r#"{{"format":"openrp-story","version":1,"session":{{"version":{}}}}}"#, Session::VERSION + 1);
        assert!(matches!(read_story(newer.as_bytes(), &portraits), Err(Error::Newer { .. })));

        // Ids that could name a file elsewhere are replaced; a portrait that
        // is not an image is dropped, and one named as no portrait is, or
        // as another kind of image, gets a name of its own.
        let png = base64(PNG);
        let hostile = format!(
            r#"{{"format":"openrp-story","version":1,
            "session":{{"id":"../../evil","world":"../w","player":{{"name":"Y","portrait":"q.jpg"}},
                "cast":[{{"name":"X","portrait":"../../x.png"}},{{"name":"Z","portrait":"../../z.png"}}]}},
            "world":{{"id":"..\\evil","name":"W","portrait":"p.png"}},
            "portraits":{{"p.png":"bm90IGFuIGltYWdl","../../x.png":"!!!","../../z.png":"{png}","q.jpg":"{png}"}}}}"#
        );
        let dir = temp("hostile");
        let portraits = Portraits::at(dir.clone());
        let story = read_story(hostile.as_bytes(), &portraits).unwrap();
        let world = story.world.unwrap();
        assert!(valid_id(&story.session.id) && valid_id(&world.id));
        assert_eq!(story.session.world.as_deref(), Some(world.id.as_str()));
        assert_eq!((world.portrait.as_str(), story.session.cast[0].portrait.as_str()), ("", ""));
        let named = [&story.session.cast[1].portrait, &story.session.player.as_ref().unwrap().portrait];
        assert!(named.iter().all(|name| name.as_str() != "q.jpg" && portraits.path(name).is_some_and(|p| p.exists())));
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2, "written only under the folder's own names");
        let _ = fs::remove_dir_all(dir);
    }
}
