//! Satu instance di Windows. Di Linux GApplication memakai D-Bus untuk ini; Windows tidak punya session bus,
//! jadi instance pertama membuka 127.0.0.1:10008 dan peluncuran berikutnya (shortcut, `kara-dm URL`) mengirim
//! argumennya ke sana lalu keluar. Kemampuannya sama dengan baris perintah biasa.
#![cfg_attr(not(windows), allow(dead_code))]

use crate::Event;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

const ADDR: &str = "127.0.0.1:10008";
const HELLO: &str = "kara-dm/1";
const MAX_ARGS: usize = 64;

/// Kirim argumen ke instance yang sudah berjalan. `true` = sudah ditangani di sana.
pub fn forward(args: &[String]) -> bool {
    let addr: SocketAddr = ADDR.parse().expect("alamat tetap");
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_millis(300)) else { return false };
    let mut msg = format!("{HELLO}\n");
    for a in args.iter().take(MAX_ARGS) {
        msg.push_str(&a.replace(['\r', '\n'], " "));
        msg.push('\n');
    }
    msg.push('\n');
    s.write_all(msg.as_bytes()).is_ok()
}

/// Terima argumen dari peluncuran berikutnya dan teruskan ke loop utama sebagai Event::Remote.
pub fn listen(tx: async_channel::Sender<Event>) {
    let Ok(listener) = TcpListener::bind(ADDR) else { return };
    let _ = std::thread::Builder::new().name("instance".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut lines = BufReader::new(stream.take(64 * 1024)).lines();
            if lines.next().and_then(Result::ok).as_deref() != Some(HELLO) {
                continue;
            }
            let args: Vec<String> = lines.map_while(Result::ok).take_while(|l| !l.is_empty()).take(MAX_ARGS).collect();
            if tx.send_blocking(Event::Remote(args)).is_err() {
                break;
            }
        }
    });
}
