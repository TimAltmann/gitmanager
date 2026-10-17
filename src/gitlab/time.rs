//! Zeitangaben der Merge-Request-Ansicht: Zeitstempel aus GraphQL
//! (RFC 3339), Datumsangaben (Token-Ablauf) und relative Zeiten
//! („vor 5 Min.“). Rein, ohne Zeitzonen-Crate; alles in UTC.

use crate::i18n::{tr, tr_fmt, Language};

/// Aktuelle Zeit in Unix-Sekunden (0, falls die Uhr vor 1970 steht).
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// Parst `JJJJ-MM-TTTHH:MM:SS[.bruch](Z|±hh:mm)` in Unix-Sekunden; `T`/`Z`
/// auch klein, Leerzeichen statt `T` erlaubt (RFC 3339 §5.6). Bruchsekunden
/// werden abgeschnitten.
pub fn parse_rfc3339(s: &str) -> Option<i64> {
    let s = s.trim();
    // Nur ASCII → Byte-Indizes sind Zeichengrenzen
    if !s.is_ascii() || s.len() < 20 {
        return None;
    }
    let b = s.as_bytes();
    let days = parse_ymd(&s[..10])?;
    if !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let hour = digits(&s[11..13])?;
    let min = digits(&s[14..16])?;
    // 60 = Schaltsekunde
    let sec = digits(&s[17..19])?;
    if hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let len = frac.bytes().take_while(u8::is_ascii_digit).count();
        if len == 0 {
            return None;
        }
        rest = &frac[len..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            if rest.len() != 6 || rest.as_bytes()[3] != b':' {
                return None;
            }
            let (oh, om) = (digits(&rest[1..3])?, digits(&rest[4..6])?);
            if oh > 23 || om > 59 {
                return None;
            }
            sign * (oh * 3600 + om * 60)
        }
    };
    Some(days * DAY + hour * 3600 + min * 60 + sec - offset)
}

/// Parst `JJJJ-MM-TT` (z. B. `expires_at` eines Tokens) in Unix-Sekunden
/// um 00:00 UTC.
pub fn parse_date(s: &str) -> Option<i64> {
    let s = s.trim();
    if !s.is_ascii() || s.len() != 10 {
        return None;
    }
    parse_ymd(s).map(|days| days * DAY)
}

/// Datum als `TT.MM.JJJJ` (UTC).
pub fn format_date(ts: i64) -> String {
    let (y, m, d) = civil_from_days(ts.div_euclid(DAY));
    format!("{d:02}.{m:02}.{y:04}")
}

/// „gerade eben“ (< 1 Min., auch Zukunft), „vor {n} Min.“, „vor {n} Std.“,
/// „gestern“ (< 48 Std.), sonst „vor {n} Tagen“.
pub fn relative_time(ts: i64, now: i64, lang: Language) -> String {
    let ago = now.saturating_sub(ts).max(0);
    let n = |unit: i64| (ago / unit).to_string();
    match ago {
        a if a < 60 => tr(lang, "rel_now"),
        a if a < 3600 => tr_fmt(lang, "rel_min", &[&n(60)]),
        a if a < DAY => tr_fmt(lang, "rel_hour", &[&n(3600)]),
        a if a < 2 * DAY => tr(lang, "rel_yesterday"),
        _ => tr_fmt(lang, "rel_days", &[&n(DAY)]),
    }
}

const DAY: i64 = 86_400;

