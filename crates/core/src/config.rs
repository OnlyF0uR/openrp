//! User configuration stored in `~/.openrp/config.toml`.
//!
//! The file is a flat TOML document of `key = "string"` pairs. Only that
//! subset is parsed: basic (`"..."`) and literal (`'...'`) strings, comments
//! and blank lines. Lines this build does not know (other keys, values of
//! other types, tables) are kept word for word and written back, so an older
//! build neither fails on a newer file nor drops what it added.
//!
//! ponytail: flat string-only TOML subset; switch to the `toml` crate once
//! the config needs tables, arrays or numbers.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, Result};

/// Name of the per-user data directory inside the home directory.
const DIR_NAME: &str = ".openrp";
/// Name of the configuration file inside [`DIR_NAME`].
const FILE_NAME: &str = "config.toml";

/// Persistent user settings.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Config {
    /// API root of the OpenAI-compatible provider, e.g.
    /// `https://openrouter.ai/api/v1`; absent until the user enters one.
    pub base_url: Option<String>,
    /// API key for the provider; absent when it needs none.
    pub api_key: Option<String>,
    /// Providers used before, to switch back to, latest first: address and
    /// key. Stored as one string, a line each, the key after a space
    /// (neither holds whitespace).
    pub providers: Vec<(String, Option<String>)>,
    /// Identifier of the model used for new messages.
    pub model: Option<String>,
    /// Reasoning effort for new messages (`none`, `low`, `medium`, `high`);
    /// absent means the model's default.
    pub reasoning: Option<String>,
    /// Colour scheme name, interpreted by the app.
    pub theme: Option<String>,
    /// How replies show the model's reasoning, interpreted by the app.
    pub reasoning_view: Option<String>,
    /// Model for the work done beside the story (memory reviews, summaries,
    /// character generation); `None` uses the story's model.
    pub utility_model: Option<String>,
    /// The window as last closed, e.g. `1200x800` or `1200x800 maximized`
    /// (logical pixels), interpreted by the app.
    pub window: Option<String>,
    /// How far the interface is zoomed, in percent (e.g. `120`),
    /// interpreted by the app; absent means 100.
    pub zoom: Option<String>,
    /// Lines this build does not understand, kept as they were.
    pub extra: Vec<String>,
}

impl std::fmt::Debug for Config {
    // Hand-written so the API key never ends up in logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("providers", &self.providers.iter().map(|(url, _)| url).collect::<Vec<_>>())
            .field("model", &self.model)
            .field("reasoning", &self.reasoning)
            .field("theme", &self.theme)
            .field("reasoning_view", &self.reasoning_view)
            .field("utility_model", &self.utility_model)
            .field("window", &self.window)
            .field("zoom", &self.zoom)
            .field("extra", &self.extra.len())
            .finish()
    }
}

/// The keys this build reads; see [`Config`]. `token` and `provider` belonged
/// to the SereChat sign-in, which is gone: read so they are dropped, never
/// written.
const KEYS: [&str; 12] = [
    "token",
    "provider",
    "base_url",
    "api_key",
    "providers",
    "model",
    "reasoning",
    "theme",
    "reasoning_view",
    "utility_model",
    "window",
    "zoom",
];

