//! The daemon's own LLM use (docs/design/20261027_02_INFERENCE_CONFIG_PLAN.md,
//! as amended at 00:59 EDT: the section is `[llm]`).
//!
//! [`resolve`] turns `[llm]`, and the deprecated `[gemini]` and
//! `[summary]`, into [`LlmSettings`]: for each [`Use`], the backend, model
//! and key it runs on, or why it is unavailable. The master switch is off
//! unless set; everything behind it is automatic: a `gemini` backend
//! discovered from a key, every use on, and each use's model chosen by
//! tier and age (the 2026-10-30 13:54 EDT amendment: `flash:newest` for the
//! general agent, `flash-lite:oldest` for summaries, never Pro by default),
//! resolved against the key's model list as [`gemini::Catalogs`] holds it.

pub mod gemini;
pub mod keys;
pub mod planning;
pub mod service;

pub use planning::{AgentProfile, PlanningConfig, PlanningPrompt, ReadPattern};
pub use service::{
    AgentToolSource, ClientFactory, ClientSpec, GeminiFactory, LlmOptions, LlmService, Overrides,
    Profile,
};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use keys::{FileKeySource, KeySource, Lookup};

use crate::config::{AppConfig, GeminiConfig, LlmBackendConfig, LlmConfig, SummaryConfig};

/// What flankr itself uses an LLM for (decision 4). The coding agents in
/// the panes are never among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Use {
    /// The turns the user starts: voice, chat, the centre button.
    GeneralAgent,
    /// Points and spoken text on Finished cards (was `[summary] cards`).
    CardSummaries,
    /// `speech.summarize` while reading aloud.
    SpokenSummaries,
    /// The general agent's turns with the planning tools and prompt
    /// (plan 20261027_04, decision 2): off unless `enabled = true`, on the
    /// general agent's backend and model, never a client of its own.
    Planning,
}

impl Use {
    /// The uses with a backend and model of their own. [`Use::Planning`]
    /// extends the general agent's and is resolved after it.
    pub const ALL: [Use; 3] = [Use::GeneralAgent, Use::CardSummaries, Use::SpokenSummaries];

    /// The use's name in `[llm.uses.<name>]`, also the name its prompt is
    /// to have in `ADDITIONAL_PROMPTS.md` later (00:59 EDT amendment, 3).
    pub fn name(self) -> &'static str {
        match self {
            Use::GeneralAgent => "general_agent",
            Use::CardSummaries => "card_summaries",
            Use::SpokenSummaries => "spoken_summaries",
            Use::Planning => "planning",
        }
    }

    pub fn from_name(name: &str) -> Option<Use> {
        Use::ALL
            .into_iter()
            .chain([Use::Planning])
            .find(|u| u.name() == name)
    }
}

/// A backend's client type (decision 3); only `gemini` for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Gemini,
}

impl Kind {
    pub fn from_name(name: &str) -> Option<Kind> {
        match name {
            "gemini" => Some(Kind::Gemini),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Gemini => "gemini",
        }
    }

    /// The kind's default model choice for `use_`, by tier and age.
    pub fn default_choice(self, use_: Use) -> gemini::ModelChoice {
        match self {
            Kind::Gemini => gemini::default_choice(use_),
        }
    }
}

/// An API key. Its `Debug` never shows it.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        ApiKey(key.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(..)")
    }
}

/// Where keys are looked for (plan 20261102_02, decision 1): the folder a
/// relative `api_key_file` is read against (its config file's, never the
/// working folder), the Keychain when it is used, flankr's config-folder
/// key file, and `GEMINI_API_KEY`.
#[derive(Clone)]
pub struct KeyEnv {
    pub config_dir: PathBuf,
    /// Only the daemon's and `flankr key`'s own setup sets it
    /// ([`KeyEnv::with_keychain`]); tests use a fake or none.
    pub keychain: Option<Arc<dyn KeySource>>,
    pub secrets_file: Option<PathBuf>,
    pub gemini_api_key: Option<ApiKey>,
}

impl KeyEnv {
    /// For the config file `config_path`: its folder, flankr's config
    /// folder and the environment, without the Keychain.
    pub fn from_process(config_path: &Path) -> Self {
        let config_dir = crate::daemon::absolute(config_path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("/"));
        KeyEnv {
            config_dir,
            keychain: None,
            secrets_file: keys::secrets_file(),
            gemini_api_key: std::env::var(gemini::KEY_ENV)
                .ok()
                .map(|k| k.trim().to_string())
                .filter(|k| !k.is_empty())
                .map(ApiKey),
        }
    }

    /// With the login keychain, unless [`keys::keychain_skipped`] says why
    /// not (another system, `FLANKR_KEY_STORE=file`, a test's `HOME`).
    pub fn with_keychain(mut self) -> Self {
        self.keychain = keys::keychain();
        self
    }

    /// `dir` as flankr's config folder, holding the config file and
    /// `secrets/gemini_api_key`; no Keychain and no environment key (tests).
    pub fn in_dir(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        KeyEnv {
            secrets_file: Some(dir.join(gemini::KEY_FILE)),
            config_dir: dir,
            keychain: None,
            gemini_api_key: None,
        }
    }

    /// No source at all.
    pub fn none() -> Self {
        KeyEnv {
            config_dir: PathBuf::from("/"),
            keychain: None,
            secrets_file: None,
            gemini_api_key: None,
        }
    }

    fn path(&self, file: &str) -> PathBuf {
        let path = PathBuf::from(file);
        if path.is_relative() {
            self.config_dir.join(path)
        } else {
            path
        }
    }

    /// Each source for a backend whose `api_key_file` is `file`, in
    /// decision 1's order, with what it holds; all of them when `all`, else
    /// up to the first found or refused.
    pub fn lookups(&self, file: Option<&str>, all: bool) -> Vec<(String, Lookup)> {
        let mut out = Vec::new();
        let mut push = |name: String, lookup: Lookup| {
            let stop = matches!(lookup, Lookup::Found(_) | Lookup::Refused(_));
            out.push((name, lookup));
            stop && !all
        };
        if let Some(file) = file {
            let source = FileKeySource {
                path: self.path(file),
            };
            if push(
                format!("api_key_file {}", keys::tilde(&source.path)),
                source.get(),
            ) {
                return out;
            }
        }
        if let Some(keychain) = &self.keychain
            && push(keychain.name(), keychain.get())
        {
            return out;
        }
        if let Some(path) = &self.secrets_file {
            let source = FileKeySource { path: path.clone() };
            if push(source.name(), source.get()) {
                return out;
            }
        }
        let env = match &self.gemini_api_key {
            Some(key) => Lookup::Found(key.clone()),
            None => Lookup::None,
        };
        push(gemini::KEY_ENV.to_string(), env);
        out
    }

    /// The key for a backend whose `api_key_file` is `file`, and the name
    /// of the source it came from; or why there is none. Never the key's
    /// text.
    pub fn find(&self, file: Option<&str>) -> Result<(ApiKey, String), NoKey> {
        let looked = self.lookups(file, false);
        match looked.last() {
            Some((name, Lookup::Found(key))) => Ok((key.clone(), name.clone())),
            Some((_, Lookup::Refused(why))) => Err(NoKey::Refused(why.clone())),
            _ => {
                let places: Vec<String> = looked
                    .iter()
                    .map(|(name, lookup)| match lookup {
                        Lookup::Locked => format!("{name} (locked)"),
                        _ => name.clone(),
                    })
                    .collect();
                Err(NoKey::Missing(format!(
                    "looked in {}; run `flankr key set gemini`",
                    places.join(", ")
                )))
            }
        }
    }
}

