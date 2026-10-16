//! FFXIV SeString decoding: plain text for exports and styled spans for the viewer.
//!
//! The dictionaries come from `dict.rs` (downloaded at run time); every lookup
//! degrades to a placeholder when an entry is missing.

use crate::dict;
use serde::Serialize;

// Macro codes (Lumina MacroCode).
const NEW_LINE: u8 = 0x10;
const ICON: u8 = 0x12;
const COLOR: u8 = 0x13;
const EDGE_COLOR: u8 = 0x14;
const SHADOW_COLOR: u8 = 0x15;
const SOFT_HYPHEN: u8 = 0x16;
const KEY: u8 = 0x17;
const SCALE: u8 = 0x18;
const BOLD: u8 = 0x19;
const ITALIC: u8 = 0x1A;
const EDGE: u8 = 0x1B;
const SHADOW: u8 = 0x1C;
const NBSP: u8 = 0x1D;
const ICON2: u8 = 0x1E;
const HYPHEN: u8 = 0x1F;
const LINK: u8 = 0x27;
const FIXED: u8 = 0x2E;
const COLOR_TYPE: u8 = 0x48;
const EDGE_COLOR_TYPE: u8 = 0x49;
const LINK_TERMINATOR: u8 = 0xCF;

/// Macros that only change appearance in ways the viewer does not reproduce.
fn ignored(kind: u8) -> bool {
    matches!(kind, SHADOW_COLOR | SOFT_HYPHEN | KEY | SCALE | EDGE | SHADOW | ICON2)
}

pub fn packed_uint(data: &[u8], mut pos: usize) -> Result<(u32, usize), String> {
    let marker = *data.get(pos).ok_or("整数が途中で切れています")?;
    pos += 1;
    if marker > 0 && marker < 0xD0 {
        return Ok((u32::from(marker) - 1, pos));
    }
    if !(0xF0..=0xFE).contains(&marker) {
        return Err("不正な整数エンコード".into());
    }
    let mask = u32::from(marker) + 1;
    let mut value = 0u32;
    for (bit, shift) in [(8, 24), (4, 16), (2, 8), (1, 0)] {
        if mask & bit != 0 {
            match data.get(pos) {
                Some(&b) if b != 0 => value |= u32::from(b) << shift,
                _ => return Err("不正な整数エンコード".into()),
            }
            pos += 1;
        }
    }
    Ok((value, pos))
}

/// Resolve an auto-translate phrase reference; None if the dictionary lacks it.
fn auto_translate_text(group: u8, key: u32) -> Option<String> {
    let dict = dict::current();
    match dict.auto.lookup.get(&u32::from(group)) {
        Some(sheet) => sheet.get(&key),
        None => dict.auto.text.get(&key),
    }
    .cloned()
}

fn ui_color(id: u32) -> Option<String> {
    dict::current().colors.get(&id).cloned()
}

fn bitmap_icon_name(id: u32) -> String {
    dict::current().icons.get(&id).cloned().unwrap_or_else(|| format!("Icon{id}"))
}

enum Token<'a> {
    /// Text, and whether invalid UTF-8 had to be replaced.
    Text(String, bool),
    Macro(u8, &'a [u8]),
    Bad,
}

/// A control sequence is `0x02 kind length payload 0x03`, with the length in the
/// game's packed integer form; the closing byte alone is not a reliable frame.
/// Parsing stops at the first malformed sequence.
fn tokenize(data: &[u8]) -> Vec<Token<'_>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        if data[pos] != 2 {
            let end = data[pos..].iter().position(|&b| b == 2).map_or(data.len(), |i| pos + i);
            let chunk = &data[pos..end];
            match std::str::from_utf8(chunk) {
                Ok(s) => out.push(Token::Text(s.to_owned(), false)),
                Err(_) => out.push(Token::Text(String::from_utf8_lossy(chunk).into_owned(), true)),
            }
            pos = end;
            continue;
        }
        let frame = (|| {
            if pos + 2 >= data.len() {
                return Err(());
            }
            let kind = data[pos + 1];
            let (size, start) = packed_uint(data, pos + 2).map_err(|_| ())?;
            let end = start + size as usize;
            if end >= data.len() || data[end] != 3 {
                return Err(());
            }
            Ok((kind, start, end))
        })();
        match frame {
            Ok((kind, start, end)) => {
                out.push(Token::Macro(kind, &data[start..end]));
                pos = end + 1;
            }
            Err(()) => {
                out.push(Token::Bad);
                break;
            }
        }
    }
    out
}

fn visible(text: &str) -> String {
    text.chars().filter(|&c| c as u32 >= 32 || c == '\n' || c == '\t').collect()
}

