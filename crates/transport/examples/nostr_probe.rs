//! Diagnostic Nostr probe: two-client round-trip mirroring the split-machine
//! E2E shape — client A subscribes (owner side), client B publishes (joiner
//! side), and A waits for B's event on wss://relay.damus.io.
//!
//! Usage: nostr_probe [relay] [topic]

#[cfg(feature = "nostr-adapter")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let relay = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "wss://relay.damus.io".to_owned());
    let topic = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "g7-probe-topic".to_owned());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let kind = nostr_sdk::Kind::Custom(31_733);

        // Client A (owner-like): connect, subscribe with retries.
        let keys_a = nostr_sdk::Keys::generate();
        let client_a = nostr_sdk::Client::new(keys_a);
        client_a
            .add_relay(relay.as_str())
            .await
            .map_err(|err| format!("A add relay failed: {err:?}"))?;
        client_a.connect().await;
        let filter_a = nostr_sdk::Filter::new()
            .kind(kind)
            .identifier(topic.clone())
            .since(nostr_sdk::Timestamp::now());
        let mut out_a = client_a
            .subscribe_with_id_to(
                [relay.as_str()],
                nostr_sdk::SubscriptionId::new("g7-probe-sub-a"),
                filter_a,
                None,
            )
            .await
            .map_err(|err| format!("A subscribe failed: {err:?}"))?;
        for _ in 0..25 {
            if !out_a.success.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            out_a = client_a
                .subscribe_with_id_to(
                    [relay.as_str()],
                    nostr_sdk::SubscriptionId::new("g7-probe-sub-a"),
                    nostr_sdk::Filter::new()
                        .kind(kind)
                        .identifier(topic.clone())
                        .since(nostr_sdk::Timestamp::now()),
                    None,
                )
                .await
                .map_err(|err| format!("A subscribe retry failed: {err:?}"))?;
        }
        println!(
            "A subscribe: success={:?} failed={:?}",
            out_a.success, out_a.failed
        );
        // Mirror the adapter's ordering: notifications receiver created AFTER
        // subscribe_with_id_to returns.
        let mut notif_a = client_a.notifications();

        // Client B (joiner-like): connect, publish.
        let keys_b = nostr_sdk::Keys::generate();
        let client_b = nostr_sdk::Client::new(keys_b);
        client_b
            .add_relay(relay.as_str())
            .await
            .map_err(|err| format!("B add relay failed: {err:?}"))?;
        client_b.connect().await;
        let builder = nostr_sdk::EventBuilder::new(kind, "cHJvYmUtYg==")
            .tag(nostr_sdk::Tag::parse(["d", &topic])?);
        let mut out_b = client_b
            .send_event_builder_to([relay.as_str()], builder.clone())
            .await
            .map_err(|err| format!("B publish failed: {err:?}"))?;
        for _ in 0..25 {
            if !out_b.success.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            out_b = client_b
                .send_event_builder_to([relay.as_str()], builder.clone())
                .await
                .map_err(|err| format!("B publish retry failed: {err:?}"))?;
        }
        println!(
            "B publish: success={:?} failed={:?}",
            out_b.success, out_b.failed
        );

        // A waits for B's event.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                println!("probe: A DID NOT RECEIVE B's event within 25s");
                break;
            }
            match tokio::time::timeout(remaining, notif_a.recv()).await {
                Ok(Ok(nostr_sdk::RelayPoolNotification::Event { event, .. })) => {
                    println!(
                        "A RECEIVED B's event: kind={} content_len={}",
                        event.kind.as_u16(),
                        event.content.len()
                    );
                    break;
                }
                Ok(Ok(nostr_sdk::RelayPoolNotification::Message { message, .. })) => {
                    println!("A relay message: {message:?}");
                }
                Ok(Ok(nostr_sdk::RelayPoolNotification::Shutdown)) => {
                    println!("A: relay pool shutdown");
                    break;
                }
                Ok(Err(error)) => println!("A notification error: {error:?}"),
                Err(_) => {}
            }
        }
        Ok(())
    })
}

#[cfg(not(feature = "nostr-adapter"))]
fn main() {
    println!("nostr_probe requires building with --features nostr-adapter");
}
