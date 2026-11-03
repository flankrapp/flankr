//! `flankr serve` (the default) starts the daemon, `flankr server`, in the
//! background and returns; `flankr ctl` sends it one request
//! (docs/design/20261022_01_ORCHESTRATION_DAEMON_PLAN.md, decision 1;
//! docs/design/20261102_01_BACKGROUND_DAEMON_PLAN.md).
//!
//! ```text
//! flankr version | --version | -V
//! flankr [serve [--foreground | --stop] | server]
//!        [--config <file>] [--transport <name> | --remote <user@host> |
//!         --socket <herdr.sock> | --mock] [--listen <flankr.sock>] [--show-tokens]
//!        [--prompts <ADDITIONAL_PROMPTS.md>] [--suggestions <SUGGESTIONS.md>]
//! flankr ctl [--listen <flankr.sock>] [--client <name>] [--follow] [--handle <text>]
//!            <method> [<params as JSON>]
//! flankr pair [--foreground] [--listen <flankr.sock>] [--name <name>] [--protocols companion,harness]
//!             [--replace] [--code <code>] [--qr-png <file>] [--timeout <secs>]
//! flankr session tail [--listen <flankr.sock>] [--pane <id or name>] [--follow]
//!                     [--entries N] [--history N] [--outline] [--from N] [--to N]
//! flankr prompt [--use general_agent] [--agent <name>] [--prompts <ADDITIONAL_PROMPTS.md>]
//!               [--config <file>] [--transport <name> | --remote <user@host> |
//!                --socket <herdr.sock> | --mock]
//! flankr key set gemini [--file] | import gemini <file> [--file] | status [--config <file>]
//! ```
//!
//! herdr's API socket is `--socket`, else `HERDR_SOCKET_PATH` (herdr's own
//! override) unless `--transport` names a transport, else `config.toml`'s
//! transport, else `~/.config/herdr/herdr.sock`.

use std::path::{Path, PathBuf};

use flankr::transport::{HerdrClient, HerdrTransport, RemoteSshConfig};

struct AppArgs {
    config: Option<String>,
    transport_name: Option<String>,
    remote: Option<String>,
    socket: Option<String>,
    mock_mode: bool,
    /// The daemon socket, for `serve` and `ctl`.
    listen: Option<String>,
    show_tokens: bool,
    /// `serve`'s prompts file, in place of the shipped
    /// `ADDITIONAL_PROMPTS.md` (plan 20261023_05, decision 7).
    prompts: Option<String>,
    /// `serve`'s suggestion rules, in place of the shipped
    /// `SUGGESTIONS.md` (plan 20261027_06, decision 1).
    suggestions: Option<String>,
    follow: bool,
    /// `ctl`'s client label.
    client: Option<String>,
    /// `ctl`'s `handle` param (`confirm.status --handle "…"`).
    handle: Option<String>,
    /// `ctl`'s method and params, in order.
    positional: Vec<String>,
    /// `pair`'s options.
    name: Option<String>,
    protocols: Vec<String>,
    replace: bool,
    /// The device code, instead of prompting for it.
    code: Option<String>,
    qr_png: Option<String>,
    /// `pair`'s offer lifetime in seconds, 1 to 300.
    timeout: Option<String>,
    /// `session tail`'s pane, backfill size and history to page back.
    pane: Option<String>,
    entries: Option<String>,
    history: Option<String>,
    /// `session tail`'s outline mode and content range.
    outline: bool,
    from: Option<String>,
    to: Option<String>,
    /// `prompt`'s use and agent (plan 20261027_04, decision 8).
    use_name: Option<String>,
    agent: Option<String>,
    /// `serve --foreground`: run the server here, as `server` does
    /// (plan 20261102_01, decision 1).
    foreground: bool,
    /// `serve --stop`: stop the background server.
    stop: bool,
    /// Arguments `parse_args` does not know: an error (the 2026-11-02
    /// 22:33 EST amendment, item 2).
    unknown: Vec<String>,
}

