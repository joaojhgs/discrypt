#[cfg(feature = "tauri-runtime")]
fn main() {
    discrypt_desktop::run();
}

#[cfg(not(feature = "tauri-runtime"))]
fn main() {
    let snapshot = discrypt_desktop::app_snapshot();
    println!(
        "discrypt desktop shell: servers={} devices={} verified_friend={}",
        snapshot.servers.len(),
        snapshot.devices.len(),
        snapshot.friend.verified
    );
}
