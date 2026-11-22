//! Splits window titles into `subject` (the thing you're on: file, chat, page, command) and
//! `context` (where: project, account, site, host), driven by the `title_rules` table.

use regex::Regex;

/// A compiled `title_rules` row.
pub struct Rule {
    pub bundle_id: String,
    pub pattern: String,
    pub priority: i16,
    regex: Regex,
}

impl Rule {
    pub fn new(bundle_id: &str, pattern: &str, priority: i16) -> Result<Self, regex::Error> {
        Ok(Self { bundle_id: bundle_id.into(), pattern: pattern.into(), priority, regex: Regex::new(pattern)? })
    }
}

/// Rules written into an empty `title_rules` on first start, for the apps seen so far.
/// They are data from then on: edit the table, not this list.
pub const SEED: &[(&str, &str)] = &[
    // dotclaude – mac_usage [default@10.168.0.32] → project, file/tab (the [connection] dropped)
    ("com.jetbrains.intellij", r"^(?P<context>.+?) – (?P<subject>.+?)(?: \[[^\]]*\])?$"),
    // root — [coolify] lazydocker ~ — ssh root@10.168.0.77 → user — tab/dir — command
    ("com.apple.Terminal", r"^[^—]+ — (?P<context>.+?) — (?P<subject>[^—]+)$"),
    // Прочиточная @ central hardware → chat, account
    ("com.tdesktop.Telegram", r"^(?P<subject>.+?) @ (?P<context>.+)$"),
    ("ru.keepcoder.Telegram", r"^(?P<subject>.+?) @ (?P<context>.+)$"),
    // <page> - YouTube → page, site (the last " - " part)
    ("com.apple.Safari", r"^(?P<subject>.+) - (?P<context>[^-]+)$"),
    ("com.google.Chrome", r"^(?P<subject>.+) - (?P<context>[^-]+)$"),
    // processed — iCloud – 0 messages → mailbox, account
    ("com.apple.mail", r"^(?P<subject>.+?) — (?P<context>.+?)(?: – \d+ messages?)?$"),
];

/// The title without invisible marks (bidi isolates, zero-width), a trailing unread counter (1–3 digits)
/// "(3)" or a terminal size "— 207×59", so one chat or file doesn't split into several titles.
pub fn clean(title: &str) -> String {
    let visible: String = title
        .chars()
        .filter(|c| !matches!(c, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'))
        .collect();
    let mut t = visible.trim().to_owned();
    loop {
        let before = t.len();
        if let Some(rest) = t.strip_suffix(')')
            && let Some((head, n)) = rest.rsplit_once(" (")
            && (1..=3).contains(&n.len())
            && n.chars().all(|c| c.is_ascii_digit())
        {
            t = head.trim_end().to_owned();
        }
        if let Some((head, size)) = t.rsplit_once(" — ")
            && let Some((w, h)) = size.split_once('×')
            && !w.is_empty() && !h.is_empty()
            && w.chars().chain(h.chars()).all(|c| c.is_ascii_digit())
        {
            t = head.trim_end().to_owned();
        }
        if t.len() == before {
            return t;
        }
    }
}

/// Generic split for apps without a rule: on the first separator kind present,
/// first part = subject, last part = context.
fn split(title: &str) -> (String, String) {
    for sep in [" — ", " – ", " | ", " - ", " @ "] {
        let parts: Vec<&str> = title.split(sep).map(str::trim).filter(|p| !p.is_empty()).collect();
        if parts.len() > 1 {
            return (parts[0].to_owned(), parts[parts.len() - 1].to_owned());
        }
    }
    (title.to_owned(), String::new())
}

/// (subject, context) of a title. `rules` must be sorted by priority.
pub fn parse(rules: &[Rule], bundle_id: &str, title: &str) -> (String, String) {
    let t = clean(title);
    if t.is_empty() {
        return Default::default();
    }
    for r in rules.iter().filter(|r| r.bundle_id.is_empty() || r.bundle_id == bundle_id) {
        if let Some(c) = r.regex.captures(&t) {
            let get = |n| c.name(n).map(|m| m.as_str().trim().to_owned()).unwrap_or_default();
            return (get("subject"), get("context"));
        }
    }
    split(&t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> Vec<Rule> {
        SEED.iter().map(|(b, p)| Rule::new(b, p, 100).unwrap()).collect()
    }

    fn p(bundle: &str, title: &str) -> (String, String) {
        parse(&seed(), bundle, title)
    }

    fn pair(s: &str, c: &str) -> (String, String) {
        (s.into(), c.into())
    }

    #[test]
    fn cleans() {
        assert_eq!(clean("\u{200e}\u{2068}🤖 bot\u{2069} @ \u{200e}\u{2068}central hardware\u{2069} (1)"), "🤖 bot @ central hardware");
        assert_eq!(clean("alex — ~ — -fish — 207×59"), "alex — ~ — -fish");
        assert_eq!(clean("Episode (2024)"), "Episode (2024)");
    }

    #[test]
    fn seeded_apps() {
        assert_eq!(p("com.jetbrains.intellij", "dotclaude – mac_usage [default@10.168.0.32]"), pair("mac_usage", "dotclaude"));
        assert_eq!(p("com.apple.Terminal", "root — [coolify] lazydocker ~ — ssh root@10.168.0.77 — 207×59"), pair("ssh root@10.168.0.77", "[coolify] lazydocker ~"));
        assert_eq!(p("com.tdesktop.Telegram", "\u{200e}\u{2068}Прочиточная\u{2069} @ \u{200e}\u{2068}central hardware\u{2069} (1)"), pair("Прочиточная", "central hardware"));
        assert_eq!(p("com.apple.Safari", "PROPERTY REVIEW #391 | SUNWAY BELFIELD, KUALA LUMPUR - YouTube"), pair("PROPERTY REVIEW #391 | SUNWAY BELFIELD, KUALA LUMPUR", "YouTube"));
        assert_eq!(p("com.apple.mail", "processed — iCloud – 0 messages"), pair("processed", "iCloud"));
    }

    #[test]
    fn falls_back() {
        assert_eq!(p("com.apple.ActivityMonitor", "Activity Monitor – All Processes"), pair("Activity Monitor", "All Processes"));
        assert_eq!(p("com.apple.Safari", "centralhardware/usageStatServer"), pair("centralhardware/usageStatServer", ""));
        assert_eq!(p("x", ""), pair("", ""));
    }
}
