//! The tools a story is told with; it has no narrator, so every reply
//! must call them:
//!
//! * `speak`: everything characters say and do in a reply, as `messages`
//!   of (character, action, text), in order, after an `introduce`
//!   list that casts newcomers with a description in the same call (a
//!   required field: models skip a separate tool far more often). One call
//!   can hold several speakers, so a scene where three characters react
//!   plays out in order. A cast member who acts moves into the scene; those
//!   in `leave` move out. A non-empty `scene` (required, so the model weighs
//!   it every turn) sets where the story is now and what it is like there.
//!   Stories saved before `messages` call the field `lines`; both are read.
//! * `create_character`: someone joins the story's cast (only the story's:
//!   the character library is the user's) without acting yet, or a member
//!   without a description gets one.
//! * `update_character`: a member gets a new name (the old one stays an
//!   alias, so earlier turns still mean them) or a new description.
//!
//! Names in calls are matched loosely (see `names.rs`): case, a first name
//! or a former name all find the character, and the model is told the name
//! to use.
//!
//! No one is cast without a description. Someone who acts but was never
//! introduced is reported instead, and the next request makes the model
//! call `create_character` for them (see `stream.rs`).
//!
//! The cast strip's Generate asks a separate generator for one character;
//! its only tool is `create_character` (see [`generator_prompt`]).
//!
//! Both run instantly on the UI thread: they only read or change the story.
//! A reply shows as [`Part`]s: a bubble per message (a character's messages
//! in a row share one) and short notes for scene
//! changes. They update while the model writes them, from arguments that
//! are still incomplete JSON. Once the calls ran, notes for what they
//! really changed (arrivals, departures, new memories) come from the
//! reply's change record, so a call that changed nothing shows nothing. There is no
//! narration: text a story reply writes outside its calls is never shown.

use std::fmt::Write as _;
use std::ops::Range;

use openrp_core::{CastMember, Player, Role, StoredMessage, ToolCall, ToolResult, new_id};
use serde_json::{Value, json};

use super::names::{self, Who};
use crate::markdown::ASIDE;

/// The speech tool's name.
pub const SPEAK: &str = "speak";
/// The character tool's name.
pub const CREATE_CHARACTER: &str = "create_character";
/// The tool changing a cast member's name or description.
pub const UPDATE_CHARACTER: &str = "update_character";
/// The memory tool's name.
pub const REMEMBER: &str = "remember";
/// Longest a memory may be, in characters; longer ones are cut.
const MEMORY_LIMIT: usize = 400;
/// How every tool that makes a character asks for their name.
const NAME: &str = "A personal name fitting the world, e.g. \"Bram Holloway\". Never a role (\"Bartender\", \"Guard 2\") or a group (\"Royal Guards\").";

/// Every tool as (name, description, JSON Schema of its arguments).
#[must_use]
pub fn tool_definitions() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        (
            SPEAK,
            "Everything the characters say and do this turn, in order. The only way to show the user anything.",
            json!({
                "type": "object",
                "properties": {
                    "plan": {
                        "type": "string",
                        "description": "Private, never shown, written first: in a line or two, what each character in the scene wants right now and what they will do about it, and what this reply changes in the story."
                    },
                    "scene": {
                        "type": "string",
                        "description": "Where the characters are and what it is like there (place, time of day, mood, weather), e.g. \"Peeta's kitchen, before dawn; warm bread, rain on the windows\". Only when the scene is unset or changes this turn; empty otherwise."
                    },
                    "introduce": {
                        "type": "array",
                        "description": "Newcomers who act in messages and are not in the cast yet, each described. Usually empty.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string", "description": format!("{NAME} Exactly as in messages.") },
                                "description": { "type": "string", "description": "Who they are: role, personality, appearance, how they speak." }
                            },
                            "required": ["name", "description"]
                        }
                    },
                    "messages": {
                        "type": "array",
                        "description": "What the characters say and do, in the order it happens.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "character": { "type": "string", "description": "Their name in the cast, or as in introduce." },
                                "action": { "type": "string", "description": "What they do, e.g. \"lowers her bow, eyes on the treeline\". May be empty." },
                                "text": { "type": "string", "description": "Exactly what they say, without quotation marks. A short action between words goes in *asterisks*, e.g. \"Stay back. *She steps closer.* I mean it.\" Empty when they only act." }
                            },
                            "required": ["character", "text"]
                        }
                    },
                    "leave": {
                        "type": "array",
                        "description": "Cast members who leave the scene this turn. Usually empty.",
                        "items": { "type": "string" }
                    }
                },
                "required": ["plan", "scene", "introduce", "messages"]
            }),
        ),
        (
            CREATE_CHARACTER,
            "Add someone to this story's cast who does not act this turn (a newcomer who acts goes in speak's introduce), or \
             describe a cast member who has no description. Never the user's character or someone already in the cast.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": NAME },
                    "description": { "type": "string", "description": "Who they are: role, personality, appearance, how they speak." },
                    "present": { "type": "boolean", "description": "Whether they are in the current scene. Defaults to true." }
                },
                "required": ["name", "description"]
            }),
        ),
        (
            UPDATE_CHARACTER,
            "Record a lasting change to a cast member: a new name (the old one keeps meaning them) or a new description. Never \
             for the user's character or a passing mood.",
            json!({
                "type": "object",
                "properties": {
                    "character": { "type": "string", "description": "Their name in the cast." },
                    "name": { "type": "string", "description": "Their new name; empty if unchanged." },
                    "description": { "type": "string", "description": "Their whole new description: everything that still holds, plus the change. Empty if unchanged." }
                },
                "required": ["character"]
            }),
        ),
        remember_tool(),
    ]
}

/// The memory tool as (name, description, JSON Schema of its arguments).
fn remember_tool() -> (&'static str, &'static str, Value) {
    (
        REMEMBER,
        "Add facts the story must not forget to its memories: promises, secrets revealed, decisions, injuries, changed \
         relationships, what someone now owns or knows. Only what matters later and is not in the memories yet.",
        json!({
            "type": "object",
            "properties": {
                "memories": {
                    "type": "array",
                    "description": "Each a short, self-contained fact naming who it is about, e.g. \"Katniss promised Prim she would come home.\"",
                    "items": { "type": "string" }
                }
            },
            "required": ["memories"]
        }),
    )
}

/// The memory review's only tool, as (name, description, JSON Schema): it
/// drops memories by number and adds new ones (see [`revise`]).
#[must_use]
pub fn review_tool() -> (&'static str, &'static str, Value) {
    (
        REMEMBER,
        "Revise the story's memories.",
        json!({
            "type": "object",
            "properties": {
                "forget": {
                    "type": "array",
                    "description": "Numbers of the memories to drop: no longer true, or rewritten or merged into memories.",
                    "items": { "type": "integer" }
                },
                "memories": {
                    "type": "array",
                    "description": "New facts, and the rewritten or merged versions of those forgotten. Each short, self-contained and naming who it is about.",
                    "items": { "type": "string" }
                }
            },
            "required": ["forget", "memories"]
        }),
    )
}

/// The out-of-character asides typed in parentheses in what a user's
/// message says: the byte range of each, its parentheses included, in
/// order. Nested ones belong to the outermost; an unclosed one is not one.
///
/// A parenthesis counts as text when a backslash escapes it, when it is
/// part of an emoticon (after `:`, `;` or `=`, with or without a `-`), or,
/// for an opening one, when it follows a letter or digit (`f(x)`), though
/// such a pair still nests inside an aside.
#[must_use]
pub fn asides(text: &str) -> Vec<Range<usize>> {
    let mut spans: Vec<Range<usize>> = Vec::new();
    // Where each open parenthesis is, and whether it can start an aside.
    let mut open: Vec<(usize, bool)> = Vec::new();
    let (mut before, mut earlier, mut escaped) = (None, None, false);
    for (i, c) in text.char_indices() {
        let emoticon = matches!(before, Some(':' | ';' | '='))
            || (before == Some('-') && matches!(earlier, Some(':' | ';' | '=')));
        match c {
            '(' if !escaped && !emoticon => {
                open.push((i, before.is_none_or(|b: char| b.is_whitespace() || b == '(')));
            }
            ')' if !escaped && !emoticon => {
                if let Some((start, true)) = open.pop()
                    && !text[start + 1..i].trim().is_empty()
                {
                    // Asides inside this one are part of it.
                    spans.retain(|span| span.start < start);
                    spans.push(start..i + 1);
                }
            }
            _ => {}
        }
        escaped = c == '\\' && !escaped;
        (earlier, before) = (before, Some(c));
    }
    spans
}

