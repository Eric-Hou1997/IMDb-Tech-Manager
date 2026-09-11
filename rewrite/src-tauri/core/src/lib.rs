pub mod acquisition;
pub mod ai;
pub mod contracts;
pub mod install;
pub mod library;
pub mod migration;
pub mod paths;
pub mod rules;
pub mod services;
pub mod specs;
pub mod store;
pub mod transaction;
pub mod update;
mod windows_replace;

pub use contracts::*;
pub(crate) fn ownership_key(value: &str) -> String {
    use unicode_casefold::UnicodeCaseFold;
    value.case_fold().collect()
}
pub(crate) fn collect_specs(out: &mut Specs, key: &str, values: Vec<String>) {
    if !specs::SECTIONS.contains(&key) {
        return;
    }
    let existing = out.entry(key.into()).or_default();
    for value in values {
        let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
        if !value.is_empty() && !existing.contains(&value) {
            existing.push(value);
        }
    }
}
use sha2::{Digest, Sha256};
pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub mod ui;

pub mod tv;

pub mod lifecycle;

pub mod startup_file;

pub mod tags;
pub mod writing;
