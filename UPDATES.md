# flankr updates

What has changed in the flankr program that runs on your computer: the daemon (`flankr serve`) and the commands you type (`flankr pair`, `flankr prompt`, `flankr session tail`, `flankr ctl`). It covers the settings in `config.toml`, what it sends to your agents and what it refuses, and the files it reads and writes. Changes that only affect the phone app are in the app's own log.

From now on, each change you can notice ships with a version of the flankr program. The program is version 0.1.0 today. A change that adds a feature, or changes something you would notice, raises the middle number (0.2.0). A fix or a small polish raises the last number (0.1.1). Entries from then on are headed with the version and the date.

# History before versioning

These changes came before the program had version numbers.

## 2026-08-31

**Added**
- Voice on the computer: flankr listened to the computer's microphone and spoke its replies aloud, with a voice that downloads on first use. `--tts-voice` and `--tts-speed` chose the voice and its speed.
- Transports: `[[transports]]` in `config.toml` names the herdr connections, `--transport` picks one, and `--remote user@host` reaches herdr over SSH.
- Settings can be kept in a `.env` file.
- Each agent is set in an `[[agents]]` table, which replaces the old `[commands]` table. `allowed_commands` lists the commands an agent may be sent, and `require_confirmation` makes flankr ask you aloud first.
- Before a command goes to an agent, you must say yes out loud. A yes applies to that one command only.
- `allow_arbitrary_commands` lets a spoken instruction that is not on the allow list be typed to the agent, limited to plain words and the punctuation `. , ' ? :`.
- Gemini reads the recent output of the panes that are working or in focus, not every pane.
- The instructions Gemini is given are in `PROMPT.md`, next to the program.
- Gemini's key can come from a service account file in `.private` or from the `GEMINI_API_KEY` setting.

## 2026-09-01

**Added**
- You can watch and unwatch a pane by voice.
- Agent notifications are spoken without talking over you.
- Agents are named in spoken announcements only as precisely as needed.

## 2026-10-22

**Added**
- `flankr serve` is a daemon that the phone app talks to in text. The phone does the listening and speaking.
- `flankr pair` prints a pairing link and its QR code for the phone to scan.
- The phone link listens on 127.0.0.1:7347 by default, set by `[link] listen`. Setting `listen = []` turns it off.
- `flankr ctl` sends one request to the daemon and prints the reply.
- A command can be confirmed on the phone as well as by voice.
- `interrupt_key` for each agent sends a key to stop it when you interrupt; it defaults to `esc`, and `""` turns this off.
- Agents and projects are named by the `[naming]` settings, including spelling out short names and aliases.

**Changed**
- The Gemini key is read from `.private/gemini_api_key`, in place of the service account sign-in.

**Removed**
- The computer no longer listens or speaks. `--tts-voice` and `--tts-speed` are gone, and so is the voice download.

## 2026-10-23

**Added**
- `flankr session tail` prints an agent's session, with `--pane`, `--follow`, `--entries`, `--history`, `--from` and `--to`.
- Gemini can see each agent's session, so it can answer questions about what the agent has done.
- A question an agent ends its turn with, or a finished pane's last question, is shown to you as a question you can answer.
- A phone that has been silent for a minute counts as asleep, and then gets only what needs you.
- Spoken summaries: when you ask for a reply to be read aloud and it is complex or has code in it, Gemini reads a short summary instead.
- `ADDITIONAL_PROMPTS.md` holds the extra prompts flankr uses, and `--prompts` names another file.
- `[urls] repowiki_url` links the `docs/` paths in an agent's replies to pages on repowiki.
- Text you dictate to the focused agent always waits for your confirmation.

**Changed**
- A project is named after its folder by default. `[naming] project_from = "workspace"` keeps herdr's workspace label instead.
- A Gemini key file that holds a service account JSON is refused, with a reason. It must be an API key.

## 2026-10-24

**Added**
- `flankr session tail --outline` shows a session's sections and where each one starts, and `--from` and `--to` read a range of them.