/// What the aside at `span` of `text` says, without its parentheses (or a
/// second pair, as roleplayers write `((this))`).
fn aside_text<'a>(text: &'a str, span: &Range<usize>) -> &'a str {
    let mut inner = text[span.start + 1..span.end - 1].trim();
    // Bounded: each pair costs a scan.
    for _ in 0..3 {
        let whole = asides(inner).first().is_some_and(|span| span.len() == inner.len());
        if !whole {
            break;
        }
        inner = inner[1..inner.len() - 1].trim();
    }
    inner
}

/// `text` with the backslashes that escape parentheses dropped.
fn unescaped(text: &str) -> String {
    text.replace("\\(", "(").replace("\\)", ")")
}

/// A user's message in a story as shown (Markdown): what they wrote, with
/// what is out of character (text in parentheses, and the instruction the
/// OOC button added) as asides where they stand: muted, without their marks
/// or a label (see [`ASIDE`]).
#[must_use]
pub fn user_display(text: &str) -> String {
    // An aside's marks do not span lines: each line carries its own.
    let aside = |ooc: &str| {
        let lines = ooc
            .lines()
            .map(plain)
            .map(|line| if line.is_empty() { line } else { format!("{}{line}{}", ASIDE.0, ASIDE.1) });
        lines.collect::<Vec<_>>().join("\n")
    };
    let (said, instruction) = split_ooc(text);
    let mut shown = String::new();
    let mut at = 0;
    for span in asides(said) {
        shown.push_str(&said[at..span.start]);
        shown.push_str(&aside(aside_text(said, &span)));
        at = span.end;
    }
    shown.push_str(said[at..].trim_end());
    if let Some(instruction) = instruction {
        if !shown.is_empty() {
            shown.push(' ');
        }
        shown.push_str(&aside(instruction));
    }
    shown
}

/// What a user's message says in character alone: its asides and its
/// out-of-character instruction left out.
#[must_use]
pub fn spoken(text: &str) -> String {
    let said = split_ooc(text).0;
    let mut out = String::new();
    let mut at = 0;
    let mut push = |part: &str| {
        // Where an aside stood, one space is enough.
        let part = if out.is_empty() || out.ends_with(char::is_whitespace) { part.trim_start() } else { part };
        out.push_str(part);
    };
    for span in asides(said) {
        push(&said[at..span.start]);
        at = span.end;
    }
    push(&said[at..]);
    unescaped(out.trim())
}

/// Everything a user's message says out of character, on one line.
#[must_use]
pub fn instructions(text: &str) -> String {
    let (said, instruction) = split_ooc(text);
    let spans = asides(said);
    let mut all: Vec<&str> = spans.iter().map(|span| aside_text(said, span)).collect();
    all.extend(instruction);
    all.join(" ")
}

/// What a user's message says as the model reads it: each aside marked
/// `(OOC: …)` where it stands. Also whether there was one.
#[must_use]
pub fn mark_asides(said: &str) -> (String, bool) {
    let spans = asides(said);
    let mut out = String::new();
    let mut at = 0;
    for span in &spans {
        let _ = write!(out, "{}(OOC: {})", unescaped(&said[at..span.start]), unescaped(aside_text(said, span)));
        at = span.end;
    }
    out.push_str(&unescaped(&said[at..]));
    (out, !spans.is_empty())
}

/// The message the composer sends for `text` whose out-of-character part
/// starts at byte `from` (the OOC button): everything from there is the
/// instruction, or the whole text when nothing follows `from`. Stored as
/// [`split_ooc`] reads it; asides typed in parentheses stay as typed
/// ([`asides`]).
#[must_use]
pub fn with_ooc(text: &str, from: Option<usize>) -> String {
    let Some(from) = from else { return text.trim().to_owned() };
    let (said, ooc) = text.split_at_checked(from).unwrap_or((text, ""));
    let (said, ooc) = match (said.trim(), ooc.trim()) {
        (said, "") => ("", said),
        pair => pair,
    };
    match (said, ooc) {
        (_, "") => said.to_owned(),
        ("", ooc) => format!("/ooc {ooc}"),
        (said, ooc) => format!("{said}\n/ooc {ooc}"),
    }
}

/// The text a user message says in character, and the out-of-character
/// instruction it ends with: everything from a line starting with `/ooc`.
#[must_use]
pub fn split_ooc(text: &str) -> (&str, Option<&str>) {
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        let rest = &line[indent..];
        if rest.strip_prefix("/ooc").is_some_and(|after| after.is_empty() || after.starts_with(char::is_whitespace)) {
            let instruction = text[start + indent + "/ooc".len()..].trim();
            return (text[..start].trim_end(), (!instruction.is_empty()).then_some(instruction));
        }
        start += line.len();
    }
    (text, None)
}

/// System instructions of the character generator, which can do one thing.
const GENERATOR: &str = "You create characters for an interactive roleplay story. Invent exactly one character from the \
    user's request and return them with one create_character call; write nothing else and never continue the story. The \
    character is one individual (person or creature) with a personal name that fits the world; a request for a group or \
    institution gets one person from it. The description is for the AI that will play them: who they are, their role, \
    personality, appearance, how they speak and what they want, in one short paragraph. Fit the world below, if any. Never \
    create the user's character or anyone already in the cast.";
/// The request sent when the user asked for nobody in particular.
pub const SURPRISE: &str = "Surprise me: someone this story would benefit from.";
/// The request sent from the Characters page when the user asked for nobody
/// in particular.
pub const SURPRISE_LIBRARY: &str = "Surprise me: a memorable character to cast in any story.";

/// The generator's only tool, as (name, description, JSON Schema).
#[must_use]
pub fn generator_tool() -> (&'static str, &'static str, Value) {
    (
        CREATE_CHARACTER,
        "Create the requested character.",
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": NAME },
                "description": { "type": "string", "description": "Who they are, in one short paragraph: role, personality, appearance, how they speak, what they want." }
            },
            "required": ["name", "description"]
        }),
    )
}

/// The generator's system prompt for a story in `world` (name and
/// description), played by `player`, whose `cast` holds these names.
#[must_use]
pub fn generator_prompt(world: Option<(&str, &str)>, player: Option<&str>, cast: &[&str]) -> String {
    with_story(GENERATOR, world, player, cast)
}

/// `instructions` followed by the story's world, player and cast names.
fn with_story(instructions: &str, world: Option<(&str, &str)>, player: Option<&str>, cast: &[&str]) -> String {
    let mut prompt = instructions.to_owned();
    if let Some((name, description)) = world {
        let _ = write!(prompt, "\n\n# World: {name}\n\n{description}");
    }
    if let Some(player) = player {
        let _ = write!(prompt, "\n\n# The user's character\n\n{player}");
    }
    if !cast.is_empty() {
        let _ = write!(prompt, "\n\n# Already in the cast\n\n{}", cast.join(", "));
    }
    prompt
}

/// System instructions of the memory review, which can do one thing.
const REVIEWER: &str = "You keep the memories of an interactive roleplay story: facts it must not forget. The user's message \
    is a transcript of its latest turns; the memories kept so far are numbered below. Call remember once and write nothing else.\n\
    - forget: the numbers of memories the transcript made untrue or outdated (an injury healed, a secret now known, a plan \
    carried out), and of those you rewrite or merge.\n\
    - memories: new facts from the transcript (promises, secrets revealed, decisions, injuries, changed relationships, what \
    someone now owns or knows), and the rewritten or merged versions of the memories you forget.\n\
    Keep under 30 memories: merge related ones. Skip scenery and passing dialogue. Leave memories that still hold untouched. If \
    nothing changes, call remember with both lists empty.";

/// The memory review's system prompt for a story in `world` (name and
/// description), played by `player`, with these `cast` names and the
/// `memories` it holds already, numbered from 1 for [`revise`].
#[must_use]
pub fn reviewer_prompt(
    world: Option<(&str, &str)>,
    player: Option<&str>,
    cast: &[&str],
    memories: &[String],
) -> String {
    let mut prompt = with_story(REVIEWER, world, player, cast);
    if !memories.is_empty() {
        prompt.push_str("\n\n# Memories kept so far\n");
        for (n, memory) in memories.iter().enumerate() {
            let _ = write!(prompt, "\n{}. {memory}", n + 1);
        }
    }
    prompt
}

