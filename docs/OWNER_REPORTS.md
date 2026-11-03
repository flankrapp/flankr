# Owner reports

Reports from the owner using the companion app, and what was done about
them. Entries are appended under dated headings, never edited; a
correction is a later entry that names the one it corrects.

## 2026-10-30 13:17 EDT — Owner: a drag in the drawer opened a session

Device: Galaxy S23 Ultra, the build installed 2026-10-29 15:18 EDT.

**Report.** "tap and drag in the left sidebar drawer should not register
as a tap on the menu items. So when holding down the [finger] and dragging
more than the height of the menu item, that should register as a drag.
When tapping, releasing within a very small slop, that should count as a
tap."

**Cause.** The drawer's remote cards ignored a release after the finger
had moved past the 10 pt slop, but the session rows did not: the release
always sent a click, so a drag that scrolled the list could open the row
under the finger
([Session drawer](design/20261028_03_SESSION_DRAWER_PLAN.md), 3.11).

**Done.** `afd9474`: a press that moved past the 10 pt slop is a drag
and never opens a row; a release within the slop is a tap. Installed on
the S23 2026-10-30 13:14 EDT.

## 2026-10-30 13:17 EDT — Owner: the session list should show at once, without "Looking for agents"

**Report.** "for the remote session list, that should be stored persisted
and restored immediately. It shouldn't have a looking for agents message
in places that both in the drawer and the overview screen. What should
happen instead is it should merge the up list as it comes in".

**Seen.** The overview shows *Looking for agents…* until the daemon's
first list arrives, and the drawer shows *Connecting to <nickname>…* in
place of the rows while the remote is offline; nothing is kept between
starts.

**Done.** Recorded as the session drawer plan's 2026-10-30 13:15 EDT
amendment (`ec4df3c`): each remote's last list is kept in its
`state.redb`, shown at once at start and on a switch, with no
looking-for message, and each new list merged in by pane. Being built.

## 2026-10-30 13:19 EDT — Owner: a drag on the overview snaps back

Device: Galaxy S23 Ultra, the build installed 2026-10-30 13:14 EDT.

**Report.** "dragging on the overview screen snaps back to the beginning
or end it doesn't persist it doesn't persist where you drag to so if you
have" (the report was cut off there).

**Seen.** Not yet looked into; handed to a subagent in its own worktree.

## 2026-10-30 13:20 EDT — Owner: the rest of the overview drag report

This completes the entry before ("a drag on the overview snaps back"),
which was cut off.

**Report.** "more if you have more tiles then fit on the screen like more
than more than three screens or whatever sometimes it won't let you it
won't let you navigate to the center tiles".

**Seen.** With several screens of cards, the drag's snap back to the
beginning or the end means the cards in the middle can't be reached.
Passed to the same subagent.

## 2026-10-30 13:29 EDT — Owner: a terminal error fills the screen with the daemon's JSON

Device: Galaxy S23 Ultra, the build installed 2026-10-30 13:29 EDT.

**Report.** "on the terminal screen with an incompatible terminal when you
attempt to drag or do something else that causes an invalid event it's
the entire screen with it looks like it's from the JSON instead that
should like a small maybe centered of the terminal just as it you know
it's only there for a couple of seconds it's out and just just a short
message like like like terminal arrow or something like that".

**Seen.** Not yet looked into; handed to a subagent in its own worktree.
Read as: when the terminal can't take an input (an incompatible terminal,
a drag or other event it rejects), the whole screen shows the raw error,
apparently the daemon's JSON. It should be a short message such as
*Terminal error*, small and centred over the terminal, gone after a couple
of seconds.

## 2026-10-30 13:30 EDT — Owner: the overview drag report, done

This follows the two entries on the overview's drag snapping back.

**Cause.** The overview grid's scroll took each move's distance over the
time since the move before as the finger's speed. The S23 delivers moves
in bursts a fraction of a millisecond apart, so even a slow drag measured
tens of thousands of points a second, and on release that became a fling
that ran the grid to the top or the end. The agent view had fixed the
same fault earlier; the overview's scroll never had the fix.

**Done.** `3da50c6`: the grid keeps the moves' samples and flings at the agent
view's release speed (a fit over the last 100 ms, capped), as the overview
plan's A1 already says. A drag now stays where the finger leaves it; a
hard flick still carries on. The daemon's re-list was checked and does not
move the scroll. Installed on the S23 the same day.