/// Why [`KeyEnv::find`] has no key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoKey {
    /// A source holds one that is not to be read: why, and the fix.
    Refused(String),
    /// None anywhere: the places looked in, and the fix.
    Missing(String),
}

impl PartialEq for KeyEnv {
    fn eq(&self, other: &Self) -> bool {
        self.config_dir == other.config_dir
            && self.secrets_file == other.secrets_file
            && self.gemini_api_key == other.gemini_api_key
            && self.keychain.as_ref().map(|k| k.name()) == other.keychain.as_ref().map(|k| k.name())
    }
}

impl Eq for KeyEnv {}

impl std::fmt::Debug for KeyEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyEnv")
            .field("config_dir", &self.config_dir)
            .field("keychain", &self.keychain.as_ref().map(|k| k.name()))
            .field("secrets_file", &self.secrets_file)
            .field("gemini_api_key", &self.gemini_api_key.is_some())
            .finish()
    }
}

/// The key in `path`, trimmed; `None` when the file does not exist or is
/// empty. A file anyone but its owner may access is refused. Errors name
/// the file, never its contents.
pub fn read_key_file(path: &Path) -> Result<Option<String>, String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = match std::fs::metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("the Gemini key file {}: {e}", path.display())),
    };
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(format!(
            "the Gemini key file {} has mode {mode:04o}; only its owner may have access \
             (chmod 600 {}); it is not read",
            path.display(),
            path.display()
        ));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("the Gemini key file {}: {e}", path.display()))?;
    // An API key is one short token. Anything else (a service-account JSON,
    // several lines) is refused here, so it never reaches a request header.
    match keys::parse_key(&text) {
        Ok(key) => Ok(key.map(|k| k.0)),
        Err(why) => Err(format!("the Gemini key file {} {why}", path.display())),
    }
}

/// What a use runs on.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Target {
    /// The backend's name (`gemini` when discovered).
    pub backend: String,
    pub kind: Kind,
    pub model: String,
    pub key: ApiKey,
}

/// A use's state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseState {
    Available(Target),
    Unavailable {
        /// Why, for the log and the phone.
        reason: String,
        /// Whether the use's own table (or `[summary] cards = false`)
        /// turned it off, as against the master switch, a backend or a key.
        turned_off: bool,
    },
}

impl UseState {
    pub fn target(&self) -> Option<&Target> {
        match self {
            UseState::Available(t) => Some(t),
            UseState::Unavailable { .. } => None,
        }
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            UseState::Available(_) => None,
            UseState::Unavailable { reason, .. } => Some(reason),
        }
    }

    fn off(reason: impl Into<String>) -> Self {
        UseState::Unavailable {
            reason: reason.into(),
            turned_off: false,
        }
    }
}

/// How an available use's model was chosen (the 13:54 EDT amendment,
/// item 4): what was asked for, and how it resolved to
/// [`Target::model`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseModel {
    pub choice: gemini::ModelChoice,
    pub how: gemini::How,
}

/// `[llm]` resolved: each use's state, and what is to be logged about the
/// config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmSettings {
    /// The master switch, after the deprecated `[gemini]`.
    pub enabled: bool,
    pub uses: BTreeMap<Use, UseState>,
    /// For each available use, how its model was chosen.
    pub models: BTreeMap<Use, UseModel>,
    /// Deprecations and conflicts, each once (decision 5), and unknown
    /// use names. Logged at info.
    pub notes: Vec<String>,
    /// Backend errors (decision 3), each once. Logged as warnings.
    pub problems: Vec<String>,
    /// `[llm.uses.planning]`'s `agents_prompt` and valid `read` patterns,
    /// used while [`Use::Planning`] is available.
    pub planning: PlanningConfig,
    /// Each backend with a key: where the key came from (plan 20261102_02,
    /// decision 7), never the key.
    pub key_sources: BTreeMap<String, String>,
}

impl LlmSettings {
    /// Nothing available: the master switch is off.
    pub fn off() -> Self {
        resolve(&AppConfig::default(), &KeyEnv::none())
    }

    pub fn state(&self, use_: Use) -> &UseState {
        &self.uses[&use_]
    }

    pub fn target(&self, use_: Use) -> Option<&Target> {
        self.state(use_).target()
    }

    /// Whether the general agent and summaries (card or spoken) are
    /// available, and why not when either is not (decision 7).
    pub fn availability(&self) -> (bool, bool, Option<String>) {
        let general = self.target(Use::GeneralAgent).is_some();
        let summaries = self.target(Use::CardSummaries).is_some()
            || self.target(Use::SpokenSummaries).is_some();
        let reason = (!general || !summaries)
            .then(|| {
                let first = if !general {
                    Use::GeneralAgent
                } else if self.target(Use::CardSummaries).is_none() {
                    Use::CardSummaries
                } else {
                    Use::SpokenSummaries
                };
                self.state(first).reason().map(str::to_string)
            })
            .flatten();
        (general, summaries, reason)
    }

    /// One line per use, for the log at start: the choice, the model it
    /// resolved to and how (`[llm] spoken_summaries: flash-lite:oldest is
    /// gemini-3.5-flash-lite (matched)`), or why it is unavailable.
    pub fn describe(&self) -> Vec<String> {
        self.uses
            .iter()
            .map(|(u, s)| match (s, self.models.get(u)) {
                (UseState::Available(t), Some(m)) => format!(
                    "[llm] {}: {} is {} ({})",
                    u.name(),
                    m.choice,
                    t.model,
                    gemini::how_name(m.how)
                ),
                (UseState::Available(t), None) => {
                    format!("[llm] {}: {}", u.name(), t.model)
                }
                (UseState::Unavailable { reason, .. }, _) => {
                    format!("[llm] {}: unavailable: {reason}", u.name())
                }
            })
            .collect()
    }

    /// The start log's key lines (plan 20261102_02, decision 7):
    /// `[llm] gemini key: Keychain (dev.flankr/gemini)`, one per backend
    /// with a key. A backend without one is a problem, logged as such.
    pub fn key_lines(&self) -> Vec<String> {
        self.key_sources
            .iter()
            .map(|(name, source)| format!("[llm] {name} key: {source}"))
            .collect()
    }
}

/// The parts of the config `[llm]` is resolved from: the section and the
/// deprecated ones (decision 6 re-reads only these).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LlmSources {
    pub llm: Option<LlmConfig>,
    pub gemini: Option<GeminiConfig>,
    pub summary: Option<SummaryConfig>,
}

impl LlmSources {
    pub fn of(config: &AppConfig) -> Self {
        LlmSources {
            llm: config.llm.clone(),
            gemini: config.gemini.clone(),
            summary: config.summary.clone(),
        }
    }
}

/// `config`'s `[llm]` resolved against `env`, with no model list (each
/// tier rule resolves offline, to libinfer-gemini's default model).
pub fn resolve(config: &AppConfig, env: &KeyEnv) -> LlmSettings {
    resolve_sources(&LlmSources::of(config), env, &gemini::Catalogs::none())
}