/// The latest `limit` characters of `messages` as a plain transcript for
/// the memory review: one line per speaker, `player` for the user's
/// messages, `partner` for a chat's replies, and scene changes in brackets.
/// Out-of-character instructions, errors, summaries and the memories
/// replies noted are left out.
#[must_use]
pub fn transcript(messages: &[StoredMessage], player: &str, partner: Option<&str>, limit: usize) -> String {
    let mut out = String::new();
    for message in messages.iter().filter(|m| !m.failed && !m.compaction) {
        if message.role == Role::User {
            let said = spoken(&message.content);
            if !said.is_empty() {
                let _ = writeln!(out, "{player}: {said}\n");
            }
            continue;
        }
        if let Some(partner) = partner {
            let said = chat_text(&message.content, partner, None);
            if !said.is_empty() {
                let _ = writeln!(out, "{partner}: {said}\n");
            }
            continue;
        }
        let mut shown = reply_parts(message);
        if shown.is_empty() {
            shown = parse(&message.content);
        }
        for part in shown {
            match part {
                Part::Said { character, text } => {
                    let _ = writeln!(out, "{character}: {text}\n");
                }
                Part::Scene(note) | Part::Note(note) if !note.starts_with("Remembered: ") => {
                    let _ = writeln!(out, "[{note}]\n");
                }
                Part::Scene(_) | Part::Note(_) => {}
            }
        }
    }
    let out = out.trim_end();
    match out.char_indices().rev().nth(limit) {
        Some((at, c)) => format!("…{}", &out[at + c.len_utf8()..]),
        None => out.to_owned(),
    }
}

/// What a chat reply says as `partner`, cleaned of what small models add
/// around it: a leading `Name:` (or `**Name**:`) for them is dropped from
/// any line, and given the `player`'s name, the reply ends where it starts
/// writing their lines (`Player:` at a line's start), which is never the
/// partner's to write.
#[must_use]
pub fn chat_text(text: &str, partner: &str, player: Option<&str>) -> String {
    let mut out = String::new();
    for line in text.trim().lines() {
        let line = match label(line) {
            Some((name, said)) if names::same(name, partner) => said,
            Some((name, _)) if player.is_some_and(|p| names::same(name, p)) => break,
            _ => line,
        };
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_owned()
}

/// The speaker a line opens with, as models label them (`Name:`,
/// `**Name**:`, `**Name:**`, `*Name*:`), and what follows; `None` when it
/// opens with no short name and a colon.
fn label(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    let (head, rest) = trimmed.split_once(':')?;
    // `**Name:**` leaves the closing marks after the colon.
    let rest = rest.strip_prefix("**").or_else(|| rest.strip_prefix('*')).unwrap_or(rest);
    let name = head.trim().trim_matches('*').trim();
    let short = !name.is_empty() && name.chars().count() <= 40 && name.split_whitespace().count() <= 4;
    (short && !name.contains(['.', ',', '!', '?', '"', '(', ')'])).then(|| (name, rest.trim_start()))
}

/// A greeting that opens a story: `character` saying `text`, as a reply
/// whose speak call already ran.
#[must_use]
pub fn greeting(character: &str, text: &str) -> StoredMessage {
    let arguments =
        json!({ "scene": "", "introduce": [], "messages": [{ "character": character, "action": "", "text": text }] })
            .to_string();
    let call = ToolCall { call_id: format!("call_{}", new_id()), name: SPEAK.to_owned(), arguments };
    let mut message = StoredMessage::new(Role::Assistant, String::new());
    message.tool_calls.push(ToolResult { call, output: "Spoken.".to_owned() });
    message
}

/// The (name, description) a generator call made, if it made someone.
#[must_use]
pub fn generated(call: &ToolCall) -> Option<(String, String)> {
    let args = serde_json::from_str::<Value>(&call.arguments).ok()?;
    let field =
        |key: &str| args.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    Some((field("name")?, field("description")?))
}

/// How the model is told that `written` means the cast member `member`,
/// when it wrote them by another name; `None` when it used theirs.
fn by_name(written: &str, member: &CastMember) -> Option<String> {
    (!names::same(written, &member.name)).then(|| format!("{written} is {} in the cast: use that name.", member.name))
}

/// Adds `name` to the cast with `description`, or describes a member who
/// has no description yet. Returns what the model is told.
fn cast_in(
    cast: &mut Vec<CastMember>,
    player: Option<&Player>,
    name: &str,
    description: &str,
    present: bool,
) -> String {
    let (name, description) = (name.trim(), description.trim());
    if name.is_empty() {
        return "Error: a character needs a name.".to_owned();
    }
    let who = names::resolve(cast, player, name);
    if who == Some(Who::Player) {
        return format!("{name} is the user's character; nothing was created.");
    }
    if description.is_empty() {
        return format!("Error: {name} needs a description; nothing was created.");
    }
    match who {
        Some(Who::Member(index)) if cast[index].description.trim().is_empty() => {
            let member = &mut cast[index];
            description.clone_into(&mut member.description);
            member.present |= present;
            format!("{} is now described.", member.name)
        }
        Some(Who::Member(index)) => match by_name(name, &cast[index]) {
            Some(_) => format!("{name} is already in the cast, as {}: use that name.", cast[index].name),
            None => format!("{name} is already in the cast."),
        },
        _ => {
            let member = CastMember {
                id: new_id(),
                name: name.to_owned(),
                description: description.to_owned(),
                present,
                ..CastMember::default()
            };
            cast.push(member);
            format!(
                "{name} joined the cast{}.",
                if present { " and is in the scene" } else { ", away from the scene for now" }
            )
        }
    }
}

/// Renames and/or describes anew the member `character` names. Returns
/// what the model is told.
fn update(cast: &mut [CastMember], player: Option<&Player>, character: &str, name: &str, description: &str) -> String {
    let (character, name, description) = (character.trim(), name.trim(), description.trim());
    let index = match names::resolve(cast, player, character) {
        Some(Who::Member(index)) => index,
        Some(Who::Player) => return format!("{character} is the user's character; only the user changes them."),
        None => {
            let names: Vec<&str> = cast.iter().map(|m| m.name.as_str()).collect();
            return format!("Error: no one in the cast is called {character}. The cast: {}.", names.join(", "));
        }
    };
    if name.is_empty() && description.is_empty() {
        return "Error: give a new name, a new description, or both.".to_owned();
    }
    let taken = !name.is_empty()
        && (player.is_some_and(|p| names::same(&p.name, name))
            || cast.iter().enumerate().any(|(i, m)| i != index && names::same(&m.name, name)));
    if taken {
        return format!("Error: someone else is already called {name}; nothing changed.");
    }
    let member = &mut cast[index];
    let mut done = Vec::new();
    if !name.is_empty() && member.name != name {
        let old = member.name.clone();
        openrp_core::rename(&mut member.name, &mut member.aliases, name);
        done.push(format!("{old} is now called {name}"));
    }
    if !description.is_empty() && member.description != description {
        description.clone_into(&mut member.description);
        done.push(format!("{} has a new description", member.name));
    }
    if done.is_empty() { "Nothing changed.".to_owned() } else { format!("Updated: {}.", done.join("; ")) }
}

/// A `speak` call's messages (`lines` in stories saved before `messages`).
fn messages(args: &Value) -> impl Iterator<Item = &Value> {
    args.get("messages").or_else(|| args.get("lines")).and_then(Value::as_array).into_iter().flatten()
}

/// The names a `speak` call's messages are spoken by.
fn speakers(args: &Value) -> impl Iterator<Item = &str> {
    messages(args).filter_map(|l| l.get("character").and_then(Value::as_str)).map(str::trim).filter(|s| !s.is_empty())
}

/// Runs `call` in a story with this `cast` (which it may grow or change),
/// `scene` (which it may set), `memories` (which it may add to) and
/// `player` (the user's character) and returns what the model is told.
pub fn run(
    call: &ToolCall,
    cast: &mut Vec<CastMember>,
    scene: &mut String,
    memories: &mut Vec<String>,
    player: Option<&Player>,
) -> String {
    let Ok(args) = serde_json::from_str::<Value>(&call.arguments) else {
        return "Error: the arguments were not valid JSON. Nothing happened; call it again.".to_owned();
    };
    let field = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
    match call.name.as_str() {
        SPEAK => {
            if speakers(&args).next().is_none() {
                return "Error: speak needs at least one message with a character.".to_owned();
            }
            let mut notes: Vec<String> = args
                .get("introduce")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|new| cast_in(cast, player, &field(new, "name"), &field(new, "description"), true))
                .filter(|note| !note.ends_with("already in the cast."))
                .collect();
            // Whoever acts is in the scene; whoever is not cast and
            // described yet is not added half-finished, but reported.
            let (mut arrived, mut missing, mut renamed, mut for_player) = (Vec::new(), Vec::new(), Vec::new(), false);
            let mut seen: Vec<&str> = Vec::new();
            for speaker in speakers(&args) {
                if seen.iter().any(|s| names::same(s, speaker)) {
                    continue;
                }
                seen.push(speaker);
                match names::resolve(cast, player, speaker) {
                    Some(Who::Player) => for_player = true,
                    Some(Who::Member(index)) if !cast[index].description.trim().is_empty() => {
                        let member = &mut cast[index];
                        renamed.extend(by_name(speaker, member));
                        if !member.present {
                            member.present = true;
                            arrived.push(member.name.clone());
                        }
                    }
                    _ if missing.iter().any(|m: &String| names::same(m, speaker)) => {}
                    _ => missing.push(speaker.to_owned()),
                }
            }
            if !arrived.is_empty() {
                notes.push(format!("Moved into the scene: {}.", arrived.join(", ")));
            }
            // Leaving comes last: someone may speak, then walk off.
            let mut left = Vec::new();
            for name in args.get("leave").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                if let Some(index) = names::member(cast, player, name).filter(|&i| cast[i].present) {
                    cast[index].present = false;
                    left.push(cast[index].name.clone());
                }
            }
            if !left.is_empty() {
                notes.push(format!("Left the scene: {}.", left.join(", ")));
            }
            let new_scene = field(&args, "scene");
            if !new_scene.trim().is_empty() {
                new_scene.trim().clone_into(scene);
                notes.push("Scene set.".to_owned());
            }
            notes.extend(renamed);
            if for_player {
                notes.push("Never speak or act for the user's character.".to_owned());
            }
            if !missing.is_empty() {
                notes.push(format!(
                    "Not in the cast with a description: {}. Add them with create_character now.",
                    missing.join(", ")
                ));
            }
            if notes.is_empty() { "Spoken.".to_owned() } else { format!("Spoken. {}", notes.join(" ")) }
        }
        CREATE_CHARACTER => {
            let present = args.get("present").and_then(Value::as_bool).unwrap_or(true);
            cast_in(cast, player, &field(&args, "name"), &field(&args, "description"), present)
        }
        UPDATE_CHARACTER => {
            update(cast, player, &field(&args, "character"), &field(&args, "name"), &field(&args, "description"))
        }
        REMEMBER => {
            if add_memories(&args, memories) == 0 {
                "Nothing new to remember.".to_owned()
            } else {
                "Remembered.".to_owned()
            }
        }
        other => format!("Error: there is no tool named {other}."),
    }
}

