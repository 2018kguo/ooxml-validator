//! A part's bytes as UTF-8, decoded the way .NET's XmlReader decodes them: a
//! byte order mark decides, else the first bytes and the declaration's
//! encoding. Only the encodings .NET has without extra providers are known
//! (UTF-8, UTF-16, UTF-32, US-ASCII, ISO-8859-1); another is an error, as it
//! is for the SDK.

use std::borrow::Cow;

pub fn to_utf8(b: &[u8]) -> Result<Cow<'_, [u8]>, String> {
    let utf16 = |be: bool, body: &[u8]| -> Result<Cow<'static, [u8]>, String> {
        if body.len() % 2 != 0 {
            return Err("Unexpected end of file: an odd number of bytes in UTF-16.".into());
        }
        let units = body.chunks_exact(2).map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) });
        let s: String = char::decode_utf16(units).collect::<Result<_, _>>().map_err(|_| "Invalid UTF-16: an unpaired surrogate.".to_string())?;
        Ok(Cow::Owned(s.into_bytes()))
    };
    let declared = |text: &[u8]| -> Option<String> {
        let head = &text[..text.len().min(200)];
        if !head.starts_with(b"<?xml") {
            return None;
        }
        let end = memchr::memmem::find(head, b"?>")?;
        let decl = std::str::from_utf8(&head[..end]).ok()?;
        let at = decl.find("encoding")?;
        let rest = decl[at + 8..].trim_start().strip_prefix('=')?.trim_start();
        let q = rest.chars().next()?;
        Some(rest[1..].split(q).next()?.to_ascii_lowercase())
    };
    let (decoded, from_bom): (Cow<[u8]>, Option<&str>) = if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (Cow::Borrowed(rest), Some("utf-8"))
    } else if let Some(rest) = b.strip_prefix(&[0xFF, 0xFE]) {
        (utf16(false, rest)?, Some("utf-16"))
    } else if let Some(rest) = b.strip_prefix(&[0xFE, 0xFF]) {
        (utf16(true, rest)?, Some("utf-16"))
    } else if b.starts_with(&[0x3C, 0x00, 0x3F, 0x00]) {
        (utf16(false, b)?, Some("utf-16"))
    } else if b.starts_with(&[0x00, 0x3C, 0x00, 0x3F]) {
        (utf16(true, b)?, Some("utf-16"))
    } else {
        (Cow::Borrowed(b), None)
    };
    let Some(enc) = declared(&decoded) else { return Ok(decoded) };
    let family = |e: &str| match e {
        "utf-8" | "utf8" => Some("utf-8"),
        "utf-16" | "utf-16le" | "utf-16be" | "unicode" | "unicodefffe" | "ucs-2" => Some("utf-16"),
        "us-ascii" | "ascii" => Some("ascii"),
        "iso-8859-1" | "latin1" | "iso_8859-1" | "l1" => Some("latin1"),
        _ => None,
    };
    let Some(kind) = family(&enc) else {
        return Err(format!("System does not support '{enc}' encoding."));
    };
    match (from_bom, kind) {
        // A byte order mark (or UTF-16's first bytes) decides; the declaration must agree.
        (Some("utf-16"), "utf-16") | (Some("utf-8"), "utf-8" | "ascii" | "latin1") => Ok(decoded),
        (Some(_), _) => Err(format!("There is no Unicode byte order mark. Cannot switch to Unicode ('{enc}' declared).")),
        (None, "utf-8") => Ok(decoded),
        (None, "utf-16") => Err(format!("There is no Unicode byte order mark. Cannot switch to Unicode ('{enc}' declared).")),
        // Single-byte: ASCII replaces bytes above 0x7F with '?'; Latin-1 maps each to U+00xx.
        (None, "ascii") => Ok(Cow::Owned(decoded.iter().map(|&c| if c < 0x80 { c } else { b'?' }).collect())),
        (None, _) => Ok(Cow::Owned(decoded.iter().map(|&c| c as char).collect::<String>().into_bytes())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_as_xmlreader() {
        let doc = r#"<?xml version="1.0" encoding="UTF-16"?><a>é</a>"#;
        let le: Vec<u8> = [0xFF, 0xFE].into_iter().chain(doc.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert_eq!(&*to_utf8(&le).unwrap(), doc.as_bytes());
        let bare: Vec<u8> = doc.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert_eq!(&*to_utf8(&bare).unwrap(), doc.as_bytes());
        assert_eq!(&*to_utf8(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>caf\xe9</a>").unwrap(), "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a>café</a>".as_bytes());
        assert!(to_utf8(b"<?xml version=\"1.0\" encoding=\"windows-1252\"?><a/>").is_err());
        assert!(to_utf8(b"\xFF\xFE<").is_err());
    }
}