fn parse_args(args: impl Iterator<Item = String>) -> AppArgs {
    let mut args = args;
    let mut parsed = AppArgs {
        config: None,
        transport_name: None,
        remote: None,
        socket: None,
        mock_mode: false,
        listen: None,
        show_tokens: false,
        prompts: None,
        suggestions: None,
        follow: false,
        client: None,
        handle: None,
        positional: Vec::new(),
        name: None,
        protocols: Vec::new(),
        replace: false,
        code: None,
        qr_png: None,
        timeout: None,
        pane: None,
        entries: None,
        history: None,
        outline: false,
        from: None,
        to: None,
        use_name: None,
        agent: None,
        foreground: false,
        stop: false,
        unknown: Vec::new(),
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" | "-c" => parsed.config = args.next(),
            "--transport" | "-t" => parsed.transport_name = args.next(),
            "--remote" => parsed.remote = args.next(),
            "--socket" => parsed.socket = args.next(),
            "--mock" => parsed.mock_mode = true,
            "--listen" => parsed.listen = args.next(),
            "--show-tokens" => parsed.show_tokens = true,
            "--prompts" => parsed.prompts = args.next(),
            "--suggestions" => parsed.suggestions = args.next(),
            "--follow" => parsed.follow = true,
            "--client" => parsed.client = args.next(),
            "--handle" => parsed.handle = args.next(),
            "--name" => parsed.name = args.next(),
            "--protocols" => {
                if let Some(list) = args.next() {
                    parsed.protocols.extend(
                        list.split(',')
                            .map(str::trim)
                            .filter(|p| !p.is_empty())
                            .map(str::to_string),
                    );
                }
            }
            "--code" => parsed.code = args.next(),
            "--replace" => parsed.replace = true,
            "--qr-png" => parsed.qr_png = args.next(),
            "--timeout" => parsed.timeout = args.next(),
            "--pane" => parsed.pane = args.next(),
            "--entries" => parsed.entries = args.next(),
            "--history" => parsed.history = args.next(),
            "--outline" => parsed.outline = true,
            "--from" => parsed.from = args.next(),
            "--to" => parsed.to = args.next(),
            "--use" => parsed.use_name = args.next(),
            "--agent" => parsed.agent = args.next(),
            "--foreground" => parsed.foreground = true,
            "--stop" => parsed.stop = true,
            _ if !arg.starts_with('-') => parsed.positional.push(arg),
            // An unknown argument is an error, never a reason to go on
            // (plan 20261102_01, the 2026-11-02 22:33 EST amendment, item 2).
            _ => parsed.unknown.push(arg),
        }
    }

    parsed
}

fn expand_tilde(p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    if let Ok(stripped) = p.strip_prefix("~")
        && let Ok(home) = std::env::var("HOME")
    {
        return PathBuf::from(home).join(stripped);
    }
    p.to_path_buf()
}

/// herdr's own override of its API socket, honoured as herdr's CLI does.
const HERDR_SOCKET_ENV: &str = "HERDR_SOCKET_PATH";

