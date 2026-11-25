fn main() {
    delay_load_media_foundation();
    tauri_build::build()
}

/// Media Foundation manque sur les éditions « N » de Windows : importé directement, il empêchait
/// l'app de démarrer (piège vu dans iBox). Chargé à la demande, il n'est requis qu'à l'usage ;
/// `remote_play::encode::mf::media_foundation_available` vérifie avant tout appel.
fn delay_load_media_foundation() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    for dll in ["mfplat.dll", "mfreadwrite.dll"] {
        println!("cargo:rustc-link-arg=/DELAYLOAD:{dll}");
    }
    println!("cargo:rustc-link-arg=delayimp.lib");
}
