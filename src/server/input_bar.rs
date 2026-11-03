//! The agent's input bar in a terminal frame
//! (docs/design/20261103_01_TERMINAL_TAPS_AND_SHELL_PANES_PLAN.md,
//! decision 3 and step 1).
//!
//! Per frame, the daemon finds the rows of the pane's input bar from the
//! pane's agent kind, and sends them with the frame
//! ([`libflankr_term::Grid::input_rows`]) to a phone that asked; a tap on
//! them raises the soft keyboard (decision 4).
//!
//! - **Claude Code** (`claude`): the rows between the two full-width
//!   horizontal rules (`─`) nearest the bottom that enclose a line starting
//!   `>` or `❯` after optional box characters.
//! - **Codex** (`codex`): the rows from the last line starting `›` to the
//!   next rule or the frame's bottom.
//! - **agy** (`agy`): the rows from its last prompt line (`>` after
//!   optional box characters) to the next rule or the frame's bottom.
//! - **Anything else**, shells included: the cursor's row, when the cursor
//!   is visible and in the bottom third of the frame.
//!
//! A frame with none of these has none. A rule is a row whose text,
//! trimmed, is `─` from end to end (a box corner or tee at either end
//! allowed), at least [`RULE_MIN`] of them and at least four fifths of the
//! frame's width: a short `────` inside an answer is not one.

use libflankr_term::{Grid, RowSpan};

/// The fewest `─` a rule has.
pub const RULE_MIN: usize = 10;

/// Characters a line may start with before its prompt: box sides and
/// spaces.
fn is_box(c: char) -> bool {
    c.is_whitespace() || ('\u{2500}'..='\u{257f}').contains(&c)
}

/// `line` with its leading box characters and spaces gone.
fn after_box(line: &str) -> &str {
    line.trim_start_matches(is_box)
}

/// A box corner or tee that may end a rule.
fn is_corner(c: char) -> bool {
    matches!(
        c,
        '╭' | '╮' | '╰' | '╯' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼'
    )
}

/// Whether `line` is a full-width rule in a frame `cols` wide.
fn is_rule(line: &str, cols: u16) -> bool {
    let t = line.trim();
    let t = t.strip_prefix(is_corner).unwrap_or(t);
    let t = t.strip_suffix(is_corner).unwrap_or(t);
    let n = t.chars().count();
    n >= RULE_MIN && n * 5 >= cols as usize * 4 && t.chars().all(|c| c == '─')
}

/// The span `first..=last` when it is not empty.
fn span(first: usize, last: usize) -> Option<RowSpan> {
    (first <= last).then(|| RowSpan {
        first: first.min(u16::MAX as usize) as u16,
        last: last.min(u16::MAX as usize) as u16,
    })
}

/// The input bar of the agent named `agent` (lowercased, as the policy has
/// it) in `grid`, or `None` when none is found.
pub fn find(agent: &str, grid: &Grid) -> Option<RowSpan> {
    let lines = grid.text_lines();
    match agent {
        "claude" => claude(&lines, grid.cols),
        "codex" => from_prompt(&lines, grid.cols, |l| l.trim_start().starts_with('›')),
        "agy" => from_prompt(&lines, grid.cols, |l| after_box(l).starts_with('>')),
        _ => cursor_row(grid),
    }
}

/// Whether `rest`, after a pointer, is a numbered option (`1. Amber`).
fn is_option(rest: &str) -> bool {
    let rest = rest.trim_start();
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && rest[digits..].starts_with(". ")
}

/// Claude Code: between the two rules nearest the bottom that enclose a
/// prompt line.
fn claude(lines: &[String], cols: u16) -> Option<RowSpan> {
    let rules: Vec<usize> = (0..lines.len())
        .filter(|&i| is_rule(&lines[i], cols))
        .collect();
    let prompt = |l: &String| {
        let l = after_box(l);
        let Some(rest) = l.strip_prefix('>').or_else(|| l.strip_prefix('❯')) else {
            return false;
        };
        // The question screen's focus pointer (`❯ 1. Amber`) is no prompt.
        !is_option(rest)
    };
    rules
        .windows(2)
        .rev()
        .find(|pair| lines[pair[0] + 1..pair[1]].iter().any(prompt))
        .and_then(|pair| span(pair[0] + 1, pair[1] - 1))
}

/// From the last line that `is_prompt` to the row before the next rule, or
/// the frame's last row.
fn from_prompt(lines: &[String], cols: u16, is_prompt: impl Fn(&str) -> bool) -> Option<RowSpan> {
    let first = lines.iter().rposition(|l| is_prompt(l))?;
    let last = lines[first + 1..]
        .iter()
        .position(|l| is_rule(l, cols))
        .map_or(lines.len() - 1, |i| first + i);
    span(first, last)
}

