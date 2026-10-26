mod install_from_loopback;
mod listing;
mod running_server;
mod selection;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::sync::{Arc, Mutex};

/// A web server on loopback answering GET requests from a table of paths; anything else is 404.
pub(crate) struct Loopback {
    pub base: String,
    pub files: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    /// The `Link` header a path is answered with, where it has one.
    pub links: Arc<Mutex<HashMap<String, String>>>,
    /// Every path asked for, in turn.
    pub asked: Arc<Mutex<Vec<String>>>,
}

impl Loopback {
    pub fn start() -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let files: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::default();
        let links: Arc<Mutex<HashMap<String, String>>> = Arc::default();
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let (served, linked, seen) = (Arc::clone(&files), Arc::clone(&links), Arc::clone(&asked));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let served = Arc::clone(&served);
                let linked = Arc::clone(&linked);
                let seen = Arc::clone(&seen);
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request = String::new();
                    if reader.read_line(&mut request).is_err() {
                        return;
                    }
                    loop {
                        let mut header = String::new();
                        if reader.read_line(&mut header).is_err() || header.trim().is_empty() {
                            break;
                        }
                    }
                    let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                    seen.lock().unwrap().push(path.clone());
                    let body = served.lock().unwrap().get(&path).cloned();
                    let (status, body) = match body {
                        Some(b) => ("200 OK", b),
                        None => ("404 Not Found", b"not found".to_vec()),
                    };
                    let link = linked
                        .lock()
                        .unwrap()
                        .get(&path)
                        .map(|l| format!("Link: {l}\r\n"))
                        .unwrap_or_default();
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{link}Connection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                });
            }
        });
        Self {
            base,
            files,
            links,
            asked,
        }
    }

    pub fn put(&self, path: &str, bytes: impl Into<Vec<u8>>) {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_owned(), bytes.into());
    }
}
