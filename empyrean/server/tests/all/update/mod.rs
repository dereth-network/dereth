mod install_from_loopback;
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
}

impl Loopback {
    pub fn start() -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let files: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::default();
        let served = Arc::clone(&files);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let served = Arc::clone(&served);
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
                    let body = served.lock().unwrap().get(&path).cloned();
                    let (status, body) = match body {
                        Some(b) => ("200 OK", b),
                        None => ("404 Not Found", b"not found".to_vec()),
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                });
            }
        });
        Self { base, files }
    }

    pub fn put(&self, path: &str, bytes: impl Into<Vec<u8>>) {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_owned(), bytes.into());
    }
}
