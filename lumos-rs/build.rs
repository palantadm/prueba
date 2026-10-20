// rust-embed needs `ui/dist` to exist at compile time. Create a placeholder when the UI is not built yet.
fn main() {
    let dist = std::path::Path::new("ui/dist");
    let _ = std::fs::create_dir_all(dist);
    let index = dist.join("index.html");
    if !index.exists() {
        let _ = std::fs::write(
            index,
            "<!doctype html><meta charset=utf-8><title>Lumos</title><body style=\"font:15px system-ui;background:#0b0c0e;color:#d9dde3;padding:2rem\"><h2>Lumos</h2><p>The web UI has not been built yet. Run <code>cd ui &amp;&amp; npm install &amp;&amp; npm run build</code>, then rebuild the server.</p>",
        );
    }
    println!("cargo:rerun-if-changed=ui/dist");
    println!("cargo:rerun-if-changed=build.rs");
}
