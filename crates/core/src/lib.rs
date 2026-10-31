//! OpenRP's model client and local data.
//!
//! * [`Client`]: model listing and streaming responses with function tools,
//!   from any OpenAI-compatible provider or from Anthropic's own API.
//! * [`Config`]: the user's settings in `~/.openrp/config.toml`.
//! * [`SessionStore`]: saved conversations in `~/.openrp/sessions/`.
//! * [`Library`]: [`World`]s, [`Character`]s and [`Persona`]s in
//!   `~/.openrp/worlds/`, `~/.openrp/characters/` and `~/.openrp/personas/`, and character cards and lorebooks to share
//!   them with other roleplay apps.
//!
//! All network calls are blocking; run them off the UI thread.

mod card;
mod client;
mod completions;
mod config;
mod error;
mod library;
mod messages;
mod responses;
mod session;
mod sse;

pub use card::{card_json, embed_card, macros, read_card, read_lorebook};
pub use client::{Client, Model};
pub use config::{Config, lock_instance};
pub use error::{Error, Result};
pub use library::{Character, Library, LoreEntry, Persona, Portraits, World};
pub use responses::{Completion, InputItem, ResponseRequest, Role, StreamEvent, ToolCall, ToolChoice, ToolSpec, Usage};
pub use session::{
    CastMember, Player, SearchHit, Session, SessionStore, SessionSummary, StoredMessage, StoryChange, ToolResult,
    new_id, rename, unix_now,
};