/// Auto-translate payload: a bare group byte followed by a packed key.
fn fixed_reference(payload: &[u8]) -> Result<(u8, u32), String> {
    if payload.is_empty() {
        return Err("定型文の参照が空です".into());
    }
    let (key, _) = packed_uint(payload, 1)?;
    Ok((payload[0], key))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Plain visible text; auto-translate phrases are written as ⇒phrase⇐.
pub fn decode_text(data: &[u8]) -> (String, Vec<&'static str>) {
    let mut out = String::new();
    let mut warnings = Vec::new();
    for token in tokenize(data) {
        match token {
            Token::Text(text, replaced) => {
                out.push_str(&visible(&text));
                if replaced {
                    warnings.push("UTF-8置換あり");
                }
            }
            Token::Bad => {
                out.push_str("[不正な制御データ]");
                warnings.push("SeString解析失敗");
            }
            Token::Macro(NEW_LINE, _) => out.push('\n'),
            Token::Macro(NBSP, _) => out.push(' '),
            Token::Macro(HYPHEN, _) => out.push('-'),
            Token::Macro(FIXED, payload) => {
                let phrase = fixed_reference(payload)
                    .ok()
                    .and_then(|(group, key)| auto_translate_text(group, key))
                    .filter(|p| !p.is_empty());
                match phrase {
                    Some(p) => out.push_str(&format!("⇒{p}⇐")),
                    None => out.push_str(&format!("[定型文:{}]", hex(payload))),
                }
            }
            Token::Macro(kind, _) => {
                if !matches!(kind, ICON | COLOR | EDGE_COLOR | BOLD | ITALIC | LINK | COLOR_TYPE | EDGE_COLOR_TYPE)
                    && !ignored(kind)
                {
                    out.push_str(&format!("[制御:{kind:02X}]"));
                }
            }
        }
    }
    (out, warnings)
}

/// Colour macro operand: `Some(Some(rgb))`, `Some(None)` = reset, `None` = unreadable.
fn read_color(payload: &[u8]) -> Option<Option<String>> {
    let first = *payload.first()?;
    if first == 0xEC {
        return Some(None);
    }
    let (value, end) = packed_uint(payload, 0).ok()?;
    if end == payload.len() && first >= 0xF0 {
        Some(Some(format!("#{:06X}", value & 0xFF_FFFF)))
    } else {
        None
    }
}

/// ColorType operand: UIColor row (0 resets).  `None` when unreadable.
fn read_color_type(payload: &[u8]) -> Option<Option<String>> {
    let (id, _) = packed_uint(payload, 0).ok()?;
    Some(if id == 0 { None } else { ui_color(id) })
}

/// Styled span for the viewer.  Keys are omitted when default:
/// s text, c colour, e edge colour, b bold, i italic, l link type,
/// k kind ("at" auto-translate, "icon" bitmap icon), n/a icon id/name, u unresolved.
#[derive(Serialize, Default, Clone, PartialEq, Debug)]
pub struct Span {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub c: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub k: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub u: Option<u8>,
}

#[derive(Default)]
struct Style {
    color: Option<String>,
    edge: Option<String>,
    bold: bool,
    italic: bool,
    link: Option<u8>,
}

fn emit(out: &mut Vec<Span>, style: &Style, text: &str) {
    let span = Span {
        s: Some(text.to_owned()),
        c: style.color.clone(),
        e: style.edge.clone(),
        b: style.bold.then_some(1),
        i: style.italic.then_some(1),
        l: style.link,
        ..Span::default()
    };
    if let Some(last) = out.last_mut() {
        let same_style = Span { s: None, ..last.clone() } == Span { s: None, ..span.clone() };
        if last.k.is_none() && last.s.is_some() && same_style {
            last.s.as_mut().expect("checked").push_str(text);
            return;
        }
    }
    out.push(span);
}

pub fn spans(data: &[u8]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut style = Style::default();
    for token in tokenize(data) {
        match token {
            Token::Text(text, _) => {
                let text = visible(&text);
                if !text.is_empty() {
                    emit(&mut out, &style, &text);
                }
            }
            Token::Bad => emit(&mut out, &style, "[不正な制御データ]"),
            Token::Macro(NEW_LINE, _) => emit(&mut out, &style, "\n"),
            Token::Macro(NBSP, _) => emit(&mut out, &style, " "),
            Token::Macro(HYPHEN, _) => emit(&mut out, &style, "-"),
            Token::Macro(COLOR, payload) => {
                if let Some(parsed) = read_color(payload) {
                    style.color = parsed;
                }
            }
            Token::Macro(COLOR_TYPE, payload) => {
                if let Some(parsed) = read_color_type(payload) {
                    style.color = parsed;
                }
            }
            Token::Macro(kind @ (EDGE_COLOR | EDGE_COLOR_TYPE), payload) => {
                let parsed = if kind == EDGE_COLOR { read_color(payload) } else { read_color_type(payload) };
                if let Some(parsed) = parsed {
                    style.edge = parsed;
                }
            }
            Token::Macro(kind @ (BOLD | ITALIC), payload) => {
                if let Ok((value, _)) = packed_uint(payload, 0) {
                    if kind == BOLD {
                        style.bold = value == 1;
                    } else {
                        style.italic = value == 1;
                    }
                }
            }
            Token::Macro(LINK, payload) => match payload.first() {
                Some(&LINK_TERMINATOR) => style.link = None,
                Some(&kind) => style.link = Some(kind),
                None => {}
            },
            Token::Macro(ICON, payload) => {
                if let Ok((icon, _)) = packed_uint(payload, 0) {
                    out.push(Span {
                        k: Some("icon"),
                        n: Some(icon),
                        a: Some(bitmap_icon_name(icon)),
                        ..Span::default()
                    });
                }
            }
            Token::Macro(FIXED, payload) => {
                let phrase = fixed_reference(payload)
                    .ok()
                    .and_then(|(group, key)| auto_translate_text(group, key))
                    .filter(|p| !p.is_empty());
                out.push(Span {
                    k: Some("at"),
                    s: Some(phrase.clone().unwrap_or_else(|| format!("定型文 {}", hex(payload)))),
                    u: phrase.is_none().then_some(1),
                    ..Span::default()
                });
            }
            Token::Macro(kind, _) => {
                if !ignored(kind) {
                    emit(&mut out, &style, &format!("[制御:{kind:02X}]"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tests need a few entries; install them instead of downloading the real tables.
    fn fixture() {
        let mut dict = dict::Dict::default();
        dict.auto.text.insert(227, "よろしくお願いします！".into());
        dict.colors.insert(500, "#FF7B1A".into());
        dict.icons.insert(88, "CrossWorld".into());
        dict::install(dict);
    }

    fn at(s: &str) -> Span {
        Span { k: Some("at"), s: Some(s.into()), ..Span::default() }
    }

    #[test]
    fn auto_translate_group_is_bare_and_key_is_row_id() {
        fixture();
        // group 2 (greetings), key 227 -> row #227
        let data = b"\x02\x2e\x04\x02\xf0\xe3\x03";
        assert_eq!(spans(data), vec![at("よろしくお願いします！")]);
        assert_eq!(decode_text(data).0, "⇒よろしくお願いします！⇐");
    }

    #[test]
    fn unknown_auto_translate_keeps_reference() {
        fixture();
        let data = b"\x02\x2e\x04\x02\xf0\xff\x03";
        assert_eq!(spans(data)[0].u, Some(1));
        assert!(decode_text(data).0.contains("定型文"));
    }

    #[test]
    fn colour_link_and_icon() {
        fixture();
        let mut data: Vec<u8> = Vec::new();
        data.extend(b"\x02\x48\x04\xf2\x01\xf4\x03A"); // ColorType 500
        data.extend(b"\x02\x48\x02\x01\x03B"); // reset
        data.extend(b"\x02\x27\x07\x01\x11F\x01\xff\x01\x03N"); // player link
        data.extend(b"\x02\x27\x07\xcf\x01\x01\x01\xff\x01\x03"); // link end
        data.extend(b"\x02\x12\x02Y\x03W"); // CrossWorld icon (88)
        let out = spans(&data);
        assert_eq!(out[0], Span { s: Some("A".into()), c: Some("#FF7B1A".into()), ..Span::default() });
        assert_eq!(out[1], Span { s: Some("B".into()), ..Span::default() });
        assert_eq!(out[2], Span { s: Some("N".into()), l: Some(1), ..Span::default() });
        assert_eq!(out[3].k, Some("icon"));
        assert_eq!(out[3].n, Some(88));
        assert_eq!(out[4], Span { s: Some("W".into()), ..Span::default() });
    }

    #[test]
    fn malformed_control_does_not_panic() {
        assert_eq!(spans(b"A\x02\x27\x05").last().unwrap().s.as_deref(), Some("A[不正な制御データ]"));
    }

    #[test]
    fn payload_length_not_first_end_byte() {
        // Payload contains 0x03 internally; framing must use its encoded length.
        let (text, warnings) = decode_text(b"A\x02\x27\x05\x03\x01\x01\x01\x03B");
        assert_eq!(text, "AB");
        assert!(warnings.is_empty());
        assert_eq!(packed_uint(b"\xf2\x01\x03", 0).unwrap(), (259, 3));
        assert!(!decode_text(b"A\x02\x27\x05").1.is_empty());
    }
}