## 2026-10-30 13:34 EDT — Owner: spoken messages interrupt voice input

Device: Galaxy S23 Ultra, the build installed 2026-10-30 13:31 EDT.

**Report.** "spoken messages shouldn't interrupt an interactive voice
input".

**Seen.** Not yet looked into; handed to a subagent in its own worktree.
Read as: while the owner is giving voice input (dictating a message or
talking to Gemini, the mic armed or hearing), a reply read aloud or a
spoken notice starts playing over it. Speech should wait until the voice
input ends.

## 2026-10-30 13:39 EDT — Owner: the terminal error report, done

This follows the entry on a terminal error filling the screen.

**Cause.** When a terminal call failed (opening, arming, input, a scroll
or mouse event, as an older herdr refuses), the client kept the daemon's
error text, and the agent view's status line drew it as *The daemon
refused the terminal: …*, wrapped across the whole terminal. The same
text also went to the overview's status line.

**Done.** The terminal mode plan's 2026-10-30 13:31 EDT amendment and
`9d35f5e`: a failure shows a small *Terminal error* pill centred over the
terminal for 2 s (restarted by a further failure), with the terminal
still drawn under it; the full error goes only to the log. A failed open
says *The terminal could not open.* Installed on the S23 the same day.

## 2026-10-30 13:46 EDT — Owner: the voice input report, done

This follows the entry on spoken messages interrupting voice input.

**Cause.** Four ways speech reached voice input: queued notices and
read-aloud replies were let out in the gap between dictated sentences,
when the mic goes idle before the next is armed; Gemini's spoken lines
were said at once, even while a message was being dictated; a tapped
Gemini turn's "stopped" sentence spoke over the new prompt; and speech
already playing when voice input started was never stopped.

**Done.** The dictated messages plan's 2026-10-30 13:37 EDT amendment and
flankr-companion 0.6.0 (`5fd5b66`): while a message or Gemini's prompt is
taken by voice, speech is held in order and plays as soon as the voice
input ends (sent, cleared or timed out); starting voice input stops what
is being said. Still spoken at once: the dictation's own read-back, its
empty, full and send-failure lines, replays you tap, and confirmation
questions (the next sentence answers them). The done ding still plays.
Installed on the S23 the same day.

## 2026-10-30 15:12 EDT — Owner: the record button does nothing

Device: Galaxy S23 Ultra, the build installed 2026-10-30 14:47 EDT (app
0.2.0, the voice catch-up).

**Report.** "tapped the record button and got no response".

**Seen.** The app's log (`adb logcat -s flankr`, read 15:12 EDT) has five
taps on record between 15:10:45 and 15:11:05, each a release logged as
`hold: pressed Message` then `agent_hit: Message`, so the tap reached the
app and `StartDictation` was sent. The client logged nothing after any of
them: no dictation started, no refusal. Before them: 15:09:21 a tap on an
answer chip; 15:10:16 the viewed agent finished and a reply was spoken;
15:10:21 "no answer in time; reading the reply here". Handed to a subagent
in its own worktree.

## 2026-10-30 15:22 EDT — Owner: stuck on refreshing, and no summary

Device: Galaxy S23 Ultra, the build installed 2026-10-30 14:47 EDT.

**Report.** "app is currently showing --refreshing...-- and doesn't have
the latest updates, it also didn't speak/pin that last summary".

**Seen.**
- **The connection.** At 15:10:21 the phone logged "no answer in time;
  reading the reply here", and the real daemon logged the phone's
  connection closed at the same second. The phone's client has logged
  nothing since: no disconnect, no reconnect. So every later input (the
  record taps in the entry before, pull to refresh) goes nowhere. Very
  likely the same fault as the record button; passed to that subagent.
- **The summary.** Every summary since the restart failed: the key's model
  list includes `gemini-2.5-flash-lite`, which *oldest* picked, but
  generating with it is refused: "This model models/gemini-2.5-flash-lite
  is no longer available to new users. Please update your code to use
  models/gemini-3.5-flash-lite". A model can be listed and still refused
  to a new key, so *oldest* can pick one that fails.

## 2026-10-30 15:27 EDT — Owner: the record button and the stuck refresh, done

This follows the two entries before ("the record button does nothing"
and "stuck on refreshing, and no summary"); both had one cause.

