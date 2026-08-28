//! Diagnostic MQTT probe: subscribes to rendezvous topics on a broker and prints traffic.
//!
//! Usage: mqtt_probe [endpoint] [filter]
//! Defaults: mqtt://broker.emqx.io:1883 discrypt/v1/rendezvous/#
//!
//! Prints one line per received publish (`PUB topic=... bytes=...`) plus
//! subscription confirmation, so split-machine runs can be observed at the
//! broker level without touching redacted payload bytes.

#[cfg(feature = "mqtt-adapter")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "mqtt://broker.emqx.io:1883".to_owned());
    let filter = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "discrypt/v1/rendezvous/#".to_owned());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let _ = rumqttc::tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
            .install_default();
        let mut options = rumqttc::MqttOptions::parse_url(&endpoint)?;
        options.set_client_id(format!("g004-probe-{}", std::process::id()));
        options.set_keep_alive(30);
        if endpoint.starts_with("mqtts://") || endpoint.starts_with("tls://") {
            options.set_transport(rumqttc::Transport::tls_with_default_config());
        }
        let (client, mut eventloop) =
            rumqttc::AsyncClient::builder(options).capacity(64).build();
        client
            .subscribe(filter, rumqttc::QoS::AtLeastOnce)
            .await
            .map_err(|error| format!("subscribe failed: {error:?}"))?;
        println!("probe: subscribed; observing...");
        loop {
            match eventloop.poll().await {
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::Publish(publish))) => {
                    let topic = String::from_utf8_lossy(&publish.topic).to_string();
                    println!("PUB topic={topic} bytes={}", publish.payload.len());
                }
                Ok(rumqttc::Event::Incoming(rumqttc::Packet::SubAck(_))) => {
                    println!("SUBSCRIBED");
                }
                Ok(_) => {}
                Err(error) => eprintln!("eventloop error: {error:?}"),
            }
        }
    })
}

#[cfg(not(feature = "mqtt-adapter"))]
fn main() {
    println!("mqtt_probe requires building with --features mqtt-adapter");
}