impl Config {
    /// Returns `~/.openrp`, the directory holding all local app data.
    ///
    /// # Errors
    /// [`Error::NoHomeDir`] if the platform reports no home directory.
    pub fn dir() -> Result<PathBuf> {
        std::env::home_dir()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|home| home.join(DIR_NAME))
            .ok_or(Error::NoHomeDir)
    }

    /// Returns the full path of the configuration file.
    ///
    /// # Errors
    /// See [`Config::dir`].
    pub fn path() -> Result<PathBuf> {
        Ok(Self::dir()?.join(FILE_NAME))
    }

    /// Loads the configuration, returning defaults if the file does not exist.
    ///
    /// # Errors
    /// I/O failures other than "not found", or a malformed file.
    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    /// Loads the configuration from an explicit path.
    ///
    /// # Errors
    /// See [`Config::load`].
    pub fn load_from(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Writes the configuration atomically to `~/.openrp/config.toml`.
    ///
    /// # Errors
    /// Any I/O failure while creating the directory or writing the file.
    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    /// Writes the configuration atomically to an explicit path.
    ///
    /// The file holds a credential, so on Unix it is created with mode `0600`
    /// inside a `0700` directory. On Windows the user profile ACLs apply.
    ///
    /// # Errors
    /// See [`Config::save`].
    pub fn save_to(&self, path: &Path) -> Result<()> {
        write_private(path, self.serialize().as_bytes())
    }

    /// Uses the provider at `base_url` from now on, with `api_key`, or else
    /// the key saved for that address; the provider it replaces is kept in
    /// [`Config::providers`] to switch back to.
    pub fn use_provider(&mut self, base_url: String, api_key: Option<String>) {
        if let Some(old) = self.base_url.take() {
            self.providers.retain(|(url, _)| *url != old);
            self.providers.insert(0, (old, self.api_key.take()));
        }
        // A saved key only ever goes to the address it was saved for.
        let at = self.providers.iter().position(|(url, _)| *url == base_url);
        let saved = at.and_then(|at| self.providers.remove(at).1);
        self.api_key = api_key.or(saved);
        self.base_url = Some(base_url);
    }

    /// Parses the flat TOML subset described in the module docs.
    ///
    /// # Errors
    /// [`Error::Config`] pointing at the first malformed line holding a key
    /// this build reads.
    pub fn parse(text: &str) -> Result<Self> {
        let mut config = Self::default();
        // Keys after a table header belong to that table: none are ours.
        let mut in_table = false;
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            in_table |= line.starts_with('[');
            let known = line
                .split_once('=')
                .map(|(key, rest)| (key.trim(), rest))
                .filter(|(key, _)| !in_table && KEYS.contains(key));
            let Some((key, rest)) = known else {
                config.extra.push(raw.to_owned());
                continue;
            };
            let value = parse_string(rest.trim()).map_err(|message| Error::Config { line: index + 1, message })?;
            match key {
                "base_url" => config.base_url = Some(value),
                "api_key" => config.api_key = Some(value),
                "providers" => {
                    let saved = value.lines().filter_map(|line| {
                        let (url, key) = line.split_once(' ').unwrap_or((line, ""));
                        (!url.is_empty()).then(|| (url.to_owned(), Some(key.to_owned()).filter(|k| !k.is_empty())))
                    });
                    config.providers = saved.collect();
                }
                "model" => config.model = Some(value),
                "reasoning" => config.reasoning = Some(value),
                "theme" => config.theme = Some(value),
                "reasoning_view" => config.reasoning_view = Some(value),
                "utility_model" => config.utility_model = Some(value),
                "window" => config.window = Some(value),
                "zoom" => config.zoom = Some(value),
                _ => {}
            }
        }
        Ok(config)
    }

    /// Serializes to TOML text: the keys this build reads, then the lines
    /// it kept as they were.
    #[must_use]
    pub fn serialize(&self) -> String {
        let mut out = String::from("# OpenRP configuration.\n");
        let saved = self.providers.iter().map(|(url, key)| match key {
            Some(key) => format!("{url} {key}"),
            None => url.clone(),
        });
        let providers = Some(saved.collect::<Vec<_>>().join("\n")).filter(|p| !p.is_empty());
        let fields = [
            ("base_url", &self.base_url),
            ("api_key", &self.api_key),
            ("providers", &providers),
            ("model", &self.model),
            ("reasoning", &self.reasoning),
            ("theme", &self.theme),
            ("reasoning_view", &self.reasoning_view),
            ("utility_model", &self.utility_model),
            ("window", &self.window),
            ("zoom", &self.zoom),
        ];
        for (key, value) in fields {
            if let Some(value) = value {
                out.push_str(key);
                out.push_str(" = ");
                push_quoted(&mut out, value);
                out.push('\n');
            }
        }
        for line in &self.extra {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

/// Parses a TOML basic or literal string, allowing a trailing comment.
fn parse_string(src: &str) -> std::result::Result<String, &'static str> {
    let mut chars = src.chars();
    let quote = chars.next().filter(|c| *c == '"' || *c == '\'').ok_or("value must be a quoted string")?;
    let mut out = String::new();
    loop {
        let c = chars.next().ok_or("unterminated string")?;
        match c {
            c if c == quote => break,
            '\\' if quote == '"' => out.push(match chars.next().ok_or("unterminated escape")? {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                't' => '\t',
                'r' => '\r',
                'b' => '\u{8}',
                'f' => '\u{c}',
                'u' => parse_unicode(&mut chars, 4)?,
                'U' => parse_unicode(&mut chars, 8)?,
                _ => return Err("invalid escape sequence"),
            }),
            '\n' | '\r' => return Err("newline in string"),
            c => out.push(c),
        }
    }
    let rest = chars.as_str().trim_start();
    if rest.is_empty() || rest.starts_with('#') { Ok(out) } else { Err("unexpected text after value") }
}

/// Reads `digits` hex digits and converts them to a scalar value.
fn parse_unicode(chars: &mut std::str::Chars<'_>, digits: usize) -> std::result::Result<char, &'static str> {
    let hex: String = chars.by_ref().take(digits).collect();
    if hex.len() != digits {
        return Err("truncated unicode escape");
    }
    u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).ok_or("invalid unicode escape")
}

/// Appends `value` as a TOML basic string.
fn push_quoted(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Atomically replaces `path` with `bytes`, readable only by the user.
///
/// Writes a sibling temporary file and renames it over the original, so a
/// crash mid-write never leaves a truncated file behind. The temporary name
/// is unique to this process and write, so two writers never share one. On
/// Unix the file is created `0600` inside a `0700` directory, which is synced
/// after the rename; on Windows the profile ACLs apply.
pub(crate) fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent();
    if let Some(dir) = dir {
        create_private_dir(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}-{}.tmp", std::process::id(), WRITES.fetch_add(1, Ordering::Relaxed)));
    let tmp = PathBuf::from(tmp);
    let written = private_file(&tmp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, path)
    });
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    #[cfg(unix)]
    if let Some(dir) = dir {
        // Makes the rename itself durable; best effort, the data is safe.
        let _ = fs::File::open(dir).and_then(|d| d.sync_all());
    }
    Ok(())
}

