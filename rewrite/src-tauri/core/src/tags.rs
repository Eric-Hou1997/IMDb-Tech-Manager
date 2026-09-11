//! Reviewed tag mutations. The embedded manifest is authoritative; text resemblance is not.
use crate::{library::canonical_tag, AppError, Result, Specs};
use roxmltree::Node;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};
use ts_rs::TS;

const OWNER: &str = "IMDb Tech Manager";
mod sidecar;
pub use sidecar::mirror;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct GeneratedEntry {
    pub value: String,
    pub field: String,
    pub source_indexes: Vec<usize>,
    pub confidence: String,
    pub operation: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Action {
    Generate {
        entries: Vec<GeneratedEntry>,
        engine: String,
        model: String,
        prompt_hash: String,
    },
    Edit {
        root_index: usize,
        value: String,
    },
    Delete {
        root_index: usize,
        confirm_external: bool,
    },
    SetOwnership {
        root_index: usize,
        ownership: String,
    },
    AddManual {
        value: String,
    },
    ClearAi {
        confirmed: bool,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct Plan {
    pub action: Action,
    pub timestamp: String,
}
#[derive(Clone)]
struct Entry {
    value: String,
    attrs: BTreeMap<String, String>,
}
struct Manifest<'a, 'input> {
    node: Option<Node<'a, 'input>>,
    attrs: BTreeMap<String, String>,
    entries: Vec<Entry>,
}
fn unsafe_skip(message: &str) -> AppError {
    AppError::new("unsafe-skip", message)
}
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn text(n: Node<'_, '_>) -> String {
    clean(
        &n.descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect::<String>(),
    )
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn element(name: &str, attrs: &BTreeMap<String, String>, body: &str) -> String {
    format!(
        "<{name}{}>{body}</{name}>",
        attrs
            .iter()
            .map(|(k, v)| format!(" {k}=\"{}\"", escape(v)))
            .collect::<String>()
    )
}
fn attrs(n: Node<'_, '_>) -> BTreeMap<String, String> {
    n.attributes()
        .map(|a| (a.name().into(), a.value().into()))
        .collect()
}
fn manifest<'a, 'i>(
    tech: Option<Node<'a, 'i>>,
    name: &str,
    schema: &str,
) -> Result<Manifest<'a, 'i>> {
    let nodes: Vec<_> = tech
        .into_iter()
        .flat_map(|n| n.children())
        .filter(|n| n.has_tag_name(name))
        .collect();
    if nodes.len() > 1 {
        return Err(unsafe_skip("Duplicate ownership manifests"));
    }
    let node = nodes.first().copied();
    if node.is_some_and(|n| {
        n.attribute("owner") != Some(OWNER) || n.attribute("schema") != Some(schema)
    }) {
        return Err(unsafe_skip("Unknown ownership manifest owner or schema"));
    }
    let mut entries = vec![];
    if let Some(n) = node {
        for child in n.children().filter(|n| n.is_element()) {
            if !child.has_tag_name("tag")
                || child.children().any(|n| n.is_element())
                || text(child).is_empty()
            {
                return Err(unsafe_skip("Unsupported ownership entry"));
            }
            entries.push(Entry {
                value: text(child),
                attrs: attrs(child),
            });
        }
    }
    Ok(Manifest {
        node,
        attrs: node.map(attrs).unwrap_or_else(|| {
            BTreeMap::from([
                ("owner".into(), OWNER.into()),
                ("schema".into(), schema.into()),
            ])
        }),
        entries,
    })
}
fn render(name: &str, m: &Manifest<'_, '_>) -> String {
    element(
        name,
        &m.attrs,
        &m.entries
            .iter()
            .map(|e| element("tag", &e.attrs, &escape(&e.value)))
            .collect::<String>(),
    )
}
// Match the original Python sorted, UTF-8 JSON representation, including separators.
fn entry_id(
    prefix: &str,
    value: &str,
    field: &str,
    indexes: &[usize],
    origin: &str,
    replaces: &str,
) -> String {
    let q = |s: &str| serde_json::to_string(s).expect("JSON string");
    let stable = format!("{{\"field\": {}, \"origin\": {}, \"replaces\": {}, \"source_indexes\": [{}], \"value\": {}}}", q(&clean(field)),q(&clean(origin)),q(&clean(replaces)),indexes.iter().map(usize::to_string).collect::<Vec<_>>().join(", "),q(&clean(value)));
    format!("{prefix}-{}", &crate::hash(stable.as_bytes())[..16])
}
fn manual(value: String, field: &str, origin: &str, replaces: &str, timestamp: &str) -> Entry {
    let mut attrs = BTreeMap::from([
        (
            "id".into(),
            entry_id("manual", &value, field, &[], origin, replaces),
        ),
        ("origin".into(), origin.into()),
        ("created".into(), timestamp.into()),
        ("modified".into(), timestamp.into()),
    ]);
    if !field.is_empty() {
        attrs.insert("field".into(), field.into());
    }
    if !replaces.is_empty() {
        attrs.insert("replaces".into(), replaces.into());
    }
    Entry { value, attrs }
}
fn value(s: &str) -> Result<String> {
    let s = clean(s);
    if s.is_empty() || s.chars().count() > 320 || s.chars().any(|c| c.is_control()) {
        return Err(AppError::new(
            "invalid-tag",
            "Tag must contain 1–320 printable characters",
        ));
    }
    Ok(s)
}
fn close(source: &str, node: Node<'_, '_>) -> Result<usize> {
    let re =
        regex::Regex::new(&format!(r"</{}\s*>$", node.tag_name().name())).expect("element name");
    re.find(&source[node.range()])
        .map(|m| node.range().start + m.start())
        .ok_or_else(|| unsafe_skip("Explicit closing element is required"))
}
pub fn effective_specs(raw: &[u8]) -> Result<Specs> {
    let all = std::str::from_utf8(raw).map_err(|e| AppError::new("invalid-encoding", e))?;
    let doc = roxmltree::Document::parse(all.trim_start_matches('\u{feff}'))
        .map_err(|e| AppError::new("invalid-xml", e))?;
    let mut out = Specs::new();
    for tech in doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("technicalspecs") && n.attribute("source") == Some("IMDb"))
    {
        for section in tech.children().filter(|n| n.has_tag_name("section")) {
            crate::collect_specs(
                &mut out,
                section.attribute("name").unwrap_or(""),
                section
                    .children()
                    .filter(|n| n.has_tag_name("item"))
                    .map(text)
                    .collect(),
            );
        }
    }
    Ok(out)
}
/// Produces exact byte edits only at root tags and the two ownership manifests.
pub fn candidate(raw: &[u8], plan: &Plan) -> Result<Vec<u8>> {
    chrono::DateTime::parse_from_rfc3339(&plan.timestamp)
        .map_err(|e| AppError::new("invalid-operation-time", e))?;
    let all = std::str::from_utf8(raw).map_err(|e| AppError::new("invalid-encoding", e))?;
    let source = all.strip_prefix('\u{feff}').unwrap_or(all);
    let bom = all.len() - source.len();
    let doc = roxmltree::Document::parse(source).map_err(|e| AppError::new("invalid-xml", e))?;
    let root = doc.root_element();
    if !["movie", "tvshow", "episodedetails"].contains(&root.tag_name().name())
        || root.descendants().filter(|n| n.is_element()).any(|n| {
            n.tag_name().namespace().is_some() || n.attributes().any(|a| a.namespace().is_some())
        })
    {
        return Err(unsafe_skip("Unsupported media kind or namespaced NFO"));
    }
    let techs: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("technicalspecs"))
        .collect();
    if techs.len() > 1
        || techs.first().is_some_and(|n| {
            n.attribute("source") != Some("IMDb")
                || n.attribute("formatVersion")
                    .is_some_and(|v| v.parse::<u32>().map_or(true, |v| v > 21))
        })
    {
        return Err(unsafe_skip("Ambiguous Technical Specs ownership or schema"));
    }
    let tech = techs.first().copied();
    let mut generated = manifest(tech, "generatedtags", "2")?;
    let mut manual_tags = manifest(tech, "manualtags", "1")?;
    let roots: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("tag") && !text(*n).is_empty())
        .collect();
    if roots.iter().any(|n| n.children().any(|c| c.is_element())) {
        return Err(unsafe_skip("Nested root tag content"));
    }
    // A single authoritative entry cannot disambiguate two identical root tags.
    let mut owned = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for (kind, m) in [("generated", &generated), ("manual", &manual_tags)] {
        for (i, e) in m.entries.iter().enumerate() {
            let key = canonical_tag(&e.value);
            let id = e
                .attrs
                .get("id")
                .filter(|s| !s.is_empty())
                .ok_or_else(|| unsafe_skip("Ownership entry has no stable ID"))?;
            if !ids.insert(id)
                || owned.insert(key.clone(), (kind, i)).is_some()
                || roots
                    .iter()
                    .filter(|n| canonical_tag(&text(**n)) == key)
                    .count()
                    != 1
            {
                return Err(unsafe_skip(
                    "Conflicting, missing or ambiguous owned root tag",
                ));
            }
        }
    }
    let mut edits: Vec<(Range<usize>, String)> = vec![];
    let mut additions = vec![];
    let mut change_generated = false;
    let mut change_manual = false;
    let selected = |index: usize| {
        roots.get(index).copied().ok_or_else(|| {
            AppError::new("invalid-tag-target", "Root tag index is no longer present")
        })
    };
    let require_tech =
        || tech.ok_or_else(|| unsafe_skip("Technical Specs is required for ownership metadata"));
    match &plan.action {
        Action::Generate {
            entries,
            engine,
            model,
            prompt_hash,
        } => {
            require_tech()?;
            if !["ai", "local-rules"].contains(&engine.as_str()) {
                return Err(AppError::new("invalid-engine", "Unknown tag generator"));
            }
            let specs = effective_specs(raw)?;
            if !specs.values().any(|v| !v.is_empty()) {
                return Err(unsafe_skip("No effective specifications for generation"));
            }
            let mut protected = BTreeSet::new();
            for n in &roots {
                let key = canonical_tag(&text(*n));
                if owned
                    .get(&key)
                    .is_some_and(|(kind, _)| *kind == "generated")
                {
                    edits.push((n.range(), String::new()));
                } else {
                    protected.insert(key);
                }
            }
            generated.entries.clear();
            let mut seen = BTreeSet::new();
            for e in entries {
                let v = value(&e.value)?;
                if !crate::specs::TAG_SECTIONS.contains(&e.field.as_str())
                    || e.source_indexes.is_empty()
                    || e.source_indexes
                        .iter()
                        .any(|i| specs.get(&e.field).is_none_or(|v| *i >= v.len()))
                    || !["high", "medium", "low"].contains(&e.confidence.as_str())
                {
                    return Err(AppError::new(
                        "schema-invalid",
                        "Generated tag source or confidence is invalid",
                    ));
                }
                let key = canonical_tag(&v);
                if protected.contains(&key) || !seen.insert(key) {
                    continue;
                }
                additions.push(v.clone());
                let mut attrs = BTreeMap::from([
                    (
                        "id".into(),
                        entry_id("generated", &v, &e.field, &e.source_indexes, "", ""),
                    ),
                    ("origin".into(), "generated".into()),
                    ("field".into(), e.field.clone()),
                    (
                        "sourceIndexes".into(),
                        e.source_indexes
                            .iter()
                            .map(usize::to_string)
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                    ("confidence".into(), e.confidence.clone()),
                ]);
                if !e.operation.is_empty() {
                    attrs.insert("operation".into(), e.operation.clone());
                }
                generated.entries.push(Entry { value: v, attrs });
            }
            generated.attrs.extend([
                ("engine".into(), engine.clone()),
                ("generated".into(), plan.timestamp.clone()),
                ("specHash".into(), crate::specs::fingerprint(&specs)?),
                ("state".into(), "current".into()),
            ]);
            for (key, v) in [("model", model), ("promptHash", prompt_hash)] {
                if v.is_empty() {
                    generated.attrs.remove(key);
                } else {
                    generated.attrs.insert(key.into(), v.clone());
                }
            }
            change_generated = true;
        }
        Action::Edit { root_index, .. }
        | Action::Delete { root_index, .. }
        | Action::SetOwnership { root_index, .. } => {
            let node = selected(*root_index)?;
            let old = text(node);
            let owner = owned.get(&canonical_tag(&old)).copied();
            match &plan.action {
                Action::Edit {
                    value: new_value, ..
                } => {
                    let v = value(new_value)?;
                    if v == old {
                        return Ok(raw.to_vec());
                    }
                    // An edited owned tag must remain distinguishable from all other root tags.
                    if (owner.is_some() || owned.contains_key(&canonical_tag(&v)))
                        && roots.iter().enumerate().any(|(i, n)| {
                            i != *root_index && canonical_tag(&text(*n)) == canonical_tag(&v)
                        })
                    {
                        return Err(unsafe_skip(
                            "Edited owned tag would collide with another root tag",
                        ));
                    }
                    edits.push((node.range(), element("tag", &attrs(node), &escape(&v))));
                    if let Some((kind, i)) = owner {
                        if kind == "generated" {
                            let old = generated.entries.remove(i);
                            manual_tags.entries.push(manual(
                                v,
                                old.attrs.get("field").map(String::as_str).unwrap_or(""),
                                "manual-edit",
                                &old.attrs["id"],
                                &plan.timestamp,
                            ));
                            change_generated = true;
                            change_manual = true;
                        } else {
                            manual_tags.entries[i].value = v;
                            manual_tags.entries[i]
                                .attrs
                                .insert("modified".into(), plan.timestamp.clone());
                            change_manual = true;
                        }
                    }
                }
                Action::Delete {
                    confirm_external, ..
                } => {
                    if owner.is_none() && !confirm_external {
                        return Err(AppError::new(
                            "external-confirmation-required",
                            "External tag deletion requires explicit confirmation",
                        ));
                    }
                    edits.push((node.range(), String::new()));
                    if let Some((kind, i)) = owner {
                        if kind == "generated" {
                            generated.entries.remove(i);
                            change_generated = true;
                        } else {
                            manual_tags.entries.remove(i);
                            change_manual = true;
                        }
                    }
                }
                Action::SetOwnership { ownership, .. } => {
                    if !["external", "manual", "ai", "local-rules"].contains(&ownership.as_str()) {
                        return Err(AppError::new(
                            "invalid-ownership",
                            "Unknown ownership choice",
                        ));
                    }
                    let current = match owner {
                        None => "external",
                        Some(("manual", _)) => "manual",
                        Some((_, i)) => generated.entries[i]
                            .attrs
                            .get("engine")
                            .or_else(|| generated.attrs.get("engine"))
                            .map(String::as_str)
                            .unwrap_or(""),
                    };
                    if current == ownership {
                        return Ok(raw.to_vec());
                    }
                    if ownership != "external" {
                        require_tech()?;
                        if roots
                            .iter()
                            .filter(|n| canonical_tag(&text(**n)) == canonical_tag(&old))
                            .count()
                            != 1
                        {
                            return Err(unsafe_skip("Cannot assign ambiguous root tags"));
                        }
                    }
                    if let Some((kind, i)) = owner {
                        if kind == "generated" {
                            generated.entries.remove(i);
                            change_generated = true;
                        } else {
                            manual_tags.entries.remove(i);
                            change_manual = true;
                        }
                    }
                    if ownership == "manual" {
                        manual_tags.entries.push(manual(
                            old,
                            "",
                            "manual-takeover",
                            "",
                            &plan.timestamp,
                        ));
                        change_manual = true;
                    } else if ownership != "external" {
                        generated.entries.push(Entry {
                            attrs: BTreeMap::from([
                                (
                                    "id".into(),
                                    entry_id("generated", &old, "", &[], "manual-takeover", ""),
                                ),
                                ("origin".into(), "manual-takeover".into()),
                                ("engine".into(), ownership.clone()),
                                ("operation".into(), "takeover".into()),
                            ]),
                            value: old,
                        });
                        change_generated = true;
                    }
                }
                _ => unreachable!(),
            }
        }
        Action::AddManual { value: v } => {
            require_tech()?;
            let v = value(v)?;
            if roots
                .iter()
                .any(|n| canonical_tag(&text(*n)) == canonical_tag(&v))
            {
                return Err(unsafe_skip(
                    "Existing tag cannot be claimed by adding a manual tag",
                ));
            }
            additions.push(v.clone());
            manual_tags
                .entries
                .push(manual(v, "", "manual-add", "", &plan.timestamp));
            change_manual = true;
        }
        Action::ClearAi { confirmed } => {
            require_tech()?;
            if !confirmed {
                return Err(AppError::new(
                    "confirmation-required",
                    "Clearing AI tags requires confirmation",
                ));
            }
            let block = generated
                .attrs
                .get("engine")
                .map(String::as_str)
                .unwrap_or("");
            let keys: BTreeSet<_> = generated
                .entries
                .iter()
                .filter(|e| e.attrs.get("engine").map(String::as_str).unwrap_or(block) == "ai")
                .map(|e| canonical_tag(&e.value))
                .collect();
            for n in &roots {
                if keys.contains(&canonical_tag(&text(*n))) {
                    edits.push((n.range(), String::new()));
                }
            }
            generated
                .entries
                .retain(|e| !keys.contains(&canonical_tag(&e.value)));
            change_generated = !keys.is_empty();
        }
    }
    // Reuse the original generated root slots, so rebuilding does not accumulate
    // blank lines or reorder unrelated tags on every run.
    if matches!(plan.action, Action::Generate { .. }) {
        let mut values = additions.into_iter();
        for (range, replacement) in &mut edits {
            if replacement.is_empty() && roots.iter().any(|n| n.range() == *range) {
                if let Some(value) = values.next() {
                    *replacement = format!("<tag>{}</tag>", escape(&value));
                }
            }
        }
        additions = values.collect();
    }
    if change_generated && generated.node.is_none() {
        for (key, value) in [
            ("engine", "preserved".into()),
            ("generated", plan.timestamp.clone()),
            (
                "specHash",
                crate::specs::fingerprint(&effective_specs(raw)?)?,
            ),
            ("state", "current".into()),
        ] {
            generated.attrs.entry(key.into()).or_insert(value);
        }
    }
    let mut append = String::new();
    for (name, m, changed) in [
        ("generatedtags", &generated, change_generated),
        ("manualtags", &manual_tags, change_manual),
    ] {
        if !changed {
            continue;
        }
        let rendered = render(name, m);
        if let Some(n) = m.node {
            edits.push((n.range(), rendered));
        } else {
            append.push_str(&rendered);
        }
    }
    if !append.is_empty() {
        let at = close(source, require_tech()?)?;
        edits.push((at..at, append));
    }
    if !additions.is_empty() {
        let at = roots
            .last()
            .map(|n| n.range().end)
            .or_else(|| {
                root.children()
                    .find(|n| n.has_tag_name("actor"))
                    .map(|n| n.range().start)
            })
            .unwrap_or(close(source, root)?);
        let nl = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        edits.push((
            at..at,
            additions
                .iter()
                .map(|v| format!("{nl}  <tag>{}</tag>", escape(v)))
                .collect(),
        ));
    }
    edits.sort_by_key(|(r, _)| (r.start, r.end));
    let mut out = raw[..bom].to_vec();
    let mut cursor = 0;
    for (r, replacement) in edits {
        if r.start < cursor {
            return Err(unsafe_skip("Overlapping tag mutations"));
        }
        out.extend_from_slice(&source.as_bytes()[cursor..r.start]);
        out.extend_from_slice(replacement.as_bytes());
        cursor = r.end;
    }
    out.extend_from_slice(&source.as_bytes()[cursor..]);
    roxmltree::Document::parse(
        std::str::from_utf8(&out)
            .expect("UTF-8 candidate")
            .trim_start_matches('\u{feff}'),
    )
    .map_err(|e| AppError::new("invalid-candidate", e))?;
    Ok(out)
}
