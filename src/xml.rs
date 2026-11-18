//! A borrowed-bytes XML tokenizer. Names and values are slices of the input;
//! a value is unescaped only when it holds `&`. Markup is found with memchr,
//! so text and long attribute values cost one vectorized scan.
//!
//! Well-formedness is checked as .NET's XmlReader checks it for the SDK
//! (DTDs prohibited): matching tags, unique attributes, defined entities and
//! prefixes, legal characters. Errors carry a byte offset.

use memchr::{memchr, memchr2, memchr3};
use std::borrow::Cow;

#[derive(Debug)]
pub struct XmlError {
    pub at: usize,
    pub message: String,
}

fn err<T>(at: usize, message: impl Into<String>) -> Result<T, XmlError> {
    Err(XmlError { at, message: message.into() })
}

#[derive(Clone, Copy)]
pub struct RawAttr<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
}

pub enum Event<'a> {
    /// A start tag; its attributes are in `Reader::attrs` until the next call.
    Start { name: &'a [u8], empty: bool },
    End { name: &'a [u8] },
    /// Character data (entities still escaped) or a CDATA section.
    Text { raw: &'a [u8], cdata: bool },
    Eof,
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    pub attrs: Vec<RawAttr<'a>>,
    open: Vec<&'a [u8]>,
    seen_root: bool,
    /// The whole buffer is valid UTF-8 with no illegal control characters:
    /// text and values need no per-piece check, and convert unchecked.
    pub clean: bool,
    /// Clean, and no `&`, CR, tab or `]]>` anywhere: text needs no check, and
    /// a value changes only where it holds a line feed.
    pub plain: bool,
}

#[inline]
fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':' || c >= 0x80
}
#[inline]
fn is_name(c: u8) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == b'-' || c == b'.'
}
#[inline]
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        // A UTF-8 byte order mark is not content.
        let pos = if buf.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
        // One vectorizable pass over the bytes, instead of a check per piece.
        // Branch-free OR reductions vectorize: control characters, and the bytes
        // that make a text or value need a closer look (&, CR, tab).
        let (control, special) = buf.iter().fold((false, false), |(ctl, sp), &c| (ctl | ((c < 0x20) & (c != b'\t') & (c != b'\r') & (c != b'\n')), sp | (c == b'&') | (c == b'\r') | (c == b'\t')));
        // simdutf8 (after simdutf): the whole-part UTF-8 check at SIMD speed.
        let clean = !control && simdutf8::basic::from_utf8(buf).is_ok();
        let plain = clean && !special && memchr::memmem::find(buf, b"]]>").is_none();
        Reader { buf, pos, attrs: Vec::with_capacity(16), open: Vec::with_capacity(64), seen_root: false, clean, plain }
    }

    pub fn offset(&self) -> usize {
        self.pos
    }

    fn name(&mut self) -> Result<&'a [u8], XmlError> {
        let start = self.pos;
        if !self.buf.get(self.pos).is_some_and(|&c| is_name_start(c)) {
            return err(self.pos, "name expected");
        }
        self.pos += 1;
        while self.buf.get(self.pos).is_some_and(|&c| is_name(c)) {
            self.pos += 1;
        }
        Ok(&self.buf[start..self.pos])
    }

    fn skip_space(&mut self) {
        while self.buf.get(self.pos).is_some_and(|&c| is_space(c)) {
            self.pos += 1;
        }
    }

    pub fn next(&mut self) -> Result<Event<'a>, XmlError> {
        loop {
            let rest = &self.buf[self.pos..];
            if rest.is_empty() {
                if let Some(open) = self.open.last() {
                    return err(self.pos, format!("unexpected end of file; '{}' is not closed", String::from_utf8_lossy(open)));
                }
                if !self.seen_root {
                    return err(self.pos, "root element is missing");
                }
                return Ok(Event::Eof);
            }
            if rest[0] != b'<' {
                let end = memchr(b'<', rest).unwrap_or(rest.len());
                let raw = &rest[..end];
                let at = self.pos;
                self.pos += end;
                if self.open.is_empty() {
                    if raw.iter().all(|&c| is_space(c)) {
                        continue;
                    }
                    return err(at, "data at the root level is invalid");
                }
                if !self.plain {
                    check_text(raw, at, self.clean)?;
                }
                return Ok(Event::Text { raw, cdata: false });
            }
            match rest.get(1) {
                Some(b'/') => {
                    let at = self.pos;
                    self.pos += 2;
                    let name = self.name()?;
                    self.skip_space();
                    if self.buf.get(self.pos) != Some(&b'>') {
                        return err(self.pos, "'>' expected");
                    }
                    self.pos += 1;
                    match self.open.pop() {
                        Some(open) if open == name => return Ok(Event::End { name }),
                        Some(open) => {
                            return err(at, format!("the '{}' start tag does not match the end tag of '{}'", String::from_utf8_lossy(open), String::from_utf8_lossy(name)));
                        }
                        None => return err(at, "unexpected end tag"),
                    }
                }
                Some(b'?') => {
                    let Some(end) = memchr::memmem::find(rest, b"?>") else { return err(self.pos, "unterminated processing instruction") };
                    self.pos += end + 2;
                }
                Some(b'!') => {
                    if rest.starts_with(b"<!--") {
                        let Some(end) = memchr::memmem::find(&rest[4..], b"--") else { return err(self.pos, "unterminated comment") };
                        if rest.get(4 + end + 2) != Some(&b'>') {
                            return err(self.pos + 4 + end, "'--' in a comment");
                        }
                        self.pos += 4 + end + 3;
                    } else if rest.starts_with(b"<![CDATA[") {
                        if self.open.is_empty() {
                            return err(self.pos, "CDATA at the root level");
                        }
                        let Some(end) = memchr::memmem::find(&rest[9..], b"]]>") else { return err(self.pos, "unterminated CDATA") };
                        let raw = &rest[9..9 + end];
                        self.pos += 9 + end + 3;
                        return Ok(Event::Text { raw, cdata: true });
                    } else if rest.starts_with(b"<!DOCTYPE") {
                        return err(self.pos, "DTD is prohibited in this XML document");
                    } else {
                        return err(self.pos, "unexpected '<!'");
                    }
                }
                _ => {
                    if self.open.is_empty() && self.seen_root {
                        return err(self.pos, "there are multiple root elements");
                    }
                    self.pos += 1;
                    let name = self.name()?;
                    self.attrs.clear();
                    loop {
                        let before = self.pos;
                        self.skip_space();
                        match self.buf.get(self.pos) {
                            Some(b'>') => {
                                self.pos += 1;
                                self.open.push(name);
                                self.seen_root = true;
                                return Ok(Event::Start { name, empty: false });
                            }
                            Some(b'/') => {
                                if self.buf.get(self.pos + 1) != Some(&b'>') {
                                    return err(self.pos, "'>' expected");
                                }
                                self.pos += 2;
                                self.seen_root = true;
                                return Ok(Event::Start { name, empty: true });
                            }
                            None => return err(self.pos, "unexpected end of file in a tag"),
                            _ => {}
                        }
                        if self.pos == before {
                            return err(self.pos, "whitespace expected between attributes");
                        }
                        let aname = self.name()?;
                        self.skip_space();
                        if self.buf.get(self.pos) != Some(&b'=') {
                            return err(self.pos, "'=' expected");
                        }
                        self.pos += 1;
                        self.skip_space();
                        let quote = match self.buf.get(self.pos) {
                            Some(&q @ (b'"' | b'\'')) => q,
                            _ => return err(self.pos, "quote expected"),
                        };
                        let vstart = self.pos + 1;
                        let Some(len) = memchr2(quote, b'<', &self.buf[vstart..]) else { return err(vstart, "unterminated attribute value") };
                        if self.buf[vstart + len] == b'<' {
                            return err(vstart + len, "'<' in an attribute value");
                        }
                        let value = &self.buf[vstart..vstart + len];
                        if !self.plain {
                            check_text(value, vstart, self.clean)?;
                        }
                        if self.attrs.iter().any(|a| a.name == aname) {
                            return err(before, format!("'{}' is a duplicate attribute name", String::from_utf8_lossy(aname)));
                        }
                        self.attrs.push(RawAttr { name: aname, value });
                        self.pos = vstart + len + 1;
                    }
                }
            }
        }
    }
}