/// The `gemini` backends whose model list is to be fetched at start (the
/// 13:54 EDT amendment, item 3): each one with a key, by name, while the
/// master switch is on. Reads the key files; never logs.
pub fn catalog_keys(sources: &LlmSources, env: &KeyEnv) -> Vec<(String, ApiKey)> {
    let llm = effective(sources, &mut Vec::new());
    if llm.enabled != Some(true) {
        return Vec::new();
    }
    let (declared, discovered) = declared(&llm);
    declared
        .iter()
        .filter_map(|(name, b)| match backend(name, b, env, discovered) {
            Ok(BackendKey {
                kind: Kind::Gemini,
                key,
                ..
            }) => Some((name.clone(), key)),
            Err(_) => None,
        })
        .collect()
}

/// Each `gemini` backend's model list for `config`, fetched once (the
/// daemon's start; never in tests).
pub async fn fetch_catalogs(config: &AppConfig, env: &KeyEnv) -> gemini::Catalogs {
    gemini::Catalogs::fetch(catalog_keys(&LlmSources::of(config), env)).await
}

/// A backend with a key.
struct BackendKey {
    kind: Kind,
    model: Option<String>,
    key: ApiKey,
    /// Where the key came from, as the start log names it (decision 7).
    source: String,
}

/// A backend as resolved, or why it cannot be used.
type BackendState = Result<BackendKey, String>;

/// The `[llm]` in force: the section, or the deprecated ones read as it
/// (decision 5), with the notes about them.
fn effective(sources: &LlmSources, notes: &mut Vec<String>) -> LlmConfig {
    // Decision 5: the old sections, read as the new ones, unless `[llm]`
    // is set. Plan 20261027_05, decision 6: `[llm]` counts as set only
    // with its `enabled` key; `[llm.uses.*]` or `[llm.backends.*]` alone
    // leave the old sections read, merged under them.
    match &sources.llm {
        Some(llm) if llm.enabled.is_some() => {
            if sources.gemini.is_some() {
                notes.push(
                    "[gemini] is deprecated and ignored: [llm] is also set, and wins \
                     (use [llm.backends.gemini])"
                        .to_string(),
                );
            }
            if sources.summary.is_some() {
                notes.push(
                    "[summary] is deprecated and ignored: [llm] is also set, and wins \
                     (use [llm.uses.card_summaries] enabled)"
                        .to_string(),
                );
            }
            llm.clone()
        }
        _ => {
            let mut llm = sources.llm.clone().unwrap_or_default();
            if sources.llm.is_some() {
                notes.push(
                    "[llm] has no enabled key, so it does not count as set: the old \
                     [gemini] and [summary] sections are still read (set [llm] enabled)"
                        .to_string(),
                );
            }
            if let Some(g) = &sources.gemini {
                notes.push(
                    "[gemini] is deprecated: it is read as [llm] enabled = true with \
                     [llm.backends.gemini]"
                        .to_string(),
                );
                llm.enabled = Some(true);
                llm.backends
                    .entry("gemini".to_string())
                    .or_insert_with(|| LlmBackendConfig {
                        kind: Some("gemini".to_string()),
                        api_key_file: g.api_key_file.clone(),
                        model: g.model.clone(),
                    });
            }
            if let Some(s) = &sources.summary {
                notes.push(
                    "[summary] is deprecated: cards is read as \
                     [llm.uses.card_summaries] enabled"
                        .to_string(),
                );
                if !s.cards {
                    let card = llm
                        .uses
                        .entry(Use::CardSummaries.name().to_string())
                        .or_default();
                    card.enabled.get_or_insert(false);
                }
            }
            llm
        }
    }
}

/// The backends, discovered when none is declared (decision 2), and
/// whether they were.
fn declared(llm: &LlmConfig) -> (BTreeMap<String, LlmBackendConfig>, bool) {
    let discovered = llm.backends.is_empty();
    let declared = if discovered {
        [(
            "gemini".to_string(),
            LlmBackendConfig {
                kind: Some("gemini".to_string()),
                ..LlmBackendConfig::default()
            },
        )]
        .into_iter()
        .collect()
    } else {
        llm.backends.clone()
    };
    (declared, discovered)
}

/// [`resolve`], from the sections alone, each use's model resolved
/// against `catalogs`.
pub fn resolve_sources(
    sources: &LlmSources,
    env: &KeyEnv,
    catalogs: &gemini::Catalogs,
) -> LlmSettings {
    let mut notes = Vec::new();
    let mut problems = Vec::new();
    let llm = effective(sources, &mut notes);
    for name in llm.uses.keys() {
        if Use::from_name(name).is_none() {
            notes.push(format!(
                "[llm.uses.{name}] is not a use flankr has (general_agent, card_summaries, \
                 spoken_summaries, planning); ignored"
            ));
        }
    }

    // Each use's own switch is known whatever else holds.
    let turned_off = |u: Use| {
        llm.uses
            .get(u.name())
            .and_then(|c| c.enabled)
            .is_some_and(|on| !on)
    };
    let mut uses = BTreeMap::new();
    if llm.enabled != Some(true) {
        for u in Use::ALL {
            let state = if turned_off(u) {
                UseState::Unavailable {
                    reason: format!("[llm.uses.{}] enabled = false", u.name()),
                    turned_off: true,
                }
            } else {
                UseState::off(
                    "LLM use is off: [llm] enabled = true is not set in the daemon's \
                     config.toml",
                )
            };
            uses.insert(u, state);
        }
        let (state, planning) = resolve_planning(
            &llm,
            &UseState::off(
                "LLM use is off: [llm] enabled = true is not set in the daemon's config.toml",
            ),
            &mut notes,
            &mut problems,
        );
        uses.insert(Use::Planning, state);
        return LlmSettings {
            enabled: false,
            uses,
            models: BTreeMap::new(),
            notes,
            problems,
            planning,
            key_sources: BTreeMap::new(),
        };
    }

    // The backends, discovered when none is declared (decision 2).
    let (declared, discovered) = declared(&llm);
    let backends: BTreeMap<String, BackendState> = declared
        .iter()
        .map(|(name, b)| {
            let state = backend(name, b, env, discovered);
            if let Err(e) = &state {
                problems.push(e.clone());
            }
            (name.clone(), state)
        })
        .collect();
    let key_sources = backends
        .iter()
        .filter_map(|(name, state)| Some((name.clone(), state.as_ref().ok()?.source.clone())))
        .collect();

    // `default`: as named, or the only backend, or `gemini`.
    let default: Result<String, String> = match &llm.default {
        Some(name) if backends.contains_key(name) => Ok(name.clone()),
        Some(name) => {
            let e = format!("[llm] default names no backend: `{name}`");
            problems.push(e.clone());
            Err(e)
        }
        None if backends.len() == 1 => Ok(backends.keys().next().unwrap().clone()),
        None if backends.contains_key("gemini") => Ok("gemini".to_string()),
        None => Err("[llm] has several backends and no default".to_string()),
    };

    let mut models = BTreeMap::new();
    for u in Use::ALL {
        let own = llm.uses.get(u.name()).cloned().unwrap_or_default();
        if turned_off(u) {
            uses.insert(
                u,
                UseState::Unavailable {
                    reason: format!("[llm.uses.{}] enabled = false", u.name()),
                    turned_off: true,
                },
            );
            continue;
        }
        let name = match own.backend.as_deref() {
            Some(name) if backends.contains_key(name) => Ok(name.to_string()),
            Some(name) => {
                let e = format!("[llm.uses.{}] backend names no backend: `{name}`", u.name());
                problems.push(e.clone());
                Err(e)
            }
            None => default.clone(),
        };
        let state = match name {
            Err(e) => UseState::off(e),
            Ok(name) => match &backends[&name] {
                Err(e) => UseState::off(e.clone()),
                Ok(BackendKey {
                    kind, model, key, ..
                }) => {
                    // Item 2: a `model` in either form; else the default
                    // rule (item 1).
                    let choice = nonempty(own.model.as_deref())
                        .or(model.as_deref())
                        .map(gemini::ModelChoice::parse)
                        .unwrap_or_else(|| kind.default_choice(u));
                    let resolved = match kind {
                        Kind::Gemini => catalogs.resolve(&name, &choice),
                    };
                    models.insert(
                        u,
                        UseModel {
                            choice,
                            how: resolved.how,
                        },
                    );
                    UseState::Available(Target {
                        backend: name,
                        kind: *kind,
                        model: resolved.model,
                        key: key.clone(),
                    })
                }
            },
        };
        uses.insert(u, state);
    }
    let (state, planning) =
        resolve_planning(&llm, &uses[&Use::GeneralAgent], &mut notes, &mut problems);
    if state.target().is_some()
        && let Some(general) = models.get(&Use::GeneralAgent).cloned()
    {
        models.insert(Use::Planning, general);
    }
    uses.insert(Use::Planning, state);
    LlmSettings {
        enabled: true,
        uses,
        models,
        notes,
        problems,
        planning,
        key_sources,
    }
}

