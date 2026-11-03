//! Typed keys as `terminal.input`'s `keys` (plan 20261027_07, decision 6,
//! step 6's app half).
//!
//! The daemon reads `keys` as whitespace-separated tokens: a named key
//! (`enter`, `escape`, `tab`, `backspace`, `space`, `up`, `down`, `left`,
//! `right`), `ctrl+<a-z>`, or printable text, one key per character. A
//! literal space is the `space` token, since whitespace separates them.
//! The app sends every printable character as its own one-character token,
//! so a typed word such as "tab" is never read as the Tab key. herdr's
//! `pane.send_keys` takes the same tokens, one key each.

use libmorph::Key;

/// The token for one typed character, or `None` for a control character
/// the daemon does not take.
pub fn char_token(c: char) -> Option<String> {
    let token = match c {
        ' ' => "space".to_string(),
        '\r' | '\n' => "enter".to_string(),
        '\t' => "tab".to_string(),
        '\u{8}' | '\u{7f}' => "backspace".to_string(),
        '\u{1b}' => "escape".to_string(),
        // Ctrl+A to Ctrl+Z, but for those above (Ctrl+H, Ctrl+I, Ctrl+J
        // and Ctrl+M are Backspace, Tab and Enter).
        '\u{1}'..='\u{1a}' => format!("ctrl+{}", (b'a' + c as u8 - 1) as char),
        c if c.is_control() => return None,
        c if c.is_whitespace() => "space".to_string(),
        c => c.to_string(),
    };
    Some(token)
}

/// The tokens for `text`, joined with single spaces; characters with no
/// token are left out.
pub fn text_keys(text: &str) -> String {
    text.chars()
        .filter_map(char_token)
        .collect::<Vec<_>>()
        .join(" ")
}

/// A physical key's token: a character's, or Escape's. `None` for the
/// system Back and for what the daemon does not take.
pub fn key_token(key: Key) -> Option<String> {
    match key {
        Key::Character(c) => char_token(c),
        Key::Escape => Some("escape".to_string()),
        _ => None,
    }
}

/// The keys that turn the soft keyboard's text `old` into `new`: a
/// Backspace for each character after their common start, then the new
/// characters. The soft keyboard reports the whole text after each edit
/// (a composition or autocorrect replaces a word), so this is how its
/// edits reach the pane.
pub fn edit_keys(old: &str, new: &str) -> String {
    let common = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let removed = old.chars().count() - common;
    let added: String = new.chars().skip(common).collect();
    let mut tokens: Vec<String> = std::iter::repeat_n("backspace".to_string(), removed).collect();
    let added = text_keys(&added);
    if !added.is_empty() {
        tokens.push(added);
    }
    tokens.join(" ")
}

/// The soft keyboard's `event` while it types into the armed terminal,
/// whose field held `old`: the keys to send and the field's text after
/// it, or `None` for an event that sends nothing (the dismissal, which the
/// app handles itself). The action key, whatever it shows (Done, Go,
/// Send or Enter), is Enter, and the keyboard stays up for it; a newline
/// in the text is Enter too (plan 20261103_01, the 2026-11-03 15:36 EST
/// amendment, item 1).
pub fn ime_keys(old: &str, event: &libmorph::TextInputEvent) -> Option<(String, String)> {
    match event {
        libmorph::TextInputEvent::Changed(text) => Some((edit_keys(old, text), text.clone())),
        libmorph::TextInputEvent::Action(_) => Some(("enter".to_string(), old.to_string())),
        _ => None,
    }
}

/// The soft keyboard asked for while it types into the terminal: one
/// line, its action key Enter (no Done or Send), no autocorrect or
/// suggestions, so what is typed reaches the pane as typed (as terminal
/// apps such as Termux ask). The prompts' and fields' keyboards are not
/// these.
pub fn terminal_text_options() -> libmorph::TextInputOptions {
    let mut options = libmorph::TextInputOptions::default();
    options.multiline = false;
    options.action = libmorph::TextAction::None;
    options.autocorrect = false;
    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_character_is_its_own_token_and_a_space_is_space() {
        assert_eq!(text_keys("ls -la"), "l s space - l a");
        // A typed word is letters, never the named key.
        assert_eq!(text_keys("tab"), "t a b");
        assert_eq!(text_keys("a+b/c"), "a + b / c");
        assert_eq!(text_keys("ü"), "ü");
    }

    #[test]
    fn named_keys_and_control_characters() {
        assert_eq!(text_keys("ls\n"), "l s enter");
        assert_eq!(char_token('\r').as_deref(), Some("enter"));
        assert_eq!(char_token('\t').as_deref(), Some("tab"));
        assert_eq!(char_token('\u{8}').as_deref(), Some("backspace"));
        assert_eq!(char_token('\u{7f}').as_deref(), Some("backspace"));
        assert_eq!(char_token('\u{1b}').as_deref(), Some("escape"));
        assert_eq!(char_token('\u{3}').as_deref(), Some("ctrl+c"));
        assert_eq!(char_token('\u{1}').as_deref(), Some("ctrl+a"));
        assert_eq!(char_token('\u{1a}').as_deref(), Some("ctrl+z"));
        // Other control characters are not sent.
        assert_eq!(char_token('\u{0}'), None);
        assert_eq!(char_token('\u{1c}'), None);
        assert_eq!(key_token(Key::Escape).as_deref(), Some("escape"));
        assert_eq!(key_token(Key::Character('x')).as_deref(), Some("x"));
        assert_eq!(key_token(Key::Back), None);
    }

    #[test]
    fn the_soft_keyboards_edits_become_keys() {
        assert_eq!(edit_keys("", "l"), "l");
        assert_eq!(edit_keys("l", "ls"), "s");
        assert_eq!(edit_keys("ls", "ls\n"), "enter");
        assert_eq!(edit_keys("ls", "l"), "backspace");
        // Autocorrect replaces a word: back to where they differ, retyped.
        assert_eq!(
            edit_keys("git sttus", "git status"),
            "backspace backspace backspace a t u s"
        );
        assert_eq!(edit_keys("same", "same"), "");
    }

    #[test]
    fn the_action_key_is_enter_and_the_text_stays() {
        use libmorph::{TextAction, TextInputEvent};
        // Whatever the key shows, it is Enter, and the field's text is
        // kept, so the next edit diffs against it.
        for action in [
            TextAction::Done,
            TextAction::Go,
            TextAction::Send,
            TextAction::Search,
            TextAction::None,
        ] {
            assert_eq!(
                ime_keys("ls", &TextInputEvent::Action(action)),
                Some(("enter".to_string(), "ls".to_string())),
                "{action:?}"
            );
        }
        // A newline typed into the text is Enter as well.
        assert_eq!(
            ime_keys("ls", &TextInputEvent::Changed("ls -la\n".into())),
            Some(("space - l a enter".to_string(), "ls -la\n".to_string()))
        );
        // The dismissal sends nothing.
        assert_eq!(ime_keys("ls", &TextInputEvent::Dismissed), None);
    }

    #[test]
    fn the_terminals_keyboard_is_one_line_with_enter_and_no_suggestions() {
        let o = terminal_text_options();
        assert!(!o.multiline);
        assert_eq!(o.action, libmorph::TextAction::None);
        assert!(!o.autocorrect);
        assert_eq!(o.kind, libmorph::TextKind::Text);
    }
}
