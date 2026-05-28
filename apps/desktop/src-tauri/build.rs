#[cfg(feature = "tauri-runtime")]
fn main() {
    tauri_build::build();
}

#[cfg(not(feature = "tauri-runtime"))]
fn main() {}