/// Adds the `memories` of a call's `args` that `memories` lacks (whitespace
/// folded, cut to [`MEMORY_LIMIT`]); returns how many.
fn add_memories(args: &Value, memories: &mut Vec<String>) -> usize {
    let mut kept = 0;
    for memory in args.get("memories").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        let memory: String =
            memory.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MEMORY_LIMIT).collect();
        if !memory.is_empty() && !memories.iter().any(|m| m.eq_ignore_ascii_case(&memory)) {
            memories.push(memory);
            kept += 1;
        }
    }
    kept
}

/// Applies a memory review's `call`: drops the memories it forgets, by
/// their number (from 1) in `shown`, the list it read, then adds its new
/// ones. Memories added or edited since it read them stay.
pub fn revise(call: &ToolCall, shown: &[String], memories: &mut Vec<String>) {
    let Ok(args) = serde_json::from_str::<Value>(&call.arguments) else {
        return;
    };
    // Some models write numbers as strings.
    let number = |v: &Value| v.as_u64().or_else(|| v.as_str()?.trim().parse().ok());
    let forget: Vec<&String> = args
        .get("forget")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(number)
        .filter_map(|n| shown.get(usize::try_from(n).ok()?.checked_sub(1)?))
        .collect();
    memories.retain(|m| !forget.contains(&m));
    add_memories(&args, memories);
}

/// Who acted in a reply's `speak` calls but is not in the cast with a
/// description: they still need one.
#[must_use]
pub fn undescribed<'a>(
    calls: impl Iterator<Item = &'a ToolCall>,
    cast: &[CastMember],
    player: Option<&Player>,
) -> Vec<String> {
    let mut missing: Vec<String> = Vec::new();
    for call in calls.filter(|c| c.name == SPEAK) {
        let Ok(args) = serde_json::from_str::<Value>(&call.arguments) else {
            continue;
        };
        for speaker in speakers(&args) {
            let known = match names::resolve(cast, player, speaker) {
                Some(Who::Player) => true,
                Some(Who::Member(index)) => !cast[index].description.trim().is_empty(),
                None => false,
            };
            if !known && !missing.iter().any(|m| names::same(m, speaker)) {
                missing.push(speaker.to_owned());
            }
        }
    }
    missing
}

/// Whether a reply's tool call shows the user something: a message where
/// a character says or does anything.
#[must_use]
pub fn shows_speech(call: &ToolCall) -> bool {
    let filled = |line: &Value, key: &str| line.get(key).and_then(Value::as_str).is_some_and(|t| !t.trim().is_empty());
    call.name == SPEAK
        && serde_json::from_str::<Value>(&call.arguments)
            .is_ok_and(|args| messages(&args).any(|l| filled(l, "text") || filled(l, "action")))
}

/// One piece of what a story reply shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// What one character does and says next: one bubble.
    Said {
        /// Who.
        character: String,
        /// Markdown: their actions in italics, and their words.
        text: String,
    },
    /// Where the story is now, as the reply set it: shown as a scene card.
    Scene(String),
    /// A change to the story (arrival, departure, memory), shown small.
    Note(String),
}

/// What a story reply's calls (name, arguments) say, in order: a new scene
/// and a [`Part::Said`] per message (a character's messages in a row share
/// one). Calls may still be streaming, as incomplete JSON. What
/// the calls changed (arrivals, departures, memories) is shown by
/// [`reply_parts`] once they ran.
#[must_use]
pub fn parts<'a>(calls: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<Part> {
    let mut parts = Vec::new();
    let text =
        |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_owned();
    for (_, arguments) in calls.filter(|(name, _)| *name == SPEAK) {
        let Some(args) = parse_partial(arguments) else {
            continue;
        };
        // A new scene first, as it is written: it sets the stage.
        let scene = Part::Scene(plain(&text(&args, "scene")));
        if !matches!(&scene, Part::Scene(s) if s.is_empty()) && !parts.contains(&scene) {
            parts.push(scene);
        }
        for message in messages(&args) {
            let (speaker, action, said) =
                (plain(&text(message, "character")), plain(&text(message, "action")), text(message, "text"));
            if speaker.is_empty() || (action.is_empty() && said.is_empty()) {
                continue;
            }
            let mut block = if action.is_empty() { String::new() } else { format!("*{action}*") };
            if !said.is_empty() {
                if !block.is_empty() {
                    block.push(' ');
                }
                block.push_str(&said);
            }
            add_said(&mut parts, &speaker, &block);
        }
    }
    parts
}

/// What a finished story reply shows: its [`parts`], with notes for what
/// its calls really changed, as its `change` record holds it. Newcomers
/// join before anyone speaks; departures and new memories follow. A call
/// that changed nothing (casting someone already cast, sending away
/// someone already gone, remembering a known fact) shows nothing.
#[must_use]
pub fn reply_parts(message: &StoredMessage) -> Vec<Part> {
    let mut parts = parts(message.tool_calls.iter().map(|r| (r.call.name.as_str(), r.call.arguments.as_str())));
    let Some(change) = &message.change else {
        return parts;
    };
    let note = |who: &str, what: &str| Part::Note(format!("{} {what}", plain(who)));
    let joined = change
        .cast
        .iter()
        .filter(|(before, _)| before.is_none())
        .map(|(_, after)| note(&after.name, "joins the story"));
    let at = parts.iter().position(|p| matches!(p, Part::Said { .. })).unwrap_or(parts.len());
    parts.splice(at..at, joined.collect::<Vec<_>>());
    let left =
        change.cast.iter().filter(|(before, after)| before.as_ref().is_some_and(|b| b.present) && !after.present);
    parts.extend(left.map(|(_, after)| note(&after.name, "leaves the scene")));
    let renamed = change
        .cast
        .iter()
        .filter_map(|(before, after)| before.as_ref().filter(|b| b.name != after.name).map(|b| (b, after)));
    parts.extend(renamed.map(|(before, after)| note(&before.name, &format!("is now called {}", plain(&after.name)))));
    parts.extend(change.memories.iter().map(|m| Part::Note(format!("Remembered: {}", plain(m)))));
    parts
}