/// The cursor's row, when it is visible and in the bottom third.
fn cursor_row(grid: &Grid) -> Option<RowSpan> {
    let c = grid.cursor.filter(|c| c.visible && !c.stale)?;
    let rows = grid.rows as usize;
    let row = c.row as usize;
    (row < rows && row * 3 >= rows * 2).then_some(RowSpan {
        first: c.row,
        last: c.row,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use libflankr_term::{Cursor, parse_ansi};

    fn grid(text: &str) -> Grid {
        parse_ansi(text)
    }

    fn rows(first: u16, last: u16) -> Option<RowSpan> {
        Some(RowSpan { first, last })
    }

    const RULE: &str = "────────────────────────────────────────";

    #[test]
    fn claude_code_s_bar_is_between_the_rules_nearest_the_bottom() {
        // Claude Code 2.1's prompt: a rule, `> `, a rule, then its footer.
        let frame = format!(
            "● Done. The build passes.\n\
             \n\
             {RULE}\n\
             > fix the failing test\n\
             {RULE}\n\
             \x20 ? for shortcuts                       ◐ medium\n"
        );
        assert_eq!(find("claude", &grid(&frame)), rows(3, 3));

        // A prompt of two lines, the pointer as ❯, and an earlier pair of
        // rules in the transcript (a table) that encloses no prompt.
        let frame = format!(
            "{RULE}\n\
             │ a table row\n\
             {RULE}\n\
             {RULE}\n\
             ❯ first line of the prompt\n\
             \x20 and its second\n\
             {RULE}\n\
             \x20 ⏵⏵ accept edits on\n"
        );
        assert_eq!(find("claude", &grid(&frame)), rows(4, 5));

        // The older boxed prompt: corners on the rules, sides on the line.
        let boxed =
            format!("text\n╭{RULE}╮\n│ > hello                                │\n╰{RULE}╯\n");
        assert_eq!(find("claude", &grid(&boxed)), rows(2, 2));
    }

    #[test]
    fn a_frame_with_no_prompt_has_no_bar() {
        // Claude Code's question screen: rules, but no `>` line between.
        let question = format!(
            "{RULE}\n\
             \x20Which colour should the badge be?\n\
             \x20❯ 1. Amber\n\
             \x20  2. Teal\n\
             {RULE}\n"
        );
        // Its focus pointer (`❯ 1.`) is not a prompt, so a tap on an
        // option is left to the choice tap. A frame with no rules has none.
        assert_eq!(find("claude", &grid(&question)), None);
        assert_eq!(find("claude", &grid("just\nsome output\n")), None);
        assert_eq!(find("codex", &grid("no prompt here\n")), None);
        assert_eq!(find("agy", &grid("no prompt here\n")), None);
        assert_eq!(find("zsh", &grid("$ ls\n")), None, "no cursor");
        // A short dash run is no rule.
        assert_eq!(find("claude", &grid("─────\n> hi\n─────\n")), None);
    }

    #[test]
    fn codex_s_bar_runs_from_its_last_pointer_to_the_bottom() {
        let frame = "› earlier message, in the transcript\n\
                     • Ran cargo test\n\
                     \n\
                     › write the docs\n\
                     \n\
                     \x20 ⏎ send   ⌃J newline   ⌃T transcript\n";
        assert_eq!(find("codex", &grid(frame)), rows(3, 5));
        // Up to the next rule when one follows.
        let frame = format!("› ask\n  more\n{RULE}\nfooter\n");
        assert_eq!(find("codex", &grid(&frame)), rows(0, 1));
    }

    #[test]
    fn agy_s_bar_runs_from_its_prompt_line() {
        let frame = format!(
            "Thinking...\n\
             {RULE}\n\
             │ > type here\n\
             {RULE}\n\
             \x20 gemini-3 · ctrl+c to quit\n"
        );
        assert_eq!(find("agy", &grid(&frame)), rows(2, 2));
        assert_eq!(find("agy", &grid("> last\nline\n")), rows(0, 1));
    }

    #[test]
    fn a_shell_s_bar_is_the_cursor_row_in_the_bottom_third() {
        let mut g = grid("one\ntwo\nthree\nfour\nfive\n$ ls");
        g.cursor = Some(Cursor {
            col: 4,
            row: 5,
            visible: true,
            ..Cursor::default()
        });
        assert_eq!(find("shell", &g), rows(5, 5));
        // An agent with no recogniser goes by the cursor too.
        assert_eq!(find("omp", &g), rows(5, 5));
        // Hidden, stale or high up: none.
        let mut hidden = g.clone();
        hidden.cursor.as_mut().unwrap().visible = false;
        assert_eq!(find("shell", &hidden), None);
        let mut stale = g.clone();
        stale.cursor.as_mut().unwrap().stale = true;
        assert_eq!(find("shell", &stale), None);
        let mut high = g;
        high.cursor.as_mut().unwrap().row = 1;
        assert_eq!(find("shell", &high), None);
    }
}
