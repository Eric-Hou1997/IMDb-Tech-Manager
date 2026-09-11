use crate::{AppError, Result, Specs};
use serde_json::Value;
pub const SECTIONS: [&str; 10] = [
    "Runtime",
    "Sound mix",
    "Color",
    "Aspect ratio",
    "Camera",
    "Laboratory",
    "Film Length",
    "Negative Format",
    "Cinematographic Process",
    "Printed Film Format",
];
pub const TAG_SECTIONS: [&str; 6] = [
    "Sound mix",
    "Camera",
    "Aspect ratio",
    "Negative Format",
    "Cinematographic Process",
    "Printed Film Format",
];
fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn attrs(item: &Value) -> Vec<String> {
    item["attributes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["text"].as_str())
        .map(clean)
        .filter(|s| !s.is_empty())
        .collect()
}
fn find_title(value: &Value) -> Option<&Value> {
    if value.get("runtimes").is_some() && value.get("technicalSpecifications").is_some() {
        return Some(value);
    }
    match value {
        Value::Object(m) => m.values().find_map(find_title),
        Value::Array(a) => a.iter().find_map(find_title),
        _ => None,
    }
}
// Structured Next data is the first migrated acquisition format. HTML fallback remains a separate gate.
pub fn parse_next_data(value: &Value) -> Result<Specs> {
    let title = find_title(value).ok_or_else(|| {
        AppError::new(
            "imdb-payload-missing",
            "No structured IMDb technical title found",
        )
    })?;
    let mut out: Specs = SECTIONS.into_iter().map(|s| (s.into(), vec![])).collect();
    for edge in title["runtimes"]["edges"].as_array().into_iter().flatten() {
        let node = &edge["node"];
        let mut base = clean(
            node["displayableProperty"]["value"]["plainText"]
                .as_str()
                .unwrap_or(""),
        );
        if base.is_empty() {
            continue;
        }
        let mut extras = vec![];
        if let Some(seconds) = node["seconds"].as_f64().filter(|s| *s > 0.0) {
            extras.push(format!("{} min", (seconds / 60.0).round_ties_even() as u64));
        }
        extras.extend(attrs(node));
        if let Some(country) = node["country"]["text"]
            .as_str()
            .map(clean)
            .filter(|s| !s.is_empty())
        {
            extras.push(country);
        }
        for extra in extras {
            base.push_str(&format!(" ({extra})"));
        }
        out.get_mut("Runtime").unwrap().push(base);
    }
    for (section, group, key) in [
        ("Sound mix", "soundMixes", "text"),
        ("Color", "colorations", "text"),
        ("Aspect ratio", "aspectRatios", "aspectRatio"),
        ("Camera", "cameras", "camera"),
        ("Laboratory", "laboratories", "laboratory"),
        ("Negative Format", "negativeFormats", "negativeFormat"),
        ("Cinematographic Process", "processes", "process"),
        ("Printed Film Format", "printedFormats", "printedFormat"),
    ] {
        for item in title["technicalSpecifications"][group]["items"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let mut base = clean(item[key].as_str().unwrap_or(""));
            if base.is_empty() {
                continue;
            }
            let extras = attrs(item);
            if !extras.is_empty() {
                base.push_str(&format!(" ({})", extras.join(", ")));
            }
            out.get_mut(section).unwrap().push(base);
        }
    }
    for item in title["technicalSpecifications"]["filmLengths"]["items"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let mut base = clean(
            item["displayableProperty"]["value"]["plainText"]
                .as_str()
                .unwrap_or(""),
        );
        if base.is_empty() {
            continue;
        }
        let mut extras = attrs(item);
        extras.extend(
            item["countries"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v["text"].as_str())
                .map(clean)
                .filter(|s| !s.is_empty()),
        );
        if !extras.is_empty() {
            base.push_str(&format!(" ({})", extras.join(", ")));
        }
        out.get_mut("Film Length").unwrap().push(base);
    }
    for values in out.values_mut() {
        let mut seen = std::collections::HashSet::new();
        values.retain(|v| seen.insert(crate::ownership_key(v)));
    }
    Ok(out)
}
pub fn fingerprint(specs: &Specs) -> Result<String> {
    let ordered: Specs = SECTIONS
        .into_iter()
        .map(|s| (s.into(), specs.get(s).cloned().unwrap_or_default()))
        .collect();
    Ok(crate::hash(serde_json::to_string(&ordered)?.as_bytes()))
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn validate_specs_only(before: &[u8], after: &[u8]) -> Result<()> {
    fn protected(raw: &[u8]) -> Result<(Vec<u8>, Vec<String>)> {
        let all = std::str::from_utf8(raw).map_err(|e| AppError::new("invalid-encoding", e))?;
        let source = all.strip_prefix('\u{feff}').unwrap_or(all);
        let bom = all.len() - source.len();
        let doc =
            roxmltree::Document::parse(source).map_err(|e| AppError::new("invalid-xml", e))?;
        let nodes: Vec<_> = doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name("technicalspecs"))
            .collect();
        if nodes.len() != 1 || nodes[0].attribute("source") != Some("IMDb") {
            return Err(AppError::new(
                "unsafe-skip",
                "Ambiguous Technical Specs ownership",
            ));
        }
        let node = nodes[0];
        let range = node.range();
        let mut outside = raw[..bom + range.start].to_vec();
        outside.extend_from_slice(&raw[bom + range.end..]);
        let manifests = node
            .children()
            .filter(|n| n.has_tag_name("generatedtags") || n.has_tag_name("manualtags"))
            .map(|n| {
                let mut attributes = n
                    .attributes()
                    .filter(|a| !(n.has_tag_name("generatedtags") && a.name() == "state"))
                    .map(|a| (a.name().to_string(), a.value().to_string()))
                    .collect::<Vec<_>>();
                attributes.sort();
                element(
                    n.tag_name().name(),
                    attributes,
                    &n.children()
                        .map(|c| source[c.range()].to_string())
                        .collect::<String>(),
                )
            })
            .collect();
        Ok((outside, manifests))
    }
    if protected(before)? != protected(after)? {
        return Err(AppError::new(
            "unsafe-skip",
            "Specs writer cannot modify other NFO bytes or tag ownership",
        ));
    }
    Ok(())
}
fn element(name: &str, attrs: Vec<(String, String)>, body: &str) -> String {
    format!(
        "<{name}{}>{body}</{name}>",
        attrs
            .into_iter()
            .map(|(k, v)| format!(" {k}=\"{}\"", escape(&v)))
            .collect::<String>()
    )
}
// Pure candidate construction. No disk writes and no mutation of root tags.
pub fn manual_candidate(raw: &[u8], specs: &Specs) -> Result<Vec<u8>> {
    if specs.keys().any(|k| !SECTIONS.contains(&k.as_str())) {
        return Err(AppError::new(
            "invalid-spec-field",
            "Unknown Technical Specs field",
        ));
    }
    let all = std::str::from_utf8(raw).map_err(|e| AppError::new("invalid-encoding", e))?;
    let source = all.strip_prefix('\u{feff}').unwrap_or(all);
    let bom = all.len() - source.len();
    let doc = roxmltree::Document::parse(source).map_err(|e| AppError::new("invalid-xml", e))?;
    let root = doc.root_element();
    if !["movie", "tvshow", "episodedetails"].contains(&root.tag_name().name()) {
        return Err(AppError::new(
            "unsupported-write-kind",
            "Season and unknown NFO writes are not supported by ITM",
        ));
    }
    let nodes: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("technicalspecs"))
        .collect();
    if nodes.len() != 1 || nodes[0].attribute("source") != Some("IMDb") {
        return Err(AppError::new(
            "unsafe-skip",
            "Requires exactly one authoritative IMDb Technical Specs node",
        ));
    }
    let tech = nodes[0];
    if tech
        .attribute("formatVersion")
        .is_some_and(|v| v.parse::<u32>().map_or(true, |v| v > 21))
    {
        return Err(AppError::new(
            "unsafe-skip",
            "Technical Specs schema is newer or unrecognized",
        ));
    }
    let mut normalized = Specs::new();
    for (field, values) in specs {
        let mut seen = std::collections::HashSet::new();
        let values = values
            .iter()
            .map(|v| clean(v))
            .filter(|v| !v.is_empty() && seen.insert(crate::ownership_key(v)))
            .collect::<Vec<_>>();
        normalized.insert(field.clone(), values);
    }
    let specs = &normalized;
    let mut current = Specs::new();
    for node in tech.children().filter(|n| n.has_tag_name("section")) {
        crate::collect_specs(
            &mut current,
            node.attribute("name").unwrap_or(""),
            node.children()
                .filter(|n| n.has_tag_name("item"))
                .map(|n| {
                    n.descendants()
                        .filter(|c| c.is_text())
                        .filter_map(|c| c.text())
                        .collect()
                })
                .collect(),
        );
    }
    if fingerprint(&current)? == fingerprint(specs)? {
        return Ok(raw.to_vec());
    }
    let nl = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut body = String::new();
    for field in SECTIONS {
        if let Some(values) = specs.get(field) {
            if !values.is_empty() {
                body.push_str(nl);
                body.push_str(&format!("    <section name=\"{}\">", escape(field)));
                for value in values {
                    body.push_str(&format!("<item>{}</item>", escape(value)));
                }
                body.push_str("</section>");
            }
        }
    }
    let mut source_sections = String::new();
    let mut has_snapshot = false;
    for child in tech.children() {
        if child.has_tag_name("section") {
            source_sections.push_str(&source[child.range()]);
            if !SECTIONS.contains(&child.attribute("name").unwrap_or("")) {
                body.push_str(&source[child.range()]);
            }
            continue;
        }
        if child.has_tag_name("sourcesnapshot") {
            has_snapshot = true;
        }
        if child.has_tag_name("generatedtags") {
            if child.attribute("owner") != Some("IMDb Tech Manager") {
                return Err(AppError::new(
                    "unsafe-skip",
                    "Conflicting generated tag owner",
                ));
            }
            let mut attributes: Vec<_> = child
                .attributes()
                .filter(|a| a.name() != "state")
                .map(|a| (a.name().into(), a.value().into()))
                .collect();
            attributes.push(("state".into(), "stale".into()));
            let inner = child
                .children()
                .map(|n| source[n.range()].to_owned())
                .collect::<String>();
            body.push_str(&element("generatedtags", attributes, &inner));
        } else {
            body.push_str(&source[child.range()]);
        }
    }
    if !has_snapshot {
        body.push_str(&element(
            "sourcesnapshot",
            vec![
                ("factOrigin".into(), "imdb".into()),
                ("specHash".into(), fingerprint(&current)?),
                (
                    "fetched".into(),
                    tech.attribute("fetched").unwrap_or("").into(),
                ),
            ],
            &source_sections,
        ));
    }
    let mut attributes: Vec<_> = tech
        .attributes()
        .filter(|a| !["specHash", "modified", "modifiedAt"].contains(&a.name()))
        .map(|a| (a.name().into(), a.value().into()))
        .collect();
    if !has_snapshot {
        attributes.retain(|(name, _)| name != "sourceSpecHash");
        attributes.push(("sourceSpecHash".into(), fingerprint(&current)?));
    }
    attributes.push(("modified".into(), "manual".into()));
    attributes.push((
        "modifiedAt".into(),
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    ));
    attributes.push(("specHash".into(), fingerprint(specs)?));
    let replacement = element("technicalspecs", attributes, &body);
    let range = tech.range();
    let mut out = raw[..bom + range.start].to_vec();
    out.extend_from_slice(replacement.as_bytes());
    out.extend_from_slice(&raw[bom + range.end..]);
    let check = std::str::from_utf8(&out).map_err(|e| AppError::new("invalid-candidate", e))?;
    roxmltree::Document::parse(check.trim_start_matches('\u{feff}'))
        .map_err(|e| AppError::new("invalid-candidate", e))?;
    Ok(out)
}