/// `[llm.uses.planning]` (plan 20261027_04, decision 2): off unless
/// `enabled = true`, then the general agent's target, or why not.
fn resolve_planning(
    llm: &LlmConfig,
    general: &UseState,
    notes: &mut Vec<String>,
    problems: &mut Vec<String>,
) -> (UseState, PlanningConfig) {
    let Some(table) = llm.uses.get(Use::Planning.name()) else {
        return (
            UseState::Unavailable {
                reason: "[llm.uses.planning] is off unless enabled = true".to_string(),
                turned_off: true,
            },
            PlanningConfig::default(),
        );
    };
    for (key, set) in [
        ("backend", table.backend.is_some()),
        ("model", table.model.is_some()),
    ] {
        if set {
            notes.push(format!(
                "[llm.uses.planning] {key} is ignored: planning runs on the general agent's \
                 backend and model"
            ));
        }
    }
    let (planning, refused) = PlanningConfig::from_use(table);
    problems.extend(refused);
    let state = if table.enabled != Some(true) {
        UseState::Unavailable {
            reason: "[llm.uses.planning] is off unless enabled = true".to_string(),
            turned_off: true,
        }
    } else {
        match general {
            UseState::Available(target) => UseState::Available(target.clone()),
            UseState::Unavailable { reason, .. } => UseState::off(format!(
                "planning needs general_agent, which is off: {reason}"
            )),
        }
    };
    (state, planning)
}

/// Each `gemini` backend `sources` declare or discover, by name, with its
/// `api_key_file` when set (`flankr key status`). Empty while the master
/// switch is off and no backend is declared.
pub fn gemini_backends(sources: &LlmSources) -> Vec<(String, Option<String>)> {
    let llm = effective(sources, &mut Vec::new());
    let (declared, _) = declared(&llm);
    declared
        .into_iter()
        .filter(|(_, b)| nonempty(b.kind.as_deref()) == Some(Kind::Gemini.name()))
        .map(|(name, b)| {
            let file = nonempty(b.api_key_file.as_deref()).map(str::to_string);
            (name, file)
        })
        .collect()
}

/// The key files `sources` read keys from: flankr's config-folder key
/// file and each backend's `api_key_file`, `[gemini]`'s included (plan
/// 20261027_05, 12:37 EDT amendment, item 3: the file watcher follows
/// them). A change in the Keychain is not watched.
pub fn key_files(sources: &LlmSources, env: &KeyEnv) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = env.secrets_file.iter().cloned().collect();
    let named = sources
        .llm
        .iter()
        .flat_map(|l| l.backends.values())
        .filter_map(|b| nonempty(b.api_key_file.as_deref()))
        .chain(
            sources
                .gemini
                .iter()
                .filter_map(|g| nonempty(g.api_key_file.as_deref())),
        );
    for file in named {
        let path = env.path(file);
        if !files.contains(&path) {
            files.push(path);
        }
    }
    files
}

