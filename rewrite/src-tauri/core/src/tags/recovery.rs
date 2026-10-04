//! v4.1 sidecars recover stripped manifests in memory. Only an approved NFO
//! transaction persists the recovered metadata; reading never rewrites media.
use super::*;
use crate::MediaItem;
use serde_json::Value;
use std::path::Path;

fn string(value: &Value, key: &str) -> Result<String> {
    match value.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(clean(s)),
        _ => Err(unsafe_skip("Invalid ownership sidecar metadata")),
    }
}
fn entries(record: &Value, key: &str, prefix: &str) -> Result<Vec<Entry>> {
    let rows = match record.get(key) {
        None if key == "manual_entries" => return Ok(vec![]),
        Some(Value::Array(rows)) => rows,
        _ => return Err(unsafe_skip("Invalid ownership sidecar entries")),
    };
    let mut out = vec![];
    for row in rows {
        if !row.is_object() {
            return Err(unsafe_skip("Invalid ownership sidecar entry"));
        }
        let value = value(&string(row, "value")?)?;
        let field = string(row, "field")?;
        let origin = string(row, "origin")?;
        let replaces = string(row, "replaces")?;
        let indexes = match row.get("source_indexes") {
            None => vec![],
            Some(Value::Array(values)) => values
                .iter()
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| unsafe_skip("Invalid ownership source indexes"))
                })
                .collect::<Result<Vec<_>>>()?,
            _ => return Err(unsafe_skip("Invalid ownership source indexes")),
        };
        let mut attrs = BTreeMap::new();
        for key in [
            "id",
            "origin",
            "field",
            "confidence",
            "operation",
            "replaces",
            "created",
            "modified",
            "engine",
        ] {
            let val = string(row, key)?;
            if !val.is_empty() {
                attrs.insert(key.into(), val);
            }
        }
        attrs
            .entry("id".into())
            .or_insert_with(|| entry_id(prefix, &value, &field, &indexes, &origin, &replaces));
        attrs.entry("origin".into()).or_insert_with(|| {
            if prefix == "manual" {
                "manual-add"
            } else {
                "generated"
            }
            .into()
        });
        if !indexes.is_empty() {
            attrs.insert(
                "sourceIndexes".into(),
                indexes
                    .iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        out.push(Entry { value, attrs });
    }
    Ok(out)
}
fn manifests(
    record: &Value,
    path: &Path,
    imdb: &str,
) -> Result<(Manifest<'static, 'static>, Manifest<'static, 'static>)> {
    if record["schema"].as_u64() != Some(2)
        || record["path"].as_str() != path.to_str()
        || !string(record, "imdb")?.eq_ignore_ascii_case(imdb)
        || imdb.is_empty()
    {
        return Err(unsafe_skip(
            "Ownership sidecar identity or schema does not match NFO",
        ));
    }
    let engine = string(record, "engine")?;
    if !["ai", "local-rules", "preserved"].contains(&engine.as_str()) {
        return Err(unsafe_skip("Unknown ownership sidecar engine"));
    }
    let mut generated = manifest(None, "generatedtags", "2")?;
    generated.entries = entries(record, "entries", "generated")?;
    generated.attrs.insert("engine".into(), engine);
    for (json_key, xml_key) in [
        ("model", "model"),
        ("prompt_hash", "promptHash"),
        ("spec_hash", "specHash"),
        ("state", "state"),
        ("generated", "generated"),
    ] {
        let val = string(record, json_key)?;
        if !val.is_empty() {
            generated.attrs.insert(xml_key.into(), val);
        }
    }
    generated
        .attrs
        .entry("state".into())
        .or_insert("current".into());
    let mut manual = manifest(None, "manualtags", "1")?;
    manual.entries = entries(record, "manual_entries", "manual")?;
    let mut ids = BTreeSet::new();
    let mut values = BTreeSet::new();
    for entry in generated.entries.iter().chain(&manual.entries) {
        if !ids.insert(entry.attrs["id"].clone()) || !values.insert(canonical_tag(&entry.value)) {
            return Err(unsafe_skip("Conflicting ownership sidecar entries"));
        }
    }
    Ok((generated, manual))
}