## 2026-10-25

**Added**
- The sessions of the agy agent are read, as the other agents' sessions are.

**Changed**
- The program is now called flankr. Its settings are in `~/.config/flankr/config.toml`, its paired-device keys in `~/.config/flankr/link_key` and `~/.config/flankr/auth.toml`.
- If `~/.config/flankr/config.toml` is missing and an older `~/.config/handlr/config.toml` exists, flankr copies the settings and the link key across on start, and logs that it did.

## 2026-10-26

**Added**
- Text sent to an agent or the general agent has the symbols it does not allow removed. Text with more than three of the symbols that count against it is refused.

**Changed**
- A tool call that exits with 0 shows as a success, not as a failure.

## 2026-10-27

**Added**
- The general agent can list and read the project's files, with `[llm.uses.planning]`. Its `read` patterns say which files, and `agents_prompt = true` adds the project's AGENTS.md.
- The general agent can see the project's recent commits.
- `flankr prompt` prints the general agent's full prompt. It takes `--use` and `--agent`.
- A `[llm]` section sets up the daemon's own use of Gemini: the general agent, card summaries and spoken summaries. Nothing calls a model until `enabled = true`. The file and the key are read again every 5 seconds when they change.
- Suggested answers for an agent's question come from `SUGGESTIONS.md`. `--suggestions` names another file.
- A terminal view of a session on the phone. It is read-only unless `[terminal] input = true`, and then only into agents with `terminal_input = true`, once you arm it on the phone. An arm lasts 60 seconds from the last key.

**Changed**
- Only the first word of a request can be a command, and any path in it must be inside the project.
- A request the general agent types to an agent starts with `via flankr: `, so the agent can see where it came from.
- A plan question is approved only by a reply that says "approve" with "plan" or "draft".
- `[llm]` replaces the `[gemini]` and `[summary]` sections. Those still work until `[llm] enabled` is set.

## 2026-10-28

**Added**
- Shell panes are listed and open read-only in the terminal view. The `shell` entry in `config.toml` sets this; remove it to refuse their terminal.
- `flankr pair` shows six words, a fingerprint and a confirmation code, and you say yes before a phone is let in. It makes one attempt, and `--timeout` sets its time from 1 to 300 seconds.
- A planning plugin lists the project's plans and their amendments. It is off until `[plugins."dev.flankr.plugins.planning"]` sets `enabled = true`.
- While a phone's terminal is open, `[terminal] attach = true` sizes herdr's tab to the phone and moves herdr's focus to that session's pane. A change you make on the desktop is not forced back.

**Changed**
- `HERDR_SOCKET_PATH` is used when neither `--socket` nor `--transport` is given.

## 2026-10-29

**Added**
- Images can be sent into a session. Files and git output can be opened on the phone, read-only and with a size cap.
- When Claude Code asks a question with choices, the phone shows the choices, and your pick is sent back. Messages to that agent wait while the question is open.
- Claude Code's subagents are listed with their agent and can be read.
- Plugins can offer the general agent their tools. `agent_tools = false` offers none, and `"all"` also offers the tools that change something; those run only after you approve them out loud.

## 2026-10-30

**Changed**
- Gemini models are picked by tier and age from the models your key can use. The general agent uses the newest flash model and summaries use the cheapest flash-lite model. The daemon lists them once when it starts and logs what it chose.

## 0.2.0 — 2026-10-30

**Added**
- When you finish speaking to the phone and replies came in meanwhile, the daemon writes one short spoken update on where things stand, in light of what you said, with the new `spoken_catch_up` prompt in `ADDITIONAL_PROMPTS.md`. It runs on the spoken summaries model and gives up after 8 seconds.

**Changed**
- Spoken summaries leave out which files changed, worktrees and branches, log and report entries, and test counts, unless a reply is only about them.

## 0.2.1 — 2026-10-30

**Fixed**
- A reply with a dash, an ellipsis, an arrow or a tick mark is summarized for the phone again. The daemon dropped the phone's request for it without an answer.

