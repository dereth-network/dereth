//! Divergence: the WebSocket endpoint's settings are Empyrean's own; ACE speaks UDP only.
//! Behaviour: none (the settings are this server's own contract).
//! `[server.websocket]` reads from empyrean.toml, is off by default, serves plain ws:// only on
//! loopback or behind a TLS proxy, needs the certificate and the key together, and reports the
//! URL it is reached at.
//! Fixture: locally constructed settings.

use empyrean_common::toml_config;
use empyrean_common::web_socket_settings::{WebSocketSettings, WebSocketSettingsError};

fn settings(text: &str) -> WebSocketSettings {
    toml_config::from_toml_str(text)
        .expect("valid empyrean.toml")
        .config
        .server
        .web_socket
}

#[test]
fn the_section_reads_from_the_file_and_is_off_by_default() {
    assert!(!settings("").enabled, "off unless enabled");
    let s = settings(
        r#"
        [server.websocket]
        enabled = true
        listen = "127.0.0.1:9180"
        tls_certificate = "cert.pem"
        tls_private_key = "key.pem"
        behind_tls_proxy = true
        trusted_proxies = ["127.0.0.1"]
        allowed_origins = ["https://play.example.org"]
        maximum_connections_per_ip_address = 2
        idle_timeout = 90
        public_url = "wss://play.example.org/ws"
        "#,
    );
    assert_eq!(
        s,
        WebSocketSettings {
            enabled: true,
            listen: "127.0.0.1:9180".to_owned(),
            tls_certificate: "cert.pem".to_owned(),
            tls_private_key: "key.pem".to_owned(),
            behind_tls_proxy: true,
            trusted_proxies: vec!["127.0.0.1".to_owned()],
            allowed_origins: vec!["https://play.example.org".to_owned()],
            maximum_connections_per_ip_address: 2,
            idle_timeout: 90,
            public_url: "wss://play.example.org/ws".to_owned(),
        }
    );
}

#[test]
fn plain_ws_is_served_only_on_loopback_or_behind_a_tls_proxy() {
    let plain = |listen: &str, behind: bool| WebSocketSettings {
        enabled: true,
        listen: listen.to_owned(),
        behind_tls_proxy: behind,
        ..WebSocketSettings::default()
    };
    assert_eq!(
        plain("127.0.0.1:9180", false).validate(),
        Ok(("127.0.0.1:9180".parse().unwrap(), false))
    );
    assert_eq!(
        plain("0.0.0.0:9443", false).validate(),
        Err(WebSocketSettingsError::PlainOnPublicAddress(
            "0.0.0.0:9443".parse().unwrap()
        ))
    );
    assert!(plain("0.0.0.0:9443", true).validate().is_ok());
    let tls = WebSocketSettings {
        tls_certificate: "cert.pem".to_owned(),
        tls_private_key: "key.pem".to_owned(),
        ..plain("0.0.0.0:9443", false)
    };
    assert_eq!(tls.validate(), Ok(("0.0.0.0:9443".parse().unwrap(), true)));
}

#[test]
fn the_certificate_and_the_key_come_together_and_the_addresses_must_parse() {
    let half = WebSocketSettings {
        tls_certificate: "cert.pem".to_owned(),
        ..WebSocketSettings::default()
    };
    assert_eq!(half.validate(), Err(WebSocketSettingsError::HalfTls));
    let bad = WebSocketSettings {
        listen: "localhost:9443".to_owned(),
        ..WebSocketSettings::default()
    };
    assert!(matches!(
        bad.validate(),
        Err(WebSocketSettingsError::Listen(_))
    ));
    let proxy = WebSocketSettings {
        listen: "127.0.0.1:9180".to_owned(),
        trusted_proxies: vec!["nginx".to_owned()],
        ..WebSocketSettings::default()
    };
    assert!(matches!(
        proxy.validate(),
        Err(WebSocketSettingsError::TrustedProxy(_))
    ));
}

#[test]
fn the_reported_url_is_the_public_one_else_the_listen_address() {
    let s = WebSocketSettings::default();
    assert_eq!(
        s.url("0.0.0.0:9443".parse().unwrap(), true),
        "wss://127.0.0.1:9443/"
    );
    assert_eq!(
        s.url("127.0.0.1:9180".parse().unwrap(), false),
        "ws://127.0.0.1:9180/"
    );
    let public = WebSocketSettings {
        public_url: "wss://play.example.org/ws".to_owned(),
        ..s
    };
    assert_eq!(
        public.url("127.0.0.1:9180".parse().unwrap(), false),
        "wss://play.example.org/ws"
    );
}