/// The herdr transport named by the flags, else by `HERDR_SOCKET_PATH`
/// (unless `--transport` names one), else by `config.toml`, else the
/// default local socket.
fn select_transport(args: &AppArgs, config: &flankr::config::AppConfig) -> HerdrTransport {
    if args.mock_mode {
        log::info!("Using Mock Transport");
        return HerdrTransport::mock();
    }
    if let Some(ref ssh_target) = args.remote {
        log::info!("Connecting via SSH to: {}", ssh_target);
        return HerdrTransport::remote_ssh_with_config(RemoteSshConfig::new(ssh_target.clone()));
    }
    if let Some(ref socket_str) = args.socket {
        let socket_path = expand_tilde(socket_str);
        log::info!("Connecting via UNIX domain socket: {:?}", socket_path);
        return HerdrTransport::local(socket_path);
    }
    if args.transport_name.is_none()
        && let Some(socket_str) = std::env::var(HERDR_SOCKET_ENV)
            .ok()
            .filter(|s| !s.is_empty())
    {
        let socket_path = expand_tilde(&socket_str);
        log::info!(
            "Connecting via UNIX domain socket (from {}): {:?}",
            HERDR_SOCKET_ENV,
            socket_path
        );
        return HerdrTransport::local(socket_path);
    }
    let Some(t) = config.find_transport(args.transport_name.as_deref()) else {
        let socket_path = flankr::transport::default_local_socket_path();
        log::info!(
            "Connecting via default UNIX domain socket: {:?}",
            socket_path
        );
        return HerdrTransport::local(socket_path);
    };
    let transport_name = t.name.as_deref().unwrap_or("unnamed");
    if t.mode.eq_ignore_ascii_case("remote") {
        if let Some(ref remote_target) = t.remote {
            log::info!(
                "Connecting via SSH (from config transport '{}') to: {}",
                transport_name,
                remote_target
            );
            return HerdrTransport::remote_ssh_with_config(RemoteSshConfig::new(
                remote_target.clone(),
            ));
        }
        let socket_path = flankr::transport::default_local_socket_path();
        log::warn!(
            "Config transport '{}' mode is 'remote' but 'remote' string is missing; falling back to local socket: {:?}",
            transport_name,
            socket_path
        );
        return HerdrTransport::local(socket_path);
    }
    let socket_path = t
        .socket
        .as_ref()
        .map(expand_tilde)
        .unwrap_or_else(flankr::transport::default_local_socket_path);
    log::info!(
        "Connecting via UNIX domain socket (from config transport '{}'): {:?}",
        transport_name,
        socket_path
    );
    HerdrTransport::local(socket_path)
}

/// The daemon socket: `--listen`, else `[server] socket`, else the default.
fn daemon_socket(args: &AppArgs, config: &flankr::config::AppConfig) -> PathBuf {
    args.listen
        .as_ref()
        .or(config.server.socket.as_ref())
        .map(expand_tilde)
        .unwrap_or_else(flankr::server::default_socket_path)
}

async fn serve(args: AppArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config = flankr::config::AppConfig::load_or_default(args.config.as_deref().map(Path::new));
    let transport = select_transport(&args, &config);
    let event_transport = (!args.mock_mode).then(|| transport.clone());
    let client = HerdrClient::new(transport);
    match client.list_panes().await {
        Ok(snapshot) => log::info!(
            "herdr reports {} pane(s)",
            flankr::core::agents::agents(&snapshot).len()
        ),
        Err(e) => log::warn!("herdr is not answering yet: {e}"),
    }

    // The link: its key is checked (and created) before anything listens.
    let settings = flankr::server::link::LinkSettings::from_config(&config.link);
    let link = if settings.enabled() {
        Some(flankr::server::link::Link::open(settings)?)
    } else {
        log::info!("The link is off ([link] listen = [])");
        None
    };

    // Each `gemini` backend's model list, fetched once before any use is
    // made (plan 20261027_02, the 2026-10-30 13:54 EDT amendment, item 3).
    // Keys: the config file's folder, the Keychain, flankr's config folder,
    // then GEMINI_API_KEY (plan 20261102_02, decision 1).
    let config_path = config_path(&args);
    let key_env = flankr::llm::KeyEnv::from_process(&config_path).with_keychain();
    let catalogs = std::sync::Arc::new(flankr::llm::fetch_catalogs(&config, &key_env).await);

    let socket = daemon_socket(&args, &config);
    let listener = flankr::server::bind(&socket).await?;
    // The pid file, once the socket is bound (plan 20261102_01, decision 4).
    let pid_file = match daemon_files(&socket) {
        Some(files) => match flankr::daemon::PidFile::create(&files.pid) {
            Ok(pid_file) => Some(pid_file),
            Err(e) => {
                log::warn!("Cannot write {}: {e}", files.pid.display());
                None
            }
        },
        None => None,
    };
    let link_listeners = match &link {
        Some(link) => link.bind().await?,
        None => Vec::new(),
    };
    log::info!("flankr serve listening on {}", socket.display());
    let mut options = flankr::server::ServeOptions {
        event_transport,
        ..Default::default()
    };
    options.show_tokens |= args.show_tokens;
    let turns = flankr::server::companion::TurnOptions {
        speech: flankr::server::speech::SpeechOptions {
            prompts_file: args.prompts.as_ref().map(expand_tilde),
            ..Default::default()
        },
        // `[llm]` is read again from this file while the daemon runs
        // (plan 20261027_02, decision 6).
        llm: flankr::llm::LlmOptions {
            config_path: Some(config_path),
            key_env: Some(key_env),
            // The `planning` prompt comes from the same file
            // (plan 20261027_04, decision 6).
            prompts_file: args.prompts.as_ref().map(expand_tilde),
            catalogs: Some(catalogs),
            ..Default::default()
        },
        // SUGGESTIONS.md: the given file or the shipped one, with the
        // user's merged over it (plan 20261027_06, the 14:13 EDT amendment).
        suggestions_file: args.suggestions.as_ref().map(expand_tilde),
        suggestions_user_file: std::env::var_os("HOME")
            .map(|home| flankr::suggestions::user_file_under(Path::new(&home))),
        ..Default::default()
    };
    let daemon = flankr::server::Daemon::with_turns(client, &config, link, turns);
    flankr::server::link::spawn(std::sync::Arc::clone(&daemon), link_listeners);
    let signal = tokio::select! {
        () = flankr::server::run(daemon, listener, options) => None,
        signal = flankr::daemon::termination() => Some(signal),
    };
    // A clean exit removes the pid file; exit at once rather than wait for
    // the runtime's blocking tasks.
    if let Some(signal) = signal {
        log::info!("flankr server stopping on {signal}");
    }
    drop(pid_file);
    std::process::exit(0)
}