/// Legal characters and well-formed references in text or a value.
fn check_text(raw: &[u8], at: usize, clean: bool) -> Result<(), XmlError> {
    let mut i = 0;
    while let Some(j) = memchr3(b'&', b'>', 0x00, &raw[i..]).map(|j| i + j).or_else(|| {
        // Control characters other than tab, CR and LF are not legal XML.
        if clean {
            return None;
        }
        raw[i..].iter().position(|&c| c < 0x20 && !matches!(c, b'\t' | b'\r' | b'\n')).map(|j| i + j)
    }) {
        let c = raw[j];
        if c == b'&' {
            let Some(semi) = memchr(b';', &raw[j..]) else { return err(at + j, "unterminated entity reference") };
            let name = &raw[j + 1..j + semi];
            if decode_entity(name).is_none() {
                return err(at + j, format!("reference to undeclared entity '{}'", String::from_utf8_lossy(name)));
            }
            i = j + semi + 1;
        } else if c == b'>' {
            if j >= 2 && &raw[j - 2..j] == b"]]" {
                return err(at + j, "']]>' in text");
            }
            i = j + 1;
        } else {
            return err(at + j, "invalid character");
        }
        // Control characters are rare: check the rest only when present.
        if i >= raw.len() {
            break;
        }
    }
    if clean {
        return Ok(());
    }
    if raw.iter().any(|&c| c < 0x20 && !matches!(c, b'\t' | b'\r' | b'\n')) {
        let j = raw.iter().position(|&c| c < 0x20 && !matches!(c, b'\t' | b'\r' | b'\n')).unwrap();
        return err(at + j, "invalid character");
    }
    std::str::from_utf8(raw).map_err(|e| XmlError { at: at + e.valid_up_to(), message: "invalid UTF-8".into() })?;
    Ok(())
}

