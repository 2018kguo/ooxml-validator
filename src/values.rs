//! The SDK's simple value classes: when a text is a valid value
//! (`OpenXmlSimpleType.IsValid`) and the number a numeric one holds
//! (`NumberValidator.TryGetValue`). Parsing follows .NET's XmlConvert.

use crate::schema::ValueKind;
use crate::schema_gen::ENUMS;

/// XmlConvert trims XML whitespace before parsing.
fn trim(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\n', '\r'])
}

/// Int32.Parse with AllowLeadingWhite | AllowTrailingWhite | AllowLeadingSign.
fn integer(s: &str) -> Option<i128> {
    let t = trim(s);
    let (neg, digits) = match t.as_bytes().first()? {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) || digits.len() > 30 {
        return None;
    }
    let v: i128 = digits.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// XmlConvert.ToDouble: NumberStyles.Float, plus INF, -INF and NaN.
fn double(s: &str) -> Option<f64> {
    let t = trim(s);
    match t {
        "INF" => return Some(f64::INFINITY),
        "-INF" => return Some(f64::NEG_INFINITY),
        "NaN" => return Some(f64::NAN),
        _ => {}
    }
    let b = t.as_bytes();
    let mut i = 0;
    if matches!(b.first(), Some(b'+' | b'-')) {
        i = 1;
    }
    let mut digits = 0;
    while b.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
        digits += 1;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    if i != b.len() {
        return None;
    }
    t.parse().ok()
}

fn hex_binary(s: &str) -> bool {
    s.len() % 2 == 0 && s.bytes().all(|c| c.is_ascii_hexdigit())
}

/// XSD dateTime and the other forms XmlConvert.ToDateTime(RoundtripKind) takes.
fn date_time(s: &str) -> bool {
    let t = trim(s).as_bytes();
    let digits = |from: usize, n: usize| t.get(from..from + n).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let mut i = 0;
    if t.first() == Some(&b'-') {
        i = 1;
    }
    // yyyy-MM-dd
    if !(digits(i, 4) && t.get(i + 4) == Some(&b'-') && digits(i + 5, 2) && t.get(i + 7) == Some(&b'-') && digits(i + 8, 2)) {
        return false;
    }
    let month = (t[i + 5] - b'0') * 10 + (t[i + 6] - b'0');
    let day = (t[i + 8] - b'0') * 10 + (t[i + 9] - b'0');
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }
    i += 10;
    if t.get(i) == Some(&b'T') {
        if !(digits(i + 1, 2) && t.get(i + 3) == Some(&b':') && digits(i + 4, 2) && t.get(i + 6) == Some(&b':') && digits(i + 7, 2)) {
            return false;
        }
        let h = (t[i + 1] - b'0') * 10 + (t[i + 2] - b'0');
        let m = (t[i + 4] - b'0') * 10 + (t[i + 5] - b'0');
        let sec = (t[i + 7] - b'0') * 10 + (t[i + 8] - b'0');
        if h > 24 || m > 59 || sec > 59 {
            return false;
        }
        i += 9;
        if t.get(i) == Some(&b'.') {
            i += 1;
            let start = i;
            while t.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
            // Any number of digits: XsdDateTime keeps seven and rounds the rest.
            if i == start {
                return false;
            }
        }
    }
    match t.get(i) {
        None => true,
        Some(b'Z') => i + 1 == t.len(),
        Some(b'+' | b'-') => digits(i + 1, 2) && t.get(i + 3) == Some(&b':') && digits(i + 4, 2) && i + 6 == t.len(),
        _ => false,
    }
}

/// Convert.FromBase64String: whitespace ignored; groups of four from the
/// alphabet, with up to two '=' only at the end.
pub fn base64_len(s: &str) -> Option<usize> {
    let b: Vec<u8> = s.bytes().filter(|c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r')).collect();
    if b.len() % 4 != 0 {
        return None;
    }
    let pad = b.iter().rev().take_while(|&&c| c == b'=').count();
    if pad > 2 || !b[..b.len() - pad].iter().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/')) {
        return None;
    }
    Some(b.len() / 4 * 3 - pad)
}

