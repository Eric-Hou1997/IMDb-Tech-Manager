use itm_core::{ai, library, specs, LibraryRoot, Space};
use std::io::Read;
fn main() {
    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).unwrap();
    let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let result = match value["mode"].as_str().unwrap() {
        "nfo" => {
            let root = LibraryRoot {
                id: "fixture".into(),
                space: Space::Movie,
                path: "/fixture".into(),
            };
            serde_json::to_value(
                library::parse(
                    &root,
                    std::path::Path::new("/fixture/input.nfo"),
                    value["raw"].as_str().unwrap().as_bytes(),
                )
                .unwrap(),
            )
            .unwrap()
        }
        "request" => {
            let cfg: ai::Config = serde_json::from_value(value["config"].clone()).unwrap();
            let input = serde_json::from_value(value["specs"].clone()).unwrap();
            ai::request(&cfg, &input, &[]).unwrap()
        }
        "rules" => serde_json::to_value(itm_core::rules::entries(
            &serde_json::from_value(value["specs"].clone()).unwrap(),
        ))
        .unwrap(),
        "legacy-ai-cache-key" => serde_json::json!(ai::legacy_cache::key(
            &serde_json::from_value(value["settings"].clone()).unwrap(),
            &serde_json::from_value(value["specs"].clone()).unwrap(),
            value["existing"].as_array().unwrap(),
            value["extra_body"].as_str().unwrap(),
        )
        .unwrap()),
        "next-data" => {
            serde_json::to_value(specs::parse_next_data(&value["data"]).unwrap()).unwrap()
        }
        _ => panic!("Unknown characterization mode"),
    };
    println!("{}", serde_json::to_string(&result).unwrap());
}
