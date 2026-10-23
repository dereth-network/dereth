//! What a host needs to share a world: this computer's address on its network, and what its
//! router says about the internet side.
//!
//! The address on the network is the one the system would send from (a UDP socket "connected"
//! to an address outside, which sends nothing). The router is asked over UPnP, which most home
//! routers answer on the local network: discovered with SSDP, then asked its address on the
//! internet (`GetExternalIPAddress`) and, for each game port, which computer it passes that UDP
//! port to (`GetSpecificPortMappingEntry`). Nothing is changed on the router, and nothing beyond
//! the local network is contacted. A router that does not answer leaves the public address
//! unknown, which the page says plainly.
//!
//! **Check from outside.** Only something outside can tell whether a friend elsewhere gets in, so
//! when the host asks, the launcher asks the Dereth Network's hub to try this computer's address at
//! the world's game port ([`dereth_launch::outside`]). The hub tries the address the request comes
//! from, never one the launcher names, and its answer (reached or not, what the world said, and
//! the address it tried) is kept for the page, the address standing in for the public address
//! when the router did not say it.

use serde::Serialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::State;

/// How long the router's answers are kept before it is asked again.
const KEPT: Duration = Duration::from_secs(5 * 60);

/// Where SSDP discovery is sent: the UPnP multicast group.
const SSDP: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(239, 255, 255, 250)), 1900);

/// What the page shows.
#[derive(Debug, Clone, Default, Serialize)]
pub struct View {
    /// This computer's address on its network.
    pub lan: Option<IpAddr>,
    /// Whether the router answered.
    pub router: bool,
    /// The router's address on the internet, when it said.
    pub public: Option<IpAddr>,
    /// The router's own outside address is a shared or private one (the internet provider puts
    /// many homes behind one address), so nothing outside can reach this computer directly.
    pub shared_address: bool,
    /// Each game port asked about, and the computer the router passes it to (none when it passes
    /// it nowhere).
    pub forwards: Vec<(u16, Option<IpAddr>)>,
    pub checking: bool,
    /// Each game port checked from outside, by port.
    pub outside: std::collections::BTreeMap<u16, Outside>,
}

/// What came of checking a world's port from outside.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Outside {
    /// The hub is being asked.
    pub checking: bool,
    /// Its last answer.
    pub outcome: Option<dereth_launch::outside::Outcome>,
}

#[derive(Default)]
pub struct Store {
    view: View,
    asked: Vec<u16>,
    checked: Option<Instant>,
    /// Each game port checked from outside, by port.
    outside: std::collections::BTreeMap<u16, Outside>,
}

pub type Shared = Arc<Mutex<Store>>;

fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Store> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn view(shared: &Shared) -> View {
    let store = lock(shared);
    let mut view = store.view.clone();
    view.outside.clone_from(&store.outside);
    // The address the hub tried is this computer's on the internet, when the router did not say.
    if view.public.is_none() {
        view.public = store
            .outside
            .values()
            .filter_map(|o| match &o.outcome {
                Some(dereth_launch::outside::Outcome::Answered(a)) => a.address.parse().ok(),
                _ => None,
            })
            .next();
        view.shared_address = view.public.is_some_and(shared_address);
    }
    view
}

/// Asks the hub at `base` to try this computer at `port`, waiting at most `wait`.
#[must_use]
pub fn ask_hub(base: &str, port: u16, wait: Duration) -> dereth_launch::outside::Outcome {
    use dereth_launch::outside::{self, Outcome};
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(wait))
        .http_status_as_error(false)
        .build()
        .into();
    let response = agent
        .post(&format!("{base}{}", outside::PATH))
        .header("Content-Type", "application/json")
        .send(outside::body(port));
    match response {
        Ok(mut r) => {
            let status = r.status().as_u16();
            let retry = r
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let body = r
                .body_mut()
                .with_config()
                .limit(64 * 1024)
                .read_to_string()
                .unwrap_or_default();
            outside::read(status, &body, retry.as_deref())
        }
        Err(_) => Outcome::Failed {
            message: "The Dereth Network could not be reached.".into(),
        },
    }
}

