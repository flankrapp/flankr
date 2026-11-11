# Companion app updates

What has changed in the flankr companion app, most recent last.

From now on, each change you can notice ships with an app version. The app is version 0.1.0 today. A change that adds a feature, or changes something you would notice, raises the middle number (0.2.0). A fix or a small polish raises the last number (0.1.1). Entries from then on are headed with the version and the date.

# History before versioning

These changes came before the app had version numbers.

## 2026-10-22

**Added**
- The companion app: a voice screen on your phone that talks with your flankr agents, speaks replies and listens for your voice.

## 2026-10-23

**Added**
- A confirmation card: when an agent asks you to confirm something, the request appears as a card with its buttons.
- A dictation card: dictate a message to the agent you are viewing.
- The agent view: a session's conversation as a scrollable view, with priority cards and a pinned card.
- A back button in the agent view's header, and a suggestion row above the message button at the bottom right.
- Play reads a reply aloud. While it is read, Play becomes Skip, and a tap skips it.
- The record button turns into Send at the bottom right, with Play beside it.
- "Continue" is suggested after 30 seconds of quiet, or straight away when something has gone wrong.
- A message you send shows as typed text in a box until the agent has it.
- The overview is the home screen: a card for each agent, in a grid.
- Cards use the whole screen, and Gemini's replies appear in a strip that opens an overlay.
- Tapping a cut-off question on the pinned card expands it; tapping again folds it.
- Plan questions offer Approve, Discuss and Implement. Links in them can be tapped.
- The agent view shows sections, lets you page back, and names the agent it shows.
- A pairing link opens the app and pairs it.

**Changed**
- Once paired, the pairing field and the Pair button are hidden.
- Context lines are a dim line after an amber dot, not a card.
- Plan text is plain, and button labels shrink to fit.
- "(loading session)" shows until the session answers.
- The pinned card is shown once, at the bottom.
- The overview remembers where you scrolled to.

**Fixed**
- When you have scrolled back, the agent view holds its place as its top entry changes.

## 2026-10-24

**Added**
- The agent view's scrollback is in sections, with a jump line below that goes to the end and says what is there.
- Grey bars, shimmering faintly, show where content not yet loaded will go.
- Two-finger scrolling moves the agent view five times as far.
- A scrollbar appears while you scroll, and you can tap or drag it.
- A scrollbar setting: None, On Scroll, or Always.
- A settings screen, opened from a gear on the overview.
- Settings explains background running and notifications, with links to Android's help pages.
- A notification keeps the app running in the background, and the app asks permission to show notifications.
- A battery row in Settings with an Enable button. It updates once you have allowed the app.
- A voice choice in Sound: step through the phone's voices with a spoken preview.
- Full message text, folded tool calls and diffs, summary cards, pull to refresh, section labels, and a Clear cache option.
- A new launcher icon: a magenta circuit dog.
- Appearance in Display: Light theme or Dark theme, applied at once. A themes file can add your own.
- Each session has its own colour, and other sessions' cards appear at the bottom.

**Changed**
- Dialogs open and close at once, without a fade, and are no longer see-through.
- A harder flick scrolls further, and the scroll follows the speed of your finger on release.

**Fixed**
- The agent view no longer snaps back during a drag when your moves arrive together.
- A settings row's status wraps short of its button, and the row grows.
- An expanded pinned card stays expanded while you scroll.
- Pull to refresh leaves the pinned card whole.

## 2026-10-25

**Added**
- The app is now called flankr. It installs as a separate app from the earlier build.
- Typed messages: a keyboard card with a placeholder and a shaded text area. The text area shows at least three lines, grows to several lines, shrinks the font to fit, and has a cursor.
- The keyboard appears when you tap the text area, unless you dismiss it or an external keyboard is in use.
- A keyboard button to the left of the record button.
- A dark navy default theme, with brighter session colours, and a subtle panel texture on the overview.

**Changed**
- The "OVERVIEW - 2-COLUMN" label above the cards is gone.
- A pinned card collapses when you take a message, and remembers your choice.

**Fixed**
- The on-screen keyboard no longer covers the screen; the content moves up above it.
- The overview panel's texture lines up with the card tiles.

## 2026-10-26