/// Overlay only when no embedded generator exists. The indexed NFO facts stay
/// untouched so a changed/removed mirror cannot leave cached ownership behind.
pub fn overlay(item: &mut MediaItem, record: &Value) -> Result<()> {
    if !item.inspection.tag_engine.is_empty()
        || item.error.is_some()
        || item.spec_status == "missing"
    {
        return Ok(());
    }
    let (generated, manual) = manifests(record, Path::new(&item.path), &item.imdb)?;
    for entry in generated.entries.iter().chain(&manual.entries) {
        if item
            .tags
            .iter()
            .filter(|tag| canonical_tag(&tag.value) == canonical_tag(&entry.value))
            .count()
            > 1
        {
            return Err(unsafe_skip("Ambiguous owned root tags"));
        }
    }
    // v4.1 _tag_rows reads embedded metadata only. Sidecar recovery affects
    // tag_state and automatic writers, not the Inspector's ownership badges
    // or the meaning of its explicit manual tag actions.
    let have: BTreeSet<_> = item.tags.iter().map(|t| canonical_tag(&t.value)).collect();
    let spec_hash = crate::specs::fingerprint(&item.specs)?;
    let engine = &generated.attrs["engine"];
    let i = &mut item.inspection;
    i.tag_status = if generated
        .entries
        .iter()
        .any(|e| !have.contains(&canonical_tag(&e.value)))
    {
        "tag-missing"
    } else if generated
        .attrs
        .get("specHash")
        .is_some_and(|s| !s.is_empty() && s != &spec_hash)
    {
        "stale"
    } else if generated.attrs["state"] == "review" {
        "review"
    } else if engine == "ai" {
        "ai-current"
    } else if engine == "local-rules" {
        "local-current"
    } else {
        "current"
    }
    .into();
    i.issues
        .retain(|s| !["tag-missing", "stale", "review"].contains(&s.as_str()));
    if ["tag-missing", "stale", "review"].contains(&i.tag_status.as_str()) {
        i.issues.push(i.tag_status.clone());
    }
    i.manifest_sidecar_match = None;
    item.inspection.lifecycle = crate::inspector::lifecycle(item);
    Ok(())
}

pub fn needed(raw: &[u8]) -> Result<bool> {
    let text = std::str::from_utf8(raw).map_err(|e| AppError::new("invalid-encoding", e))?;
    let doc = roxmltree::Document::parse(text.trim_start_matches('\u{feff}'))
        .map_err(|e| AppError::new("invalid-xml", e))?;
    Ok(doc.root_element().children().any(|n| {
        n.has_tag_name("technicalspecs")
            && n.attribute("source") == Some("IMDb")
            && !n.children().any(|c| {
                c.has_tag_name("generatedtags")
                    && c.attribute("engine").is_some_and(|s| !s.trim().is_empty())
            })
    }))
}

pub fn restore(raw: &[u8], path: &Path, record: &Value) -> Result<Vec<u8>> {
    if !needed(raw)? {
        return Err(unsafe_skip(
            "Embedded ownership takes precedence over recovery",
        ));
    }
    let all = std::str::from_utf8(raw).map_err(|e| AppError::new("invalid-encoding", e))?;
    let source = all.trim_start_matches('\u{feff}');
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
    if techs.len() != 1
        || techs[0].attribute("source") != Some("IMDb")
        || techs[0]
            .attribute("formatVersion")
            .is_some_and(|v| v.parse::<u32>().map_or(true, |v| v > 21))
    {
        return Err(unsafe_skip("Ambiguous Technical Specs ownership or schema"));
    }
    let tech = techs[0];
    let imdb = tech.attribute("imdbid").unwrap_or_else(|| {
        root.children()
            .find(|n| n.has_tag_name("uniqueid") && n.attribute("type") == Some("imdb"))
            .and_then(|n| n.text())
            .unwrap_or("")
    });
    let (generated, manual) = manifests(record, path, imdb)?;
    let old_generated = manifest(Some(tech), "generatedtags", "2")?;
    let old_manual = manifest(Some(tech), "manualtags", "1")?;
    if !old_generated.entries.is_empty() {
        return Err(unsafe_skip(
            "Embedded ownership cannot be replaced by sidecar",
        ));
    }
    // Existing embedded manual ownership is authoritative even during recovery.
    let mut additions = render("generatedtags", &generated);
    if old_manual.node.is_none() && !manual.entries.is_empty() {
        additions.push_str(&render("manualtags", &manual));
    }
    let at = close(source, tech)?;
    let mut edits = vec![(at..at, additions)];
    if let Some(node) = old_generated.node {
        edits.push((node.range(), String::new()));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut result = raw.to_vec();
    for (range, value) in edits {
        result.splice(bom + range.start..bom + range.end, value.bytes());
    }
    let roots: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("tag") && !text(*n).is_empty())
        .collect();
    if roots.iter().any(|n| n.children().any(|c| c.is_element())) {
        return Err(unsafe_skip("Nested root tag content"));
    }
    owned_roots(
        &generated,
        if old_manual.node.is_some() {
            &old_manual
        } else {
            &manual
        },
        &roots,
    )?;
    Ok(result)
}