/// `--config`, else `~/.config/flankr/config.toml`.
fn config_path(args: &AppArgs) -> PathBuf {
    args.config
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(flankr::config::AppConfig::default_config_path)
}

/// The log and pid files for the listen socket `socket`, under
/// `~/.local/state/flankr` (plan 20261102_01, decision 4).
fn daemon_files(socket: &Path) -> Option<flankr::daemon::DaemonFiles> {
    let home = std::env::var_os("HOME")?;
    Some(flankr::daemon::DaemonFiles::for_socket(
        &flankr::daemon::state_dir(Path::new(&home)),
        socket,
        &flankr::server::default_socket_path(),
    ))
}

/// `flankr serve`: the server in the background, or `--stop`
/// (plan 20261102_01, decision 1). `rest` is the command line after the
/// command word, passed on to `flankr server`.
async fn serve_background(
    args: &AppArgs,
    rest: Vec<String>,
) -> Result<i32, Box<dyn std::error::Error>> {
    let config = flankr::config::AppConfig::load_or_default(args.config.as_deref().map(Path::new));
    let socket = flankr::daemon::absolute(&daemon_socket(args, &config));
    let files = daemon_files(&socket).ok_or("HOME is not set")?;
    if args.stop {
        return Ok(flankr::daemon::stop(&socket, &files).await);
    }
    let working_dir = std::env::current_dir()?;
    // The key in this shell, and none where the background daemon looks
    // (plan 20261102_02, decision 1).
    let key_env = flankr::llm::KeyEnv::from_process(&config_path(args)).with_keychain();
    if key_env.gemini_api_key.is_some() {
        let stored = flankr::llm::KeyEnv {
            gemini_api_key: None,
            ..key_env
        };
        if stored.find(None).is_err() {
            eprintln!(
                "flankr: {} in this shell is not passed to the background daemon; store the key with `flankr key set gemini` (or run `flankr serve --foreground`)",
                flankr::llm::gemini::KEY_ENV,
            );
        }
    }
    let background = flankr::daemon::Background {
        socket,
        files,
        program: std::env::current_exe()?,
        args: std::iter::once("server".to_string())
            .chain(rest)
            .map(Into::into)
            .collect(),
        working_dir,
        env: flankr::daemon::passed_env(std::env::vars_os()),
    };
    Ok(flankr::daemon::serve_background(&background).await)
}