**Added**
- A centre button in the bottom bar talks to the general agent: tap to talk, or hold for 2 seconds to open it. It replaces tap-to-talk on the overview.
- A general agent card, pinned, with an overlay and a text entry area.
- The centre button pulses while your voice is being recognised.
- Voice starts at once when you press the centre button, and Gemini's card appears on press.
- Replies that use Markdown (headings, lists, bold, code) show as formatted text.
- Voice input ends sooner after a pause in your speech.

**Changed**
- The centre button starts idle, and is lit only while pressed.
- The general agent's card, overlay and state look and behave the same on every screen.

**Fixed**
- Reactivating the general agent after a turn, and interrupting a turn, work cleanly.

## 2026-10-27

**Added**
- A terminal view for a session, opened from a button between Play and the centre button. Pinch to zoom, pan, and hold to type.
- In the terminal, a tap starts typing and brings up the keyboard, and a vertical drag scrolls.
- A question's suggested answers appear, and tapping one answers the question.
- A "Tab follows focus" setting in Settings' Voice section, off by default. When on, voice goes to the agent whose view is open.

**Changed**
- Finished cards show one line.
- Plan answers name the plan by its number, as in "Approve plan 03".
- Tapping a suggestion, or the Submit button, sends at once with no confirmation card. A message sent after silence still asks you first.
- One prompt card serves the general agent and typed messages on both screens. The centre button's 2-second hold opens Gemini's card pinned.
- The centre button, the Gemini strip and the Settings toggle follow the agent's setup. They are hidden or dimmed when no general agent is available, and Settings says why.

## 2026-10-28

**Added**
- Terminal: the cursor is drawn, the terminal fits above the keyboard, and taps and drags work in programs that use the mouse.
- A keys row above the keyboard: Esc, Tab, Ctrl, Alt and the arrows. Ctrl and Alt apply to the next key you press.
- Shell panes are listed and open straight into the terminal.
- A Remotes section in Settings lists the flankr computers you have paired. Add another with +, remove one with its X after a confirmation, or use "Unpair this device".
- Each remote's details show its nickname and fingerprint, and the nickname can be edited.
- Six-word pairing: the computer shows six words, and you confirm they match. Details shows the full fingerprint, the address and the time left, with a device-code fallback.
- Pairing by QR code: tap to scan, pinch to zoom, and refusals show their reason.
- Pairing shows its progress: "Code read", the words, then "Paired with ..." until you tap Done. A failure stays on screen with its reason.
- A session drawer with cards for each remote and a row for each session, with a spinner for busy ones. You can switch between remotes from it.
- Plugins: a Plugins section in Settings, a menu (⋮) on the overview, agent view and Settings, title-bar icons for plugins, and a planning popup that lists plans.

**Changed**
- Typing over an armed terminal shows only a coloured frame, with no countdown.
- Typing in the terminal no longer times out.
- Terminal input notices are no longer spoken.
- The pairing screen has no tap-to-listen. A pairing link opened while already paired opens the add mode.
- The pairing screen is quieter: its placeholder says only to scan the QR code, and there is no status line while idle.
- Two computers with the same address are told apart by the first part of their fingerprint.

## 2026-10-29

**Added**
- A speaker button that mutes and unmutes. The mute is kept across restarts. While muted, nothing is spoken, and a slash shows through the speaker.
- A 2-second press on the speaker opens its menu, with speak options.
- Tap the speaker while it is speaking to skip. Hold it for 1 second to stop and clear what is queued, without muting.
- A volume menu with speak switches, and a text row with + and a keyboard toggle in the bottom bar.
- A "Speak Gemini replies" setting. When off, a Gemini reply is shown but not spoken.
- You can send images into a session. You can open read-only files and git output on the phone, with previews of Markdown, text and diffs. Paths and commit links in cards can be tapped, and "Show uncommitted changes" is available. Paste sends text from the clipboard into the terminal.
- Choice questions: a live question card offers the agent's choices, with details, multi-select, an Other option, a "1 of N" count and a review, and results.
- Subagents: a ⊙ menu lists running and finished subagents. You can open one from the flow, and the overview counts them. "All agents…" opens the full list.
- The planning popup has a plan table, and "All plans…" lists every plan.
- A microphone on the text cards: a tap goes on by voice and keeps the text you typed.

**Changed**
- The centre and right buttons open their card typed on a 500 ms hold. The volume button and + keep the 2-second hold.

## 2026-10-30