## 0.2.2 — 2026-10-30

**Fixed**
- Spoken and card summaries work: the oldest model is now chosen from 3.x up, since older ones can be listed and still refused.

## 0.2.3 — 2026-10-30

**Fixed**
- If Google refuses a model while the daemon runs, it moves to the next allowed one for that tier and age, and tries again, instead of failing every summary until a restart. A model named exactly in the config is never changed.

## 0.3.0 — 2026-10-30

**Added**
- A choice question Claude is showing can be dismissed from the phone. The daemon presses Esc in Claude's question screen, as dismissing it at the Mac does, but only once it sees the question's first page there; otherwise it sends nothing and tells the phone.

## 0.3.1 — 2026-10-30

**Changed**
- Spoken summaries no longer add a question or an offer the reply didn't make; a plan in the reply is said as a statement.
- Each spoken summary and catch-up is written to the log (`speech.summarize: said …`, `speech.catch_up: said …`), so what was heard can be checked.

## 0.4.0 — 2026-10-30

**Added**
- Gemini can ask a coding agent a question with the new `ask_agent` tool, such as "what did we ship today?". The question goes through the same approval as any request typed to an agent (the phone asks "Approve sending your message to Claude?"; nothing is typed until you approve, and an agent with `allow_messages = false` is never asked). Once approved, the daemon waits up to 2 minutes for the agent's answer in its session, gives it to Gemini (redacted, at most 16 KiB) with no tools available, and Gemini says a short summary. If no answer comes in time, Gemini says the agent is still working and its reply will show on its card. Not available when herdr is reached with `--remote`, since the agent's session files are not on this machine.

## 0.4.1 — 2026-10-30

**Changed**
- Stopping a Gemini turn (a tap on the phone's centre button) now interrupts only the agents Gemini sent a command to. An agent it only asked a question (`ask_agent`) keeps working, and its answer shows on its card; the agent in focus is no longer interrupted just because it is working.

## 0.5.0 — 2026-10-30

**Added**
- While the general agent is answering from the phone, the daemon tells the phone what it is doing: each tool Gemini runs, by a plain name such as "Looking at recent commits…", and when it goes back to thinking. While it waits for a coding agent's answer to `ask_agent`, the phone reads "Asking Claude…" (the agent's name). Nothing more is said aloud.

## 0.5.1 — 2026-10-30

**Fixed**
- A confirmation could be missing from its agent's session on the phone after the daemon restarted, because its number had been used before. Confirmation numbers now carry on after the highest one in the approval journals (`~/.local/state/flankr/sessions/`), and the log says where they start.

**Changed**
- When Gemini asks to send something to a coding agent (`ask_agent` or `send_agent_command`), the approval question and the card in the agent's session quote the exact text, such as `Approve sending "What did we ship today?" to Claude?`. A message you dictate still asks "Approve sending your message to Claude?".
- Each line Gemini says to the phone, and each approval question its turn asks, is written to the log (`Turn 3 said (seq 0): …`, `Turn 3 asked confirmation 4: …`), redacted, so what was heard can be checked.

## 0.6.0 — 2026-11-02

**Changed**
- `flankr serve`, and `flankr` on its own, now start the daemon in the background and return once it answers, printing `flankr is running on <socket>; log: <file>`. Quitting the terminal (or the terminal app on macOS) no longer stops it. If one is already running on that socket, it says `flankr is already running on <socket> (pid <n>)` and starts nothing. If it does not come up within 5 seconds, the end of its log is shown. For the old behaviour, the daemon in this terminal, use `flankr serve --foreground`.
- The background daemon gets only `HOME`, `PATH`, `USER`, `LANG`, `TMPDIR`, `RUST_LOG`, `HERDR_SOCKET_PATH` and any `XDG_*` from your shell, never `GEMINI_API_KEY`. It finds the key in `.private/gemini_api_key` or `.env` in the folder you started it from; when the key is only in your shell, `serve` says so in one line.