fn nonempty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// One backend: its kind, its model, its key and where the key came
/// from, or why it cannot be used.
fn backend(name: &str, b: &LlmBackendConfig, env: &KeyEnv, discovered: bool) -> BackendState {
    let kind = match nonempty(b.kind.as_deref()) {
        None => return Err(format!("[llm.backends.{name}] has no kind")),
        Some(k) => Kind::from_name(k).ok_or_else(|| {
            format!("[llm.backends.{name}]: unknown kind `{k}` (only \"gemini\" for now)")
        })?,
    };
    let (key, source) = match env.find(nonempty(b.api_key_file.as_deref())) {
        Ok(found) => found,
        Err(NoKey::Refused(why)) => return Err(format!("[llm.backends.{name}]: {why}")),
        Err(NoKey::Missing(why)) if discovered => return Err(format!("no Gemini key: {why}")),
        Err(NoKey::Missing(why)) => return Err(format!("[llm.backends.{name}] has no key: {why}")),
    };
    Ok(BackendKey {
        kind,
        model: nonempty(b.model.as_deref()).map(str::to_string),
        key,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> AppConfig {
        toml::from_str(text).unwrap()
    }

    /// A config folder with `secrets/gemini_api_key` holding `key`, mode
    /// 0600.
    fn with_key(key: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        write_key(&dir.path().join(gemini::KEY_FILE), key);
        dir
    }

    fn write_key(path: &Path, key: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, key).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn model(s: &LlmSettings, u: Use) -> &str {
        &s.target(u).unwrap().model
    }

    #[test]
    fn no_section_is_off() {
        let dir = with_key("k1");
        let env = KeyEnv::in_dir(dir.path());
        for text in ["", "[llm]\n", "[llm]\nenabled = false\n"] {
            let s = resolve(&parse(text), &env);
            assert!(!s.enabled, "{text:?}");
            for u in Use::ALL {
                assert!(s.target(u).is_none(), "{text:?} {u:?}");
            }
            let (general, summaries, reason) = s.availability();
            assert!(!general && !summaries);
            assert!(reason.unwrap().contains("[llm] enabled"), "{text:?}");
        }
        assert!(!LlmSettings::off().enabled);
    }

    #[test]
    fn enabled_alone_discovers_gemini_with_a_key_and_nothing_without() {
        let cfg = parse("[llm]\nenabled = true\n");
        let dir = with_key("k1");
        let s = resolve(&cfg, &KeyEnv::in_dir(dir.path()));
        for u in Use::ALL {
            let t = s.target(u).unwrap();
            assert_eq!((t.backend.as_str(), t.kind), ("gemini", Kind::Gemini));
            assert_eq!(t.key.expose(), "k1");
        }
        assert_eq!(s.availability(), (true, true, None));
        assert!(s.problems.is_empty() && s.notes.is_empty());

        // GEMINI_API_KEY, when the file is not there.
        let empty = tempfile::tempdir().unwrap();
        let env = KeyEnv {
            gemini_api_key: Some(ApiKey("k2".into())),
            ..KeyEnv::in_dir(empty.path())
        };
        let s = resolve(&cfg, &env);
        assert_eq!(s.target(Use::GeneralAgent).unwrap().key.expose(), "k2");

        // Neither: nothing, and why.
        let s = resolve(&cfg, &KeyEnv::in_dir(empty.path()));
        assert!(Use::ALL.iter().all(|&u| s.target(u).is_none()));
        let (general, summaries, reason) = s.availability();
        assert!(!general && !summaries);
        let reason = reason.unwrap();
        assert!(reason.contains("no Gemini key"), "{reason}");
        assert!(reason.contains("GEMINI_API_KEY"), "{reason}");
        assert_eq!(s.problems.len(), 1, "logged once: {:?}", s.problems);
        // The key is never in the settings' Debug.
        let s = resolve(&cfg, &KeyEnv::in_dir(dir.path()));
        assert!(!format!("{s:?} {:?}", KeyEnv::in_dir(dir.path())).contains("k1"));
    }

    /// A key's model list as the API gives it, in its shapes: versioned
    /// stable names (2.x ones below *oldest*'s 3.0 floor), revisions,
    /// previews, `-latest` aliases and Pro.
    const LISTED: [&str; 14] = [
        "models/gemini-2.0-flash-lite",
        "models/gemini-2.0-flash-lite-001",
        "models/gemini-1.5-flash-lite-preview",
        "models/gemini-2.5-flash-lite",
        "models/gemini-3.5-flash-lite-001",
        "models/gemini-3.5-flash-lite",
        "models/gemini-2.5-flash",
        "models/gemini-3.8-flash",
        "models/gemini-3.9-flash-preview",
        "models/gemini-flash-lite-latest",
        "models/gemini-2.5-pro",
        "models/gemini-3.8-pro",
        "models/gemini-pro-latest",
        "models/text-embedding-004",
    ];

    /// [`LISTED`] as the `gemini` backend's list, plus `other` for
    /// `cheap`, with no network.
    fn listed(other: &[&str]) -> gemini::Catalogs {
        gemini::Catalogs::of([
            ("gemini", gemini::ModelCatalog::from_names(LISTED)),
            ("cheap", gemini::ModelCatalog::from_names(other)),
        ])
    }

    fn rule(tier: gemini::Tier, age: gemini::Age) -> gemini::ModelChoice {
        gemini::ModelChoice::Rule { tier, age }
    }

    fn resolve_on(text: &str, env: &KeyEnv, catalogs: &gemini::Catalogs) -> LlmSettings {
        resolve_sources(&LlmSources::of(&parse(text)), env, catalogs)
    }

    fn how(s: &LlmSettings, u: Use) -> gemini::How {
        s.models[&u].how
    }

    /// The 13:54 EDT amendment, item 1: the defaults are rules, never Pro.
    #[test]
    fn each_use_has_its_default_rule_and_never_pro() {
        use gemini::{Age, Tier};
        for (u, choice) in [
            (Use::GeneralAgent, rule(Tier::Flash, Age::Newest)),
            (Use::Planning, rule(Tier::Flash, Age::Newest)),
            (Use::CardSummaries, rule(Tier::FlashLite, Age::Oldest)),
            (Use::SpokenSummaries, rule(Tier::FlashLite, Age::Oldest)),
        ] {
            assert_eq!(Kind::Gemini.default_choice(u), choice, "{u:?}");
        }
        assert_eq!(
            Kind::Gemini.default_choice(Use::CardSummaries).to_string(),
            "flash-lite:oldest"
        );

        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let s = resolve_on(
            "[llm]\nenabled = true\n[llm.uses.planning]\nenabled = true\n",
            &env,
            &listed(&[]),
        );
        for u in Use::ALL.into_iter().chain([Use::Planning]) {
            assert_eq!(s.models[&u].choice, Kind::Gemini.default_choice(u));
            assert!(!model(&s, u).contains("pro"), "{u:?}");
        }
        // Summaries: the oldest stable Flash-Lite from 3.0 up, not a 2.x,
        // a revision or a preview; the general agent: the `-latest` alias when there is
        // none for Flash, so the newest stable Flash.
        assert_eq!(model(&s, Use::CardSummaries), "gemini-3.5-flash-lite");
        assert_eq!(model(&s, Use::SpokenSummaries), "gemini-3.5-flash-lite");
        assert_eq!(how(&s, Use::SpokenSummaries), gemini::How::Matched);
        assert_eq!(model(&s, Use::GeneralAgent), "gemini-3.8-flash");
        assert_eq!(how(&s, Use::GeneralAgent), gemini::How::Matched);
        assert_eq!(s.target(Use::Planning), s.target(Use::GeneralAgent));

        // With the Flash alias listed, the general agent takes it.
        let aliased = gemini::Catalogs::of([(
            "gemini",
            gemini::ModelCatalog::from_names(LISTED.iter().chain(&["gemini-flash-latest"])),
        )]);
        let s = resolve_on("[llm]\nenabled = true\n", &env, &aliased);
        assert_eq!(model(&s, Use::GeneralAgent), "gemini-flash-latest");
        assert_eq!(how(&s, Use::GeneralAgent), gemini::How::Alias);
        assert_eq!(model(&s, Use::CardSummaries), "gemini-3.5-flash-lite");
    }

    /// Item 4: one line per use, the choice, the model and how.
    #[test]
    fn the_start_log_names_the_choice_the_model_and_how() {
        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let s = resolve_on(
            "[llm]\nenabled = true\n[llm.uses.card_summaries]\nmodel = \"gemini-9-flash\"\n",
            &env,
            &listed(&[]),
        );
        assert_eq!(
            s.describe(),
            [
                "[llm] general_agent: flash:newest is gemini-3.8-flash (matched)",
                "[llm] card_summaries: gemini-9-flash is gemini-9-flash \
                 (named, not in the key's model list)",
                "[llm] spoken_summaries: flash-lite:oldest is gemini-3.5-flash-lite (matched)",
                "[llm] planning: unavailable: [llm.uses.planning] is off unless enabled = true",
            ]
        );
        let s = resolve_on(
            "[llm]\nenabled = true\n[llm.uses.planning]\nenabled = true\n",
            &env,
            &listed(&[]),
        );
        assert_eq!(
            s.describe()[3],
            "[llm] planning: flash:newest is gemini-3.8-flash (matched)"
        );
    }

    /// Item 3: without a list (the fetch failed, or none was made), a rule
    /// is libinfer-gemini's default model and a name is used as given.
    #[test]
    fn without_a_model_list_rules_fall_back_and_names_stand() {
        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let text = "[llm]\nenabled = true\n[llm.uses.general_agent]\nmodel = \"gemini-2.5-pro\"\n";
        for s in [
            resolve(&parse(text), &env),
            resolve_on(text, &env, &gemini::Catalogs::none()),
            // An empty list counts as none.
            resolve_on(
                text,
                &env,
                &gemini::Catalogs::of([("gemini", gemini::ModelCatalog::from_names([""; 0]))]),
            ),
        ] {
            assert_eq!(model(&s, Use::GeneralAgent), "gemini-2.5-pro");
            assert_eq!(how(&s, Use::GeneralAgent), gemini::How::Named);
            for u in [Use::CardSummaries, Use::SpokenSummaries] {
                assert_eq!(model(&s, u), libinfer_gemini::DEFAULT_MODEL);
                assert_eq!(how(&s, u), gemini::How::Fallback);
            }
            assert!(
                s.describe()[2].ends_with("(fallback)"),
                "{:?}",
                s.describe()
            );
        }
    }

    /// Item 5: a choice is resolved once per backend; the same settings
    /// come out of every later resolution.
    #[test]
    fn a_choice_is_resolved_once_and_kept() {
        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let catalogs = listed(&[]);
        let a = resolve_on("[llm]\nenabled = true\n", &env, &catalogs);
        let b = resolve_on("[llm]\nenabled = true\n", &env, &catalogs);
        assert_eq!(a, b);
        assert_eq!(catalogs.backends(), ["cheap", "gemini"]);
    }

    /// Item 2: `model` in either form, per backend and per use; a `pro:`
    /// rule or a Pro name only where it is written.
    #[test]
    fn models_are_overridden_per_backend_and_per_use() {
        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let catalogs = listed(&["gemini-2.5-flash-lite", "gemini-3.0-flash-lite"]);
        // A rule on the backend, a name on a use.
        let s = resolve_on(
            r#"
[llm]
enabled = true
[llm.backends.gemini]
kind = "gemini"
model = "flash:newest"
[llm.uses.general_agent]
model = "gemini-3.8-pro"
"#,
            &env,
            &catalogs,
        );
        assert_eq!(model(&s, Use::GeneralAgent), "gemini-3.8-pro");
        assert_eq!(how(&s, Use::GeneralAgent), gemini::How::Named);
        assert_eq!(model(&s, Use::CardSummaries), "gemini-3.8-flash");
        assert_eq!(model(&s, Use::SpokenSummaries), "gemini-3.8-flash");
        assert_eq!(how(&s, Use::SpokenSummaries), gemini::How::Matched);

        // A name on the backend, a rule on a use; a `pro:` rule is Pro.
        let s = resolve_on(
            r#"
[llm]
enabled = true
[llm.backends.gemini]
kind = "gemini"
model = "gemini-2.5-flash"
[llm.uses.general_agent]
model = " Pro:Newest "
[llm.uses.card_summaries]
model = "gemini-flash-lite:oldest"
"#,
            &env,
            &catalogs,
        );
        assert_eq!(model(&s, Use::GeneralAgent), "gemini-pro-latest");
        assert_eq!(how(&s, Use::GeneralAgent), gemini::How::Alias);
        assert_eq!(
            s.models[&Use::GeneralAgent].choice.to_string(),
            "pro:newest"
        );
        assert_eq!(model(&s, Use::CardSummaries), "gemini-3.5-flash-lite");
        assert_eq!(model(&s, Use::SpokenSummaries), "gemini-2.5-flash");
        assert_eq!(how(&s, Use::SpokenSummaries), gemini::How::Named);

        // Two backends of one kind, each resolved on its own list.
        write_key(&dir.path().join("other_key"), "k2");
        let s = resolve_on(
            r#"
[llm]
enabled = true
default = "main"
[llm.backends.main]
kind = "gemini"
[llm.backends.cheap]
kind = "gemini"
api_key_file = "other_key"
[llm.uses.card_summaries]
backend = "cheap"
[llm.uses.spoken_summaries]
enabled = false
"#,
            &env,
            &catalogs,
        );
        let general = s.target(Use::GeneralAgent).unwrap();
        assert_eq!(general.backend, "main");
        // `main` has no list: offline.
        assert_eq!(general.model, libinfer_gemini::DEFAULT_MODEL);
        let cards = s.target(Use::CardSummaries).unwrap();
        assert_eq!(cards.backend, "cheap");
        assert_eq!(cards.key.expose(), "k2");
        assert_eq!(cards.model, "gemini-3.0-flash-lite");
        assert!(matches!(
            s.state(Use::SpokenSummaries),
            UseState::Unavailable {
                turned_off: true,
                ..
            }
        ));
        assert!(!s.models.contains_key(&Use::SpokenSummaries));
        // Summaries are available while one of the two is.
        assert!(s.availability().1);
    }

    /// Item 3: the backends whose list is fetched: each `gemini` one with a
    /// key, and none while the switch is off.
    #[test]
    fn the_lists_fetched_are_the_keyed_backends() {
        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let keys = |text: &str| -> Vec<(String, String)> {
            catalog_keys(&LlmSources::of(&parse(text)), &env)
                .into_iter()
                .map(|(name, key)| (name, key.expose().to_string()))
                .collect()
        };
        assert!(keys("").is_empty());
        assert!(keys("[llm]\nenabled = false\n").is_empty());
        assert_eq!(
            keys("[llm]\nenabled = true\n"),
            [("gemini".to_string(), "k".to_string())]
        );
        assert_eq!(
            keys("[gemini]\n"),
            [("gemini".to_string(), "k".to_string())],
            "the deprecated section too"
        );
        assert_eq!(
            keys(
                "[llm]\nenabled = true\n[llm.backends.a]\nkind = \"gemini\"\n\
                 [llm.backends.b]\nkind = \"gemini\"\napi_key_file = \"missing\"\n\
                 [llm.backends.c]\nkind = \"llama\"\n"
            ),
            // b's missing file falls through to the config folder's key
            // (plan 20261102_02, decision 1).
            [
                ("a".to_string(), "k".to_string()),
                ("b".to_string(), "k".to_string())
            ]
        );
    }

    #[test]
    fn an_unknown_kind_or_a_missing_backend_affects_only_its_uses() {
        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let s = resolve(
            &parse(
                r#"
[llm]
enabled = true
default = "gemini"
[llm.backends.gemini]
kind = "gemini"
[llm.backends.local]
kind = "llama"
[llm.uses.card_summaries]
backend = "local"
[llm.uses.spoken_summaries]
backend = "nowhere"
"#,
            ),
            &env,
        );
        assert!(s.target(Use::GeneralAgent).is_some());
        let cards = s.state(Use::CardSummaries).reason().unwrap();
        assert!(cards.contains("unknown kind `llama`"), "{cards}");
        let spoken = s.state(Use::SpokenSummaries).reason().unwrap();
        assert!(spoken.contains("no backend: `nowhere`"), "{spoken}");
        assert_eq!(s.problems.len(), 2, "{:?}", s.problems);
        let (general, summaries, reason) = s.availability();
        assert!(general && !summaries);
        assert!(reason.unwrap().contains("llama"));

        // A default naming no backend: only the uses that take it.
        let s = resolve(
            &parse(
                r#"
[llm]
enabled = true
default = "gone"
[llm.backends.gemini]
kind = "gemini"
[llm.uses.general_agent]
backend = "gemini"
"#,
            ),
            &env,
        );
        assert!(s.target(Use::GeneralAgent).is_some());
        assert!(
            s.state(Use::CardSummaries)
                .reason()
                .unwrap()
                .contains("default names no backend")
        );
        // A backend with no kind, or no key, is an error for it alone.
        let empty = tempfile::tempdir().unwrap();
        let s = resolve(
            &parse(
                r#"
[llm]
enabled = true
default = "a"
[llm.backends.a]
api_key_file = "x"
[llm.backends.b]
kind = "gemini"
api_key_file = "missing"
[llm.uses.general_agent]
backend = "b"
"#,
            ),
            &KeyEnv::in_dir(empty.path()),
        );
        assert!(
            s.state(Use::CardSummaries)
                .reason()
                .unwrap()
                .contains("has no kind")
        );
        assert!(
            s.state(Use::GeneralAgent)
                .reason()
                .unwrap()
                .contains("[llm.backends.b] has no key")
        );
    }

    #[test]
    fn old_gemini_and_summary_sections_read_as_before_each_noted_once() {
        let dir = with_key("k");
        write_key(&dir.path().join("my_key"), "mine");
        let env = KeyEnv::in_dir(dir.path());
        let s = resolve(
            &parse("[gemini]\nmodel = \"gemini-3.8-flash\"\napi_key_file = \"my_key\"\n"),
            &env,
        );
        assert!(s.enabled, "[gemini] turns the switch on");
        for u in Use::ALL {
            let t = s.target(u).unwrap();
            assert_eq!(t.model, "gemini-3.8-flash", "{u:?}");
            assert_eq!(t.key.expose(), "mine");
        }
        assert_eq!(s.notes.len(), 1, "{:?}", s.notes);
        assert!(s.notes[0].contains("[gemini] is deprecated"));

        // An empty [gemini] is the discovered backend, with tier models.
        let s = resolve(&parse("[gemini]\n[summary]\ncards = false\n"), &env);
        assert_eq!(model(&s, Use::GeneralAgent), libinfer_gemini::DEFAULT_MODEL);
        assert!(matches!(
            s.state(Use::CardSummaries),
            UseState::Unavailable {
                turned_off: true,
                ..
            }
        ));
        assert!(s.target(Use::SpokenSummaries).is_some());
        assert_eq!(s.notes.len(), 2, "one each: {:?}", s.notes);
        assert!(s.notes[1].contains("[summary] is deprecated"));

        // [summary] alone changes nothing while the switch is off.
        let s = resolve(&parse("[summary]\ncards = true\n"), &env);
        assert!(!s.enabled && s.notes.len() == 1);

        // Both old and new: the new one, and the conflict noted.
        let s = resolve(
            &parse("[gemini]\nmodel = \"old\"\n[summary]\ncards = false\n[llm]\nenabled = true\n"),
            &env,
        );
        assert_eq!(model(&s, Use::GeneralAgent), libinfer_gemini::DEFAULT_MODEL);
        assert!(s.target(Use::CardSummaries).is_some(), "[summary] ignored");
        assert_eq!(s.notes.len(), 2);
        assert!(
            s.notes.iter().all(|n| n.contains("ignored")),
            "{:?}",
            s.notes
        );

        // An unknown use is noted.
        let s = resolve(&parse("[llm.uses.poetry]\nenabled = true\n"), &env);
        assert!(s.notes.iter().any(|n| n.contains("[llm.uses.poetry]")));
    }

    /// Plan 20261027_05, decision 6: `[llm.uses.*]` without `[llm]
    /// enabled` leaves `[gemini]` read; `enabled = false` turns it off.
    #[test]
    fn old_gemini_is_read_until_llm_enabled_is_set() {
        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let text = "[gemini]\nmodel = \"gemini-3.8-flash\"\n\
                    [llm.uses.planning]\nenabled = true\nread = [\"*.md\"]\n";
        let s = resolve(&parse(text), &env);
        assert!(s.enabled, "[gemini] still turns the switch on");
        for u in Use::ALL {
            let t = s.target(u).unwrap();
            assert_eq!((t.backend.as_str(), t.kind), ("gemini", Kind::Gemini));
            assert_eq!(t.model, "gemini-3.8-flash", "{u:?}");
        }
        assert_eq!(s.target(Use::Planning), s.target(Use::GeneralAgent));
        assert_eq!(s.planning.read.len(), 1);
        assert_eq!(
            s.notes
                .iter()
                .filter(|n| n.contains("[llm] has no enabled key"))
                .count(),
            1,
            "{:?}",
            s.notes
        );
        assert!(
            s.notes
                .iter()
                .any(|n| n.contains("[gemini] is deprecated:"))
        );

        // [llm.backends.*] alone doesn't count as set either.
        let s = resolve(
            &parse("[gemini]\n[llm.backends.gemini]\nkind = \"gemini\"\nmodel = \"m\"\n"),
            &env,
        );
        assert!(s.enabled);
        assert_eq!(
            model(&s, Use::GeneralAgent),
            "m",
            "the declared backend wins"
        );

        // enabled = false: everything off, [gemini] ignored.
        let s = resolve(&parse(&format!("[llm]\nenabled = false\n{text}")), &env);
        assert!(!s.enabled);
        for u in Use::ALL {
            assert!(s.target(u).is_none(), "{u:?}");
        }
        assert!(s.target(Use::Planning).is_none());
        assert!(
            s.notes
                .iter()
                .any(|n| n.contains("[gemini] is deprecated and ignored"))
        );
    }

    #[test]
    fn planning_is_off_without_its_table_and_on_with_enabled() {
        let dir = with_key("k");
        let env = KeyEnv::in_dir(dir.path());
        let planning_off = |s: &LlmSettings| {
            matches!(
                s.state(Use::Planning),
                UseState::Unavailable {
                    turned_off: true,
                    ..
                }
            )
        };
        // No table, or one without enabled = true: off, even with [llm] on.
        for text in [
            "[llm]\nenabled = true\n",
            "[llm]\nenabled = true\n[llm.uses.planning]\nagents_prompt = true\n",
            "[llm]\nenabled = true\n[llm.uses.planning]\nenabled = false\n",
        ] {
            let s = resolve(&parse(text), &env);
            assert!(planning_off(&s), "{text:?}");
            assert!(s.target(Use::GeneralAgent).is_some());
        }
        // On: the general agent's backend and model; backend and model
        // keys noted as ignored.
        let s = resolve(
            &parse(
                r#"
[llm]
enabled = true
[llm.uses.general_agent]
model = "gemini-3.8-pro"
[llm.uses.planning]
enabled = true
model = "other"
agents_prompt = true
read = ["*.md", "docs/**/*.md", "a?"]
"#,
            ),
            &env,
        );
        assert_eq!(s.target(Use::Planning), s.target(Use::GeneralAgent));
        assert_eq!(model(&s, Use::Planning), "gemini-3.8-pro");
        assert!(
            s.notes
                .iter()
                .any(|n| n.contains("planning] model is ignored"))
        );
        assert!(s.planning.agents_prompt);
        assert_eq!(s.planning.read.len(), 2);
        assert_eq!(s.problems.len(), 1, "the refused pattern: {:?}", s.problems);
        assert!(
            s.describe()
                .iter()
                .any(|l| l.starts_with("[llm] planning: gemini-3.8-pro is gemini-3.8-pro"))
        );
        // The availability the phone is told is the three uses'.
        assert_eq!(s.availability(), (true, true, None));

        // Off while general_agent is off, with that reason.
        let s = resolve(
            &parse(
                "[llm]\nenabled = true\n[llm.uses.general_agent]\nenabled = false\n\
                 [llm.uses.planning]\nenabled = true\n",
            ),
            &env,
        );
        let reason = s.state(Use::Planning).reason().unwrap();
        assert!(reason.contains("needs general_agent"), "{reason}");
        // And while the master switch is off.
        let s = resolve(&parse("[llm.uses.planning]\nenabled = true\n"), &env);
        assert!(s.target(Use::Planning).is_none());
        assert!(!planning_off(&s));
        assert!(Use::from_name("planning") == Some(Use::Planning));
    }

    #[test]
    fn a_key_file_must_be_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = with_key("k");
        let path = dir.path().join(gemini::KEY_FILE);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let s = resolve(
            &parse("[llm]\nenabled = true\n"),
            &KeyEnv::in_dir(dir.path()),
        );
        let reason = s.state(Use::GeneralAgent).reason().unwrap();
        assert!(reason.contains("chmod 600"), "{reason}");
    }

    /// The Keychain stand-in (decision 8): what it holds, or locked.
    struct FakeKeychain(Lookup);

    impl KeySource for FakeKeychain {
        fn name(&self) -> String {
            keys::keychain_name()
        }
        fn get(&self) -> Lookup {
            self.0.clone()
        }
        fn set(&self, _: &ApiKey) -> Result<(), String> {
            Err("read-only".to_string())
        }
    }

    fn key_and_line(text: &str, env: &KeyEnv) -> (Option<String>, Vec<String>, Vec<String>) {
        let s = resolve(&parse(text), env);
        let key = s
            .target(Use::GeneralAgent)
            .map(|t| t.key.expose().to_string());
        (key, s.key_lines(), s.problems.clone())
    }

    /// Decision 1: api_key_file, the Keychain, the config folder's file,
    /// then GEMINI_API_KEY; the first found wins, and the start log names
    /// it (decision 7).
    #[test]
    fn the_sources_are_read_in_order_and_the_log_names_the_one_used() {
        let dir = with_key("test-key-file");
        write_key(&dir.path().join("own_key"), "test-key-own");
        let full = |keychain: Lookup, env_key: bool| KeyEnv {
            keychain: Some(Arc::new(FakeKeychain(keychain))),
            gemini_api_key: env_key.then(|| ApiKey::new("test-key-env")),
            ..KeyEnv::in_dir(dir.path())
        };
        let found = || Lookup::Found(ApiKey::new("test-key-chain"));
        let named = "[llm]\nenabled = true\n[llm.backends.gemini]\nkind = \"gemini\"\n\
                     api_key_file = \"own_key\"\n";
        let plain = "[llm]\nenabled = true\n";
        let own = dir.path().join("own_key");
        let (key, lines, _) = key_and_line(named, &full(found(), true));
        assert_eq!(key.as_deref(), Some("test-key-own"));
        assert_eq!(
            lines,
            [format!("[llm] gemini key: api_key_file {}", own.display())]
        );

        let (key, lines, _) = key_and_line(plain, &full(found(), true));
        assert_eq!(key.as_deref(), Some("test-key-chain"));
        assert_eq!(lines, ["[llm] gemini key: Keychain (dev.flankr/gemini)"]);

        // A locked Keychain is passed over.
        let secrets = dir.path().join(gemini::KEY_FILE);
        for keychain in [Lookup::None, Lookup::Locked] {
            let (key, lines, _) = key_and_line(plain, &full(keychain, true));
            assert_eq!(key.as_deref(), Some("test-key-file"));
            assert_eq!(
                lines,
                [format!("[llm] gemini key: file {}", secrets.display())]
            );
        }

        std::fs::remove_file(&secrets).unwrap();
        let (key, lines, _) = key_and_line(plain, &full(Lookup::Locked, true));
        assert_eq!(key.as_deref(), Some("test-key-env"));
        assert_eq!(lines, ["[llm] gemini key: GEMINI_API_KEY"]);

        // None: the places looked in, the locked one marked, and the fix.
        let (key, lines, problems) = key_and_line(plain, &full(Lookup::Locked, false));
        assert!(key.is_none() && lines.is_empty());
        assert_eq!(
            problems,
            [format!(
                "no Gemini key: looked in Keychain (dev.flankr/gemini) (locked), file {}, \
                 GEMINI_API_KEY; run `flankr key set gemini`",
                secrets.display()
            )]
        );
    }

    /// Decision 1.1: a relative api_key_file is read against its config
    /// file's folder, never the working folder.
    #[test]
    fn a_relative_key_file_is_next_to_its_config_file() {
        let config = tempfile::tempdir().unwrap();
        let cwd = std::env::current_dir().unwrap();
        assert_ne!(cwd, config.path());
        write_key(&config.path().join("k"), "test-key-here");
        let env = KeyEnv {
            secrets_file: None,
            ..KeyEnv::from_process(&config.path().join("config.toml"))
        };
        assert_eq!(env.config_dir, config.path());
        let (key, _, _) = key_and_line(
            "[llm]\nenabled = true\n[llm.backends.gemini]\nkind = \"gemini\"\napi_key_file = \"k\"\n",
            &KeyEnv {
                gemini_api_key: None,
                ..env
            },
        );
        assert_eq!(key.as_deref(), Some("test-key-here"));
        // The working folder's `.private` is no longer among the files.
        assert!(
            key_files(&LlmSources::default(), &KeyEnv::in_dir(config.path()))
                .iter()
                .all(|p| !p.to_string_lossy().contains(".private"))
        );
    }

    /// Decision 2: a key file group or others may read is refused, not
    /// read, with the path and the fix; no key text in any line.
    #[test]
    fn a_key_file_others_may_read_is_refused_and_no_line_has_the_key() {
        use std::os::unix::fs::PermissionsExt;
        let dir = with_key("test-key-secret");
        let path = dir.path().join(gemini::KEY_FILE);
        for mode in [0o640, 0o604] {
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            let env = KeyEnv {
                gemini_api_key: Some(ApiKey::new("test-key-env")),
                ..KeyEnv::in_dir(dir.path())
            };
            let s = resolve(&parse("[llm]\nenabled = true\n"), &env);
            assert!(
                s.target(Use::GeneralAgent).is_none(),
                "not read, not passed over"
            );
            let reason = s.state(Use::GeneralAgent).reason().unwrap();
            assert!(reason.contains(&path.display().to_string()), "{reason}");
            assert!(reason.contains("chmod 600"), "{reason}");
            let all: Vec<String> = s
                .notes
                .iter()
                .chain(&s.problems)
                .cloned()
                .chain(s.describe())
                .chain(s.key_lines())
                .chain([format!("{s:?}"), format!("{env:?}")])
                .collect();
            assert!(all.iter().all(|l| !l.contains("test-key")), "{all:?}");
        }
        // Text that is not a key is refused without being repeated.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::write(&path, "test-key not one token").unwrap();
        let err = read_key_file(&path).unwrap_err();
        assert!(!err.contains("test-key"), "{err}");
    }

    #[test]
    fn the_config_folder_follows_xdg_config_home_else_home() {
        // Read-only: the paths these give for the process's own variables.
        let dir = keys::flankr_config_dir();
        match std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            Some(xdg) if Path::new(&xdg).is_absolute() => {
                assert_eq!(dir, Some(PathBuf::from(xdg).join("flankr")))
            }
            _ => {
                if let Some(home) = std::env::var_os("HOME") {
                    assert_eq!(dir, Some(PathBuf::from(home).join(".config/flankr")));
                }
            }
        }
        if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
            let path = PathBuf::from(home)
                .join(".config/flankr")
                .join(gemini::KEY_FILE);
            assert_eq!(
                keys::tilde(&path),
                "~/.config/flankr/secrets/gemini_api_key"
            );
        }
    }
}
