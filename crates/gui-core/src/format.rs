//! Numbers as tables and Details show them, as CSV writes them, and as people type them.

/// Magnitudes from this one up are shown in exponent notation (forces in N, masses in kg, ...):
/// five digits before the point.
pub const EXPONENT_FROM: f64 = 1e4;

/// Up to six decimals without trailing zeros; magnitudes below 1e-4 or from `EXPONENT_FROM` up in
/// exponent notation with seven significant digits ("7.624381e+06").
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return if value.is_nan() {
            "NaN".into()
        } else if value > 0.0 {
            "inf".into()
        } else {
            "-inf".into()
        };
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-4..EXPONENT_FROM).contains(&magnitude) {
        // seven significant digits, as %g
        return format_exponent(value, 6, true);
    }
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" { "0".into() } else { text.into() }
}

/// `value` in exponent notation with `decimals` digits after the mantissa's point ("1.25e+07"),
/// trailing zeros of the mantissa removed when `trim`.
pub fn format_exponent(value: f64, decimals: usize, trim: bool) -> String {
    let text = format!("{value:.decimals$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let mantissa =
        if trim && mantissa.contains('.') { mantissa.trim_end_matches('0').trim_end_matches('.') } else { mantissa };
    let exponent: i32 = exponent.parse().unwrap_or(0);
    format!("{mantissa}e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
}

/// Record ranges as people type them: "129-140, 150" (a single record alone).
pub fn ranges_text(ranges: &[[i64; 2]]) -> String {
    ranges
        .iter()
        .map(|r| if r[0] == r[1] { r[0].to_string() } else { format!("{}-{}", r[0], r[1]) })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reads `ranges_text`; empty text is no range. None: not ranges of records (first >= 1, last >=
/// first).
pub fn parse_ranges(text: &str) -> Option<Vec<[i64; 2]>> {
    let mut out = Vec::new();
    for part in text.split([',', ';']).map(str::trim).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
            None => {
                let a: i64 = part.parse().ok()?;
                (a, a)
            }
        };
        if a < 1 || b < a {
            return None;
        }
        out.push([a, b]);
    }
    Some(out)
}

/// Accepts "1.5", "1,5" (a decimal comma), exponents and surrounding spaces; finite values only.
pub fn parse_number(text: &str) -> Option<f64> {
    let t = text.trim();
    let value = t.parse::<f64>().ok().or_else(|| {
        // a decimal comma, as a German or French Windows writes it
        (t.matches(',').count() == 1 && !t.contains('.')).then(|| t.replace(',', ".").parse::<f64>().ok()).flatten()
    })?;
    value.is_finite().then_some(value)
}

/// "1.2 GB", "350 MB" (powers of 1024, as Windows shows memory).
pub fn format_bytes(bytes: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut value, mut unit) = (bytes, 0);
    while value.abs() >= 1024.0 && unit < 4 {
        value /= 1024.0;
        unit += 1;
    }
    let decimals = if value < 10.0 && unit > 0 { 1 } else { 0 };
    format!("{value:.decimals$} {}", UNITS[unit])
}

/// The shortest text that reads back as the same value. A value that is exactly a float (the
/// engine's float32 results) is written as that float ("1234.5677", not "1234.5677490234375").
pub fn csv_number(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let single = value as f32;
    if single.is_finite() && f64::from(single) == value { single.to_string() } else { value.to_string() }
}

/// A CSV field: quoted when it holds a comma, a quote or a line break (RFC 4180).
pub fn csv_text(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) { format!("\"{}\"", text.replace('"', "\"\"")) } else { text.to_string() }
}

/// "hh:mm:ss"
pub fn format_elapsed(seconds: f64) -> String {
    let s = seconds.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_show_six_decimals_at_most() {
        assert_eq!(format_number(0.4), "0.4");
        assert_eq!(format_number(2712.980225), "2712.980225");
        assert_eq!(format_number(1.0 / 3.0), "0.333333");
        assert_eq!(format_number(-0.0000001), "-1e-07");
        assert_eq!(format_number(1.5e12), "1.5e+12");
        // from five digits before the point (forces, masses): seven significant digits
        assert_eq!(format_number(9_999.5), "9999.5");
        assert_eq!(format_number(12_583.683701), "1.258368e+04");
        assert_eq!(format_number(7_624_381.27), "7.624381e+06");
        assert_eq!(format_number(-1.2e7), "-1.2e+07");
        assert_eq!(parse_number(&format_number(7_624_381.27)), Some(7.624381e6));
        assert_eq!(format_exponent(12_500_000.0, 2, false), "1.25e+07");
        assert_eq!(format_exponent(1e7, 1, false), "1.0e+07");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(0.0), "0");
        assert_eq!(format_number(f64::NAN), "NaN");
    }

    #[test]
    fn numbers_parse_with_a_point_or_a_comma() {
        assert_eq!(parse_number(" 1.5 "), Some(1.5));
        assert_eq!(parse_number("1,5"), Some(1.5));
        assert_eq!(parse_number("2e3"), Some(2000.0));
        assert_eq!(parse_number("x"), None);
        assert_eq!(parse_number("inf"), None);
    }

    #[test]
    fn csv_is_shortest_round_trip() {
        assert_eq!(csv_number(f64::from(1234.5677f32)), "1234.5677");
        assert_eq!(csv_number(0.1), "0.1");
        assert_eq!(csv_number(1.0 / 3.0), "0.3333333333333333");
        assert_eq!(csv_text("a,b"), "\"a,b\"");
        assert_eq!(csv_text("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(format_bytes(1536.0 * 1024.0 * 1024.0), "1.5 GB");
        assert_eq!(format_bytes(350.0 * 1024.0 * 1024.0), "350 MB");
        assert_eq!(format_elapsed(3725.0), "01:02:05");
    }
}
