use super::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SourceStatus {
    #[default]
    Ok,
    Empty,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SourceSpecs {
    #[serde(default)]
    pub status: SourceStatus,
    pub imdb: String,
    pub specs: Specs,
    pub fetched_at: String,
    pub parser: String,
}
impl SourceSpecs {
    pub fn validate(&self) -> Result<()> {
        imdb_url(&self.imdb)?;
        chrono::DateTime::parse_from_rfc3339(&self.fetched_at)
            .map_err(|e| AppError::new("invalid-fetch-time", e))?;
        if self.specs.keys().any(|k| !SECTIONS.contains(&k.as_str()))
            || (self.status == SourceStatus::Empty)
                == self.specs.values().flatten().any(|v| !v.trim().is_empty())
        {
            return Err(AppError::new(
                "invalid-source-specs",
                "Source status does not match the validated specifications",
            ));
        }
        Ok(())
    }
}
pub fn imdb_url(imdb: &str) -> Result<String> {
    if !regex::Regex::new(r"^tt[0-9]{5,12}$")
        .unwrap()
        .is_match(imdb)
    {
        return Err(AppError::new(
            "invalid-imdb",
            "A valid IMDb title ID is required",
        ));
    }
    Ok(format!("https://www.imdb.com/title/{imdb}/technical/"))
}
pub fn parse_page(imdb: &str, page: &str) -> Result<SourceSpecs> {
    imdb_url(imdb)?;
    if page.len() > 8 * 1024 * 1024 {
        return Err(AppError::new(
            "imdb-response-too-large",
            "IMDb response exceeds 8 MiB",
        ));
    }
    let script = regex::Regex::new(
        r#"(?is)<script\b[^>]*\bid\s*=\s*["']__NEXT_DATA__["'][^>]*>(.*?)</script\s*>"#,
    )
    .unwrap();
    let payload = script.captures(page).ok_or_else(|| {
        AppError::new(
            "imdb-layout-unrecognized",
            "IMDb technical data was not found in this page",
        )
    })?;
    let value: Value =
        serde_json::from_str(&payload[1]).map_err(|e| AppError::new("imdb-json-invalid", e))?;
    let title = find_title(&value).ok_or_else(|| {
        AppError::new("imdb-payload-missing", "No technical title in IMDb payload")
    })?;
    if title["id"].as_str() != Some(imdb) {
        return Err(AppError::new(
            "imdb-title-mismatch",
            "IMDb response title does not match the requested NFO",
        ));
    }
    let specs = parse_next_data(&value)?;
    let status = if specs.values().any(|values| !values.is_empty()) {
        SourceStatus::Ok
    } else {
        // An absent or unfamiliar payload is not proof that IMDb has no facts.
        let explicit_empty = title["runtimes"]["edges"]
            .as_array()
            .is_some_and(Vec::is_empty)
            && title["technicalSpecifications"]
                .as_object()
                .is_some_and(|sections| {
                    [
                        "soundMixes",
                        "colorations",
                        "aspectRatios",
                        "cameras",
                        "laboratories",
                        "negativeFormats",
                        "processes",
                        "printedFormats",
                        "filmLengths",
                    ]
                    .iter()
                    .any(|key| sections.get(*key).is_some_and(|v| v["items"].is_array()))
                });
        if !explicit_empty {
            return Err(AppError::new(
                "imdb-empty-unconfirmed",
                "IMDb payload does not prove an empty technical result",
            ));
        }
        SourceStatus::Empty
    };
    Ok(SourceSpecs {
        status,
        imdb: imdb.into(),
        specs,
        fetched_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        parser: "next-data".into(),
    })
}
fn sections(specs: &Specs, nl: &str) -> String {
    SECTIONS
        .into_iter()
        .filter_map(|field| {
            let values = specs.get(field)?;
            if values.is_empty() {
                return None;
            }
            Some(format!(
                "{nl}    {}",
                element(
                    "section",
                    vec![("name".into(), field.into())],
                    &values
                        .iter()
                        .map(|v| element("item", vec![], &escape(v)))
                        .collect::<String>()
                )
            ))
        })
        .collect()
}
/// Source refresh keeps manually edited effective facts and their tag state.
/// Only changes within the authoritative node are permitted, including first insertion.
pub fn source_candidate(raw: &[u8], fetched: &SourceSpecs) -> Result<Vec<u8>> {
    let url = imdb_url(&fetched.imdb)?;
    fetched.validate()?;
    let mut normalized = Specs::new();
    for (field, values) in &fetched.specs {
        let mut seen = std::collections::HashSet::new();
        normalized.insert(
            field.clone(),
            values
                .iter()
                .map(|v| clean(v))
                .filter(|v| !v.is_empty() && seen.insert(crate::ownership_key(v)))
                .collect(),
        );
    }
    let all = std::str::from_utf8(raw).map_err(|e| AppError::new("invalid-encoding", e))?;
    let source = all.strip_prefix('\u{feff}').unwrap_or(all);
    let bom = all.len() - source.len();
    let doc = roxmltree::Document::parse(source).map_err(|e| AppError::new("invalid-xml", e))?;
    let root = doc.root_element();
    if !["movie", "tvshow", "episodedetails"].contains(&root.tag_name().name()) {
        return Err(AppError::new(
            "unsupported-write-kind",
            "Unsupported NFO media kind",
        ));
    }
    let nodes: Vec<_> = root
        .children()
        .filter(|n| n.has_tag_name("technicalspecs"))
        .collect();
    if nodes.len() > 1
        || nodes.first().is_some_and(|n| {
            n.attribute("source") != Some("IMDb")
                || n.attribute("formatVersion")
                    .is_some_and(|v| v.parse::<u32>().map_or(true, |v| v > 21))
        })
    {
        return Err(AppError::new(
            "unsafe-skip",
            "Ambiguous or unsupported Technical Specs ownership/schema",
        ));
    }
    let tech = nodes.first().copied();
    if tech
        .and_then(|n| n.attribute("imdbid"))
        .is_some_and(|id| !id.is_empty() && id != fetched.imdb)
    {
        return Err(AppError::new(
            "imdb-title-mismatch",
            "Existing Technical Specs belongs to a different title",
        ));
    }
    let manual = tech.is_some_and(|n| n.attribute("modified") == Some("manual"));
    let mut old_specs = Specs::new();
    if let Some(tech) = tech {
        for child in tech.children().filter(|n| n.has_tag_name("section")) {
            crate::collect_specs(
                &mut old_specs,
                child.attribute("name").unwrap_or(""),
                child
                    .children()
                    .filter(|n| n.has_tag_name("item"))
                    .map(|n| {
                        n.descendants()
                            .filter(|n| n.is_text())
                            .filter_map(|n| n.text())
                            .collect()
                    })
                    .collect(),
            );
        }
    }
    let source_hash = fingerprint(&normalized)?;
    let effective_hash = if manual {
        fingerprint(&old_specs)?
    } else {
        source_hash.clone()
    };
    let nl = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut body = if manual {
        String::new()
    } else {
        sections(&normalized, nl)
    };
    if let Some(tech) = tech {
        for child in tech.children() {
            if child.is_text() && child.text().is_some_and(|text| text.trim().is_empty()) {
                continue;
            }
            if child.has_tag_name("sourcesnapshot") || child.has_tag_name("url") {
                continue;
            }
            if child.has_tag_name("section")
                && !manual
                && SECTIONS.contains(&child.attribute("name").unwrap_or(""))
            {
                continue;
            }
            if child.has_tag_name("generatedtags")
                && child.attribute("specHash") != Some(effective_hash.as_str())
            {
                if child.attribute("owner") != Some("IMDb Tech Manager") {
                    return Err(AppError::new(
                        "unsafe-skip",
                        "Conflicting generated tag owner",
                    ));
                }
                let mut attrs: Vec<_> = child
                    .attributes()
                    .filter(|a| a.name() != "state")
                    .map(|a| (a.name().into(), a.value().into()))
                    .collect();
                attrs.push(("state".into(), "stale".into()));
                body.push_str(&element(
                    "generatedtags",
                    attrs,
                    &child
                        .children()
                        .map(|c| source[c.range()].to_string())
                        .collect::<String>(),
                ));
            } else {
                body.push_str(&source[child.range()]);
            }
        }
    }
    if manual {
        body.push_str(&element(
            "sourcesnapshot",
            vec![
                ("factOrigin".into(), "imdb".into()),
                ("specHash".into(), source_hash.clone()),
                ("fetched".into(), fetched.fetched_at.clone()),
            ],
            &sections(&normalized, nl),
        ));
    }
    body.push_str(&element("url", vec![], &escape(&url)));
    let mut attrs: Vec<_> = tech
        .map(|n| {
            n.attributes()
                .filter(|a| {
                    ![
                        "source",
                        "imdbid",
                        "mediatype",
                        "fetched",
                        "formatVersion",
                        "specHash",
                        "sourceSpecHash",
                        "factOrigin",
                        "status",
                    ]
                    .contains(&a.name())
                })
                .map(|a| (a.name().into(), a.value().into()))
                .collect()
        })
        .unwrap_or_default();
    attrs.extend([
        ("source".into(), "IMDb".into()),
        ("imdbid".into(), fetched.imdb.clone()),
        (
            "mediatype".into(),
            match root.tag_name().name() {
                "tvshow" => "tvshow",
                "episodedetails" => "episode",
                _ => "movie",
            }
            .into(),
        ),
        ("fetched".into(), fetched.fetched_at.clone()),
        ("formatVersion".into(), "21".into()),
        ("specHash".into(), effective_hash),
        ("sourceSpecHash".into(), source_hash),
        ("factOrigin".into(), "imdb".into()),
        (
            "status".into(),
            if manual {
                if old_specs.values().flatten().any(|v| !v.trim().is_empty()) {
                    "ok"
                } else {
                    "empty"
                }
            } else if fetched.status == SourceStatus::Empty {
                "empty"
            } else {
                "ok"
            }
            .into(),
        ),
    ]);
    let replacement = element("technicalspecs", attrs, &body);
    let range = if let Some(tech) = tech {
        tech.range()
    } else {
        let end = root.range().end;
        let closing = regex::Regex::new(&format!(r"</{}\s*>$", root.tag_name().name())).unwrap();
        // A self-closing root cannot be expanded without modifying protected bytes.
        let closing = closing.find(&source[..end]).ok_or_else(|| {
            AppError::new("unsafe-skip", "NFO requires an explicit closing root tag")
        })?;
        closing.start()..closing.start()
    };
    let mut out = raw[..bom + range.start].to_vec();
    out.extend_from_slice(replacement.as_bytes());
    out.extend_from_slice(&raw[bom + range.end..]);
    validate_specs_only(raw, &out)?;
    Ok(out)
}