/// The flags a started server takes from another command's: only the
/// server's own, never pairing's (plan 20261102_01, the 2026-11-02 22:26
/// EST amendment, item 2).
fn server_flags(args: &AppArgs) -> Vec<String> {
    let mut flags = Vec::new();
    let mut valued = |flag: &str, value: &Option<String>| {
        if let Some(value) = value {
            flags.push(flag.to_string());
            flags.push(value.clone());
        }
    };
    valued("--config", &args.config);
    valued("--listen", &args.listen);
    valued("--transport", &args.transport_name);
    valued("--remote", &args.remote);
    valued("--socket", &args.socket);
    valued("--prompts", &args.prompts);
    valued("--suggestions", &args.suggestions);
    if args.mock_mode {
        flags.push("--mock".to_string());
    }
    if args.show_tokens {
        flags.push("--show-tokens".to_string());
    }
    flags
}

async fn pair(args: AppArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let config = flankr::config::AppConfig::load_or_default(args.config.as_deref().map(Path::new));
    let socket = daemon_socket(&args, &config);
    let usage = "usage: flankr pair [--foreground] [--listen <socket>] [--name <name>] [--protocols companion,harness] [--replace] [--code <code>] [--qr-png <file>] [--timeout <secs>]";
    if !args.positional.is_empty() || args.stop {
        return Err(usage.into());
    }
    // No daemon answering: start one in the background first, as `serve`
    // would, unless `--foreground` (the 22:26 EST amendment, items 1 and 3).
    if !args.foreground
        && flankr::daemon::answers(&flankr::daemon::absolute(&socket))
            .await
            .is_none()
    {
        let rest = server_flags(&args);
        let code = serve_background(&args, rest).await?;
        if code != 0 {
            return Ok(code);
        }
    }
    let timeout_secs = match args.timeout.as_deref() {
        Some(value) => match flankr::pair::parse_timeout(value) {
            Ok(secs) => Some(secs),
            Err(e) => {
                eprintln!("flankr pair: {e}");
                return Ok(2);
            }
        },
        None => None,
    };
    let options = flankr::pair::PairOptions {
        name: args.name.clone(),
        protocols: (!args.protocols.is_empty()).then(|| args.protocols.clone()),
        replace: args.replace,
        code: args.code.clone(),
        qr_png: args.qr_png.as_deref().map(PathBuf::from),
        timeout_secs,
    };
    Ok(flankr::pair::run(&socket, options).await?)
}

async fn ctl(args: AppArgs) -> Result<i32, Box<dyn std::error::Error>> {
    let config = flankr::config::AppConfig::load_or_default(args.config.as_deref().map(Path::new));
    let socket = daemon_socket(&args, &config);
    let mut positional = args.positional.iter();
    let Some(method) = positional.next() else {
        return Err(
            "usage: flankr ctl [--listen <socket>] [--client <name>] [--follow] [--handle <text>] <method> [<params as JSON>]".into(),
        );
    };
    let mut params: serde_json::Value = match positional.next() {
        Some(json) => {
            serde_json::from_str(json).map_err(|e| format!("params are not JSON: {e}"))?
        }
        None => serde_json::json!({}),
    };
    if let Some(handle) = &args.handle {
        let obj = params
            .as_object_mut()
            .ok_or("--handle needs params that are a JSON object")?;
        obj.insert("handle".to_string(), serde_json::json!(handle));
    }
    Ok(flankr::ctl::run(
        &socket,
        args.client.as_deref().unwrap_or("ctl"),
        method,
        params,
        args.follow,
    )
    .await?)
}

