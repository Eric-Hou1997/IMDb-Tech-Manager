fn main() {
    println!("cargo:rerun-if-env-changed=ITM_RELEASE_BUILD");
    println!("cargo:rerun-if-env-changed=REWRITE_UPDATE_FEED");
    if std::env::var("ITM_RELEASE_BUILD").as_deref() == Ok("1") {
        println!("cargo:rustc-env=REWRITE_UPDATE_FEED=https://github.com/Eric-Hou1997/IMDb-Tech-Manager/releases/latest/download/ITM-update.json");
    }
    tauri_build::build()
}
