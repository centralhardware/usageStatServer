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

/// What a title says: `subject`, `context`, and any other named groups of the matching rule.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub subject: String,
    pub context: String,
    pub fields: Vec<(String, String)>,
}

/// Parses a title. `rules` must be sorted by priority.
pub fn parse(rules: &[Rule], bundle_id: &str, title: &str) -> Parsed {
    let t = clean(title);
    if t.is_empty() {
        return Parsed::default();
    }
    for r in rules.iter().filter(|r| r.bundle_id.is_empty() || r.bundle_id == bundle_id) {
        if let Some(c) = r.regex.captures(&t) {
            let mut p = Parsed::default();
            for name in r.regex.capture_names().flatten() {
                let Some(m) = c.name(name) else { continue };
                let v = m.as_str().trim().to_owned();
                match name {
                    "subject" => p.subject = v,
                    "context" => p.context = v,
                    _ if !v.is_empty() => p.fields.push((name.to_owned(), v)),
                    _ => {}
                }
            }
            return p;
        }
    }
    let (subject, context) = split(&t);
    Parsed { subject, context, fields: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rules V11 inserts, read from the migration itself: ('bundle', 'pattern') tuples with
    /// ClickHouse string escaping (\\ → \\ in the regex).
    fn seed() -> Vec<Rule> {
        let sql = include_str!("../migrations/V11__reseed_title_rules.sql");
        let rules: Vec<Rule> = sql
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("('"))
            .map(|l| {
                let l = l.trim_end_matches([',', ';']).trim_start_matches("('").trim_end_matches("')");
                let (b, rest) = l.split_once("', '").unwrap();
                let (p, prio) = rest.rsplit_once("', ").unwrap();
                Rule::new(b, &p.replace("\\\\", "\\"), prio.trim_end_matches(')').parse().unwrap()).unwrap()
            })
            .collect();
        assert_eq!(rules.len(), 14);
        let mut rules = rules;
        rules.sort_by_key(|r| r.priority);
        rules
    }

    fn f(bundle: &str, title: &str) -> Vec<(String, String)> {
        parse(&seed(), bundle, title).fields
    }

    fn kv(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect()
    }

    fn p(bundle: &str, title: &str) -> (String, String) {
        let r = parse(&seed(), bundle, title);
        (r.subject, r.context)
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
    fn seeded_fields() {
        const T: &str = "com.apple.Terminal";
        const S: &str = "com.apple.Safari";
        assert_eq!(f("com.jetbrains.intellij", "dotclaude – mac_usage [default@10.168.0.32]"), kv(&[("db_user", "default"), ("db_host", "10.168.0.32")]));
        assert_eq!(f("com.jetbrains.intellij", "dotclaude – Main.kt"), kv(&[]));
        assert_eq!(f(T, "root — [coolify] lazydocker ~ — ssh root@10.168.0.77 — 207×59"), kv(&[("user", "root"), ("command", "ssh"), ("ssh_user", "root"), ("ssh_host", "10.168.0.77")]));
        assert_eq!(
            p(T, "alex — claudesrv ~ — ssh -t root@10.168.0.55 su - claude -c 'tmux new-session -A -s claude' — 207×59"),
            pair("ssh -t root@10.168.0.55 su - claude -c 'tmux new-session -A -s claude'", "claudesrv ~")
        );
        assert_eq!(f(T, "alex — claudesrv ~ — ssh -t root@10.168.0.55 su - claude"), kv(&[("user", "alex"), ("command", "ssh"), ("ssh_user", "root"), ("ssh_host", "10.168.0.55")]));
        assert_eq!(f(T, "alex — ~ — -fish — 207×59"), kv(&[("user", "alex"), ("command", "fish")]));
        assert_eq!(f(T, "alex — brew update && brew  ~ — curl ◂ bash -p /opt/homebrew/Library/Homebrew/brew.sh update — 207×59"), kv(&[("user", "alex"), ("command", "curl")]));
        let pr = "agent: sample the front app every second by centralhardware · Pull Request #9 · centralhardware/usageStatServer";
        assert_eq!(p(S, pr), pair("agent: sample the front app every second", "centralhardware/usageStatServer"));
        assert_eq!(f(S, pr), kv(&[("author", "centralhardware"), ("kind", "Pull Request"), ("number", "9")]));
        let commit = "agent: media source sticks to the tab that started playing · centralhardware/usageStatServer@2b5d60b";
        assert_eq!(p(S, commit), pair("agent: media source sticks to the tab that started playing", "centralhardware/usageStatServer"));
        assert_eq!(f(S, commit), kv(&[("commit", "2b5d60b")]));
        assert_eq!(p(S, "Commits · centralhardware/usageStatServer"), pair("Commits", "centralhardware/usageStatServer"));
        assert_eq!(p(S, "Виброс — контактный микрофон | Pangear"), pair("Виброс — контактный микрофон", "Pangear"));
        assert_eq!(p(S, "track you mac usage - Google Search"), pair("track you mac usage", "Google Search"));
        assert_eq!(p(S, "Tool to track computer/app usage? : r/macsysadmin"), pair("Tool to track computer/app usage?", "r/macsysadmin"));
        assert_eq!(p(S, "Messages - Telegram - Dashboards - Grafana"), pair("Messages - Telegram - Dashboards", "Grafana"));
        assert_eq!(f("com.apple.mail", "processed — iCloud – 0 messages"), kv(&[("messages", "0")]));
    }

    #[test]
    fn extra_groups_become_fields() {
        let rules = vec![Rule::new("t", r"^(?P<user>\S+) — (?P<subject>ssh \S+@(?P<host>\S+))$", 1).unwrap()];
        let r = parse(&rules, "t", "root — ssh root@10.168.0.77");
        assert_eq!(r.subject, "ssh root@10.168.0.77");
        assert_eq!(r.fields, vec![("user".into(), "root".into()), ("host".into(), "10.168.0.77".into())]);
    }

    #[test]
    fn falls_back() {
        assert_eq!(p("com.apple.ActivityMonitor", "Activity Monitor – All Processes"), pair("Activity Monitor", "All Processes"));
        assert_eq!(p("com.apple.Safari", "centralhardware/usageStatServer"), pair("centralhardware/usageStatServer", ""));
        assert_eq!(p("x", ""), pair("", ""));
    }
}
