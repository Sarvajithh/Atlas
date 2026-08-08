//! Best-effort authored/publication-date extraction (Research Mode
//! Timeline, §8.2.4's "Timeline" tab). Previously nothing populated this
//! at all -- `DocumentRecord` only ever carried a filesystem `mtime`,
//! which is actively misleading to show as a paper's date (a re-saved or
//! re-indexed older document would sort as "recent"). This module is the
//! one place that turns real, already-extracted document content into a
//! genuine `YYYY-MM-DD` authored date, or honestly returns `None` when it
//! can't find one -- it never falls back to `mtime` or "now".
//!
//! Deliberately dependency-free (no `regex` crate): the patterns this
//! looks for are narrow enough that hand-written scanning is both simpler
//! to audit and avoids adding a new dependency for a handful of fixed
//! shapes.

/// Scans `text` (only the first `SCAN_WINDOW_CHARS` -- authored-date
/// evidence that matters for a Timeline is front matter, not something
/// buried mid-document) for an explicit "Published ..." / "Date: ..."
/// line and returns it as `YYYY-MM-DD`. Only patterns that are
/// unambiguously a *publication* date are matched -- a bare date with no
/// "Published"/"Date" label is deliberately NOT matched, since that's as
/// likely to be an example, a deadline, or an unrelated date mentioned in
/// the body as it is to be the document's own authored date, and a wrong
/// guess is worse than an honest `None` here.
const SCAN_WINDOW_CHARS: usize = 2000;

const LABELS: &[&str] = &["published on", "published:", "published", "date published", "publication date", "date:"];

const MONTHS: &[(&str, u32)] = &[
    ("january", 1), ("february", 2), ("march", 3), ("april", 4), ("may", 5), ("june", 6),
    ("july", 7), ("august", 8), ("september", 9), ("october", 10), ("november", 11), ("december", 12),
    ("jan", 1), ("feb", 2), ("mar", 3), ("apr", 4), ("jun", 6), ("jul", 7),
    ("aug", 8), ("sep", 9), ("sept", 9), ("oct", 10), ("nov", 11), ("dec", 12),
];

pub fn extract_authored_date_from_text(text: &str) -> Option<String> {
    let window: String = text.chars().take(SCAN_WINDOW_CHARS).collect();
    let lower = window.to_ascii_lowercase();

    for label in LABELS {
        let mut search_from = 0usize;
        while let Some(rel_idx) = lower[search_from..].find(label) {
            let label_end = search_from + rel_idx + label.len();
            let rest = &window[label_end..];
            if let Some(date) = parse_leading_date(rest.trim_start_matches([':', ' ', '\t']).trim_start()) {
                return Some(date);
            }
            search_from = label_end;
        }
    }
    None
}

/// Parses a date starting at the beginning of `s`, in either
/// `YYYY-MM-DD` / `YYYY/MM/DD` or `Month DD, YYYY` / `DD Month YYYY`
/// shape. Returns `None` (rather than guessing) if what follows the label
/// doesn't cleanly parse as one of these -- a label with no real date
/// after it (e.g. "Published under a Creative Commons license") must not
/// produce a fabricated result.
fn parse_leading_date(s: &str) -> Option<String> {
    let s = s.trim_start();
    if let Some(date) = parse_numeric_date(s) {
        return Some(date);
    }
    if let Some(date) = parse_month_name_date(s) {
        return Some(date);
    }
    None
}

fn parse_numeric_date(s: &str) -> Option<String> {
    let token: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '/')
        .collect();
    let parts: Vec<&str> = token.split(['-', '/']).filter(|p| !p.is_empty()).collect();
    if parts.len() != 3 {
        return None;
    }
    // Only accept the unambiguous YYYY-MM-DD ordering (4-digit first
    // part) -- MM/DD/YYYY vs DD/MM/YYYY is locale-ambiguous and guessing
    // wrong silently corrupts the date, which is worse than not
    // extracting one at all.
    if parts[0].len() != 4 {
        return None;
    }
    let year: u32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    let day: u32 = parts[2].parse().ok()?;
    valid_date(year, month, day)
}