/// Adds `text` to the last bubble when it is `character`'s, else starts
/// one, so someone who speaks again after another gets a new bubble;
/// returns its index.
fn add_said(parts: &mut Vec<Part>, character: &str, text: &str) -> usize {
    if let Some(Part::Said { character: c, text: said }) = parts.last_mut()
        && names::same(c, character)
    {
        said.push_str("\n\n");
        said.push_str(text);
        return parts.len() - 1;
    }
    parts.push(Part::Said { character: character.to_owned(), text: text.to_owned() });
    parts.len() - 1
}

/// The speaker a Markdown block opens with (`**Name**`) and what follows
/// the name, if the block is one character's speech.
fn speaker(block: &str) -> Option<(&str, &str)> {
    let rest = block.strip_prefix("**")?;
    let end = rest.find("**")?;
    let (name, after) = (rest[..end].trim(), &rest[end + 2..]);
    let valid = !name.is_empty() && !name.contains('\n') && name.chars().count() <= 80;
    (valid && (after.is_empty() || after.starts_with([':', ' ', '\n']))).then_some((name, after))
}

/// Parts read back from a reply's Markdown: [`display`]'s format, or the
/// `**Name** · *action*` line above a quote of earlier versions. For
/// replies edited when edits still became plain text; empty when no one
/// speaks in it.
#[must_use]
pub fn parse(markdown: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut current = None;
    for block in markdown.split("\n\n").map(str::trim).filter(|b| !b.is_empty()) {
        if let Some((name, after)) = speaker(block) {
            let text = if let Some(said) = after.strip_prefix(':') {
                said.trim().to_owned()
            } else {
                // `**Name** · *action*`, then `> ` lines.
                let (header, quote) = after.split_once('\n').unwrap_or((after, ""));
                let action = plain(header.trim().trim_start_matches('·'));
                let said: Vec<&str> = quote.lines().map(|l| l.trim_start().trim_start_matches('>').trim()).collect();
                let said = said.join("\n");
                match (action.is_empty(), said.is_empty()) {
                    (true, _) => said,
                    (false, true) => format!("*{action}*"),
                    (false, false) => format!("*{action}* {said}"),
                }
            };
            current = Some(add_said(&mut parts, name, &text));
            continue;
        }
        let italic = block.len() > 2 && block.starts_with('*') && block.ends_with('*') && !block.starts_with("**");
        let event = italic
            && [" joins the story", " leaves the scene"]
                .iter()
                .any(|e| block.trim_end_matches(['*', '.']).ends_with(e));
        match current {
            // A paragraph after someone's speech is more of it.
            Some(index) if !event => {
                if let Some(Part::Said { text, .. }) = parts.get_mut(index) {
                    text.push_str("\n\n");
                    text.push_str(block);
                }
            }
            // Arrivals and departures once ended with a full stop.
            _ if event => parts.push(Part::Note(plain(block).trim_end_matches('.').to_owned())),
            _ => parts.push(Part::Note(plain(block))),
        }
    }
    if parts.iter().any(|p| matches!(p, Part::Said { .. })) { parts } else { Vec::new() }
}

/// Replaces the speech of a reply's `calls` with `said` (character and
/// Markdown, empty ones dropped) in one speak call where the first was,
/// keeping its scene, arrivals, departures and results; other calls stay.
/// With no speak call yet, one is added.
pub fn respeak(calls: &mut Vec<ToolResult>, said: &[(&str, &str)]) {
    let (mut scene, mut introduce, mut leave, mut outputs) = (String::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut at, mut call_id) = (None, None);
    let mut kept = Vec::with_capacity(calls.len());
    for result in calls.drain(..) {
        if result.call.name != SPEAK {
            kept.push(result);
            continue;
        }
        at.get_or_insert(kept.len());
        call_id.get_or_insert(result.call.call_id);
        if let Ok(args) = serde_json::from_str::<Value>(&result.call.arguments) {
            if scene.is_empty() {
                args.get("scene").and_then(Value::as_str).unwrap_or_default().clone_into(&mut scene);
            }
            introduce.extend(args.get("introduce").and_then(Value::as_array).into_iter().flatten().cloned());
            leave.extend(args.get("leave").and_then(Value::as_array).into_iter().flatten().cloned());
        }
        if !result.output.is_empty() {
            outputs.push(result.output);
        }
    }
    let messages: Vec<Value> = said
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(character, text)| json!({ "character": character, "text": text.trim() }))
        .collect();
    if !messages.is_empty() {
        let arguments =
            json!({ "scene": scene, "introduce": introduce, "messages": messages, "leave": leave }).to_string();
        let call = ToolCall {
            call_id: call_id.unwrap_or_else(|| format!("call_{}", new_id())),
            name: SPEAK.into(),
            arguments,
        };
        let output = if outputs.is_empty() { "Spoken.".to_owned() } else { outputs.join(" ") };
        kept.insert(at.unwrap_or(kept.len()), ToolResult { call, output });
    }
    *calls = kept;
}

/// `parts` as Markdown, for copying and editing.
#[must_use]
pub fn display(parts: &[Part]) -> String {
    let blocks: Vec<String> = parts
        .iter()
        .map(|part| match part {
            Part::Said { character, text } => format!("**{character}**: {text}"),
            Part::Scene(note) | Part::Note(note) => format!("*{note}*"),
        })
        .collect();
    blocks.join("\n\n")
}