/// `flankr session tail` (plan 20261023_03, section 8): read-only.
async fn session(args: AppArgs) -> Result<i32, Box<dyn std::error::Error>> {
    const USAGE: &str = "usage: flankr session tail [--listen <socket>] [--pane <id or name>] [--follow] [--entries N] [--history N] [--outline] [--from N] [--to N]";
    if args.positional.first().map(String::as_str) != Some("tail") || args.positional.len() > 1 {
        return Err(USAGE.into());
    }
    let number = |v: &Option<String>, default: usize| -> Result<usize, String> {
        v.as_deref().map_or(Ok(default), |n| {
            n.parse().map_err(|_| format!("not a number: {n}"))
        })
    };
    let config = flankr::config::AppConfig::load_or_default(args.config.as_deref().map(Path::new));
    let socket = daemon_socket(&args, &config);
    let options = flankr::tail::TailOptions {
        pane: args.pane.clone(),
        follow: args.follow,
        sections: number(
            &args.entries,
            libflankr_proto::common::SUBSCRIBE_SECTIONS_DEFAULT,
        )?,
        history: number(&args.history, 0)?,
        outline: args.outline,
        content: {
            let index = |v: &Option<String>| -> Result<Option<u64>, String> {
                v.as_deref()
                    .map(|n| n.parse().map_err(|_| format!("not an index: {n}")))
                    .transpose()
            };
            let (from, to) = (index(&args.from)?, index(&args.to)?);
            (from.is_some() || to.is_some()).then_some((from, to))
        },
    };
    let mut out = std::io::stdout();
    Ok(flankr::tail::run(&socket, &options, &mut out).await?)
}

/// `flankr prompt` (plan 20261027_04, decision 8): the general agent's
/// assembled prompt; reads the config and herdr, calls no LLM.
async fn prompt(args: AppArgs) -> Result<i32, Box<dyn std::error::Error>> {
    if !args.positional.is_empty() {
        return Err(PROMPT_USAGE.into());
    }
    let config = flankr::config::AppConfig::load_or_default(args.config.as_deref().map(Path::new));
    let client = HerdrClient::new(select_transport(&args, &config));
    let prompts_file = args.prompts.as_ref().map(expand_tilde);
    let options = flankr::prompt_cmd::PromptOptions {
        use_name: args.use_name.as_deref(),
        agent: args.agent.as_deref(),
        prompts_file: prompts_file.as_deref(),
    };
    let env = flankr::llm::KeyEnv::from_process(&config_path(&args)).with_keychain();
    let out = flankr::prompt_cmd::report(&config, &env, &client, &options).await?;
    print!("{out}");
    Ok(0)
}

const PROMPT_USAGE: &str = "usage: flankr prompt [--use general_agent] [--agent <name>] [--prompts <ADDITIONAL_PROMPTS.md>]";

const USAGE: &str = "\
flankr version | --version | -V
flankr [serve [--foreground | --stop] | server]
       [--config <file>] [--transport <name> | --remote <user@host> |
        --socket <herdr.sock> | --mock] [--listen <flankr.sock>] [--show-tokens]
       [--prompts <ADDITIONAL_PROMPTS.md>] [--suggestions <SUGGESTIONS.md>]
  serve (the default) starts `flankr server` in the background and returns;
  log and pid in ~/.local/state/flankr/. --foreground runs it here, as
  `server` does; --stop stops it.
flankr ctl [--listen <flankr.sock>] [--client <name>] [--follow] [--handle <text>]
           <method> [<params as JSON>]
flankr pair [--foreground] [--listen <flankr.sock>] [--name <name>] [--protocols companion,harness]
            [--replace] [--code <code>] [--qr-png <file>] [--timeout <secs>]
flankr session tail [--listen <flankr.sock>] [--pane <id or name>] [--follow]
                    [--entries N] [--history N] [--outline] [--from N] [--to N]
flankr prompt [--use general_agent] [--agent <name>] [--prompts <ADDITIONAL_PROMPTS.md>]
              [--config <file>] [--transport <name> | --remote <user@host> |
               --socket <herdr.sock> | --mock]
flankr key set gemini [--file]
flankr key import gemini <file> [--file]
flankr key status [--config <file>]
  the Gemini key: in the macOS Keychain (dev.flankr/gemini), or with --file
  in ~/.config/flankr/secrets/gemini_api_key; never printed.
herdr's socket: --socket, else HERDR_SOCKET_PATH, else config.toml, else ~/.config/herdr/herdr.sock";

