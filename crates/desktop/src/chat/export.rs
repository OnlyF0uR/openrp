//! A story as a readable transcript, to keep or share. What is out of
//! character, errors, summaries and the replies swiped away stay out. (A
//! story file, which holds all of a story and imports again, is
//! `openrp_core::write_story`.)

use std::fmt::Write as _;

use openrp_core::{Role, Session};

use super::tools::{self, Part};

/// A line of a story as exported.
enum Line {
    /// Someone's message: who, and the text.
    Said(String, String),
    /// A change to the story (the scene, an arrival).
    Note(String),
}

/// What `session` shows, in order; `player` speaks for the user.
fn lines(session: &Session, player: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for message in session.messages.iter().filter(|m| !m.failed && !m.compaction) {
        if message.role == Role::User {
            let said = tools::spoken(&message.content);
            if !said.is_empty() {
                lines.push(Line::Said(player.to_owned(), said));
            }
            continue;
        }
        let mut parts = tools::reply_parts(message);
        if parts.is_empty() {
            parts = tools::parse(&message.content);
        }
        // A plain chat's reply (from before stories) has no characters.
        if parts.is_empty() && !message.content.trim().is_empty() {
            lines.push(Line::Said("Assistant".to_owned(), message.content.trim().to_owned()));
        }
        for part in parts {
            lines.push(match part {
                Part::Said { character, text } => Line::Said(character, text),
                Part::Scene(note) | Part::Note(note) => Line::Note(note),
            });
        }
    }
    lines
}

/// `session`, played in `world`, as a transcript: its title, then who said
/// what, with changes to the story in brackets.
#[must_use]
pub fn text(session: &Session, world: &str) -> String {
    let player = session.player.as_ref().map_or("You", |p| p.name.as_str());
    let mut out = if session.title.is_empty() { "Untitled story".to_owned() } else { session.title.clone() };
    if !world.is_empty() {
        let _ = write!(out, "\nIn {world}, played as {player}.");
    }
    for line in lines(session, player) {
        let _ = match line {
            Line::Said(name, text) => write!(out, "\n\n{name}: {text}"),
            Line::Note(note) => write!(out, "\n\n[{note}]"),
        };
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use openrp_core::{Player, StoredMessage};

    use super::*;

    #[test]
    fn stories_export_as_text() {
        let reply = tools::greeting("Mira", "*wipes a glass* What'll it be?");
        let mut failed = StoredMessage::new(Role::Assistant, "boom".into());
        failed.failed = true;
        let session = Session {
            title: "The Lantern".into(),
            created: 2,
            player: Some(Player { name: "Gale".into(), ..Player::default() }),
            messages: vec![reply, StoredMessage::new(Role::User, "Ale. (a cheap one)\n/ooc be brief".into()), failed],
            ..Session::default()
        };
        let text = text(&session, "Ard");
        assert_eq!(
            text,
            "The Lantern\nIn Ard, played as Gale.\n\nMira: *wipes a glass* What'll it be?\n\nGale: Ale.\n"
        );
    }
}