fn parse_month_name_date(s: &str) -> Option<String> {
    let words: Vec<&str> = s
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|w| !w.is_empty())
        .take(3)
        .collect();
    if words.len() < 3 {
        return None;
    }

    // "Month DD, YYYY"
    if let Some(month) = month_number(words[0]) {
        let day: Option<u32> = words[1].trim_end_matches(['s', 't', 'h', 'n', 'd', 'r']).parse().ok();
        let year: Option<u32> = words[2].parse().ok();
        if let (Some(day), Some(year)) = (day, year) {
            if let Some(date) = valid_date(year, month, day) {
                return Some(date);
            }
        }
    }

    // "DD Month YYYY"
    if let Some(month) = month_number(words[1]) {
        let day: Option<u32> = words[0].trim_end_matches(['s', 't', 'h', 'n', 'd', 'r']).parse().ok();
        let year: Option<u32> = words[2].parse().ok();
        if let (Some(day), Some(year)) = (day, year) {
            if let Some(date) = valid_date(year, month, day) {
                return Some(date);
            }
        }
    }

    None
}

fn month_number(word: &str) -> Option<u32> {
    let lower = word.trim_matches(|c: char| !c.is_ascii_alphabetic()).to_ascii_lowercase();
    MONTHS.iter().find(|(name, _)| *name == lower).map(|(_, n)| *n)
}

fn valid_date(year: u32, month: u32, day: u32) -> Option<String> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Loose sanity bound, not full historical validity -- a study
    // document's authored date is realistically somewhere in this range;
    // outside it, it's more likely a mis-parsed number than a real date.
    if !(1900..=2100).contains(&year) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// Parses a PDF `/CreationDate` info-dict value, e.g. `D:20230114153000-05'00'`
/// or a bare `20230114`, into `YYYY-MM-DD`. PDF's own date spec (§7.9.4)
/// always puts year first, so this is unambiguous (unlike the free-text
/// numeric case above).
pub fn parse_pdf_date(raw: &str) -> Option<String> {
    let digits: String = raw.trim_start_matches("D:").chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() < 8 {
        return None;
    }
    let year: u32 = digits[0..4].parse().ok()?;
    let month: u32 = digits[4..6].parse().ok()?;
    let day: u32 = digits[6..8].parse().ok()?;
    valid_date(year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_iso_date_after_published_label() {
        let text = "My Paper Title\n\nPublished: 2023-06-14\n\nAbstract...";
        assert_eq!(extract_authored_date_from_text(text), Some("2023-06-14".to_string()));
    }

    #[test]
    fn extracts_month_name_date_after_published_on() {
        let text = "Published on June 14, 2023\n\nSome intro text.";
        assert_eq!(extract_authored_date_from_text(text), Some("2023-06-14".to_string()));
    }

    #[test]
    fn extracts_day_month_year_ordering() {
        let text = "Date: 14 June 2023\n\nBody...";
        assert_eq!(extract_authored_date_from_text(text), Some("2023-06-14".to_string()));
    }

    #[test]
    fn does_not_fabricate_a_date_when_no_label_is_present() {
        let text = "This paper was written in June 2023 and discusses many things about 2023.";
        assert_eq!(extract_authored_date_from_text(text), None);
    }

    #[test]
    fn does_not_fabricate_a_date_when_label_has_no_real_date_after_it() {
        let text = "Published under a Creative Commons license. No specific date given here at all.";
        assert_eq!(extract_authored_date_from_text(text), None);
    }

    #[test]
    fn rejects_ambiguous_mm_dd_yyyy_ordering() {
        // 3-part numeric with a non-4-digit first segment is ambiguous
        // (MM/DD/YYYY vs DD/MM/YYYY) -- must not guess.
        let text = "Published: 06/14/2023";
        assert_eq!(extract_authored_date_from_text(text), None);
    }

    #[test]
    fn pdf_creation_date_parses_the_standard_d_prefixed_format() {
        assert_eq!(parse_pdf_date("D:20230614153000-05'00'"), Some("2023-06-14".to_string()));
    }

    #[test]
    fn pdf_creation_date_rejects_too_short_a_value() {
        assert_eq!(parse_pdf_date("D:2023"), None);
    }
}