/// Takes the lock named `name` in `~/.openrp/` that keeps a second copy of
/// the app off the same data: `Ok(None)` when another process holds it.
/// The lock lasts while the returned file is open, and the OS releases it
/// when the process ends, so a crash never leaves it stuck.
///
/// # Errors
/// No home directory, or the lock file cannot be opened.
pub fn lock_instance(name: &str) -> Result<Option<fs::File>> {
    let dir = Config::dir()?;
    create_private_dir(&dir)?;
    let file = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join(name))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(e.into()),
    }
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    Ok(())
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    Ok(())
}

#[cfg(unix)]
fn private_file(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> std::io::Result<fs::File> {
    fs::OpenOptions::new().write(true).create_new(true).open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_escapes() {
        let config = Config {
            base_url: Some("https://openrouter.ai/api/v1".into()),
            api_key: Some("sk-or".into()),
            providers: vec![
                ("https://api.anthropic.com/v1".into(), Some("sk-ant".into())),
                ("http://localhost:11434/v1".into(), None),
            ],
            model: Some("claude-sonnet-5.5".into()),
            reasoning: Some("high".into()),
            theme: Some("li\"ght\\\n\u{1}é".into()),
            reasoning_view: Some("expanded".into()),
            utility_model: Some("gemma".into()),
            window: Some("1200x800 maximized".into()),
            zoom: Some("120".into()),
            extra: Vec::new(),
        };
        assert_eq!(Config::parse(&config.serialize()).unwrap(), config);
        let saved = vec![("https://x.example/v1".into(), Some("s4ved".into()))];
        let secret = Config { api_key: Some("k3y".into()), providers: saved, ..Config::default() };
        let debug = format!("{secret:?}");
        assert!(!debug.contains("k3y") && !debug.contains("s4ved"), "credentials never show in Debug output");
        assert!(debug.contains("x.example"));
    }

    #[test]
    fn parses_comments_literals_and_unknown_keys() {
        let text = "# hi\n\ntheme = 'raw\\n' # trailing\nfuture = \"x\"\nmodel=\"a\\u00e9\"\n";
        let config = Config::parse(text).unwrap();
        assert_eq!(config.theme.as_deref(), Some("raw\\n"));
        assert_eq!(config.model.as_deref(), Some("aé"));

        // The old sign-in's lines are dropped, not kept as unknown ones.
        let old = Config::parse("token = \"apk_live_x\"\nprovider = \"custom\"\nmodel = \"m\"\n").unwrap();
        assert!(old.extra.is_empty() && old.serialize() == "# OpenRP configuration.\nmodel = \"m\"\n");

        // A newer build's settings survive an older one: kept and written back.
        let newer = "model = \"t\"\nfont_size = 14\nlist = [1, 2]\n[window]\ntheme = \"not ours\"\n";
        let config = Config::parse(newer).unwrap();
        assert_eq!(
            (config.model.as_deref(), config.theme.as_deref()),
            (Some("t"), None),
            "keys in a table are not ours"
        );
        assert_eq!(config.extra, ["font_size = 14", "list = [1, 2]", "[window]", "theme = \"not ours\""]);
        assert_eq!(Config::parse(&config.serialize()).unwrap(), config);
    }

    #[test]
    fn switching_providers_keeps_their_keys() {
        let (a, b, c) = ("https://a.example/v1", "https://b.example/v1", "http://localhost:1/v1");
        let mut config = Config::default();
        config.use_provider(a.into(), Some("ka".into()));
        config.use_provider(b.into(), Some("kb".into()));
        config.use_provider(c.into(), None);
        assert_eq!(config.providers, [(b.into(), Some("kb".into())), (a.into(), Some("ka".into()))]);
        // Back to a: its saved key comes with it, and c is kept keyless.
        config.use_provider(a.into(), None);
        assert_eq!((config.base_url.as_deref(), config.api_key.as_deref()), (Some(a), Some("ka")));
        assert_eq!(config.providers, [(c.into(), None), (b.into(), Some("kb".into()))]);
        // The same address again keeps its key, or takes a new one typed.
        config.use_provider(a.into(), None);
        assert_eq!(config.api_key.as_deref(), Some("ka"));
        config.use_provider(a.into(), Some("new".into()));
        assert_eq!((config.api_key.as_deref(), config.providers.len()), (Some("new"), 2));
        assert_eq!(Config::parse(&config.serialize()).unwrap(), config);
    }

    #[test]
    fn rejects_garbage() {
        for bad in ["model = x", "model = \"open", "model = \"a\" b", "model = \"\\q\""] {
            assert!(matches!(Config::parse(bad), Err(Error::Config { line: 1, .. })), "{bad}");
        }
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("openrp-config-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let config = Config { model: Some("abc".into()), ..Config::default() };
        config.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path).unwrap(), config);
        config.save_to(&path).unwrap();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no temporary file is left behind");
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(Config::load_from(&path).unwrap(), Config::default());
    }
}