fn base64(s: &str) -> bool {
    s.is_empty() || base64_len(s).is_some()
}

/// StringValue.Length as each class counts it: UTF-16 code units, hex
/// binary octets, base64 decoded bytes.
pub fn string_length(kind: ValueKind, s: &str) -> i64 {
    match kind {
        ValueKind::HexBinaryValue => (s.encode_utf16().count() / 2) as i64,
        ValueKind::Base64BinaryValue => base64_len(s).unwrap_or(0) as i64,
        _ => s.encode_utf16().count() as i64,
    }
}

pub fn enum_member(enum_id: u16, s: &str) -> bool {
    ENUMS.get(usize::from(enum_id)).is_some_and(|(_, values)| values.contains(&s))
}

/// OpenXmlSimpleType.IsValid for a present attribute's text.
pub fn is_valid(kind: ValueKind, enum_id: u16, s: &str) -> bool {
    use ValueKind::*;
    // OpenXmlSimpleValue.ShouldParse: an empty string is no value.
    let nonempty = !s.is_empty();
    match kind {
        StringValue => true,
        HexBinaryValue => hex_binary(s),
        Base64BinaryValue => base64(s),
        // ListValue keeps its text; items are checked by its validators.
        ListValue => true,
        EnumValue => enum_member(enum_id, s),
        BooleanValue => nonempty && matches!(trim(s), "true" | "false" | "1" | "0"),
        OnOffValue => matches!(s, "true" | "1" | "on" | "false" | "0" | "off"),
        TrueFalseValue => matches!(s, "true" | "t" | "false" | "f"),
        TrueFalseBlankValue => matches!(s, "true" | "t" | "false" | "f" | ""),
        DateTimeValue => nonempty && date_time(s),
        DoubleValue => nonempty && double(s).is_some(),
        SingleValue => nonempty && double(s).is_some_and(|v| v.is_nan() || v.is_infinite() || v.abs() <= f32::MAX as f64),
        DecimalValue => nonempty && double(s).is_some_and(|v| v.is_finite()) && !trim(s).contains(['e', 'E']),
        _ => nonempty && number(kind, s).is_some(),
    }
}

