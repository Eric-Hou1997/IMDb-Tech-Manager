//! Pure local tag generation. Ownership authorization belongs to the write use case.
use crate::{ownership_key, specs::TAG_SECTIONS, Specs};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub value: String,
    pub field: String,
    pub source_indexes: Vec<usize>,
}
macro_rules! re {
    ($pattern:literal) => {{
        static VALUE: std::sync::LazyLock<Regex> =
            std::sync::LazyLock::new(|| Regex::new($pattern).expect("constant rule expression"));
        &*VALUE
    }};
}
fn clean(value: &str) -> String {
    html_escape::decode_html_entities(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn unique(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|v| clean(&v))
        .filter(|v| !v.is_empty() && seen.insert(ownership_key(v)))
        .collect()
}
fn chunks(value: &str) -> Vec<String> {
    let mut depth = 0usize;
    let mut start = 0;
    let mut out = vec![];
    for (i, c) in value.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' | ';' if depth == 0 => {
                if !value[start..i].trim().is_empty() {
                    out.push(value[start..i].trim().into());
                }
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if !value[start..].trim().is_empty() {
        out.push(value[start..].trim().into());
    }
    out
}
fn without_scene(value: &str) -> String {
    let pattern = re!(r"(?i)\s*\([^)]*\)\s*$");
    if let Some(m) = pattern.find(value) {
        if re!(r"(?i)scene|shot").is_match(m.as_str()) {
            return value[..m.start()].trim().into();
        }
    }
    value.trim().into()
}
fn fold(chunks: Vec<String>) -> Vec<String> {
    let mut out = vec![];
    let mut i = 0;
    while i < chunks.len() {
        let first = clean(&chunks[i]);
        if !re!(r"\b[A-Za-z0-9]+-$").is_match(&first) {
            out.push(first);
            i += 1;
            continue;
        }
        let mut group = vec![first.clone()];
        let mut j = i + 1;
        let mut complete = false;
        while j < chunks.len() {
            let next = clean(&chunks[j]);
            if re!(r"^[A-Za-z0-9]+-$").is_match(&next) {
                group.push(next);
                j += 1;
                continue;
            }
            if re!(r"(?i)^[A-Za-z0-9]+-\s+(?:and|&)\s+[A-Za-z0-9]+-Series\s+.+$").is_match(&next) {
                group.push(next);
                j += 1;
                complete = true;
            }
            break;
        }
        if complete {
            out.push(group.join(", "));
            i = j;
        } else {
            out.push(first);
            i += 1;
        }
    }
    out
}
fn expand(value: &str) -> Vec<String> {
    let pattern = re!(
        r"(?i)^(.+?\s)([A-Za-z0-9]+)-((?:\s*,\s*[A-Za-z0-9]+-)*)\s*(?:,\s*)?(?:and|&)\s*([A-Za-z0-9]+)-Series\s+(.+)$"
    );
    let Some(m) = pattern.captures(value) else {
        return vec![value.into()];
    };
    let mut series = vec![m[2].to_string()];
    series.extend(
        re!(r"([A-Za-z0-9]+)-")
            .captures_iter(&m[3])
            .map(|c| c[1].to_string()),
    );
    series.push(m[4].into());
    unique(
        series
            .into_iter()
            .map(|name| format!("{}{name}-Series {}", &m[1], &m[5])),
    )
}
fn cross_maker(value: &str) -> Vec<String> {
    let pattern = re!(r"(?i)^(.+?)\s+(?:and|&)\s+(.+?\s+(Lens|Lenses))$");
    let Some(m) = pattern.captures(value) else {
        return vec![value.into()];
    };
    let mut left = clean(&m[1]);
    let right = clean(&m[2]);
    let maker = re!(
        r"(?i)^(?:ARRI|Arriflex|Panavision|Zeiss|Angenieux|Cooke|Leica|Leitz|Canon|Fujinon|Sony|RED|Vantage|Hawk|Kowa|Lomo|Nikon|Sigma|Tokina|Atlas|Schneider|Technovision|Blackmagic|DJI|Rodenstock|ISCO|P\+S Technik)\b"
    );
    if !maker.is_match(&left) || !maker.is_match(&right) {
        return vec![value.into()];
    }
    if !re!(r"(?i)\b(?:Lens|Lenses)$").is_match(&left) {
        left = format!("{} {}", left, &m[3]);
    }
    unique([left, right])
}
pub fn camera_tags(value: &str) -> Vec<String> {
    let values = chunks(&clean(value))
        .iter()
        .map(|v| without_scene(v))
        .filter(|v| !v.is_empty())
        .collect();
    unique(
        fold(values)
            .iter()
            .flat_map(|v| expand(v))
            .flat_map(|v| cross_maker(&v)),
    )
}
pub fn values(section: &str, value: &str) -> Vec<String> {
    if section == "Camera" {
        return camera_tags(value);
    }
    if section == "Sound mix" {
        let value = clean(value);
        if re!(r"(?i)^DTS\s*\(\s*DTS\s*:\s*X\s*\)$").is_match(&value) {
            return vec!["DTS:X".into()];
        }
        return unique([re!(r"\s*\([^)]*\)\s*$").replace(&value, "").trim().into()]);
    }
    if section == "Aspect ratio" {
        let value = clean(value);
        let value = re!(r"\s*\([^)]*\)\s*$").replace(&value, "").into_owned();
        return unique([re!(r"\s*:\s*").replace_all(&value, ":").into_owned()]);
    }
    if value.trim().is_empty() {
        vec![]
    } else {
        vec![value.trim().into()]
    }
}
pub fn entries(specs: &Specs) -> Vec<Entry> {
    let mut seen = HashSet::new();
    let mut out = vec![];
    for field in TAG_SECTIONS {
        for (index, value) in specs.get(field).into_iter().flatten().enumerate() {
            for value in values(field, value) {
                let value = clean(&value);
                if !value.is_empty() && seen.insert(ownership_key(&value)) {
                    out.push(Entry {
                        value,
                        field: field.into(),
                        source_indexes: vec![index],
                    });
                }
            }
        }
    }
    out
}
