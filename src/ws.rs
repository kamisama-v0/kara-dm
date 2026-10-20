//! Server WebSocket yang meniru sisi aplikasi NDM (127.0.0.1:10007, subprotocol neatextension.v1).
//!
//! Halaman web biasa juga bisa membuka WebSocket ke localhost, jadi hanya koneksi dengan Origin ekstensi
//! browser (moz-extension:// / chrome-extension://) yang diterima; selain itu ditolak 403 saat handshake.

use crate::Event;
use crate::protocol::{PORT, SUBPROTOCOL};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tungstenite::Message;
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::http::{HeaderValue, StatusCode};
use tungstenite::protocol::WebSocketConfig;

const MAX_MESSAGE: usize = 1 << 20;

pub struct Shared {
    pub connections: AtomicUsize,
    pub media_panel: AtomicBool,
}

/// Buka 127.0.0.1:10007. Kalau port masih dipakai (mis. Neat Download Manager belum ditutup saat login), coba
/// lagi tiap 5 detik di latar belakang dan kirim `Event::WsReady` begitu berhasil. Hasil percobaan pertama
/// dikembalikan supaya bisa ditampilkan.
pub fn start(tx: async_channel::Sender<Event>, shared: Arc<Shared>) -> std::io::Result<()> {
    let first = TcpListener::bind(("127.0.0.1", PORT));
    let err = first.as_ref().err().map(|e| std::io::Error::new(e.kind(), e.to_string()));
    std::thread::Builder::new().name("ws-accept".into()).spawn(move || {
        let listener = match first {
            Ok(l) => l,
            Err(_) => loop {
                std::thread::sleep(Duration::from_secs(5));
                if let Ok(l) = TcpListener::bind(("127.0.0.1", PORT)) {
                    let _ = tx.send_blocking(Event::WsReady);
                    break l;
                }
            },
        };
        for stream in listener.incoming().flatten() {
            let (tx, shared) = (tx.clone(), shared.clone());
            let _ = std::thread::Builder::new().name("ws-conn".into()).spawn(move || handle(stream, tx, shared));
        }
    })?;
    err.map_or(Ok(()), Err)
}

fn origin_allowed(req: &Request) -> bool {
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
    let origin = header("origin");
    let extension = origin.starts_with("moz-extension://") || origin.starts_with("chrome-extension://");
    extension && header("sec-websocket-protocol").split(',').any(|p| p.trim() == SUBPROTOCOL)
}

fn handle(stream: TcpStream, tx: async_channel::Sender<Event>, shared: Arc<Shared>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    // Tipe Err ditentukan trait Callback milik tungstenite.
    #[allow(clippy::result_large_err)]
    let callback = |req: &Request, mut resp: Response| -> Result<Response, ErrorResponse> {
        if !origin_allowed(req) {
            let mut err = ErrorResponse::new(Some("forbidden".into()));
            *err.status_mut() = StatusCode::FORBIDDEN;
            return Err(err);
        }
        resp.headers_mut().insert("Sec-WebSocket-Protocol", HeaderValue::from_static(SUBPROTOCOL));
        Ok(resp)
    };
    let config = WebSocketConfig::default().max_message_size(Some(MAX_MESSAGE)).max_frame_size(Some(MAX_MESSAGE));
    let Ok(mut ws) = tungstenite::accept_hdr_with_config(stream, callback, Some(config)) else { return };
    let _ = ws.get_ref().set_read_timeout(None);
    shared.connections.fetch_add(1, Ordering::SeqCst);
    let panel = if shared.media_panel.load(Ordering::SeqCst) { "ShowPanelFox=1" } else { "ShowPanelFox=0" };
    let _ = ws.send(Message::text(panel));
    loop {
        match ws.read() {
            Ok(Message::Text(t)) => {
                if tx.send_blocking(Event::Extension(t.as_str().to_owned())).is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }
    shared.connections.fetch_sub(1, Ordering::SeqCst);
}