/// `text` on one line without Markdown emphasis or code marks, so a name
/// or action never breaks the formatting around it.
fn plain(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .filter(|c| !matches!(c, '*' | '_' | '`') && *c != ASIDE.0 && *c != ASIDE.1)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Parses JSON that may be cut off part-way, as tool arguments are while
/// they stream: open strings, arrays and objects are closed, and a
/// trailing key or value too incomplete for that is dropped.
#[must_use]
pub fn parse_partial(text: &str) -> Option<Value> {
    let mut end = text.len();
    // Each attempt cuts back to an earlier boundary, so this ends.
    for _ in 0..32 {
        let (closed, boundaries) = close(&text[..end]);
        if let Ok(value) = serde_json::from_str(&closed) {
            return Some(value);
        }
        end = boundaries.into_iter().rfind(|&b| b < end)?;
    }
    None
}

/// `text` with its open string, arrays and objects closed, and the places
/// it can be cut back to (before a comma, after an opening bracket).
fn close(text: &str) -> (String, Vec<usize>) {
    let (mut stack, mut boundaries) = (Vec::new(), Vec::new());
    let (mut in_string, mut escaped) = (false, false);
    for (i, c) in text.char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' | '[' => {
                stack.push(if c == '{' { '}' } else { ']' });
                boundaries.push(i + 1);
            }
            '}' | ']' => {
                stack.pop();
            }
            ',' => boundaries.push(i),
            _ => {}
        }
    }
    let mut closed = text.to_owned();
    if escaped {
        // A lone backslash would escape the closing quote.
        closed.pop();
    }
    if in_string {
        closed.push('"');
    }
    while let Some(closer) = stack.pop() {
        closed.push(closer);
    }
    (closed, boundaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parentheses_are_out_of_character() {
        let spans = |text: &str| asides(text).into_iter().map(|s| text[s].to_owned()).collect::<Vec<_>>();
        assert_eq!(spans("I knock. (make it tense) I wait. (slowly)"), ["(make it tense)", "(slowly)"]);
        assert_eq!(spans("(tell her (quietly) to go) ok"), ["(tell her (quietly) to go)"], "the outermost");
        assert_eq!(spans("(see f(x) here)"), ["(see f(x) here)"], "a pair inside nests");
        assert_eq!(spans("an (open one, and (a closed one)"), ["(a closed one)"]);
        assert!(spans("f(x), sad :( and glad :-) and \\(kept\\) and () and )(").is_empty(), "text, not asides");
        assert_eq!(spans("(I'm sad :( really)"), ["(I'm sad :( really)"], "an emoticon is no parenthesis");
        assert!(spans(&"(".repeat(5000)).is_empty());
        assert_eq!(spans(&format!("{}é{}", "(".repeat(3000), ")".repeat(3000))).len(), 1);

        assert_eq!(
            user_display("I knock. (make it *tense*) I wait.\n/ooc and slow"),
            "I knock. \u{E000}make it tense\u{E001} I wait. \u{E000}and slow\u{E001}"
        );
        assert_eq!(user_display("((skip ahead))"), "\u{E000}skip ahead\u{E001}", "a second pair is dropped too");
        assert_eq!(user_display("(one\n\ntwo)"), "\u{E000}one\u{E001}\n\n\u{E000}two\u{E001}", "each line marked");
        assert_eq!(spoken("I knock. (make it tense) I wait. \\(sic\\)\n/ooc slow"), "I knock. I wait. (sic)");
        assert_eq!(spoken("(only this)"), "");
        assert_eq!(instructions("I knock. (tense) I wait.\n/ooc slow"), "tense slow");
        assert_eq!(mark_asides("I knock. (tense) \\(sic\\)"), ("I knock. (OOC: tense) (sic)".to_owned(), true));
        assert_eq!(mark_asides("I knock."), ("I knock.".to_owned(), false));
    }

    #[test]
    fn the_ooc_button_marks_the_rest_or_everything() {
        assert_eq!(with_ooc(" I knock. ", None), "I knock.");
        assert_eq!(with_ooc("I knock. make it tense", Some(8)), "I knock.\n/ooc make it tense");
        assert_eq!(with_ooc("I knock. ", Some(9)), "/ooc I knock.", "nothing after it: all of it");
        assert_eq!(with_ooc("skip ahead", Some(0)), "/ooc skip ahead");
        assert_eq!(with_ooc("é", Some(1)), "/ooc é", "off a char boundary");
        assert_eq!(with_ooc("  ", Some(1)), "");
        for (text, from) in [("I knock.\nmake it\ntense", 9), ("a", 0)] {
            let sent = with_ooc(text, Some(from));
            assert_eq!(split_ooc(&sent).1, Some(text[from..].trim()), "{sent:?} reads back");
        }
    }

    fn call(name: &str, arguments: &Value) -> ToolCall {
        ToolCall { call_id: "c".into(), name: name.into(), arguments: arguments.to_string() }
    }

    fn member(name: &str, present: bool) -> CastMember {
        CastMember {
            id: name.to_lowercase(),
            name: name.into(),
            description: format!("{name}, described."),
            present,
            ..CastMember::default()
        }
    }

    #[test]
    fn the_memory_review_reads_a_plain_transcript() {
        let mut reply = StoredMessage::new(Role::Assistant, String::new());
        let speak = json!({ "scene": "The woods.", "messages": [{ "character": "Katniss", "action": "lowers her bow", "text": "Stay back." }] });
        reply.tool_calls = vec![
            ToolResult { call: call(SPEAK, &speak), output: String::new() },
            ToolResult { call: call(REMEMBER, &json!({ "memories": ["x"] })), output: String::new() },
        ];
        let mut failed = StoredMessage::new(Role::Assistant, "Boom.".into());
        failed.failed = true;
        let old = StoredMessage::new(Role::Assistant, "**Peeta**: Hello.".into());
        let messages = [StoredMessage::new(Role::User, "I wave.\n/ooc be brief".into()), reply, failed, old];
        let text = transcript(&messages, "Gale", None, 10_000);
        assert_eq!(text, "Gale: I wave.\n\n[The woods.]\n\nKatniss: *lowers her bow* Stay back.\n\nPeeta: Hello.");

        // Only the end is kept, and never cut inside a character.
        assert_eq!(transcript(&messages, "Gale", None, 6), "…Hello.");
        assert_eq!(transcript(&[StoredMessage::new(Role::User, "ééé".into())], "G", None, 3), "…ééé");
        assert!(transcript(&[], "Gale", None, 5).is_empty());
        // A chat's replies are its partner's plain text.
        let chat = [
            StoredMessage::new(Role::User, "Hi.".into()),
            StoredMessage::new(Role::Assistant, "Mira: *nods* Hey.".into()),
        ];
        assert_eq!(transcript(&chat, "Gale", Some("Mira"), 100), "Gale: Hi.\n\nMira: *nods* Hey.");

        let prompt = reviewer_prompt(
            Some(("Panem", "Twelve districts.")),
            Some("Gale"),
            &["Katniss"],
            &["Rue is hurt.".to_owned()],
        );
        assert!(
            prompt.starts_with(REVIEWER) && prompt.contains("# World: Panem") && prompt.ends_with("1. Rue is hurt.")
        );
    }

    #[test]
    fn reviews_drop_outdated_memories_by_number() {
        let shown = ["Rue is hurt.".to_owned(), "Gale owes Peeta.".to_owned(), "Prim is safe.".to_owned()];
        // Added and edited while the review ran: both stay.
        let mut memories =
            vec!["Rue is hurt.".to_owned(), "Gale owes Peeta 5 coins.".to_owned(), "Prim is safe.".to_owned()];
        memories.push("Katniss lost her bow.".to_owned());
        let revision = json!({ "forget": [1, "2", 0, 9, "x"], "memories": ["Rue has healed.", " prim is SAFE. "] });
        revise(&call(REMEMBER, &revision), &shown, &mut memories);
        assert_eq!(memories, ["Gale owes Peeta 5 coins.", "Prim is safe.", "Katniss lost her bow.", "Rue has healed."]);
        revise(&call(REMEMBER, &json!("not an object")), &shown, &mut memories);
        assert_eq!(memories.len(), 4, "a broken call changes nothing");
    }

    #[test]
    fn chat_replies_are_cleaned_of_what_small_models_add() {
        assert_eq!(chat_text("  Mira: *smiles* Hi.", "Mira", None), "*smiles* Hi.");
        assert_eq!(chat_text("**Mira**: Hi.\n**Mira:** Again.", "mira", None), "Hi.\nAgain.");
        // It stops where it starts writing for the user.
        assert_eq!(chat_text("Hi.\n\nGale: Hello!\nMira: Ha.", "Mira", Some("Gale")), "Hi.");
        assert_eq!(chat_text("Hi.\n\nGale: Hello!", "Mira", None), "Hi.\n\nGale: Hello!", "not without a name");
        // Colons in prose stay.
        assert_eq!(chat_text("Listen: it's late. Time: 3am.", "Mira", Some("Gale")), "Listen: it's late. Time: 3am.");
        assert_eq!(chat_text("Gale: hi", "Mira", Some("Gale")), "");
        assert_eq!(label("*Captain Rex*: Move."), Some(("Captain Rex", "Move.")));
        assert_eq!(label("Well, here's the thing: no."), None);
    }

    #[test]
    fn partial_json_streams_line_by_line() {
        let full = r#"{"lines":[{"character":"Katniss","action":"draws","text":"Stay back."},{"character":"Peeta","text":"Easy."}]}"#;
        for end in 0..=full.len() {
            // Every prefix parses (or is too short to say anything) without panicking.
            let _ = parse_partial(&full[..end]);
        }
        let cut = |n: &str| parse_partial(&full[..full.find(n).unwrap() + n.len()]).unwrap();
        assert_eq!(cut("Stay ba")["lines"][0]["text"], "Stay ba", "an open string is shown so far");
        assert_eq!(cut(r#""Peeta","te"#)["lines"][1]["character"], "Peeta", "a half key is dropped");
        assert_eq!(cut(r#""text":"#)["lines"][0]["action"], "draws");
        assert_eq!(parse_partial(r#"{"lines":[{"text":"a\"#).unwrap()["lines"][0]["text"], "a");
        assert_eq!(parse_partial(full).unwrap()["lines"][1]["text"], "Easy.");
        assert!(parse_partial("").is_none());
    }

    fn said(character: &str, text: &str) -> Part {
        Part::Said { character: character.into(), text: text.into() }
    }

    #[test]
    fn speaking_again_after_another_starts_a_new_bubble() {
        let args = json!({ "messages": [
            { "character": "Katniss", "action": "draws her *bow*", "text": "Stay back." },
            { "character": "Peeta", "text": "Easy." },
            { "character": "katniss", "text": "I mean it." },
            { "character": "Katniss", "text": "Now." },
            { "character": "Haymitch", "action": "takes a long drink", "text": "" },
            { "character": "", "text": "nobody" },
            { "character": "Effie", "text": " " },
        ]});
        let shown = parts([(SPEAK, args.to_string().as_str())].into_iter());
        assert_eq!(
            shown,
            [
                said("Katniss", "*draws her bow* Stay back."),
                said("Peeta", "Easy."),
                said("katniss", "I mean it.\n\nNow."),
                said("Haymitch", "*takes a long drink*")
            ]
        );
        assert_eq!(display(&shown[3..]), "**Haymitch**: *takes a long drink*");
        // Stories saved before `messages` still show.
        let old = json!({ "lines": [{ "character": "Peeta", "text": "Easy." }] }).to_string();
        assert_eq!(parts([(SPEAK, old.as_str())].into_iter()), [said("Peeta", "Easy.")]);
        let created = json!({ "name": "Cato", "description": "A career." }).to_string();
        assert!(
            parts([(CREATE_CHARACTER, created.as_str())].into_iter()).is_empty(),
            "what calls change shows once they ran"
        );
    }

    #[test]
    fn edited_text_reads_back_into_bubbles() {
        // What display writes reads back as it was.
        let shown = [
            Part::Note("The woods.".into()),
            said("Katniss", "*draws* Stay back.\n\nI mean it."),
            said("Peeta", "Easy."),
        ];
        assert_eq!(parse(&display(&shown)), shown);
        // Earlier versions: a name over a quote, after narration.
        let old = "Leaves rustle.\n\n**Katniss** · *draws her bow*\n> Stay back.\n> I mean it.\n\n**Peeta**\n> Easy.\n\n*Rue joins the story.*";
        assert_eq!(
            parse(old),
            [
                Part::Note("Leaves rustle.".into()),
                said("Katniss", "*draws her bow* Stay back.\nI mean it."),
                said("Peeta", "Easy."),
                Part::Note("Rue joins the story".into())
            ]
        );
        assert!(parse("Just an answer, **bold** in it.").is_empty(), "no one speaks: plain Markdown");
        assert!(parse("**Note**:").len() == 1 && parse("**").is_empty() && parse("****: x").is_empty());
        for text in ["**é", "**a**", "*", "**\n**: x", "**Ünï**· x\n>"] {
            let _ = parse(text);
        }
    }

    #[test]
    fn remembering_adds_new_facts_once() {
        let (mut cast, mut scene, mut memories) = (Vec::new(), String::new(), vec!["Rue is hurt.".to_owned()]);
        let facts = call(REMEMBER, &json!({ "memories": [" Katniss  promised\nPrim. ", "rue is hurt.", "", 7] }));
        assert_eq!(run(&facts, &mut cast, &mut scene, &mut memories, None), "Remembered.");
        assert_eq!(memories, ["Rue is hurt.", "Katniss promised Prim."]);
        assert_eq!(run(&facts, &mut cast, &mut scene, &mut memories, None), "Nothing new to remember.");
        let long = call(REMEMBER, &json!({ "memories": ["é".repeat(1000)] }));
        run(&long, &mut cast, &mut scene, &mut memories, None);
        assert_eq!(memories[2].chars().count(), MEMORY_LIMIT);
    }

    #[test]
    fn notes_show_only_what_really_changed() {
        // Katniss is here; the user sent Haymitch away by hand.
        let mut cast = vec![member("Katniss", true), member("Haymitch", false)];
        let (mut scene, mut memories) = (String::new(), vec!["Rue is hurt.".to_owned()]);
        let (cast_before, scene_before, known) = (cast.clone(), scene.clone(), memories.len());
        let speak = json!({
            "scene": "",
            "introduce": [{ "name": "Katniss", "description": "Again." }, { "name": "Rue", "description": "A girl." }],
            "messages": [{ "character": "Katniss", "text": "Go." }, { "character": "Rue", "text": "Hi." }],
            "leave": ["Haymitch", "Katniss", "katniss"],
        });
        let facts = json!({ "memories": ["Katniss left.", "rue is hurt."] });
        let calls = [
            call(SPEAK, &speak),
            call(CREATE_CHARACTER, &json!({ "name": "Rue", "description": "Twice." })),
            call(REMEMBER, &facts),
            call(REMEMBER, &facts),
        ];
        let mut reply = StoredMessage::new(Role::Assistant, String::new());
        for call in calls {
            let output = run(&call, &mut cast, &mut scene, &mut memories, None);
            reply.tool_calls.push(ToolResult { call, output });
        }
        reply.change =
            openrp_core::StoryChange::between(&cast_before, &scene_before, &cast, &scene, &memories[known..]);
        assert_eq!(
            reply_parts(&reply),
            [
                Part::Note("Rue joins the story".into()),
                said("Katniss", "Go."),
                said("Rue", "Hi."),
                Part::Note("Katniss leaves the scene".into()),
                Part::Note("Remembered: Katniss left.".into()),
            ],
            "no join for Katniss (already here), no leave for Haymitch (already gone), each fact once"
        );
        // Cut off before anything ran: speech only.
        reply.change = None;
        assert_eq!(reply_parts(&reply), [said("Katniss", "Go."), said("Rue", "Hi.")]);
    }

    #[test]
    fn edits_rewrite_the_speak_call() {
        let speak = |id: &str, args: Value| ToolResult {
            call: ToolCall { call_id: id.into(), name: SPEAK.into(), arguments: args.to_string() },
            output: "Spoken.".into(),
        };
        let created = ToolResult {
            call: call(CREATE_CHARACTER, &json!({ "name": "Rue", "description": "A girl." })),
            output: "Rue joined the cast and is in the scene.".into(),
        };
        let mut calls = vec![
            created.clone(),
            speak(
                "a",
                json!({ "scene": "The woods.", "messages": [{ "character": "Katniss", "text": "Hi." }, { "character": "Peeta", "text": "Yo." }] }),
            ),
            speak("b", json!({ "messages": [{ "character": "Rue", "text": "Hey." }], "leave": ["Peeta"] })),
        ];
        respeak(&mut calls, &[("Katniss", "Hello there."), ("Peeta", " "), ("Rue", "Hey.")]);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], created, "other calls stay");
        assert_eq!((calls[1].call.call_id.as_str(), calls[1].output.as_str()), ("a", "Spoken. Spoken."));
        let shown = parts(calls.iter().map(|r| (r.call.name.as_str(), r.call.arguments.as_str())));
        assert_eq!(shown, [Part::Scene("The woods.".into()), said("Katniss", "Hello there."), said("Rue", "Hey.")]);
        // A reply read back from text gets a call.
        let mut none = Vec::new();
        respeak(&mut none, &[("Katniss", "Hi.")]);
        assert!(none[0].call.name == SPEAK && none[0].call.call_id.starts_with("call_") && none[0].output == "Spoken.");
    }

    #[test]
    fn names_are_matched_and_characters_renamed() {
        let gale = Player { name: "Gale".into(), ..Player::default() };
        let mut cast = vec![member("Captain Rex", true), member("Alice", false)];
        let (mut scene, mut memories) = (String::new(), Vec::new());
        let mut run =
            |call: &ToolCall, cast: &mut Vec<CastMember>| run(call, cast, &mut scene, &mut memories, Some(&gale));

        // A short name finds the character, and the model is told theirs.
        let short = call(SPEAK, &json!({ "messages": [{ "character": "Rex", "text": "Sir." }] }));
        let said = run(&short, &mut cast);
        assert!(said.contains("Rex is Captain Rex in the cast") && !said.contains("Not in the cast"), "{said}");
        assert!(
            undescribed([&short].into_iter(), &cast, Some(&gale)).is_empty(),
            "no repair round for a known character"
        );
        let again = run(&call(CREATE_CHARACTER, &json!({ "name": "rex", "description": "A clone." })), &mut cast);
        assert!(again.contains("already in the cast, as Captain Rex") && cast.len() == 2, "{again}");

        // The model renames someone; the old name keeps meaning them.
        let renamed = run(
            &call(
                UPDATE_CHARACTER,
                &json!({ "character": "Alice", "name": "Queen Alice", "description": "Crowned now." }),
            ),
            &mut cast,
        );
        assert!(renamed.starts_with("Updated: Alice is now called Queen Alice"), "{renamed}");
        assert_eq!(
            (cast[1].name.as_str(), cast[1].aliases.as_slice(), cast[1].description.as_str()),
            ("Queen Alice", ["Alice".to_owned()].as_slice(), "Crowned now.")
        );
        let old_name =
            call(SPEAK, &json!({ "messages": [{ "character": "Alice", "text": "Hello." }], "leave": ["Captain Rex"] }));
        let said = run(&old_name, &mut cast);
        assert!(
            said.contains("Moved into the scene: Queen Alice.") && said.contains("Left the scene: Captain Rex."),
            "{said}"
        );
        assert!(cast[1].present && !cast[0].present);
        let mut reply = StoredMessage::new(Role::Assistant, String::new());
        let before = vec![CastMember { name: "Alice".into(), ..cast[1].clone() }];
        reply.change = openrp_core::StoryChange::between(&before, "", &cast[1..], "", &[]);
        assert_eq!(reply_parts(&reply), [Part::Note("Alice is now called Queen Alice".into())], "the rename shows");

        // Errors name the cast; the user's character is not the model's.
        assert!(
            run(&call(UPDATE_CHARACTER, &json!({ "character": "Bob", "name": "Robert" })), &mut cast)
                .contains("Captain Rex, Queen Alice")
        );
        assert!(
            run(&call(UPDATE_CHARACTER, &json!({ "character": "Gale", "name": "G" })), &mut cast)
                .contains("user's character")
        );
        assert!(
            run(&call(UPDATE_CHARACTER, &json!({ "character": "Rex", "name": "Queen Alice" })), &mut cast)
                .starts_with("Error"),
            "taken"
        );
        assert!(
            run(&call(UPDATE_CHARACTER, &json!({ "character": "Rex" })), &mut cast).starts_with("Error"),
            "nothing to change"
        );
    }

    #[test]
    fn tools_run_against_the_story() {
        let gale = Player { name: "Gale".into(), ..Player::default() };
        let mut cast = vec![member("Katniss", true), member("Haymitch", false)];
        let (mut scene, mut memories) = (String::new(), Vec::new());
        let created = run(
            &call(CREATE_CHARACTER, &json!({ "name": "Cato", "description": "A career." })),
            &mut cast,
            &mut scene,
            &mut memories,
            Some(&gale),
        );
        assert!(created.contains("Cato joined"));
        assert!(cast.last().is_some_and(|m| m.name == "Cato" && m.present && !m.id.is_empty()));
        assert!(
            run(
                &call(CREATE_CHARACTER, &json!({ "name": "cato", "description": "Again." })),
                &mut cast,
                &mut scene,
                &mut memories,
                None
            )
            .contains("already")
        );
        assert!(
            run(
                &call(CREATE_CHARACTER, &json!({ "name": "Gale", "description": "Me." })),
                &mut cast,
                &mut scene,
                &mut memories,
                Some(&gale)
            )
            .contains("user's character")
        );
        assert!(
            run(
                &call(CREATE_CHARACTER, &json!({ "name": "Rue", "description": " " })),
                &mut cast,
                &mut scene,
                &mut memories,
                None
            )
            .starts_with("Error"),
            "never undescribed"
        );
        assert_eq!(cast.len(), 3);

        let lines = |who: &[&str]| json!({ "introduce": [], "messages": who.iter().map(|w| json!({ "character": w, "text": "Hi." })).collect::<Vec<_>>() });
        assert_eq!(
            run(&call(SPEAK, &lines(&["Katniss", "Cato"])), &mut cast, &mut scene, &mut memories, Some(&gale)),
            "Spoken."
        );
        // Acting moves someone into the scene; a stranger is reported, not added.
        let noted = run(
            &call(SPEAK, &lines(&["Haymitch", "Rue", "Gale", "Rue"])),
            &mut cast,
            &mut scene,
            &mut memories,
            Some(&gale),
        );
        assert!(
            noted.contains("Moved into the scene: Haymitch.")
                && noted.contains("Not in the cast with a description: Rue.")
        );
        assert!(noted.contains("Never speak or act for the user's character.") && !noted.contains("Rue, Rue"));
        assert!(cast.iter().all(|m| m.present) && !cast.iter().any(|m| m.name == "Rue" || m.name == "Gale"));
        assert_eq!(
            undescribed([&call(SPEAK, &lines(&["Rue", "Katniss", "rue", "Gale"]))].into_iter(), &cast, Some(&gale)),
            ["Rue"]
        );

        // Introduced in the same call, a newcomer is cast before they act.
        let introduced = json!({ "introduce": [{ "name": "Rue", "description": "A girl from District 11." }], "lines": [{ "character": "Rue", "text": "Hi." }] });
        let spoken = run(&call(SPEAK, &introduced), &mut cast, &mut scene, &mut memories, None);
        assert!(spoken.starts_with("Spoken. Rue joined the cast") && !spoken.contains("Not in the cast"), "{spoken}");
        assert!(undescribed([&call(SPEAK, &introduced)].into_iter(), &cast, None).is_empty());

        // A member without a description (older stories) can be described.
        cast.push(CastMember { id: "x".into(), name: "Rex".into(), present: true, ..CastMember::default() });
        assert_eq!(undescribed([&call(SPEAK, &lines(&["Rex"]))].into_iter(), &cast, None), ["Rex"]);
        assert_eq!(
            run(
                &call(CREATE_CHARACTER, &json!({ "name": "Rex", "description": "A clone captain." })),
                &mut cast,
                &mut scene,
                &mut memories,
                None
            ),
            "Rex is now described."
        );
        assert!(undescribed([&call(SPEAK, &lines(&["Rex"]))].into_iter(), &cast, None).is_empty());

        assert!(
            run(&call(SPEAK, &json!({ "lines": [] })), &mut cast, &mut scene, &mut memories, None).starts_with("Error")
        );
        assert!(
            run(
                &ToolCall { call_id: "c".into(), name: SPEAK.into(), arguments: "{".into() },
                &mut cast,
                &mut scene,
                &mut memories,
                None
            )
            .starts_with("Error")
        );
        assert!(run(&call("fly", &json!({})), &mut cast, &mut scene, &mut memories, None).starts_with("Error"));
        assert!(scene.is_empty(), "no call set a scene yet");

        // Moving on: a new scene, and Haymitch stays behind after speaking.
        let moved = json!({ "scene": " The train, at dusk. ", "lines": [{ "character": "Haymitch", "text": "Go." }], "leave": ["haymitch", "Nobody"] });
        let noted = run(&call(SPEAK, &moved), &mut cast, &mut scene, &mut memories, None);
        assert!(noted.contains("Left the scene: Haymitch.") && noted.ends_with("Scene set."), "{noted}");
        assert!(scene == "The train, at dusk." && cast.iter().any(|m| m.name == "Haymitch" && !m.present));
        let shown = display(&parts([(SPEAK, moved.to_string().as_str())].into_iter()));
        assert_eq!(shown, "*The train, at dusk.*\n\n**Haymitch**: Go.");
        let kept = json!({ "scene": "", "lines": [{ "character": "Katniss", "text": "Hi." }] });
        run(&call(SPEAK, &kept), &mut cast, &mut scene, &mut memories, None);
        assert_eq!(scene, "The train, at dusk.", "an empty scene keeps the last one");

        assert!(shows_speech(&call(SPEAK, &lines(&["Katniss"]))));
        assert!(!shows_speech(&call(SPEAK, &json!({ "lines": [{ "character": "K", "text": " " }] }))));
        assert!(
            shows_speech(&call(SPEAK, &json!({ "lines": [{ "character": "K", "action": "nods", "text": "" }] }))),
            "acting counts"
        );
        assert!(!shows_speech(&call(CREATE_CHARACTER, &json!({ "name": "X" }))));
    }

    #[test]
    fn the_generator_makes_one_described_character() {
        let prompt = generator_prompt(Some(("Panem", "Twelve districts.")), Some("Gale"), &["Katniss", "Peeta"]);
        assert!(prompt.starts_with(GENERATOR) && prompt.contains("# World: Panem\n\nTwelve districts."));
        assert!(prompt.contains("# The user's character\n\nGale") && prompt.ends_with("Katniss, Peeta"));
        assert_eq!(generator_prompt(None, None, &[]), GENERATOR);

        let made =
            generated(&call(CREATE_CHARACTER, &json!({ "name": " Rue ", "description": "A girl from District 11." })));
        assert_eq!(made, Some(("Rue".into(), "A girl from District 11.".into())));
        assert!(
            generated(&call(CREATE_CHARACTER, &json!({ "name": "Rue", "description": " " }))).is_none(),
            "never undescribed"
        );
        assert!(
            generated(&ToolCall { call_id: "c".into(), name: CREATE_CHARACTER.into(), arguments: "{".into() })
                .is_none()
        );
    }
}