/// The number a numeric value class holds (NumberValidator.TryGetValue);
/// None for other classes or text that does not parse.
pub fn number(kind: ValueKind, s: &str) -> Option<f64> {
    use ValueKind::*;
    let (min, max): (i128, i128) = match kind {
        ByteValue => (0, u8::MAX.into()),
        SByteValue => (i8::MIN.into(), i8::MAX.into()),
        Int16Value => (i16::MIN.into(), i16::MAX.into()),
        UInt16Value => (0, u16::MAX.into()),
        Int32Value => (i32::MIN.into(), i32::MAX.into()),
        UInt32Value => (0, u32::MAX.into()),
        Int64Value | IntegerValue => (i64::MIN.into(), i64::MAX.into()),
        UInt64Value => (0, u64::MAX.into()),
        DoubleValue => return double(s),
        SingleValue => return double(s).map(|v| v as f32 as f64),
        _ => return None,
    };
    // XmlConvert parses the unsigned classes without AllowLeadingSign: "+0" and "-0" fail.
    if min == 0 && matches!(trim(s).as_bytes().first(), Some(b'+' | b'-')) {
        return None;
    }
    let v = integer(s)?;
    (min..=max).contains(&v).then_some(v as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ValueKind::*;

    #[test]
    fn parses_as_xmlconvert() {
        assert_eq!(number(Int32Value, " +12 "), Some(12.0));
        assert_eq!(number(Int32Value, "1.0"), None);
        assert_eq!(number(UInt32Value, "-1"), None);
        assert_eq!(number(UInt64Value, "-0"), None);
        assert_eq!(number(UInt32Value, "+1"), None);
        assert_eq!(number(SByteValue, "-0"), Some(0.0));
        assert_eq!(number(Int32Value, "2147483648"), None);
        assert!(is_valid(DoubleValue, 0, "1.5e3") && is_valid(DoubleValue, 0, "-INF") && !is_valid(DoubleValue, 0, "Infinity"));
        assert!(is_valid(OnOffValue, 0, "on") && !is_valid(OnOffValue, 0, "On") && !is_valid(OnOffValue, 0, ""));
        assert!(is_valid(TrueFalseBlankValue, 0, "") && !is_valid(TrueFalseValue, 0, ""));
        assert!(is_valid(HexBinaryValue, 0, "00A1") && !is_valid(HexBinaryValue, 0, "0A1"));
        assert!(is_valid(DateTimeValue, 0, "2026-11-17T12:00:00Z") && is_valid(DateTimeValue, 0, "2026-11-17") && !is_valid(DateTimeValue, 0, "17/11/2026"));
        assert!(!is_valid(Int32Value, 0, ""));
    }
}

/// What CellType's validator (Schema/Spreadsheet/CellType.cs) requires of a
/// cell's `x:v` text for its `t`: a Boolean is exactly 0, 1, true or false;
/// a Number parses as a double, an int or a decimal; a Date is an ISO time
/// with milliseconds, with or without an offset (CellValue.TryGet*).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CellKind {
    Boolean,
    Number,
    Date,
}

pub fn cell_value(kind: CellKind, s: &str) -> bool {
    match kind {
        CellKind::Boolean => matches!(s, "0" | "1" | "true" | "false"),
        CellKind::Number => number_styles(s) || special_double(s),
        CellKind::Date => cell_date(s),
    }
}

/// Whitespace as .NET number parsing allows it (IsWhite): tab to CR, space.
fn white(c: u8) -> bool {
    c == b' ' || (0x09..=0x0d).contains(&c)
}

