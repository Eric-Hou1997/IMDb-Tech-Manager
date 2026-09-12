//! Cross-platform counterpart of the original HTMLLineExtractor/parse_specs.
//! HTML tokenization uses the already locked html5ever dependency; no scripts run.
use super::*;
use html5ever::tokenizer::{
    states::RawKind, BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer,
};
use std::cell::RefCell;

#[derive(Default)]
struct Lines {
    text: String,
    fragments: Vec<String>,
    lines: Vec<(String, bool)>,
    visual_text: String,
    visual_lines: Vec<(String, bool)>,
    table_value: bool,
    skip: usize,
    identities: Vec<String>,
}
fn legacy_clean(value: &str) -> String {
    clean(&html_escape::decode_html_entities(value))
}
impl Lines {
    fn fragment(&mut self) {
        let value = legacy_clean(&std::mem::take(&mut self.text));
        if !value.is_empty() {
            self.fragments.push(value);
        }
    }
    fn flush(&mut self) {
        self.fragment();
        let value = legacy_clean(&self.fragments.join(" "));
        self.fragments.clear();
        if !value.is_empty() {
            self.lines.push((value, self.table_value));
        }
        let visual = legacy_clean(&std::mem::take(&mut self.visual_text));
        if !visual.is_empty() {
            self.visual_lines.push((visual, self.table_value));
        }
    }
}
struct Extractor(RefCell<Lines>);
impl TokenSink for Extractor {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        let mut out = self.0.borrow_mut();
        match token {
            Token::CharacterTokens(text) if out.skip == 0 => {
                out.text.push_str(&text);
                out.visual_text.push_str(&text);
            }
            Token::TagToken(tag) => {
                out.fragment();
                let name = tag.name.as_ref();
                let start = tag.kind == TagKind::StartTag;
                if matches!(name, "script" | "style" | "noscript" | "svg") {
                    if start {
                        out.skip += 1;
                    } else {
                        out.skip = out.skip.saturating_sub(1);
                    }
                    if start && matches!(name, "script" | "style") {
                        return TokenSinkResult::RawData(if name == "script" {
                            RawKind::ScriptData
                        } else {
                            RawKind::Rawtext
                        });
                    }
                } else if out.skip == 0 {
                    let attr = |key: &str| {
                        tag.attrs
                            .iter()
                            .find(|a| a.name.local.as_ref() == key)
                            .map(|a| a.value.to_string())
                    };
                    if start
                        && name == "link"
                        && attr("rel").is_some_and(|v| {
                            v.split_whitespace()
                                .any(|v| v.eq_ignore_ascii_case("canonical"))
                        })
                    {
                        if let Some(url) = attr("href") {
                            out.identities.push(url);
                        }
                    }
                    if start
                        && name == "meta"
                        && attr("property").is_some_and(|v| v.eq_ignore_ascii_case("og:url"))
                    {
                        if let Some(url) = attr("content") {
                            out.identities.push(url);
                        }
                    }
                    if matches!(
                        name,
                        "article"
                            | "aside"
                            | "blockquote"
                            | "br"
                            | "dd"
                            | "div"
                            | "dl"
                            | "dt"
                            | "footer"
                            | "h1"
                            | "h2"
                            | "h3"
                            | "h4"
                            | "h5"
                            | "h6"
                            | "header"
                            | "li"
                            | "main"
                            | "nav"
                            | "ol"
                            | "p"
                            | "section"
                            | "td"
                            | "th"
                            | "tr"
                            | "ul"
                    ) {
                        out.flush();
                    }
                    if name == "td" {
                        out.table_value = start;
                    }
                }
            }
            Token::EOFToken => out.flush(),
            Token::CommentToken(_) => out.fragment(),
            _ => {}
        }
        TokenSinkResult::Continue
    }
}
fn append(values: &mut Vec<String>, value: &str) {
    let value = legacy_clean(value);
    if value.starts_with('(') && !values.is_empty() {
        let previous = values.last_mut().unwrap();
        *previous = legacy_clean(&format!("{previous} {value}"));
        return;
    }
    static BULLETS: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\s+[·•]\s+").unwrap());
    for value in BULLETS.split(&value) {
        let value = value.strip_prefix(['•', '·', '-']).unwrap_or(value);
        let value = legacy_clean(value);
        if !value.is_empty() {
            values.push(value);
        }
    }
}
fn parse(lines: &[(String, bool)]) -> Specs {
    let mut result: Specs = SECTIONS.into_iter().map(|s| (s.into(), vec![])).collect();
    let mut current = None;
    let mut headings = SECTIONS;
    headings.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let more = regex::Regex::new(r"(?i)^\d+\s+more$").unwrap();
    for (raw, table_value) in lines {
        let mut line = legacy_clean(raw);
        while line.starts_with(['•', '·']) {
            line = line.chars().skip(1).collect::<String>().trim_start().into();
        }
        if line.is_empty() {
            continue;
        }
        // A table value "Color" is a fact, even though the heading is also
        // "Color". The old flattened parser dropped it as another heading.
        if *table_value && current == Some(line.as_str()) {
            append(result.get_mut(current.unwrap()).unwrap(), &line);
            continue;
        }
        if SECTIONS.contains(&line.as_str()) {
            current = SECTIONS.iter().copied().find(|s| *s == line);
            continue;
        }
        if let Some(heading) = headings.iter().find(|h| line.starts_with(&format!("{h} "))) {
            current = Some(*heading);
            append(result.get_mut(*heading).unwrap(), &line[heading.len()..]);
            continue;
        }
        if matches!(
            line.as_str(),
            "Edit"
                | "See more"
                | "See all"
                | "Technical specifications"
                | "Technical Specifications"
        ) || more.is_match(&line)
        {
            continue;
        }
        if [
            "Contribute to this page",
            "Suggest an edit",
            "More from this title",
            "Recently viewed",
        ]
        .iter()
        .any(|s| line.starts_with(s))
        {
            current = None;
            continue;
        }
        if let Some(section) = current {
            append(result.get_mut(section).unwrap(), &line);
        }
    }
    for values in result.values_mut() {
        let mut seen = std::collections::HashSet::new();
        values.retain(|v| seen.insert(crate::ownership_key(v)));
    }
    result
}
pub(super) fn fallback(
    imdb: &str,
    page: &str,
    structured_identity: bool,
) -> Result<Option<(Specs, &'static str)>> {
    let tokenizer = Tokenizer::new(
        Extractor(RefCell::new(Lines::default())),
        Default::default(),
    );
    let input = BufferQueue::default();
    input.push_back(page.into());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let lines = tokenizer.sink.0.into_inner();
    let mut specs = parse(&lines.lines);
    let mut parser = "html-lines";
    if !specs.values().any(|v| !v.is_empty()) {
        // Preserve inline adjacency for the visual-text fallback. The old
        // macOS textutil path performed this conversion in another process.
        specs = parse(&lines.visual_lines);
        parser = "html-visual-text";
    }
    if !specs.values().any(|v| !v.is_empty()) {
        return Ok(None);
    }
    for identity in &lines.identities {
        let matches = url::Url::parse(identity).is_ok_and(|url| {
            url.scheme() == "https" && matches!(url.host_str(), Some("www.imdb.com" | "imdb.com" | "m.imdb.com"))
                && url.username().is_empty() && url.password().is_none() && url.port().is_none()
                && matches!(url.path().trim_end_matches('/'), path if path == format!("/title/{imdb}") || path == format!("/title/{imdb}/technical"))
        });
        if !matches {
            return Err(AppError::new(
                "imdb-title-mismatch",
                "HTML canonical identity differs from the requested title",
            ));
        }
    }
    if !structured_identity && lines.identities.is_empty() {
        return Err(AppError::new(
            "imdb-identity-unconfirmed",
            "HTML specifications have no verifiable title identity",
        ));
    }
    Ok(Some((specs, parser)))
}