**Added**
- `flankr server` runs the daemon in the foreground, as `flankr serve` used to; `flankr serve --foreground` does the same.
- `flankr serve --stop` stops the daemon on that socket (it waits up to 5 seconds, then kills it), and removes a pid file left by one that is gone.
- The daemon's log and pid file are in `~/.local/state/flankr/`: `daemon.log` and `daemon.pid` for the default socket, `daemon-<8 hex>.log` and `.pid` for one given with `--listen` or `[server] socket`. The log is appended to.

## 0.6.1 — 2026-11-02

**Changed**
- `flankr pair` starts the daemon in the background first when none is running, as `flankr serve` does, then pairs. `flankr pair --foreground` never starts one.

## 0.6.2 — 2026-11-02

**Added**
- `flankr --version`, `flankr -V` and `flankr version` print the version and the commit it was built from, e.g. `flankr 0.6.2 (1a2b3c4)`.

**Changed**
- An unknown argument is now an error (`flankr: unknown argument '…'; see flankr --help`, exit 2) instead of a warning: before, `flankr --anything` went on to start the daemon. Bare `flankr` with nothing after it still starts it.

## 0.7.0 — 2026-11-02

**Changed**
- The daemon no longer reads the Gemini key from `.private/gemini_api_key` in the folder you start it from, nor from `.env`, so it finds the key whatever folder it starts in. It looks, in this order: the `api_key_file` a backend names in `config.toml` (a relative path is now taken next to `config.toml`, not in the working folder), the macOS Keychain (item `dev.flankr`, account `gemini`), `~/.config/flankr/secrets/gemini_api_key` (under `$XDG_CONFIG_HOME/flankr/` when that is set), then `GEMINI_API_KEY` when you run `flankr serve --foreground` from a shell that has it.
- To move a key you already have, run `flankr key import gemini .private/gemini_api_key` in the folder that has it, or `flankr key set gemini` and paste it. The old file can be deleted afterwards. `.env` is still read for its other settings; a `GEMINI_API_KEY` line in it is skipped, with a note saying so.
- The log names where the key came from when the daemon starts, e.g. `[llm] gemini key: Keychain (dev.flankr/gemini)`; with no key, it lists the places it looked and says to run `flankr key set gemini`.
- A key file that others can read (group or world permissions) is refused, not read, with the fix: `chmod 600 <file>`.

**Added**
- `flankr key set gemini` asks for the key without showing it (or reads one line from stdin when it is piped) and stores it in the macOS Keychain; elsewhere, or with `--file`, in `~/.config/flankr/secrets/gemini_api_key`, created only readable by you. It prints where it stored it, never the key. An empty key is refused.
- `flankr key import gemini <file>` stores the key from a file the same way and leaves the file for you to delete.
- `flankr key status` lists each place in the order above with `found`, `none`, `locked` or `refused: <why>`, and which one the daemon would use.

## 0.7.1 — 2026-11-03

**Added**
- With Tailscale running, the daemon makes Tailscale forward this machine's tailnet port 7347 to the link itself, by running `tailscale serve --bg --tcp 7347 tcp://127.0.0.1:7347` when the forward is missing; it logs `[link] tailscale: forwarding <name>:7347 to the link`, or `[link] tailscale: <name>:7347 already forwards to the link`. The forward stays after the daemon stops. If Tailscale refuses (for example, serving is off for the tailnet), the log gives its error and the command to run by hand. Tailscale's CLI is found on `PATH`, else in `/Applications/Tailscale.app`.
- `--vpn tailscale|none` on `flankr serve`, `flankr server` and `flankr pair`, and `vpn = "tailscale" | "none"` under `[link]` in `config.toml` (the flag wins; `tailscale` is the default). `--vpn none` runs and reads nothing for Tailscale.
- `--bind <addr:port>` (repeatable) on the same commands replaces `[link] listen` for that run, e.g. `--vpn none --bind 0.0.0.0:7347` to pair over this network. A pairing then names this machine first (`<name>.local` on macOS), then its addresses; for `0.0.0.0` or `::`, each network interface's address.
- `flankr pair` passes `--vpn` and `--bind` on to a daemon it starts.