/// double.TryParse(s, NumberStyles.Number | AllowExponent, Invariant)'s
/// grammar (Number.Parsing ParseNumber): whitespace, a sign, digits with
/// thousands separators after the first, a point, an exponent, then a sign
/// if none came first, and whitespace. An overflow parses (as infinity), so
/// the grammar is the answer; int and decimal accept nothing more.
fn number_styles(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    let mut sign = false;
    // Leading whitespace, then at most one sign (no whitespace after it).
    loop {
        match b.get(i) {
            Some(&c) if white(c) && !sign => i += 1,
            Some(b'+' | b'-') if !sign => {
                sign = true;
                i += 1;
            }
            _ => break,
        }
    }
    let mut digits = false;
    let mut point = false;
    loop {
        match b.get(i) {
            Some(c) if c.is_ascii_digit() => {
                digits = true;
                i += 1;
            }
            Some(b',') if digits && !point => i += 1,
            Some(b'.') if !point => {
                point = true;
                i += 1;
            }
            _ => break,
        }
    }
    if !digits {
        return false;
    }
    // An exponent needs digits; without them the 'e' is left unparsed.
    if matches!(b.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(b.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if b.get(j).is_some_and(u8::is_ascii_digit) {
            while b.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            i = j;
        }
    }
    // Trailing whitespace and one sign, if none led.
    while let Some(&c) = b.get(i) {
        if white(c) {
            i += 1;
        } else if (c == b'+' || c == b'-') && !sign {
            sign = true;
            i += 1;
        } else if c == 0 {
            i += 1;
        } else {
            return false;
        }
    }
    true
}

/// Number.TryParseFloat's fallback: the invariant infinity and NaN symbols,
/// without case, around String.Trim's whitespace.
fn special_double(s: &str) -> bool {
    let t = s.trim();
    let is = |x: &str, w: &str| x.eq_ignore_ascii_case(w);
    if is(t, "Infinity") || is(t, "-Infinity") || is(t, "NaN") {
        return true;
    }
    if let Some(r) = t.strip_prefix('+') {
        return is(r, "Infinity") || is(r, "NaN");
    }
    t.strip_prefix('-').is_some_and(|r| is(r, "NaN"))
}

/// DateTime.TryParseExact(s, "yyyy-MM-ddTHH:mm:ss.fff") or DateTimeOffset's
/// with "zzz" after it (a sign, 1-2 digit hours, an optional colon, 2 digit
/// minutes, within 14 hours), DateTimeStyles.None.
fn cell_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 23 {
        return false;
    }
    let n = |r: std::ops::Range<usize>| -> Option<u32> {
        let d = b.get(r)?;
        d.iter().all(u8::is_ascii_digit).then(|| d.iter().fold(0, |a, c| a * 10 + (c - b'0') as u32))
    };
    let at = |i: usize, c: u8| b.get(i) == Some(&c);
    let (Some(y), Some(mo), Some(d), Some(h), Some(mi), Some(se), Some(_)) = (n(0..4), n(5..7), n(8..10), n(11..13), n(14..16), n(17..19), n(20..23)) else { return false };
    if !(at(4, b'-') && at(7, b'-') && at(10, b'T') && at(13, b':') && at(16, b':') && at(19, b'.')) {
        return false;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if y == 0 || !(1..=12).contains(&mo) || d == 0 || d > days[mo as usize - 1] || h > 23 || mi > 59 || se > 59 {
        return false;
    }
    let rest = &b[23..];
    if rest.is_empty() {
        return true;
    }
    if !matches!(rest[0], b'+' | b'-') {
        return false;
    }
    let digits = |r: &[u8]| r.iter().all(u8::is_ascii_digit);
    let (hours, minutes) = match &rest[1..] {
        [h, b':', m @ ..] | [h, m @ ..] if m.len() == 2 && h.is_ascii_digit() => (vec![*h], m.to_vec()),
        [h1, h2, b':', m @ ..] | [h1, h2, m @ ..] if m.len() == 2 && h1.is_ascii_digit() && h2.is_ascii_digit() => (vec![*h1, *h2], m.to_vec()),
        _ => return false,
    };
    if !digits(&minutes) {
        return false;
    }
    let hv = hours.iter().fold(0u32, |a, c| a * 10 + (c - b'0') as u32);
    let mv = (minutes[0] - b'0') as u32 * 10 + (minutes[1] - b'0') as u32;
    mv < 60 && hv * 60 + mv <= 14 * 60
}

#[cfg(test)]
mod cell_tests {
    use super::*;

    #[test]
    fn cell_values_as_the_sdk_parses_them() {
        for ok in ["1", "-1", " 1.5e3 ", "1,000", "5-", ".5", "5.", "1E+10", "1e999", "Infinity", "-nan", "+NaN"] {
            assert!(cell_value(CellKind::Number, ok), "{ok}");
        }
        for bad in ["", "abc", ",5", "1e", "- 1", "1.2.3", "--1", "1-+", "0x10", "1 2"] {
            assert!(!cell_value(CellKind::Number, bad), "{bad}");
        }
        assert!(cell_value(CellKind::Date, "2026-11-17T08:30:00.000"));
        assert!(cell_value(CellKind::Date, "2024-02-29T23:59:59.999+05:30"));
        assert!(cell_value(CellKind::Date, "2024-02-29T23:59:59.999-0530"));
        assert!(!cell_value(CellKind::Date, "2023-02-29T00:00:00.000"));
        assert!(!cell_value(CellKind::Date, "2026-11-17T08:30:00"));
        assert!(!cell_value(CellKind::Date, "2026-11-17T08:30:00.000Z"));
        assert!(!cell_value(CellKind::Boolean, "TRUE"));
    }
}