/// Ask the Dereth Network's hub to try this computer at the world's game `port`, from outside.
/// The answer comes in the snapshot.
#[tauri::command]
pub fn check_from_outside(shared: State<'_, Shared>, port: u16) {
    let shared = shared.inner().clone();
    {
        let mut store = lock(&shared);
        let entry = store.outside.entry(port).or_default();
        if entry.checking {
            return;
        }
        entry.checking = true;
    }
    std::thread::spawn(move || {
        let base =
            dereth_launch::outside::hub_base(std::env::var("DERETH_HUB_LIST").ok().as_deref());
        let outcome = ask_hub(&base, port, Duration::from_secs(20));
        let mut store = lock(&shared);
        store.outside.insert(
            port,
            Outside {
                checking: false,
                outcome: Some(outcome),
            },
        );
    });
}

/// This computer's address on its network: the one it would send from to an address outside.
#[must_use]
pub fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    // A documentation address: connecting a UDP socket only picks the route; nothing is sent.
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// Whether `ip`, as a router's outside address, cannot be reached from the internet: a private
/// address, or the shared range internet providers put whole neighbourhoods behind.
#[must_use]
pub fn shared_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            let o = a.octets();
            a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_unspecified()
                || (o[0] == 100 && (o[1] & 0xC0) == 64)
        }
        IpAddr::V6(_) => false,
    }
}

/// An address on the local network (or this computer), the only kind a router's description and
/// control addresses may name.
fn local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => a.is_private() || a.is_loopback() || a.is_link_local(),
        IpAddr::V6(a) => a.is_loopback() || (a.segments()[0] & 0xffc0) == 0xfe80,
    }
}

/// The `LOCATION` an SSDP answer names.
#[must_use]
pub fn ssdp_location(answer: &str) -> Option<String> {
    answer.lines().find_map(|l| {
        let (name, value) = l.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("location")
            .then(|| value.trim().to_owned())
    })
}

/// The text inside the first `<name>` element of `xml`.
#[must_use]
pub fn tag_text<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))?;
    Some(xml[start..start + end].trim())
}

/// `scheme://host:port` of a URL, and its host's address when it is one.
fn origin(url: &str) -> Option<(String, IpAddr)> {
    let rest = url.strip_prefix("http://")?;
    let authority = &rest[..rest.find('/').unwrap_or(rest.len())];
    let host = match authority.strip_prefix('[') {
        Some(v6) => &v6[..v6.find(']')?],
        None => authority.rsplit_once(':').map_or(authority, |(h, _)| h),
    };
    Some((format!("http://{authority}"), host.parse().ok()?))
}

/// The router's connection service in its description: its type and its control address.
#[must_use]
pub fn connection_service(description: &str, location: &str) -> Option<(String, String)> {
    let (base, _) = origin(location)?;
    let mut rest = description;
    while let Some(at) = rest.find("<service>") {
        let block_end = rest[at..].find("</service>").map_or(rest.len(), |e| at + e);
        let block = &rest[at..block_end];
        rest = &rest[block_end..];
        let Some(kind) = tag_text(block, "serviceType") else {
            continue;
        };
        if !(kind.contains(":WANIPConnection:") || kind.contains(":WANPPPConnection:")) {
            continue;
        }
        let control = tag_text(block, "controlURL")?;
        let url = if control.starts_with("http://") {
            control.to_owned()
        } else {
            format!("{base}/{}", control.trim_start_matches('/'))
        };
        return Some((kind.to_owned(), url));
    }
    None
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .into()
}

/// A router document or answer, from an address on the local network only.
fn fetch(url: &str, soap: Option<(&str, &str, String)>) -> Option<String> {
    let (_, host) = origin(url)?;
    if !local_ip(host) {
        return None;
    }
    let mut response = match soap {
        None => agent().get(url).call().ok()?,
        Some((service, action, body)) => agent()
            .post(url)
            .header("Content-Type", "text/xml; charset=\"utf-8\"")
            .header("SOAPAction", format!("\"{service}#{action}\""))
            .send(body)
            .ok()?,
    };
    response
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_to_string()
        .ok()
}