**Changed**
- The session list shows at once on the overview and in the drawer, online or not. "Looking for agents" and "Connecting" lines are gone, and switching remotes shows that remote's last list.
- A terminal error shows as a small "Terminal error" pill for 2 seconds, not as the error text across the screen.

**Fixed**
- Dragging across the drawer's session rows no longer opens a session. Only a tap does.
- The overview's flick follows your finger's speed, so a drag stays where you leave it and the grid no longer runs to the top or the end.
- Nothing is spoken over voice input. Speech waits, and plays once the voice input ends.

## 0.2.0 — 2026-10-30

**Changed**
- After you finish speaking, replies that came in while you talked are no longer read one by one: you hear one short update on where things stand now, in light of what you just said. If no update can be made within a few seconds, the replies are read as before.
- Spoken summaries are shorter: they leave out which files changed, branches, log entries and test counts, and keep what was done, what was found and any question for you.

## 0.2.1 — 2026-10-30

**Fixed**
- The record button works again after a reply with a dash, an ellipsis, an arrow or a tick mark is read aloud. Reading such a reply on the phone stopped its link to the computer: nothing more was said, pulling to refresh stayed on refreshing, and taps on record did nothing.

## 0.3.0 — 2026-10-30

**Added**
- After you speak, the update on where things stand also shows as a pinned card, "While you spoke", even when muted. Tap its ✕ to dismiss it; the next update replaces it.
- In a session the card sits above the buttons, where the pinned card goes. A waiting question, approval or choice comes first, and the card shows once you answer it.

## 0.3.1 — 2026-10-30

**Fixed**
- Replies are spoken and summarized on their cards again.

## 0.4.0 — 2026-10-30

**Added**
- Drag the card above the buttons left or right to put it away; let go early and it springs back. Scrolling up and down still scrolls the session.
- A Finished card dragged away stays as one line, "Finished · " and the reply's first line; tap the line to see it whole. Its reading aloud is dropped.
- Something broke and other spoken cards dragged away are gone, with what was waiting to be said about them.
- An approval dragged away is denied, and a yes or no question is answered No; a dim line in the session says so. A question about a plan is not answered: it stays as one line.
- The "While you spoke" card can be dragged away too, which also stops its update being read.

**Changed**
- A tap on the card above the buttons no longer folds or expands it. A long card shows More at its bottom right, and Less once open; only those change its size.

**Fixed**
- Links in that card's text open when tapped.

## 0.5.0 — 2026-10-30

**Added**
- Settings has *At startup*, beside *Tab follows focus*: *Overview* (as before) or *Last session*. With *Last session* the app opens on the session you last had open, on the remote you last used; if that session is gone, on the most recent session of another remote, and otherwise on the overview.
- The session you return to shows at once as you left it, scrolling and links working, with "refreshing…" at the bottom until it has caught up.
- A session that no longer exists when you come back to it says "This session has ended", with a button back to the overview.

**Changed**
- Going back from a session to the overview makes the overview what opens next time, with *Last session* on.

## 0.6.0 — 2026-10-30

**Added**
- A question with options from Claude can be dragged away like the other cards: it is dismissed as pressing Esc at the Mac would, and a dim line in the session says "Dismissed the question". If it can't be dismissed, the card springs back and says "Couldn't dismiss the question".

## 0.7.0 — 2026-10-30

**Added**
- While Gemini works on an answer, its card shows a small spinner and "Thinking…", and the centre button spins until the answer is done; tapping it still stops Gemini.
- While Gemini looks something up, the card says what, such as "Looking at recent commits…", and while it waits for a coding agent's answer to a question it asked, "Asking Claude…". None of this is spoken.

## 0.7.1 — 2026-10-30

**Fixed**
- When Gemini asks to send something to a coding agent, its approval card now also shows in that agent's session, with Approve and Deny; before, it could be missing there after the server restarted. For another agent, its card shows above the session too.
- The approval question and the card say exactly what will be sent, such as "Approve sending "What did we ship today?" to Claude?".
- A message you dictate to an agent while Gemini waits for your approval no longer answers Gemini's question: it stays on its card until you tap Approve or Deny, or answer Gemini by voice with the centre button ("approve" or "deny").
- The centre button no longer cancels Gemini's waiting question; a message typed to Gemini meanwhile goes once the question is answered.
- On the overview, the message you are dictating shows beside an approval card instead of being hidden by it.