**Changed**
- When every address the link has is this machine's own (127.0.0.1), `flankr pair` shows no QR code or pairing URL: it prints `flankr is only reachable from this machine (127.0.0.1:7347): a phone can't connect to it.` and what would fix it (start Tailscale, or `--vpn none --bind 0.0.0.0:7347`, or Tailscale's error), and exits 1. The daemon's log says the same at start.
- A pairing offer through Tailscale names the tailnet address first, then 127.0.0.1:7347, as before; it no longer needs you to run `tailscale serve`.

## 0.7.2 — 2026-11-03

**Fixed**
- The daemon finds Tailscale's command when it is not on `PATH`: it also looks in `/opt/homebrew/bin`, `/usr/local/bin`, and the Tailscale app in `/Applications` or `~/Applications`. Before, a daemon started over SSH or in the background could miss a Homebrew install and report Tailscale as not running.

## 0.7.3 — 2026-11-03

**Changed**
- Pairing a device under a name that is already paired no longer fails and needs a second `flankr pair --replace` run. After you type `yes` to the six words, `flankr pair` asks `'<name>' is already paired (paired <date>). Replace it with this device? [y/N]`. `y` replaces the old entry (keeping its `agents` and `approve` settings) and disconnects the old key; anything else keeps it, prints `Not paired: '<name>' was kept.` and exits 1. The phone waits while you answer, within the same offer time. With `--name`, the name asked about is that one.
- `--replace` still works and skips the question.

## 0.7.4 — 2026-11-03

**Added**
- `flankr serve --restart` stops the flankr daemon running on the socket, whichever copy it is and however it was started (in the background, or with `flankr serve --foreground` or `flankr server` in a terminal), then starts this `flankr` in the background. It prints the `flankr stopped on …` line, then `flankr is running on <socket>; log: <file>`. With no daemon running it is plain `flankr serve`. If the stop fails, nothing is started. `--restart` with `--stop` or `--foreground` is refused: `flankr: --restart and --stop do not go together`, exit 2.
- When `flankr serve` finds a daemon already running from another copy of `flankr`, it says so after `flankr is already running on <socket> (pid <pid>)`: `It runs an older copy of <path>; run flankr serve --restart to restart it.` when the file at its path was replaced since it started (as `scripts/install.sh` does), or `It runs <its path>, not <this path>; run flankr serve --restart to restart it.` when it runs from somewhere else. `flankr pair` shows the same two lines when it finds such a daemon. Nothing is stopped.
- `scripts/install.sh --restart` runs `~/.local/bin/flankr serve --restart` after installing, here and on each `--remote`. Without it the script still never stops or starts a daemon.

## 0.7.5 — 2026-11-03

**Changed**
- Typing into a terminal from the phone is now on by default. `[terminal] input` defaults to `true`, and so does each `[[agents]]` entry's `terminal_input`; a paired device that may approve can type once its first key arms the terminal. To turn typing off everywhere, set `input = false` under `[terminal]` in `config.toml`; to turn it off for one agent, set `terminal_input = false` in its `[[agents]]` entry. `terminal_signals` (ctrl+c, ctrl+d, ctrl+z, ctrl+\) still defaults to `false`.
- Any herdr pane opens in the phone's terminal view: a plain shell (zsh, bash, htop, a build), and an agent that has no `[[agents]]` entry, as well as the configured agents. A pane with no agent takes typing and the signal keys; an agent with no entry takes typing but not the signal keys. `[[agents]]` still decides what the voice side and the general agent may send. The shipped `shell` entry no longer needs `terminal_input = true`; `terminal_input = false` there makes every pane without an agent read-only.

**Added**
- The log says `Terminal <n> open on shell pane '<pane>' for <device>, …, typing allowed` (or `typing off`) when a phone opens a shell pane, and `<device> typed <n> key(s) into terminal …` for each batch, as for agents; the keys themselves are never logged.
- The log says `<device> picked option <n> of question <n> of choice question <id> in terminal …` when a tap on one of Claude Code's question options in the phone's terminal view picks it.

## 0.7.7 — 2026-11-03

**Fixed**
- A test run that prints its totals as `passed 622 failed 0` no longer reads as 622 failing tests, so the agent's view no longer shows it as *Something broke*. The count after `failed` (`failed 0`, `failed: 0`, `failed=0`) is now taken as the number of failures; a count before it (`2 failed`) still counts, unless it belongs to an earlier label such as `passed`. A sentence such as `the build failed 3 times` is not read as a test summary.

## 0.7.8 — 2026-11-03

**Changed**
- A pane open in the phone's terminal view that takes typing now takes images from the phone too, a plain shell or an agent with no `[[agents]]` entry included, so the keys row's + can paste an image there. The terminal gets the image's absolute path in the daemon's temporary folder, typed as it is (it has no spaces). Other panes still need an `[[agents]]` entry for images, and files and `git` output still need one.

## 0.7.9 — 2026-11-03

**Fixed**
- A Claude Code session continued after its context ran out keeps updating in the phone's list view. Claude goes on in a new transcript while herdr still names the old one; the daemon now follows the old transcript's `continued-in` record to the new one (and on, if it is continued again), the earlier conversation staying above the new. The log says `[sessions] <pane>: Claude continued <old id> as <new id>; following it` once for each pane at each switch.

## 0.7.10 — 2026-11-03

**Fixed**
- A tap on a Claude Code session's input line in the phone's terminal view raises the phone's keyboard again in a session that has a name. Claude Code writes the name into the line above its prompt, and the daemon no longer took that line for something else, so it found no input line to send the phone. The input line the phone gets is now the prompt's whole box, its two lines included, so it is easier to hit.

## 0.7.12 — 2026-11-12

**Added**
- A new plugin, Plan policy (`[plugins."dev.flankr.plugins.policy"]`), off until its table says `enabled = true`. When you choose Build on a plan from the phone, the plan becomes an active task. While that task's autocontinue is on, the daemon reads each Finished card's summary from the task's pane and decides continue, stop or ask. It answers only the agent's own "continue with step N?" question, and only by asking you to confirm `continue` on the phone; nothing is sent until you tap Yes. A choice question from the agent holds the task until it is answered.
- Its settings: `agents` (the `[[agents]]` names it may drive; none by default), `continue_command` (default `continue`, which must be on that agent's `allowed_commands`), `max_continues` (3 per task, then it asks you instead), `confirm = "always"` and `quiet_secs` (15: only the newest card from a pane is judged, after this long).
- `[llm.uses.policy]`: the model the plugin's judgement uses, `flash-lite:oldest` by default like the summaries. `enabled = false` turns the judgement off; every card is then left to you.
- The active tasks are kept in `~/.local/state/flankr/plugins/policy/approvals.toml`. A task's autocontinue pauses when its plan file changes, and stops when the plan's status no longer allows work.
- The log says `[policy] pane=<pane> agent=<agent> plan=<plan> step=<n> decision=<continue|stop|ask> rule=<rule> serial=<card> reason=<why>` for each decision, and which confirmation it opened and how it ended.

## 0.7.13 — 2026-11-12

**Added**
- With the Plan policy plugin on, the daemon works out which plan each agent pane is on, for the phone's Active tasks view: the plan you chose Build on for that pane, else a plan path in its newest reply, else a plan's title, else the `plan/<date>_<nn>` branch of the worktree it is in, else one question to the `[llm.uses.policy]` model with the titles of the project's active plans. This only places the pane in the phone's list; it never makes the plugin send anything. The log says `[policy] pane=<pane> plan=<plan> linked_by=<how> worktree=<path or ->` when a pane's plan changes.
- The phone's agent list now says which herdr worktree each agent is in, from herdr's own list of worktrees, on a herdr on this computer.