/// A UPnP action's request.
fn envelope(service: &str, action: &str, args: &[(&str, &str)]) -> String {
    let args: String = args
        .iter()
        .map(|(name, value)| format!("<{name}>{value}</{name}>"))
        .collect();
    format!(
        "<?xml version=\"1.0\"?>\r\n<s:Envelope xmlns:s=\"http://schemas.xmlsoap.org/soap/envelope/\" s:encodingStyle=\"http://schemas.xmlsoap.org/soap/encoding/\"><s:Body><u:{action} xmlns:u=\"{service}\">{args}</u:{action}></s:Body></s:Envelope>\r\n"
    )
}

/// What a router says: its outside address, and where it passes each of `ports`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Router {
    pub public: Option<IpAddr>,
    pub forwards: Vec<(u16, Option<IpAddr>)>,
}

/// Ask the router found by SSDP discovery sent to `to`. `None` when no router answers within
/// `wait`.
#[must_use]
pub fn ask_router(to: SocketAddr, ports: &[u16], wait: Duration) -> Option<Router> {
    let socket = UdpSocket::bind(if to.ip().is_loopback() {
        "127.0.0.1:0"
    } else {
        "0.0.0.0:0"
    })
    .ok()?;
    socket.set_read_timeout(Some(wait)).ok()?;
    for st in [
        "urn:schemas-upnp-org:device:InternetGatewayDevice:1",
        "urn:schemas-upnp-org:device:InternetGatewayDevice:2",
    ] {
        let search = format!(
            "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: {st}\r\n\r\n"
        );
        socket.send_to(search.as_bytes(), to).ok()?;
    }
    let until = Instant::now() + wait;
    let mut buf = [0u8; 2048];
    let location = loop {
        if Instant::now() >= until {
            return None;
        }
        // A refusal from something that is not listening (Windows says so on the next read) is
        // not the end: another answer may come.
        if let Ok((n, _)) = socket.recv_from(&mut buf) {
            if let Some(l) = ssdp_location(&String::from_utf8_lossy(&buf[..n])) {
                break l;
            }
        }
    };
    let description = fetch(&location, None)?;
    let (service, control) = connection_service(&description, &location)?;
    let public = fetch(
        &control,
        Some((
            &service,
            "GetExternalIPAddress",
            envelope(&service, "GetExternalIPAddress", &[]),
        )),
    )
    .and_then(|a| tag_text(&a, "NewExternalIPAddress")?.parse().ok());
    let forwards = ports
        .iter()
        .map(|port| {
            let port_text = port.to_string();
            let body = envelope(
                &service,
                "GetSpecificPortMappingEntry",
                &[
                    ("NewRemoteHost", ""),
                    ("NewExternalPort", &port_text),
                    ("NewProtocol", "UDP"),
                ],
            );
            let to = fetch(
                &control,
                Some((&service, "GetSpecificPortMappingEntry", body)),
            )
            .and_then(|a| tag_text(&a, "NewInternalClient")?.parse().ok());
            (*port, to)
        })
        .collect();
    Some(Router { public, forwards })
}

