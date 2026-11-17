// Embeds the icon and the "run as administrator" manifest into dyno.exe.
fn main() {
    println!("cargo:rerun-if-changed=dyno.rc");
    println!("cargo:rerun-if-changed=dyno.manifest");
    println!("cargo:rerun-if-changed=dyno.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let _ = embed_resource::compile("dyno.rc", embed_resource::NONE);
    }
}