/// Nur ASCII-Ziffern (kein Vorzeichen wie bei `str::parse`).
fn digits(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// `JJJJ-MM-TT` → Tage seit 1970-01-01, mit Prüfung auf gültige Tage.
fn parse_ymd(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let (y, m, d) = (digits(&s[..4])?, digits(&s[5..7])?, digits(&s[8..10])?);
    if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Tage seit 1970-01-01 im proleptischen gregorianischen Kalender
/// (Algorithmus von Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Umkehrung von `days_from_civil`: (Jahr, Monat, Tag).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: i64 = 60;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;

    #[test]
    fn parse_rfc3339_utc() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-10-17T09:35:12Z"), Some(1_792_229_712));
        // Kleinbuchstaben und Leerzeichen als Trenner (RFC 3339 erlaubt beides)
        assert_eq!(parse_rfc3339("2026-10-17t09:35:12z"), Some(1_792_229_712));
        assert_eq!(parse_rfc3339("2026-10-17 09:35:12Z"), Some(1_792_229_712));
        assert_eq!(
            parse_rfc3339(" 2026-10-17T09:35:12Z\n"),
            Some(1_792_229_712)
        );
    }

    #[test]
    fn parse_rfc3339_offset_and_fraction() {
        // 09:00 +02:00 = 07:00 UTC
        assert_eq!(
            parse_rfc3339("2026-10-17T09:00:00+02:00"),
            parse_rfc3339("2026-10-17T07:00:00Z")
        );
        assert_eq!(
            parse_rfc3339("2026-10-16T22:30:00-05:30"),
            parse_rfc3339("2026-10-17T04:00:00Z")
        );
        assert_eq!(
            parse_rfc3339("2026-10-17T09:55:00.123Z"),
            parse_rfc3339("2026-10-17T09:55:00Z")
        );
        assert_eq!(
            parse_rfc3339("2026-10-17T09:55:00.123456789+00:00"),
            parse_rfc3339("2026-10-17T09:55:00Z")
        );
        // Schaltjahr und Jahreswechsel
        assert_eq!(
            parse_rfc3339("2024-02-29T23:59:59Z").map(|t| t + 1),
            parse_rfc3339("2024-03-01T00:00:00Z")
        );
        assert_eq!(
            parse_rfc3339("2027-01-01T00:30:00+01:00"),
            parse_rfc3339("2026-12-31T23:30:00Z")
        );
        // Vor 1970
        assert_eq!(parse_rfc3339("1969-12-31T23:59:59Z"), Some(-1));
    }

    #[test]
    fn parse_rfc3339_rejects_garbage() {
        for s in [
            "",
            "gestern",
            "2026-10-17",
            "2026-10-17T09:35:12",
            "2026-10-17T09:35Z",
            "2026-13-01T00:00:00Z",
            "2026-02-30T00:00:00Z",
            "2025-02-29T00:00:00Z",
            "2026-10-17T24:00:00Z",
            "2026-10-17T09:60:00Z",
            "2026-10-17T09:35:12.Z",
            "2026-10-17T09:35:12+0200",
            "2026-10-17T09:35:12+25:00",
            "2026-10-17X09:35:12Z",
            "2026-10-17T09:35:12Zx",
            "2026‐10‐17T09:35:12Z",
            "+2026-10-17T09:35:12Z",
        ] {
            assert_eq!(parse_rfc3339(s), None, "{s:?} muss abgelehnt werden");
        }
    }

    #[test]
    fn parse_and_format_date() {
        assert_eq!(parse_date("1970-01-02"), Some(DAY));
        assert_eq!(
            parse_date("2026-11-30"),
            parse_rfc3339("2026-11-30T00:00:00Z")
        );
        assert_eq!(parse_date(" 2026-11-30 "), parse_date("2026-11-30"));
        for s in [
            "",
            "2026-11-31",
            "30.11.2026",
            "2026-11-30T00:00:00Z",
            "2026-1-3",
        ] {
            assert_eq!(parse_date(s), None, "{s:?} muss abgelehnt werden");
        }
        assert_eq!(format_date(0), "01.01.1970");
        assert_eq!(format_date(parse_date("2026-11-30").unwrap()), "30.11.2026");
        // Uhrzeit spielt keine Rolle
        let late = parse_rfc3339("2024-02-29T23:59:59Z").unwrap();
        assert_eq!(format_date(late), "29.02.2024");
        assert_eq!(format_date(-1), "31.12.1969");
    }

    #[test]
    fn relative_time_de() {
        let now = parse_rfc3339("2026-10-17T10:00:00Z").unwrap();
        let de = |ago: i64| relative_time(now - ago, now, Language::De);
        assert_eq!(de(0), "gerade eben");
        assert_eq!(de(59), "gerade eben");
        assert_eq!(de(MIN), "vor 1 Min.");
        assert_eq!(de(25 * MIN + 30), "vor 25 Min.");
        assert_eq!(de(HOUR - 1), "vor 59 Min.");
        assert_eq!(de(HOUR), "vor 1 Std.");
        assert_eq!(de(23 * HOUR + 59 * MIN), "vor 23 Std.");
        assert_eq!(de(DAY), "gestern");
        assert_eq!(de(2 * DAY - 1), "gestern");
        assert_eq!(de(2 * DAY), "vor 2 Tagen");
        assert_eq!(de(40 * DAY + HOUR), "vor 40 Tagen");
        // Zeitstempel in der Zukunft (Uhrenabweichung)
        assert_eq!(de(-5 * MIN), "gerade eben");
    }

    #[test]
    fn relative_time_en() {
        let now = 1_000_000_000;
        let en = |ago: i64| relative_time(now - ago, now, Language::En);
        assert_eq!(en(10), "just now");
        assert_eq!(en(5 * MIN), "5 min ago");
        assert_eq!(en(3 * HOUR), "3 h ago");
        assert_eq!(en(30 * HOUR), "yesterday");
        assert_eq!(en(3 * DAY), "3 days ago");
    }

    #[test]
    fn now_unix_is_after_2020() {
        assert!(now_unix() > parse_rfc3339("2020-01-01T00:00:00Z").unwrap());
    }
}
