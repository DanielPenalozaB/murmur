//! Small facts read from notes Markdown for library cards.

/// The first summary bullet (or first paragraph) as plain text, for a card preview.
pub fn excerpt(notes: &str) -> String {
    let summary = section(notes, "summary").unwrap_or(notes);
    let line = summary
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .unwrap_or_default();
    plain(line)
}

/// (open, done) task-list items, e.g. `- [ ] Ana: send the plan`.
pub fn count_actions(notes: &str) -> (u32, u32) {
    let mut open = 0;
    let mut done = 0;
    for line in notes.lines().map(str::trim_start) {
        let item = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "));
        match item.map(|i| i.get(..3)) {
            Some(Some("[ ]")) => open += 1,
            Some(Some("[x]" | "[X]")) => done += 1,
            _ => {}
        }
    }
    (open, done)
}

/// The body of the `## <name>` section (case-insensitive), up to the next `## `.
fn section<'a>(notes: &'a str, name: &str) -> Option<&'a str> {
    let mut offset = 0;
    for line in notes.split_inclusive('\n') {
        offset += line.len();
        if let Some(title) = line.trim().strip_prefix("## ")
            && title.trim().eq_ignore_ascii_case(name)
        {
            let rest = &notes[offset..];
            let end = rest.find("\n## ").map_or(rest.len(), |i| i + 1);
            return Some(&rest[..end]);
        }
    }
    None
}

/// Strips list markers, emphasis and `[mm:ss]` citations.
fn plain(line: &str) -> String {
    let line = line.trim_start_matches(['-', '*', ' ']);
    let line = line
        .strip_prefix("[ ] ")
        .or_else(|| line.strip_prefix("[x] "))
        .unwrap_or(line);
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find(']') {
            Some(close)
                if after[..close]
                    .chars()
                    .all(|c| c.is_ascii_digit() || ":,–- ".contains(c)) =>
            {
                // Drop the space before the citation too: "Friday [12:04]." → "Friday."
                out.truncate(out.trim_end().len());
                rest = &after[close + 1..];
            }
            _ => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "")
        .replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTES: &str = "## Summary\n- **Launch** moves to Friday [12:04, 12:30].\n- Second point\n\n## Action items\n- [ ] Ana: send the plan [03:10]\n- [x] Bo: book the room\n  - [ ] nested follow-up\n\n## My notes\n\nship it";

    #[test]
    fn excerpt_is_the_first_summary_bullet_in_plain_text() {
        assert_eq!(excerpt(NOTES), "Launch moves to Friday.");
        assert_eq!(excerpt("Just a line [00:01]"), "Just a line");
        assert_eq!(excerpt(""), "");
        assert_eq!(excerpt("- see [docs] here"), "see [docs] here");
    }

    #[test]
    fn counts_open_and_done_actions() {
        assert_eq!(count_actions(NOTES), (2, 1));
        assert_eq!(count_actions("no tasks"), (0, 0));
    }
}