/// Look again at this computer's address and ask the router, for the game `ports`, unless that
/// was done a moment ago for the same ports.
#[tauri::command]
pub fn check_sharing(shared: State<'_, Shared>, ports: Vec<u16>, force: bool) {
    let shared = shared.inner().clone();
    {
        let mut store = lock(&shared);
        let fresh = store.checked.is_some_and(|at| at.elapsed() < KEPT) && store.asked == ports;
        if store.view.checking || (fresh && !force) {
            return;
        }
        store.view.checking = true;
        store.asked.clone_from(&ports);
    }
    std::thread::spawn(move || {
        let lan = lan_address();
        let router = ask_router(SSDP, &ports, Duration::from_secs(2));
        let mut store = lock(&shared);
        store.view = View {
            lan,
            router: router.is_some(),
            shared_address: router
                .as_ref()
                .and_then(|r| r.public)
                .is_some_and(shared_address),
            public: router.as_ref().and_then(|r| r.public),
            forwards: router.map(|r| r.forwards).unwrap_or_default(),
            checking: false,
            outside: std::collections::BTreeMap::new(),
        };
        store.checked = Some(Instant::now());
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    const DESCRIPTION: &str = "<?xml version=\"1.0\"?><root><device><deviceList><device><serviceList>\
        <service><serviceType>urn:schemas-upnp-org:service:WANCommonInterfaceConfig:1</serviceType><controlURL>/common</controlURL></service>\
        <service><serviceType>urn:schemas-upnp-org:service:WANIPConnection:1</serviceType><controlURL>/ctl/IPConn</controlURL></service>\
        </serviceList></device></deviceList></device></root>";

    #[test]
    fn a_routers_answers_are_read() {
        assert_eq!(
            ssdp_location("HTTP/1.1 200 OK\r\nCACHE-CONTROL: max-age=120\r\nLocation: http://192.168.1.1:5000/rootDesc.xml\r\n\r\n").as_deref(),
            Some("http://192.168.1.1:5000/rootDesc.xml")
        );
        assert_eq!(ssdp_location("HTTP/1.1 200 OK\r\n\r\n"), None);
        assert_eq!(
            connection_service(DESCRIPTION, "http://192.168.1.1:5000/rootDesc.xml"),
            Some((
                "urn:schemas-upnp-org:service:WANIPConnection:1".into(),
                "http://192.168.1.1:5000/ctl/IPConn".into()
            ))
        );
        assert_eq!(
            tag_text(
                "<a><NewExternalIPAddress> 203.0.113.7 </NewExternalIPAddress></a>",
                "NewExternalIPAddress"
            ),
            Some("203.0.113.7")
        );
    }

    #[test]
    fn a_shared_or_private_outside_address_cannot_be_reached_from_the_internet() {
        for ip in [
            "10.1.2.3",
            "192.168.0.1",
            "172.20.0.1",
            "100.64.0.1",
            "100.127.255.254",
        ] {
            assert!(shared_address(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["203.0.113.7", "100.128.0.1", "8.8.8.8"] {
            assert!(!shared_address(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn a_router_on_the_internet_is_never_fetched_from() {
        assert_eq!(fetch("http://203.0.113.7/rootDesc.xml", None), None);
        assert_eq!(fetch("https://192.168.1.1/rootDesc.xml", None), None);
    }

    /// A stand-in router on this computer: an SSDP answer, a description and two actions.
    #[test]
    fn a_router_says_its_outside_address_and_which_computer_each_port_goes_to() {
        let http = TcpListener::bind("127.0.0.1:0").unwrap();
        let http_port = http.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for stream in http.incoming().take(4) {
                let Ok(mut stream) = stream else { return };
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                // The head, then as much body as it says.
                loop {
                    let Ok(n) = stream.read(&mut buf) else { return };
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&request);
                    if let Some(head_end) = text.find("\r\n\r\n") {
                        let length = text[..head_end]
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if request.len() >= head_end + 4 + length {
                            break;
                        }
                    }
                }
                let text = String::from_utf8_lossy(&request);
                let (status, body) = if text.starts_with("GET /rootDesc.xml") {
                    ("200 OK", DESCRIPTION.to_owned())
                } else if text.contains("GetExternalIPAddress") {
                    (
                        "200 OK",
                        "<r><NewExternalIPAddress>203.0.113.7</NewExternalIPAddress></r>"
                            .to_owned(),
                    )
                } else if text.contains("<NewExternalPort>9000</NewExternalPort>") {
                    ("200 OK", "<r><NewInternalPort>9000</NewInternalPort><NewInternalClient>192.168.1.20</NewInternalClient></r>".to_owned())
                } else {
                    (
                        "500 Internal Server Error",
                        "<r><errorCode>714</errorCode></r>".to_owned(),
                    )
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        let ssdp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let ssdp_at = ssdp.local_addr().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            // Both searches are read, and the first answered.
            for i in 0..2 {
                if let Ok((n, from)) = ssdp.recv_from(&mut buf) {
                    assert!(String::from_utf8_lossy(&buf[..n]).starts_with("M-SEARCH"));
                    let answer = format!(
                        "HTTP/1.1 200 OK\r\nST: urn:schemas-upnp-org:device:InternetGatewayDevice:1\r\nLOCATION: http://127.0.0.1:{http_port}/rootDesc.xml\r\n\r\n"
                    );
                    if i == 0 {
                        let _ = ssdp.send_to(answer.as_bytes(), from);
                    }
                }
            }
        });
        // The wait only guards against a hang: the stand-in answers at once.
        let router = ask_router(ssdp_at, &[9000, 9002], Duration::from_secs(30))
            .expect("the stand-in answers");
        assert_eq!(router.public, Some("203.0.113.7".parse().unwrap()));
        assert_eq!(
            router.forwards,
            [(9000, Some("192.168.1.20".parse().unwrap())), (9002, None)]
        );
    }

    /// A stand-in for the hub on this computer: each request it is sent is kept, and answered
    /// with the next of `answers` (a status, its extra header lines, and a body).
    fn stand_in_hub(
        answers: Vec<(&'static str, &'static str, String)>,
    ) -> (String, std::sync::mpsc::Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for ((status, headers, body), stream) in answers.into_iter().zip(listener.incoming()) {
                let Ok(mut stream) = stream else { return };
                let mut request = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    let Ok(n) = stream.read(&mut buf) else { return };
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&request).into_owned();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let length = text[..end]
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&request).into_owned());
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (base, rx)
    }

    /// The hub is asked to try this computer at the world's port, with the port alone, and its
    /// answer read: reached with what the world said, or a limit to wait out. A hub that cannot be
    /// reached is said plainly. The real hub is never asked.
    #[test]
    fn the_hub_is_asked_to_try_the_worlds_port_and_its_answer_read() {
        use dereth_launch::outside::Outcome;
        let (base, asked) = stand_in_hub(vec![
            (
                "200 OK",
                "",
                r#"{"address":"203.0.113.7","port":9000,"reached":true,"world":{"name":"My world","players":2}}"#.to_owned(),
            ),
            (
                "429 Too Many Requests",
                "Retry-After: 720\r\n",
                r#"{"error":{"code":"RATE_LIMITED","message":"Too many requests","retryAfterSeconds":720}}"#.to_owned(),
            ),
        ]);
        let wait = Duration::from_secs(60);
        let Outcome::Answered(answer) = ask_hub(&base, 9000, wait) else {
            panic!("an answer");
        };
        assert!(answer.reached);
        assert_eq!(answer.address, "203.0.113.7");
        assert_eq!(
            answer.world.and_then(|w| w.name).as_deref(),
            Some("My world")
        );
        let request = asked.recv().unwrap();
        assert!(request.starts_with("POST /v1/probe HTTP/1.1"), "{request}");
        assert!(
            request.ends_with(r#"{"port":9000}"#),
            "only the port: {request}"
        );
        assert_eq!(
            ask_hub(&base, 9000, wait),
            Outcome::TooSoon { seconds: 720 }
        );
        // Nothing listens at this address once the stand-in has answered twice.
        let closed = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", l.local_addr().unwrap())
        };
        assert!(matches!(
            ask_hub(&closed, 9000, wait),
            Outcome::Failed { .. }
        ));
    }

    /// The address the hub tried stands in for the public address when the router did not say it.
    #[test]
    fn the_address_the_hub_tried_stands_in_for_the_public_address() {
        use dereth_launch::outside::{Answer, Outcome};
        let shared: Shared = Shared::default();
        assert_eq!(view(&shared).public, None);
        lock(&shared).outside.insert(
            9000,
            Outside {
                checking: false,
                outcome: Some(Outcome::Answered(Answer {
                    address: "203.0.113.7".into(),
                    port: 9000,
                    reached: false,
                    world: None,
                })),
            },
        );
        let v = view(&shared);
        assert_eq!(v.public, Some("203.0.113.7".parse().unwrap()));
        assert!(v.outside.contains_key(&9000));
        lock(&shared).view.public = Some("198.51.100.4".parse().unwrap());
        assert_eq!(
            view(&shared).public,
            Some("198.51.100.4".parse().unwrap()),
            "the router's word first"
        );
    }
}