/// `flankr --version`'s line: the version and the commit built from (the
/// 2026-11-02 22:33 EST amendment, item 1).
fn version_line() -> String {
    format!(
        "flankr {} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("FLANKR_COMMIT")
    )
}

/// `.env` in the working folder, for its other settings (`RUST_LOG`,
/// `HERDR_SOCKET_PATH`, …), never over the environment, and never the
/// Gemini key (plan 20261102_02, decision 1): a `GEMINI_API_KEY` there is
/// skipped with a note.
fn load_dotenv() {
    let Ok(iter) = dotenvy::dotenv_iter() else {
        return;
    };
    for item in iter {
        let Ok((name, value)) = item else {
            continue;
        };
        if name == flankr::llm::gemini::KEY_ENV {
            eprintln!(
                "flankr: {name} in .env is no longer read; store the key with `flankr key set gemini`"
            );
        } else if std::env::var_os(&name).is_none() {
            // SAFETY: called at the start of `main`, before any thread.
            unsafe { std::env::set_var(name, value) };
        }
    }
}

/// `flankr key …` (plan 20261102_02, decisions 3 to 5).
fn key(argv: &[String]) -> i32 {
    if argv
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("{}", flankr::key_cmd::USAGE);
        return 0;
    }
    let parsed = match flankr::key_cmd::parse(argv) {
        Ok(parsed) => parsed,
        Err(e) => {
            eprintln!("flankr key: {e}\n{}", flankr::key_cmd::USAGE);
            return 2;
        }
    };
    let config_path = parsed
        .config
        .unwrap_or_else(flankr::config::AppConfig::default_config_path);
    let config = flankr::config::AppConfig::load_or_default(Some(&config_path));
    let env = flankr::llm::KeyEnv::from_process(&config_path).with_keychain();
    flankr::key_cmd::run(
        argv,
        &env,
        &flankr::llm::LlmSources::of(&config),
        &mut flankr::key_cmd::read_key_line,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map(String::as_str) == Some("version")
        || argv.iter().any(|a| a == "--version" || a == "-V")
    {
        println!("{}", version_line());
        return Ok(());
    }
    if argv.first().map(String::as_str) == Some("key") {
        load_dotenv();
        std::process::exit(key(&argv[1..]));
    }
    let command = match argv.first().map(String::as_str) {
        Some("serve") | Some("server") | Some("ctl") | Some("pair") | Some("session")
        | Some("prompt") => argv.remove(0),
        _ => "serve".to_string(),
    };
    if command == "server" {
        // First thing, before any thread: out of the launchd job that
        // started it, if one did (plan 20261102_01, decision 2).
        if let Err(e) = flankr_daemon::leave_job() {
            eprintln!("flankr server: could not leave the launchd job: {e}");
        }
    }
    if argv
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("{USAGE}");
        return Ok(());
    }
    let args = parse_args(argv.clone().into_iter());
    if let Some(arg) = args.unknown.first() {
        eprintln!("flankr: unknown argument '{arg}'; see flankr --help");
        std::process::exit(2);
    }
    let background = command == "serve" && !args.foreground;
    // The background server loads `.env` itself, from the same folder;
    // `serve` reads only this shell's environment (decision 3).
    if !background {
        load_dotenv();
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(command, args, argv, background))
}

async fn run(
    command: String,
    args: AppArgs,
    argv: Vec<String>,
    background: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if command == "ctl" {
        std::process::exit(ctl(args).await?);
    }
    if command == "pair" {
        std::process::exit(pair(args).await?);
    }
    if command == "session" {
        std::process::exit(session(args).await?);
    }
    if command == "prompt" {
        std::process::exit(prompt(args).await?);
    }
    if !args.positional.is_empty() {
        return Err(format!("unexpected arguments: {:?}", args.positional).into());
    }
    if args.foreground && args.stop {
        return Err("--foreground and --stop do not go together".into());
    }
    if command == "server" && (args.foreground || args.stop) {
        return Err(
            "`flankr server` runs in the foreground; use `flankr serve --stop` to stop it".into(),
        );
    }
    if background {
        std::process::exit(serve_background(&args, argv).await?);
    }
    serve(args).await
}