**Changed**
- If Gemini's request is denied or not answered in time, Gemini's card says so briefly, such as "Not sent: denied."

## 0.7.2 — 2026-11-02

**Fixed**
- Pairing another remote with **+**, and pairing the first one, opens the camera at once, with no tap on *Tap to scan*. If the Camera permission isn't allowed, including after an "Only this time" allowance has run out, Android asks for it each time the screen opens.
- If the Camera permission is refused, the screen says *Camera is off for flankr*: *Tap to scan* asks again, and the pairing link can still be pasted. When Android won't ask again, *Open Settings* goes to flankr's page in Settings, and the camera opens as soon as you come back with it allowed.

## 0.7.3 — 2026-11-02

**Fixed**
- Pairing another remote with **+** in the drawer, opened over a session, shows the pairing screen with its camera and the paste field, instead of a blank screen.

## 0.7.4 — 2026-11-03

**Fixed**
- A pairing code whose only address is the computer itself (127.0.0.1 or localhost) no longer offers this phone's code to paste, which could never connect. The screen says the code only works on that computer, and to set up Tailscale there or run flankr with `--vpn none --bind 0.0.0.0:7347`, then pair again.
- A pairing code that lists the computer itself beside other addresses skips the local one and uses the others.
- When none of a pairing code's addresses can be reached, the screen says *Can't reach* the computer and to check that the phone is on the same Tailscale network or Wi-Fi, instead of offering this phone's code, which would need the same addresses.

## 0.7.5 — 2026-11-03

**Changed**
- Every press-and-hold now takes half a second: a remote's card in the drawer, the centre and record buttons, the speaker button, the **+** on a text card, and a pinned plugin card. Before, most took two seconds.
- Holding a remote's card opens its details, the same screen as **Details** after pairing: nickname, hostname, when it was paired, the fingerprint, Close and Remove.
- **Details** after pairing opens that remote's details. Closing them goes where **Done** goes. The button shows once the phone has the new remote's entry.
- While you pair, the fingerprint under **Details** is a *Fingerprint* row that opens and closes the same way as in a remote's details, with the same six words and BLAKE2s-256 digits.
- Holding the speaker button opens its menu. While something is being read aloud, the menu starts with **Stop**, which ends the reply and clears what is queued without muting. Tapping the speaker still skips the current reply while something is read, and mutes or unmutes otherwise.

**Removed**
- The progress bars that filled along a held button or card.
- Holding the speaker for one second to stop reading. Use **Stop** in its menu instead.

## 0.7.6 — 2026-11-03

**Changed**
- When pairing completes, the pairing box turns straight into the new remote's details: nickname, hostname, when it was paired, the fingerprint, Close and Remove. A "Paired with <name>" message shows at the top for three seconds, then fades. It shows only then, not when you open a remote's details later.
- Close on those details goes where Done went: back to Settings if you paired from there, otherwise to the main screen.

**Removed**
- The separate "Paired with <name>" screen with its Details and Done buttons, and its "Its addresses were updated." line.

## 0.7.7 — 2026-11-03

**Added**
- A card you are speaking into, a message or Gemini's, has a keyboard button at its top right, beside the arrow. Tap it to carry on typing: what you said so far stays as the text, the cursor at its end and the keyboard up, and the microphone stops listening.

**Fixed**
- A card that arrives while you type no longer closes or empties the text card you are typing in. When Gemini finishes a reply, a prompt you had started typing to it stays, with its text.
- A typed message is sent only by its send button. Before, if the microphone was still listening from a spoken start, something it heard could be added to the text and the message sent after the silence.
- Tapping a card's record button while a typed message is open carries that message on by voice with its text, instead of starting an empty one.
- When the keyboard does not come up after opening a text card, the app asks for it once more, so you no longer have to tap the text first.

## 0.7.8 — 2026-11-03

**Added**
- In the terminal view, tapping one of Claude Code's question options, its name or the line under it, picks that option, as pressing its number would. The keyboard does not come up.
- Shell panes (a plain zsh or bash tab, htop, a build) and agents the Mac's settings don't list now open in the terminal view, and you can type into them. They are listed in the drawer below the agent sessions.

