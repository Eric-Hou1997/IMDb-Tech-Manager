use super::*;
use crate::presentation::{Catalog, Descriptor, Pack, MAX_PACK_BYTES};
use base64::Engine;
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Cached {
    schema: u32,
    bytes: String,
}
fn key(descriptor: &Descriptor) -> String {
    format!("presentation-asset:{}", descriptor.sha256)
}
fn decode(catalog: &Catalog, locale: &str, body: &str) -> Result<Pack> {
    let cached: Cached =
        serde_json::from_str(body).map_err(|e| AppError::new("language-cache-corrupt", e))?;
    if cached.schema != 1 {
        return Err(AppError::new(
            "language-cache-corrupt",
            "Unknown language cache format",
        ));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cached.bytes)
        .map_err(|e| AppError::new("language-cache-corrupt", e))?;
    catalog.decode(locale, &bytes)
}
fn read(db: &Connection, key: &str) -> Result<Option<String>> {
    // Bound the SQLite text read before allocating a possibly corrupted row.
    let row: Option<Option<String>> = db.query_row(
        "SELECT CASE WHEN length(CAST(body AS BLOB))<=?2 THEN body ELSE NULL END FROM preferences WHERE key=?1",
        params![key, MAX_PACK_BYTES * 2], |row| row.get(0),
    ).optional()?;
    match row {
        None => Ok(None),
        Some(Some(body)) => Ok(Some(body)),
        Some(None) => Err(AppError::new(
            "language-size",
            "Cached language pack exceeds the size limit",
        )),
    }
}
impl Store {
    pub fn presentation_pack(&self, catalog: &Catalog, locale: &str) -> Result<Option<Pack>> {
        catalog.validate()?;
        let descriptor = catalog.external.get(locale).ok_or_else(|| {
            AppError::new(
                "language-not-external",
                "Locale is not an external language pack",
            )
        })?;
        read(&*self.db()?, &key(descriptor))?
            .map(|body| decode(catalog, locale, &body))
            .transpose()
    }
    pub fn install_presentation_pack(
        &self,
        catalog: &Catalog,
        locale: &str,
        bytes: &[u8],
    ) -> Result<Descriptor> {
        // Finish complete identity, hash, coverage and presentation validation
        // before touching preferences. Neither roots nor active locale changes.
        catalog.decode(locale, bytes)?;
        let descriptor = catalog.external[locale].clone();
        let key = key(&descriptor);
        let candidate = serde_json::to_string(&Cached {
            schema: 1,
            bytes: base64::engine::general_purpose::STANDARD.encode(bytes),
        })?;
        let mut db = self.db()?;
        self.writable()?;
        let tx = db.transaction()?;
        match read(&tx, &key) {
            Ok(Some(existing)) if existing == candidate => {
                tx.commit()?;
                return Ok(descriptor);
            }
            Ok(_) => {}
            Err(error) if error.code == "language-size" => {}
            Err(error) => return Err(error),
        }
        // An explicit verified reinstall may repair a damaged public language
        // asset. Other cached versions remain available for rollback/reuse.
        tx.execute("INSERT INTO preferences(key,body) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET body=excluded.body", params![key,candidate])?;
        tx.commit()?;
        Ok(descriptor)
    }
}
