use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

pub fn serve<F>(count: usize, mut handler: F) -> String
where
    F: FnMut(&str, &str, &str, &str) -> (u16, String) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(count) {
            let Ok(mut stream) = stream else { continue };
            let Ok(read) = stream.try_clone() else {
                continue;
            };
            let mut reader = BufReader::new(read);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_owned();
            let path = parts.next().unwrap_or("").to_owned();
            let (mut length, mut auth) = (0, String::new());
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).is_err() {
                    break;
                }
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                let Some((key, value)) = header.split_once(':') else {
                    continue;
                };
                match key.to_ascii_lowercase().as_str() {
                    "content-length" => length = value.trim().parse().unwrap_or(0),
                    "authorization" => auth = value.trim().to_owned(),
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_err() {
                continue;
            }
            let body = String::from_utf8_lossy(&body).into_owned();
            let (status, text) = handler(&method, &path, &auth, &body);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    format!("http://{addr}")
}