**Changed**
- The keyboard comes up only when you tap the agent's input line (Claude Code's prompt box, Codex's or agy's prompt, a shell's prompt line). A tap elsewhere in the terminal no longer brings it up, so the screen no longer shifts under your finger.
- The row of Esc, Tab, arrow, CTRL and ALT keys is always there in the terminal view while you can type into it, not only after a tap. The first key you press, type or paste starts typing.
- With a program that takes the mouse, a tap still clicks where you tap; the keyboard comes up only if the tap is on the input line.

## 0.7.9 — 2026-11-03

**Added**
- Holding the record button on a question card (one with Approve, Discuss or the agent's own answers) for half a second opens your reply typed: the card's question stays above the text, the keyboard comes up, and the microphone does not listen. A quick tap still answers by voice.
- A card you are speaking into also has its keyboard button when it is expanded to the full sheet, at the right of its title.

**Changed**
- A drag that starts on a question card's record button no longer scrolls the conversation; start the scroll beside it.

## 0.7.10 — 2026-11-03

**Fixed**
- A drag that starts on a question card's record button scrolls the conversation again, from where your finger is once it moves, with no jump. Holding the button still opens your reply typed, and a quick tap still answers by voice.

## 0.7.11 — 2026-11-03

**Added**
- The drawer groups your sessions under their herdr workspace, in herdr's order: a small header with the workspace's name and how many sessions it has, then its agent sessions by tab, then its shell panes. A workspace with only one session has no header; that session shows its full name, such as "Claude in libmorph, tab -mb".
- Tap a workspace's header to fold its sessions away, and again to show them. The phone remembers which are folded for each remote, after a restart too. A folded workspace shows a dot while one of its sessions has a question or approval waiting, and opens by itself when one starts waiting or when you open one of its sessions from elsewhere.
- A new setting, Drawer sessions, under At startup: Grouped by workspace (the default) or Not grouped, which keeps the single list as before.

## 0.7.12 — 2026-11-03

**Fixed**
- More and Less are always at the card's bottom right, on its last line, folded or open. On a card with buttons they now sit below the buttons, with room of their own, so tapping them no longer answers or starts a voice reply by mistake.
- Less on an open Finished card with spoken text is at the card's bottom right again, not in the middle beside "spoken", and the replay button stays where it was.

## 0.7.13 — 2026-11-03

**Fixed**
- A passing test run no longer shows as Something broke.

## 0.7.14 — 2026-11-03

**Changed**
- The terminal's cursor is an underline that blinks, whatever shape the program asks for. It shows again at once when it moves or you type, and stays hidden when the program hides it or the desktop has the tab.

## 0.7.15 — 2026-11-03

**Added**
- The terminal's keys are two rows: + ESC TAB, a wide ⏎ Enter, ↑ and ⛶ on top; CTRL ALT, ← ↓ → and the keyboard key below. Enter sends Enter into the terminal, with CTRL or ALT if you latched them.
- ⛶ hides everything of flankr's over the terminal (the header, the bottom buttons and the cards), so the terminal takes the whole screen above the keys. Tap ⛶ again or press Back to bring them back. It stays that way as you move between sessions until you change it, and starts shown after a restart. While hidden, a dot on ⛶ says an approval or a question came in.

**Fixed**
- The + in the terminal's keys works: it opens a menu with Paste text, which types the clipboard's text into the terminal.

## 0.7.16 — 2026-11-03

**Fixed**
- A session continued after a long conversation keeps updating in the list view.

## 0.7.17 — 2026-11-03

**Fixed**
- The keyboard's Enter key now sends Enter in the terminal.
- The keyboard's Done key saves a remote's nickname.
- The keyboard's Enter key starts a new line in a prompt.

## 0.7.18 — 2026-11-03

**Fixed**
- A tap on a Claude Code session's input line in terminal view raises the keyboard again, also in a session with a name. The whole box around the prompt takes the tap, so it is easier to hit.
- The keyboard no longer goes down again just after it came up in the terminal; it is asked for once more.

## 0.7.19 — 2026-11-03

**Changed**
- In terminal view the agent's own screen is where you answer it. When Claude asks a question, a one-line notice above the terminal says so ("Claude is asking — answer in the terminal") in place of the question card; it goes once the question is answered.
- Over the terminal, only a Finished card's line and an approval flankr needs from you still show. Something broke, *While you spoke* and the message you just sent no longer show over the terminal; the terminal shows the message itself. Switch to the list view to see them.

## 0.7.20 — 2026-11-11

**Added**
- While Claude is thinking, the session shows a spinner and Thinking… at its end.
