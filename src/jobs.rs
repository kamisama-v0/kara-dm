//! Unduhan yang tidak bisa ditangani aria2: HLS (yt-dlp), POST (curl), dan penggabungan DASH (ffmpeg).
//! Cookie dan body POST ditulis ke berkas privat di folder runtime, bukan di argumen perintah.

use crate::Event;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub type Slot = Arc<Mutex<Option<Child>>>;

pub struct Job {
    pub id: String,
    pub cmd: Command,
    /// Folder untuk menyelesaikan path relatif yang dicetak perintah.
    pub dir: PathBuf,
    /// File sementara yang dihapus setelah proses selesai.
    pub temp: Vec<PathBuf>,
    /// Path hasil kalau perintah tidak mencetaknya.
    pub output: Option<PathBuf>,
}

pub fn write_temp(dir: &Path, name: &str, data: &[u8]) -> std::io::Result<PathBuf> {
    let p = dir.join(format!("{name}-{}", crate::model::new_id()));
    let mut f = crate::platform::create_private_file(&p)?;
    f.write_all(data)?;
    Ok(p)
}

/// File cookie format Netscape untuk yt-dlp, dari header Cookie milik host URL.
pub fn netscape_cookies(url: &str, cookie: &str) -> String {
    let host = crate::protocol::url_host(url);
    let secure = if url.to_ascii_lowercase().starts_with("https://") { "TRUE" } else { "FALSE" };
    let mut s = String::from("# Netscape HTTP Cookie File\n");
    for pair in cookie.split(';') {
        if let Some((k, v)) = pair.trim().split_once('=') {
            s.push_str(&format!("{host}\tFALSE\t/\t{secure}\t0\t{}\t{}\n", k.trim(), v.trim()));
        }
    }
    s
}

pub fn spawn(mut job: Job, tx: async_channel::Sender<Event>) -> Result<Slot, String> {
    let mut child = job
        .cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{:?}: {e}", job.cmd.get_program()))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let slot: Slot = Arc::new(Mutex::new(Some(child)));
    let err_tail = Arc::new(Mutex::new(Vec::<String>::new()));
    {
        let err_tail = err_tail.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut t = err_tail.lock().unwrap();
                if !line.trim().is_empty() {
                    t.push(line);
                }
                if t.len() > 3 {
                    t.remove(0);
                }
            }
        });
    }
    let s2 = slot.clone();
    std::thread::spawn(move || {
        let mut path: Option<PathBuf> = job.output.clone();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("KDM ") {
                let f: Vec<&str> = rest.split_whitespace().collect();
                let num = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) as u64;
                let total = if num(1) > 0 { num(1) } else { num(2) };
                let _ = tx.send_blocking(Event::JobProgress { id: job.id.clone(), done: num(0), total, speed: num(3) });
            } else if !line.is_empty() {
                let p = PathBuf::from(line);
                path = Some(if p.is_absolute() { p } else { job.dir.join(p) });
            }
        }
        let status = s2.lock().unwrap().as_mut().map(|c| c.wait());
        for t in &job.temp {
            let _ = fs::remove_file(t);
        }
        let result = match status {
            Some(Ok(s)) if s.success() => Ok(path.map(|p| p.to_string_lossy().into_owned())),
            Some(Ok(s)) => {
                let tail = err_tail.lock().unwrap().join(" | ");
                Err(if tail.is_empty() { format!("keluar dengan {s}") } else { tail })
            }
            Some(Err(e)) => Err(e.to_string()),
            None => Err("dihentikan".into()),
        };
        let _ = tx.send_blocking(Event::JobDone { id: job.id, result });
    });
    Ok(slot)
}

pub fn kill(slot: &Slot) {
    if let Some(c) = slot.lock().unwrap().as_mut() {
        let _ = c.kill();
    }
}
