#[cfg(feature = "tauri-runtime")]
fn main() {
    discrypt_desktop::run();
}

#[cfg(not(feature = "tauri-runtime"))]
fn main() {
    match discrypt_desktop::app_snapshot() {
        Ok(snapshot) => println!(
            "discrypt desktop shell: servers={} devices={} verified_friend={}",
            snapshot.servers.len(),
            snapshot.devices.len(),
            snapshot.friend.verified
        ),
        Err(error) => {
            eprintln!("discrypt desktop shell failed to load state: {error}");
            std::process::exit(1);
        }
    }
}
