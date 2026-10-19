//! aria2c sebagai proses anak: RPC hanya di 127.0.0.1 dengan port acak dan token acak per sesi. Token ditulis
//! ke file conf privat di folder runtime (bukan argumen, agar tidak terlihat di `ps`). Antrean yang belum
//! selesai disimpan aria2 sendiri (save-session) sehingga resume tetap jalan setelah restart.

use crate::platform;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Aria2 {
    url: String,
    token: String,
    conf: PathBuf,
    agent: ureq::Agent,
    child: Mutex<Option<Child>>,
    stopping: AtomicBool,
}

pub struct Options<'a> {
    pub runtime_dir: PathBuf,
    pub session: PathBuf,
    pub log: PathBuf,
    pub dir: &'a str,
    pub connections: u32,
    pub max_concurrent: u32,
    pub speed_limit_kib: u32,
    pub user_agent: &'a str,
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).expect("getrandom");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Aria2 {
    pub fn start(o: &Options) -> Result<Arc<Aria2>, String> {
        let port =
            std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map_err(|e| e.to_string())?.port();
        let token = random_hex(16);
        if !o.session.exists() {
            fs::write(&o.session, "").map_err(|e| e.to_string())?;
        }
        let n = o.connections.clamp(1, 16);
        let conf_text = format!(
            "enable-rpc=true\nrpc-listen-all=false\nrpc-listen-port={port}\nrpc-secret={token}\n\
             dir={dir}\ncontinue=true\nsplit={n}\nmax-connection-per-server={n}\nmin-split-size=1M\n\
             max-concurrent-downloads={mc}\nmax-overall-download-limit={lim}K\nfile-allocation=none\n\
             auto-file-renaming=true\ncontent-disposition-default-utf8=true\ninput-file={sess}\n\
             save-session={sess}\nsave-session-interval=10\nmax-tries=5\nretry-wait=3\n\
             user-agent={ua}\nlog={log}\nlog-level=warn\nconsole-log-level=error\nsummary-interval=0\n\
             stop-with-process={pid}\n",
            dir = o.dir,
            mc = o.max_concurrent.max(1),
            lim = o.speed_limit_kib,
            sess = o.session.display(),
            ua = o.user_agent,
            log = o.log.display(),
            pid = std::process::id(),
        );
        let conf = o.runtime_dir.join("aria2.conf");
        let _ = fs::remove_file(&conf);
        platform::create_private_file(&conf)
            .and_then(|mut f| f.write_all(conf_text.as_bytes()))
            .map_err(|e| format!("tulis {}: {e}", conf.display()))?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(5)))
            .proxy(None)
            .build()
            .into();
        let a = Arc::new(Aria2 {
            url: format!("http://127.0.0.1:{port}/jsonrpc"),
            token: format!("token:{token}"),
            conf,
            agent,
            child: Mutex::new(None),
            stopping: AtomicBool::new(false),
        });
        a.spawn()?;
        a.wait_ready()?;
        let watch = a.clone();
        std::thread::Builder::new()
            .name("aria2-watch".into())
            .spawn(move || watch.supervise())
            .map_err(|e| e.to_string())?;
        Ok(a)
    }

    fn spawn(&self) -> Result<(), String> {
        let child = platform::tool("aria2c")
            .arg(format!("--conf-path={}", self.conf.display()))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .map_err(|e| format!("aria2c tidak bisa dijalankan: {e}"))?;
        *self.child.lock().unwrap() = Some(child);
        Ok(())
    }

    fn wait_ready(&self) -> Result<(), String> {
        let t0 = Instant::now();
        loop {
            match self.call("aria2.getVersion", vec![]) {
                Ok(_) => return Ok(()),
                Err(e) if t0.elapsed() > Duration::from_secs(8) => return Err(format!("aria2 tidak merespons: {e}")),
                Err(_) => std::thread::sleep(Duration::from_millis(100)),
            }
        }
    }

    /// Jalankan ulang aria2 kalau mati sendiri (port & token sama, sesi dimuat ulang dari file).
    fn supervise(&self) {
        while !self.stopping.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(500));
            let exited = match self.child.lock().unwrap().as_mut() {
                Some(c) => matches!(c.try_wait(), Ok(Some(_))),
                None => false,
            };
            if exited && !self.stopping.load(Ordering::SeqCst) {
                eprintln!("kara-dm: aria2c berhenti, dijalankan ulang");
                std::thread::sleep(Duration::from_secs(2));
                if self.spawn().is_ok() {
                    let _ = self.wait_ready();
                }
            }
        }
    }

    pub fn call(&self, method: &str, params: Vec<Value>) -> Result<Value, String> {
        let mut p = vec![Value::String(self.token.clone())];
        p.extend(params);
        self.raw(json!({"jsonrpc": "2.0", "id": "k", "method": method, "params": p}))
    }

    /// Beberapa panggilan dalam satu request (system.multicall); tiap hasil = Ok(nilai) atau Err(pesan).
    pub fn multicall(&self, calls: &[(&str, Vec<Value>)]) -> Result<Vec<Result<Value, String>>, String> {
        let list: Vec<Value> = calls
            .iter()
            .map(|(m, ps)| {
                let mut p = vec![Value::String(self.token.clone())];
                p.extend(ps.iter().cloned());
                json!({"methodName": m, "params": p})
            })
            .collect();
        let res = self.raw(json!({"jsonrpc": "2.0", "id": "k", "method": "system.multicall", "params": [list]}))?;
        Ok(res
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|r| match r {
                Value::Array(mut a) if !a.is_empty() => Ok(a.remove(0)),
                other => Err(other.get("message").and_then(Value::as_str).unwrap_or("galat aria2").to_string()),
            })
            .collect())
    }

    fn raw(&self, body: Value) -> Result<Value, String> {
        let mut resp = self.agent.post(&self.url).send_json(&body).map_err(|e| e.to_string())?;
        let v: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
        if let Some(err) = v.get("error") {
            return Err(err.get("message").and_then(Value::as_str).unwrap_or("galat aria2").to_string());
        }
        Ok(v.get("result").cloned().unwrap_or(Value::Null))
    }

    pub fn shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = self.call("aria2.saveSession", vec![]);
        let _ = self.call("aria2.shutdown", vec![]);
        if let Some(mut c) = self.child.lock().unwrap().take() {
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_secs(4) {
                if let Ok(Some(_)) = c.try_wait() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let _ = c.kill();
            let _ = c.wait();
        }
        let _ = fs::remove_file(&self.conf);
    }
}