**Cause.** The rules that read a reply aloud split a word at a byte
offset when checking for "am" or "pm", which crashed on a word ending in
a character such as —, …, → or ✓. The daemon crashed on it while
measuring the reply for a summary, so the summary was never answered;
after 10 s the phone read the reply itself, crashed the same way, and the
crash ended its connection thread without a word. From then on every tap
and refresh went nowhere.

**Done.** `0d1cb5e` and the commits after it: such a word is read as it
is; the phone's connection thread now logs a crash that ends it and every
connection that ends, and a tap that does nothing says why in the log.
App 0.2.1 and daemon 0.2.1. The summary's model refusal is a separate
fix, in progress.

## 2026-10-30 22:05 EDT — Owner: a Gemini turn shows no sign it is working

Device: Galaxy S23 Ultra, the build installed 2026-10-30 17:05 EDT (app
0.6.0).

**Report.** "The problem was it it didn't seem to do anything. It didn't
have any any kind of thinking Feedback or anything else".

**Seen.** At 21:59:57 a spoken question to Gemini was sent and the turn
started; Gemini called `recent_commits` and was answering with its result
when, at 22:00:03, a second tap on the centre button interrupted the turn,
so no answer came. For those six seconds the phone showed nothing: the
client keeps the turn internally but gives the app no event for it, and
the daemon sends nothing while Gemini thinks or runs a tool. Handed to a
subagent to amend the general agent chat plan and build the feedback.

## 2026-10-30 22:11 EDT — Owner: Gemini should ask the coding agent and summarize its answer

**Report.** "I asked the general agent what we shipped today. I would
expect it to use a tool call to pass that request to you Get the response
back and summarize it as a list of changes".

**Seen.** At 21:59:57 Gemini answered the question itself: it called
`recent_commits` (5 commits) instead. It has `send_agent_command`, which
types a command into a coding agent's pane after the owner approves it,
but no tool that waits for the agent's reply and gives it back, so it
cannot ask an agent and summarize what it says. Handed to a subagent to
amend the general agent planning plan and build it.

## 2026-10-30 22:37 EDT — Owner: Gemini's turn feedback and asking the agent, done

