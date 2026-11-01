//! The friends service's stream on the desktop: one TCP connection at a time, opened, read and
//! written on threads of its own so the frame never waits on it.
//!
//! The shell asks for a connection ([`FriendsSocket::dial`]) and writes to it; what happened comes
//! back as news, read each frame. An attempt that cannot connect within a few seconds fails, so a
//! world that leaves the port unanswered delays the login no longer than that.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream, ToSocketAddrs};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use auberean_shell::FriendsNews;

/// How long one address may take to answer.
const CONNECT_WAIT: Duration = Duration::from_secs(5);

/// One open (or opening) connection.
struct Open {
    out: Sender<Vec<u8>>,
    news: Receiver<FriendsNews>,
    /// A handle to shut the connection down from this side.
    stream: Receiver<TcpStream>,
    held: Option<TcpStream>,
}

/// The friends service's stream.
#[derive(Default)]
pub struct FriendsSocket {
    open: Option<Open>,
}

impl FriendsSocket {
    /// Connect to `host` and `port` (closing a connection open before).
    pub fn dial(&mut self, host: &str, port: u16) {
        self.close();
        let (out_tx, out_rx) = channel::<Vec<u8>>();
        let (news_tx, news_rx) = channel();
        let (stream_tx, stream_rx) = channel();
        let host = host.to_owned();
        let spawned = std::thread::Builder::new()
            .name("friends stream".into())
            .spawn(move || run(&host, port, &out_rx, &news_tx, &stream_tx));
        if spawned.is_err() {
            let (tx, rx) = channel();
            let _ = tx.send(FriendsNews::Failed);
            self.open = Some(Open {
                out: channel().0,
                news: rx,
                stream: channel().1,
                held: None,
            });
            return;
        }
        self.open = Some(Open {
            out: out_tx,
            news: news_rx,
            stream: stream_rx,
            held: None,
        });
    }

    /// Write to the connection.
    pub fn send(&mut self, bytes: &[u8]) {
        if let Some(o) = &self.open {
            let _ = o.out.send(bytes.to_vec());
        }
    }

    /// Close the connection: whatever its threads still hold ends with it.
    pub fn close(&mut self) {
        if let Some(mut o) = self.open.take() {
            if o.held.is_none() {
                o.held = o.stream.try_recv().ok();
            }
            if let Some(s) = &o.held {
                let _ = s.shutdown(Shutdown::Both);
            }
        }
    }

    /// The next thing that happened.
    pub fn news(&mut self) -> Option<FriendsNews> {
        let o = self.open.as_mut()?;
        if o.held.is_none() {
            o.held = o.stream.try_recv().ok();
        }
        o.news.try_recv().ok()
    }
}

/// The connection's own thread: connect to the first address that answers, then read on another
/// thread while this one writes.
fn run(
    host: &str,
    port: u16,
    out: &Receiver<Vec<u8>>,
    news: &Sender<FriendsNews>,
    handle: &Sender<TcpStream>,
) {
    let stream = (host, port).to_socket_addrs().ok().and_then(|addrs| {
        addrs
            .into_iter()
            .find_map(|a| TcpStream::connect_timeout(&a, CONNECT_WAIT).ok())
    });
    let Some(mut stream) = stream else {
        let _ = news.send(FriendsNews::Failed);
        return;
    };
    let _ = stream.set_nodelay(true);
    let (Ok(mut reading), Ok(held)) = (stream.try_clone(), stream.try_clone()) else {
        let _ = news.send(FriendsNews::Failed);
        return;
    };
    let _ = handle.send(held);
    let _ = news.send(FriendsNews::Opened);
    let read_news = news.clone();
    let reader = std::thread::Builder::new()
        .name("friends stream reader".into())
        .spawn(move || {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                match reading.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if read_news
                            .send(FriendsNews::Bytes(buf[..n].to_vec()))
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
            let _ = read_news.send(FriendsNews::Closed);
        });
    if reader.is_err() {
        let _ = news.send(FriendsNews::Closed);
        return;
    }
    while let Ok(bytes) = out.recv() {
        if stream.write_all(&bytes).is_err() {
            break;
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}
