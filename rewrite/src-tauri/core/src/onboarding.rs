use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct RootCandidate {
    pub path: String,
    pub suggested_space: String,
    pub source: String,
    pub online: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "OnboardingInfo")]
pub struct Info {
    pub onboarding_required: bool,
    pub library_roots_confirmed: bool,
    pub candidates: Vec<RootCandidate>,
}

/// Same optional, read-only TMM datasource discovery as v4.1. Missing or
/// malformed optional settings do not prevent manual first-run confirmation.
fn add_path(value: &str, output: &mut Vec<RootCandidate>) {
    if !Path::new(value).is_absolute() || !Path::new(value).is_dir() {
        return;
    }
    let Ok(real) = Path::new(value).canonicalize() else {
        return;
    };
    let Some(path) = real.to_str() else {
        return;
    };
    if output.iter().any(|c| c.path == path) {
        return;
    }
    let lower = path.to_lowercase();
    let suggested_space = if ["tv", "series", "show", "电视剧", "电视"]
        .iter()
        .any(|v| lower.contains(v))
    {
        "tv"
    } else if ["movie", "film", "电影"].iter().any(|v| lower.contains(v)) {
        "movies"
    } else {
        "unassigned"
    };
    output.push(RootCandidate {
        path: path.into(),
        suggested_space: suggested_space.into(),
        source: "tinyMediaManager".into(),
        online: real.is_dir(),
    });
}
// Read keys in their original document order without retaining unrelated TMM
// settings or changing serde_json ordering used by existing request hashes.
#[derive(Default)]
struct Found {
    direct: Vec<RootCandidate>,
    nested: Vec<RootCandidate>,
}
struct CandidateReader {
    collect: bool,
}
impl<'de> serde::de::DeserializeSeed<'de> for CandidateReader {
    type Value = Found;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        d: D,
    ) -> std::result::Result<Found, D::Error> {
        d.deserialize_any(self)
    }
}
impl<'de> serde::de::Visitor<'de> for CandidateReader {
    type Value = Found;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TMM settings JSON")
    }
    fn visit_map<M: serde::de::MapAccess<'de>>(
        self,
        mut map: M,
    ) -> std::result::Result<Found, M::Error> {
        let mut found = Found::default();
        while let Some(key) = map.next_key::<String>()? {
            let normalized: String = key
                .to_ascii_lowercase()
                .chars()
                .filter(char::is_ascii_lowercase)
                .collect();
            let child = map.next_value_seed(CandidateReader {
                collect: normalized.contains("datasource"),
            })?;
            // Original walk calls add(value) before descending into it. A
            // datasource's string/list paths precede its nested datasource keys.
            found.nested.extend(child.direct);
            found.nested.extend(child.nested);
        }
        Ok(found)
    }
    fn visit_seq<S: serde::de::SeqAccess<'de>>(
        self,
        mut seq: S,
    ) -> std::result::Result<Found, S::Error> {
        let mut found = Found::default();
        while let Some(child) = seq.next_element_seed(CandidateReader {
            collect: self.collect,
        })? {
            found.direct.extend(child.direct);
            found.nested.extend(child.nested);
        }
        Ok(found)
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> std::result::Result<Found, E> {
        let mut found = Found::default();
        if self.collect {
            add_path(value, &mut found.direct);
        }
        Ok(found)
    }
    fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<Found, E> {
        Ok(Found::default())
    }
    fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Found, E> {
        Ok(Found::default())
    }
    fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Found, E> {
        Ok(Found::default())
    }
    fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Found, E> {
        Ok(Found::default())
    }
    fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Found, E> {
        Ok(Found::default())
    }
}
pub fn candidates(data: &Path) -> Vec<RootCandidate> {
    use serde::de::DeserializeSeed;
    let mut output = vec![];
    for name in ["movies.json", "tvshows.json"] {
        let Ok(path) = crate::paths::within(data, &data.join(name)) else {
            continue;
        };
        let Ok(file) = std::fs::File::open(path) else {
            continue;
        };
        // Original TMM libraries may exceed 16 MiB. Stream the document and
        // retain datasource paths only, rather than imposing a new cutoff.
        let mut reader = serde_json::Deserializer::from_reader(std::io::BufReader::new(file));
        if let Ok(found) = (CandidateReader { collect: false }).deserialize(&mut reader) {
            if reader.end().is_err() {
                continue;
            }
            for candidate in found.direct.into_iter().chain(found.nested) {
                if !output
                    .iter()
                    .any(|c: &RootCandidate| c.path == candidate.path)
                {
                    output.push(candidate);
                }
            }
        }
    }
    output
}

impl crate::store::Store {
    pub fn library_roots_confirmed(&self) -> Result<bool> {
        if !self.configuration()?.roots.is_empty()
            || self.preferences("library-roots-confirmed")?.as_bool() == Some(true)
        {
            return Ok(true);
        }
        let legacy = self.preferences("legacy:itm-engine")?;
        Ok(legacy
            .get("library_roots_confirmed")
            .and_then(Value::as_bool)
            == Some(true)
            || legacy
                .get("roots")
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
            || ["movies", "tv"].iter().any(|space| {
                legacy
                    .get("library_roots")
                    .and_then(|v| v.get(space))
                    .and_then(Value::as_array)
                    .is_some_and(|v| !v.is_empty())
            }))
    }
    pub fn onboarding_info(
        &self,
        locate: impl FnOnce() -> Option<std::path::PathBuf>,
    ) -> Result<Info> {
        let confirmed = self.library_roots_confirmed()?;
        Ok(Info {
            onboarding_required: !confirmed,
            library_roots_confirmed: confirmed,
            candidates: if confirmed {
                vec![]
            } else {
                locate().map(|p| candidates(&p)).unwrap_or_default()
            },
        })
    }
}