This follows the two entries before ("a Gemini turn shows no sign it is
working" and "Gemini should ask the coding agent and summarize its
answer").

**Done.**
- **Asking the agent:** daemon 0.4.0. Gemini has `ask_agent`: after the
  owner approves the question on its card, it is typed into the agent's
  pane, the same turn waits up to 120 s for the agent's finished reply,
  and Gemini summarizes it as a short list; past 120 s it says the reply
  will show on the agent's card. Daemon 0.4.1: a tap that stops Gemini
  leaves an agent it only asked alone, and no longer interrupts the
  focused agent.
- **Feedback:** app 0.7.0 and daemon 0.5.0. While a turn runs, Gemini's
  card shows a spinner and *Thinking…*, or what it is doing (*Looking at
  recent commits…*, *Asking Claude…*), and the centre button spins until
  the turn ends.

## 2026-10-30 22:41 EDT — Owner: no approval card for ask_agent, and nothing heard

Device: Galaxy S23 Ultra, the build installed 2026-10-30 22:38 EDT (app
0.7.0, daemon 0.5.0).

**Report.** "Didn't get the approval card or the verbal".

**Seen** (phone and daemon logs):
- 22:39:53 the spoken question went to Gemini; 22:39:58 Gemini called
  `ask_agent` (agent "Claude in flankr", question "What did we ship
  today?") and the daemon opened confirmation 1; the card showed
  *Asking Claude in flankr…*.
- 22:40:03 and 22:40:06 two short lines were played (4 s and 3 s; not
  muted). Turn lines are not logged, so what they said is not known; the
  owner heard nothing useful.
- No approval card was shown: the phone logged none.
- 22:40:30 the owner tapped record and dictated a message to Claude; at
  22:40:36, as speech was heard, confirmation 1 resolved *denied*: a
  pending voice-turn confirmation takes the next utterance as its answer,
  so a message to another agent answered Gemini's approval.

Handed to a subagent in its own worktree.

## 2026-10-30 23:10 EDT — Owner: the ask_agent approval, done

This follows the entry "no approval card for ask_agent, and nothing heard".

**Cause.**
- **No card:** the daemon numbers confirmations from 1 at every start,
  and the session's approval journal already held a confirmation 1 from
  2026-10-27, so the new one was skipped as a duplicate and the agent view
  never made its card.
- **Denied by the message:** the client answered a pending voice-turn
  confirmation with the next thing said, so dictating to Claude denied
  Gemini's question.
- **What was heard:** Gemini's "I'll ask Claude in flankr once you
  approve", then the question "Approve sending your message to Claude in
  flankr?", which did not say what would be sent.

**Done.** Daemon 0.5.1, client 0.8.0, app 0.7.1, installed 23:10 EDT:
confirmation numbers continue past the journals', so the card shows;
Gemini's questions quote the text (Approve sending "What did we ship
today?" to Claude?); only a tap on the card or a reply to Gemini answers
it, never a dictated or typed message; *Not sent: denied.* shows on
Gemini's card; each line a turn speaks and each question it asks is
logged.

## 2026-11-02 22:43 EST — Owner: adding a remote does not open the camera

Device: Galaxy S23 Ultra (app 0.7.1).

**Report.** "it didn't open the camera, regardless of the trailscale serve"
(pairing the MacBook, after `flankr pair` there).

**Seen.**
- The app logged `remotes: pairing another` at 22:38:26 and 22:38:35
  (the drawer's **+**), and nothing about the camera or a permission
  after either: no scanner start, no permission request.
- `dumpsys package dev.flankr.companion`: `android.permission.CAMERA:
  granted=false, flags=[… ONE_TIME]`: the camera was once allowed "only
  this time", and Android has since revoked it. The add screen neither
  asks again nor says the camera is off.
- Separately, the MacBook's daemon could not be reached anyway: its link
  listens on 127.0.0.1:7347 and Tailscale did not forward the port.

Handed to a subagent in its own worktree.

## 2026-11-02 22:57 EST — Owner: adding a remote and the camera, done

This follows the entry "adding a remote does not open the camera".

**Cause.** The camera started only from a tap on *Tap to scan*, as the
pairing QR scan plan said (its 16:40 EDT amendment: "Tap to scan every
time", no permission request at launch); opening the window started
nothing and asked nothing, and a revoked "only this time" permission had
no visible state.

**Done.** App 0.7.2, installed 22:57 EST: opening the pairing window
(first pairing or the drawer's **+**) starts the camera, and Android asks
for the Camera permission each time it opens while not granted. Refused,
the window says *Camera is off for flankr*; refused for good, it adds
**Open Settings**. Pasting a link works throughout. This reverses the
16:40 EDT choice for first pairing too (the pairing QR scan plan's
2026-11-02 22:46 EST amendment).

## 2026-11-02 23:03 EST — Owner: the pairing window was blank

Device: Galaxy S23 Ultra, app 0.7.1 (the report is about the tries at
22:38 EST, before 0.7.2 was installed at 22:57).

**Report.** "I'm not sure why it didn't show the placeholder either, it
just showed a blank screen".

**Seen.** Opening the drawer's **+** (`remotes: pairing another`, then
`drawer: hidden`, at 22:38:26 and 22:38:35) showed a blank screen: no
*Tap to scan* placeholder, no paste field. The log has no drawing errors.
Handed to a subagent in its own worktree; 0.7.2 changes what opening does
(it starts the camera) and may hide or change this.

## 2026-11-02 23:14 EST — Owner: the pairing window was blank, done

This follows the entry "the pairing window was blank" and its evidence on
0.7.2 ("blank after giving permission": the camera opened and read the QR
code in 6 s with nothing on screen).

**Cause.** When the drawer's **+** was tapped over a session's view, the
app chose the pairing screen to draw but never laid it out: its layout
returned early at the open session view, before the pairing screen's
content was built. With no content there were no texts, no paste field and
no camera area to draw the picture into, so only the background showed.
From the overview it worked, which is why first pairing had shown the
camera before.

**Done.** App 0.7.3 (`607c66d`): the pairing screen is laid out over a
session's view too.

## 2026-11-03 00:04 EST — Owner: a pairing code for 127.0.0.1 offers a device code that cannot work

Device: Galaxy S23 Ultra (app 0.7.3).

**Report.** "the phone says 127.0.0.1 cannot be accessed directly and
suggests pasting the bip code, but that's not actually going to set up the
connection".

**Seen.** The MacBook's `flankr pair` offered only 127.0.0.1:7347 (its link
listens on loopback, and Tailscale was not forwarding). The phone, given a
pairing link whose only address is a loopback one, falls back to the
device code (`pairing: the request reached no daemon; the device code
instead`). The device code still has to reach the daemon over that
address, so it can never complete.

**In hand.** The daemon side is in the link VPN plan's 2026-11-02 23:53 EST
amendment (no QR code or URL for an offer only the machine can reach),
being built. The phone side, for a link that names only loopback addresses
(from an older daemon, or pasted), is handed to a subagent.

## 2026-11-03 00:20 EST — Owner: the 127.0.0.1 pairing code and the device code, done

This follows the entry "a pairing code for 127.0.0.1 offers a device code
that cannot work".

**Cause.** The device code only adds the phone's key on the computer; the
pairing completes when the phone then reaches the daemon at the same
addresses from the same link. With none reachable, the code could never
complete.

**Done.**
- **Computer** (daemon 0.7.1, the link VPN plan's 23:53 EST amendment):
  `flankr pair` shows no QR code or link when the daemon is reachable only
  from that machine, and says how to make it reachable; with Tailscale it
  now adds the forward itself.
- **Phone** (client 0.8.1, app 0.7.4, the multiple remotes plan's
  2026-11-03 00:07 EST amendment): a code that names only 127.0.0.1, ::1 or
  localhost is refused without connecting: *This pairing code only works on
  the computer that made it…*, with the two fixes; loopback addresses in a
  mixed code are dropped; when no address takes a connection the phone says
  *Can't reach …* and offers no device code. The device code is kept only
  where it can work.

## 2026-11-03 11:38 EST — Owner: re-pairing a name should ask, not need `--replace` and a second run

Device: Galaxy S23 Ultra, pairing with `flankr pair` (daemon 0.7.2).

**Report.** "the --replace should be a question when pairing, not
requiring a second cycle".

**Seen.** A device whose name is already in `auth.toml` is asked the usual
*Pair?* question, and a `yes` then fails with *the name '…' is already
paired (use --replace to re-pair it)*; the phone is told it was refused.
Re-pairing needs a second `flankr pair --replace` run and a second scan.

**In hand.** Handed to a subagent: an amendment to the link and companion
plan (its `--replace` of the 2026-10-22 18:20 EDT amendment), with
`flankr pair` asking whether to replace the existing entry in the same run.

## 2026-11-03 11:46 EST — Owner: re-pairing a name should ask, done

This follows the entry "re-pairing a name should ask, not need `--replace`
and a second run".

**Done** (daemon 0.7.3, the link and companion plan's 2026-11-03 11:40 EST
amendment). After a `yes` to the six words, when the name is already
paired and `--replace` was not given, `flankr pair` asks *'<name>' is
already paired (paired <date>). Replace it with this device? [y/N]* in the
same run, with the phone still waiting. `y` replaces the entry, keeping
its `agents` and `approve` settings; anything else keeps it. `--replace`
still skips the question.

## 2026-11-03 12:06 EST — Owner: one details screen for a remote; a 1 s hold on its card

Device: Galaxy S23 Ultra (app 0.7.4).

**Report.** "the details screen shown on connection should be the same one
as shown when holding the remote card/chip, and the holding the remote/card
chip should only be 1s with no progress bar".

**Seen.** Holding a remote's card in the drawer opens its details after
2 s, with a bar filling along the card's bottom; the details shown on
connecting are a different screen.

**In hand.** Handed to a subagent: an amendment to the covering plan, one
details screen for both, and a 1 s hold with no progress bar.

## 2026-11-03 12:07 EST — Owner: the key and fingerprint expands, the same in both

This adds to the entry "one details screen for a remote; a 1 s hold on its
card".

**Report.** "the key and fingerprint expands should work the same in both
modals".

**In hand.** Given to the same subagent, for the same amendment.

## 2026-11-03 12:08 EST — Owner: no progress bars on holds

This adds to the entry "one details screen for a remote; a 1 s hold on its
card".

**Report.** "you can remove the progress bar for that in the app, it's bad
ux, the time should be adjusted instead of it's too long".

**Seen.** Four holds draw a bar filling along the control's bottom: a
remote's card in the drawer (2 s), the overview's and the agent view's
2 s presses, and the bottom bar's 500 ms hold.

**In hand.** Given to the same subagent: every hold's bar removed, the
drawer card's hold made 1 s, and the other holds' times kept for the owner
to decide.

## 2026-11-03 12:10 EST — Owner: every hold 500 ms

This adds to the entry "no progress bars on holds".

**Report.** Asked "should they all be 500ms?", then, offered every hold at
500 ms with the volume button's hold opening its menu (with a **Stop** item
while something is being spoken, in place of the 1 s stop), "yes, tell it
to make them all 500ms".

**In hand.** Given to the same subagent: the drawer card, the text input's
**+**, the centre and right buttons and the volume button all fire at
500 ms; the drawer card's 1 s from the first entry is replaced.

## 2026-11-03 12:36 EST — Owner: one modal after pairing, with a flash

Device: Galaxy S23 Ultra (app 0.7.5).

**Report.** "do we need two modals? once the pairing is complete the
details modal should work, it should have a \"flash\" message if that makes
sense on the first use"; then, offered the outline below, "yes, build it
and install on s23".

**Seen.** In 0.7.5, a finished pairing shows "Paired with <name>" with
Details and Done; Details closes it and opens the remote's details modal.

**In hand.** Handed to a subagent: when pairing completes, the pairing
modal becomes the remote's details modal in place, with a "Paired with
<name>" flash at its top for about 3 s on that first open only; the
"Paired with" screen goes.

## 2026-11-03 13:05 EST — Owner: a hold on record should open text input

Device: Galaxy S23 Ultra (app 0.7.6).

**Report.** "hold on that record should have opened text input".

**Seen** (the phone's log). Two 500 ms holds on the record button, at
12:59:21 and 13:02:34, each logged `hold: fired Message` and opened the
message card typed (`dictation: on`, keyboard mode, no listening). After
the first, nothing was typed until a tap on the card's text area at
12:59:24 (`agent_hit: PromptText(Message)`), so the soft keyboard did
not come up on its own; after the second, typing began at 13:02:37 with
no tap. The card opens; the focus and keyboard after a hold do not always
follow.

**In hand.** Handed to a subagent to find why the keyboard does not
always rise after the hold, and fix it.

## 2026-11-03 13:06 EST — Owner: the record on the approval card; a new card dismissed input; a keyboard on voice cards

This corrects the entry "a hold on record should open text input": the
record button meant there is the approval card's, not the bottom bar's
(asked which, the owner answered).

**Report.** "the record on the approval card. also a new card dismissed my
input, and voice input cards should have the keyboard icon to switch to
text input".

1. **The approval card's record button** should open text input.
2. **A card that arrived dismissed the input being typed.**
3. **Voice input cards** should carry the keyboard icon, to switch to
   typing.

**In hand.** All three handed to a subagent; the bottom bar's hold in the
earlier entry is not the report, though its keyboard not rising at
12:59:22 is passed on with it.

## 2026-11-03 14:07 EST — Owner: the approval card's record, a hold for typing

This follows the entry "the record on the approval card; a new card
dismissed input; a keyboard on voice cards", item 1.

**Report.** Offered a tap that stays the voice reply and a 500 ms hold that
opens the reply typed, or a tap that types instead, the owner chose the
hold: "Yes, make a 500 ms hold on the approval card's record button open
the reply typed".

**Done so far** (app 0.7.7): items 2 and 3, a card arriving no longer
dismisses a typed card, and the voice cards carry the keyboard glyph.

**In hand.** The hold, given to the same subagent, for app 0.7.9.

## 2026-11-03 14:35 EST — Owner: More and Less always at the card's bottom right

Device: Galaxy S23 Ultra (app 0.7.10).

**Report.** "Small thing, but the more or less link should always be at
the bottom right of the card".

**Seen.** The pinned cards' *More* and *Less* (the harness protocol plan's
L3 amendment of 2026-10-30 15:20 EDT, item 1) and the summary cards' dim
"more" are placed by each card's own layout, so they do not always sit at
its bottom right.

**In hand.** Handed to a subagent: *More*, *Less* and "more" at the bottom
right of every card that shows them, folded or open.

## 2026-11-03 14:50 EST — Owner: More and Less below the buttons; they don't work on button cards

This follows the entry "More and Less always at the card's bottom right".

**Report.** Asked where the link goes on a card with buttons, the owner
first answered "Yes, on cards with buttons, put More Less at the text's
bottom right, above the buttons", then "actually should be below the
buttons but they don't really work on cards with buttons".

**In hand.** Given to the same subagent: the link below the button row on
cards with buttons, and why *More*/*Less* does not work on those cards.