fn decode_entity(name: &[u8]) -> Option<char> {
    match name {
        b"lt" => Some('<'),
        b"gt" => Some('>'),
        b"amp" => Some('&'),
        b"apos" => Some('\''),
        b"quot" => Some('"'),
        [b'#', b'x', hex @ ..] => u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok().and_then(xml_char),
        [b'#', dec @ ..] => std::str::from_utf8(dec).ok()?.parse::<u32>().ok().and_then(xml_char),
        _ => None,
    }
}

fn xml_char(c: u32) -> Option<char> {
    let ch = char::from_u32(c)?;
    (matches!(c, 0x9 | 0xA | 0xD) || (0x20..=0xD7FF).contains(&c) || (0xE000..=0xFFFD).contains(&c) || c >= 0x10000).then_some(ch)
}

/// A value with its references replaced (and line ends normalized, as the
/// XML spec requires of parsers). Borrowed when nothing changes.
pub fn unescape(raw: &[u8], attribute: bool) -> Cow<'_, str> {
    unescape_in(raw, attribute, false)
}

/// As `unescape`, for bytes from a clean Reader (known valid UTF-8).
pub fn unescape_in(raw: &[u8], attribute: bool, clean: bool) -> Cow<'_, str> {
    let s = if clean {
        // SAFETY: the Reader checked the whole buffer is valid UTF-8, and its
        // pieces split only at ASCII delimiters.
        unsafe { std::str::from_utf8_unchecked(raw) }
    } else {
        std::str::from_utf8(raw).unwrap_or("")
    };
    // Most values are a few bytes, where a plain loop beats memchr's setup.
    let special = if raw.len() < 32 {
        raw.iter().any(|&c| c == b'&' || c == b'\r' || (attribute && (c == b'\t' || c == b'\n')))
    } else {
        memchr3(b'&', b'\r', if attribute { b'\t' } else { b'&' }, raw).is_some() || (attribute && memchr(b'\n', raw).is_some())
    };
    if !special {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '&' => {
                let semi = s[i..].find(';').unwrap();
                out.push(decode_entity(s[i + 1..i + semi].as_bytes()).unwrap());
                while chars.peek().is_some_and(|&(j, _)| j <= i + semi) {
                    chars.next();
                }
            }
            '\r' => {
                if chars.peek().is_some_and(|&(_, n)| n == '\n') {
                    chars.next();
                }
                out.push(if attribute { ' ' } else { '\n' });
            }
            // Attribute value normalization: whitespace characters become spaces.
            '\n' | '\t' if attribute => out.push(' '),
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(s: &str) -> Result<Vec<String>, XmlError> {
        let mut r = Reader::new(s.as_bytes());
        let mut out = vec![];
        loop {
            match r.next()? {
                Event::Start { name, empty } => {
                    let attrs: Vec<String> = r.attrs.iter().map(|a| format!("{}={}", String::from_utf8_lossy(a.name), unescape(a.value, true))).collect();
                    out.push(format!("<{}{}{}>", String::from_utf8_lossy(name), if attrs.is_empty() { String::new() } else { format!(" {}", attrs.join(" ")) }, if empty { "/" } else { "" }));
                }
                Event::End { name } => out.push(format!("</{}>", String::from_utf8_lossy(name))),
                Event::Text { raw, .. } => out.push(unescape(raw, false).into_owned()),
                Event::Eof => return Ok(out),
            }
        }
    }

    #[test]
    fn reads_and_rejects() {
        assert_eq!(events("<?xml version=\"1.0\"?><a x='1&amp;2'><b/>t&#x41;<![CDATA[<c>]]></a>").unwrap(), vec!["<a x=1&2>", "<b/>", "tA", "<c>", "</a>"]);
        for bad in ["<a><b></a>", "<a x='1' x='2'/>", "<a>&nope;</a>", "<a/><b/>", "<a>", "<!DOCTYPE a><a/>", "<a>\u{1}</a>", "", "x<a/>", "<a x='<'/>"] {
            assert!(events(bad).is_err(), "{bad:?} accepted");
        }
    }
}
