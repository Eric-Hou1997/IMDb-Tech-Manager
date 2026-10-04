//! Presentation-only messages. Catalogs come from this application build, never
//! from a downloaded pack. Packs supply only text, never executable commands,
//! resource paths, model prompts or protocol schemas. Render the returned strings as text, never as HTML.
use crate::{hash, services::LOCALES, AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;
pub const PRODUCT: &str = "itm";
const PREFIX: &str = "ITM";
pub const MAX_PACK_BYTES: usize = 4 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(rename = "LanguageMessage")]
pub struct Message {
    pub text: String,
    pub protected: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(rename = "LanguageDescriptor")]
pub struct Descriptor {
    pub locale: String,
    pub revision: u32,
    pub asset: String,
    pub released_with: String,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(rename = "PresentationCatalog")]
pub struct Catalog {
    pub schema: u32,
    pub product: String,
    pub app_version: String,
    pub message_set_hash: String,
    pub messages: BTreeMap<String, Message>,
    pub external: BTreeMap<String, Descriptor>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(rename = "LanguagePack")]
pub struct Pack {
    pub schema: u32,
    pub product: String,
    pub locale: String,
    pub revision: u32,
    pub message_set_hash: String,
    pub messages: BTreeMap<String, String>,
}
fn error(code: &str, detail: impl ToString) -> AppError {
    AppError::new(code, detail)
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn version(value: &str) -> Result<semver::Version> {
    let parsed = value
        .strip_prefix('v')
        .and_then(|s| semver::Version::parse(s).ok())
        .ok_or_else(|| {
            error(
                "language-catalog-invalid",
                "Expected a canonical application release version",
            )
        })?;
    if !parsed.pre.is_empty()
        || !parsed.build.is_empty()
        || parsed.major < 4
        || format!("v{parsed}") != value
    {
        return Err(error(
            "language-catalog-invalid",
            "Invalid application release version",
        ));
    }
    Ok(parsed)
}
#[derive(Debug)]
enum Part {
    Text(String),
    Parameter(String),
}
fn parts(value: &str) -> Result<Vec<Part>> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' if chars.peek() == Some(&c) => {
                chars.next();
                text.push(c);
            }
            '{' => {
                if !text.is_empty() {
                    out.push(Part::Text(std::mem::take(&mut text)));
                }
                let mut name = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if !closed
                    || name.is_empty()
                    || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    || name.as_bytes()[0].is_ascii_digit()
                {
                    return Err(error(
                        "language-placeholder-invalid",
                        "Invalid message placeholder",
                    ));
                }
                out.push(Part::Parameter(name));
            }
            '}' => {
                return Err(error(
                    "language-placeholder-invalid",
                    "Unmatched message brace",
                ))
            }
            _ => text.push(c),
        }
    }
    if !text.is_empty() {
        out.push(Part::Text(text));
    }
    Ok(out)
}
fn parameters(value: &str) -> Result<BTreeMap<String, usize>> {
    let mut result = BTreeMap::new();
    for part in parts(value)? {
        if let Part::Parameter(name) = part {
            *result.entry(name).or_default() += 1;
        }
    }
    Ok(result)
}
fn text_valid(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 16384
        && !value.chars().any(|c| {
            (c.is_control() && c != '\n' && c != '\t')
                || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
}
fn han(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}
fn script_valid(locale: &str, source: &str, translated: &str) -> bool {
    // Technical-only labels may legitimately stay unchanged. Otherwise require
    // the target writing system and reject untranslated Han prose in non-CJK packs.
    if !source.chars().any(char::is_alphabetic) {
        return true;
    }
    match locale {
        "ru-RU" => {
            translated
                .chars()
                .any(|c| matches!(c, '\u{0400}'..='\u{052f}'))
                && !translated.chars().any(han)
        }
        "th-TH" => {
            translated
                .chars()
                .any(|c| matches!(c, '\u{0e00}'..='\u{0e7f}'))
                && !translated.chars().any(han)
        }
        "ja-JP" => translated
            .chars()
            .any(|c| han(c) || matches!(c, '\u{3040}'..='\u{30ff}')),
        "en-US" | "fr-FR" | "es-ES" => {
            translated.chars().any(|c| matches!(c, 'A'..='Z' | 'a'..='z' | '\u{00c0}'..='\u{024f}' | '\u{1e00}'..='\u{1eff}')) && !translated.chars().any(han)
        }
        "zh-CN" | "zh-Hant" => translated.chars().any(han),
        _ => false,
    }
}
impl Catalog {
    pub fn source_hash(&self) -> Result<String> {
        Ok(hash(&serde_json::to_vec(&(2u32, PRODUCT, &self.messages))?))
    }
    pub fn validate(&self) -> Result<()> {
        if self.schema != 2
            || self.product != PRODUCT
            || self.messages.is_empty()
            || self.messages.len() > 8192
            || self.message_set_hash != self.source_hash()?
        {
            return Err(error(
                "language-catalog-invalid",
                "Application message catalog does not match its declared schema, product or hash",
            ));
        }
        let current = version(&self.app_version)?;
        for (key, source) in &self.messages {
            if key.is_empty()
                || key.len() > 160
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
                || !text_valid(&source.text)
            {
                return Err(error(
                    "language-catalog-invalid",
                    format!("Invalid message definition: {key}"),
                ));
            }
            parameters(&source.text)?;
            for token in &source.protected {
                if token.is_empty() || !source.text.contains(token) {
                    return Err(error(
                        "language-catalog-invalid",
                        format!("Invalid protected token in {key}"),
                    ));
                }
            }
        }
        let expected: Vec<_> = LOCALES
            .iter()
            .filter(|l| !l.built_in)
            .map(|l| l.code)
            .collect();
        if self.external.len() != expected.len()
            || expected.iter().any(|l| !self.external.contains_key(*l))
        {
            return Err(error("language-catalog-invalid", "Each application version requires exactly one descriptor for every external locale"));
        }
        for (locale, descriptor) in &self.external {
            if descriptor.locale != *locale
                || descriptor.revision == 0
                || !digest(&descriptor.sha256)
                || descriptor.asset
                    != format!("{PREFIX}-Language-{locale}-r{}.json", descriptor.revision)
                || version(&descriptor.released_with)? > current
            {
                return Err(error(
                    "language-catalog-invalid",
                    format!("Invalid language descriptor: {locale}"),
                ));
            }
        }
        Ok(())
    }
    pub fn validate_messages(
        &self,
        locale: &str,
        messages: &BTreeMap<String, String>,
    ) -> Result<()> {
        if !LOCALES.iter().any(|l| l.code == locale) || messages.keys().ne(self.messages.keys()) {
            return Err(error(
                "language-coverage",
                "Translations must cover exactly the application's message set",
            ));
        }
        for (key, source) in &self.messages {
            let translated = &messages[key];
            if !text_valid(translated) || parameters(&source.text)? != parameters(translated)? {
                return Err(error(
                    "language-placeholder-invalid",
                    format!("Invalid text or placeholders in {key}"),
                ));
            }
            let (mut plain_source, mut plain_target) = (source.text.clone(), translated.clone());
            for token in &source.protected {
                if source.text.matches(token).count() != translated.matches(token).count() {
                    return Err(error(
                        "language-protected-token",
                        format!("Protected token changed in {key}"),
                    ));
                }
                plain_source = plain_source.replace(token, "");
                plain_target = plain_target.replace(token, "");
            }
            // Placeholder values are never examined or translated by this layer.
            let plain = |text: &str| -> Result<String> {
                Ok(parts(text)?
                    .into_iter()
                    .filter_map(|p| match p {
                        Part::Text(t) => Some(t),
                        _ => None,
                    })
                    .collect())
            };
            if !script_valid(locale, &plain(&plain_source)?, &plain(&plain_target)?) {
                return Err(error(
                    "language-script",
                    format!("Unexpected writing system in {key}"),
                ));
            }
        }
        Ok(())
    }
    pub fn decode(&self, locale: &str, bytes: &[u8]) -> Result<Pack> {
        self.validate()?;
        let descriptor = self.external.get(locale).ok_or_else(|| {
            error(
                "language-not-external",
                "Built-in or unknown locales cannot be replaced by a language pack",
            )
        })?;
        if bytes.is_empty() || bytes.len() > MAX_PACK_BYTES {
            return Err(error(
                "language-size",
                "Language pack exceeds the size limit",
            ));
        }
        if hash(bytes) != descriptor.sha256 {
            return Err(error(
                "language-hash",
                "Language pack does not match the application catalog",
            ));
        }
        let pack: Pack = serde_json::from_slice(bytes).map_err(|e| error("language-format", e))?;
        if pack.schema != 2
            || pack.product != PRODUCT
            || pack.locale != locale
            || pack.revision != descriptor.revision
            || pack.message_set_hash != self.message_set_hash
        {
            return Err(error(
                "language-incompatible",
                "Language pack identity, revision or message set does not match this application",
            ));
        }
        self.validate_messages(locale, &pack.messages)?;
        Ok(pack)
    }
}
pub fn render(template: &str, values: &BTreeMap<String, String>) -> Result<String> {
    let expected = parameters(template)?;
    if expected.keys().ne(values.keys()) {
        return Err(error(
            "language-parameters",
            "Message arguments do not match the template",
        ));
    }
    let mut result = String::new();
    for part in parts(template)? {
        match part {
            Part::Text(text) => result.push_str(&text),
            Part::Parameter(key) => result.push_str(&values[&key]),
        }
    }
    Ok(result)
}
