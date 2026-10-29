//! Cek pembaruan dari rilis GitHub. Lewat curl (ureq di sini tanpa TLS), sama seperti unduhan POST.
//! Windows: installer rilis diunduh, SHA-256-nya dicocokkan dengan yang dicatat GitHub, lalu dipasang diam-diam
//! (installer membuka Kara DM lagi). Linux (Flatpak/paket distro): hanya memberi tahu dan membuka halaman rilis,
//! karena aplikasi tidak bisa mengganti dirinya sendiri di sana.

use crate::platform;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const RELEASES_URL: &str = "https://github.com/kamisama-v0/kara-dm/releases";
const API_LATEST: &str = "https://api.github.com/repos/kamisama-v0/kara-dm/releases/latest";
const DOWNLOAD_PREFIX: &str = "https://github.com/kamisama-v0/kara-dm/releases/download/";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub installer: Option<Asset>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub sha256: String,
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.trim().trim_start_matches('v').split(['-', '+']).next()?;
    let mut p = core.split('.');
    let mut n = || p.next().map(str::parse::<u64>).unwrap_or(Ok(0)).ok();
    let v = (n()?, n()?, n()?);
    p.next().is_none().then_some(v)
}

/// `latest` lebih baru dari `current` (semver x.y.z; label pra-rilis diabaikan).
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(a), Some(b)) if a > b)
}

/// Rilis dari JSON `releases/latest`. Installer hanya diterima dari repo ini dan dengan digest SHA-256.
pub fn parse_release(v: &Value) -> Option<Release> {
    if v["draft"].as_bool() == Some(true) || v["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let tag = v["tag_name"].as_str()?;
    parse_version(tag)?;
    let page = v["html_url"].as_str().filter(|u| u.starts_with(RELEASES_URL)).unwrap_or(RELEASES_URL);
    let installer = v["assets"].as_array().into_iter().flatten().find_map(|a| {
        let name = a["name"].as_str()?;
        let url = a["browser_download_url"].as_str()?;
        let sha = a["digest"].as_str()?.strip_prefix("sha256:")?;
        (name.ends_with("-windows-x64-setup.exe")
            && !name.contains(['/', '\\'])
            && url.starts_with(DOWNLOAD_PREFIX)
            && sha.len() == 64
            && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| Asset { name: name.into(), url: url.into(), sha256: sha.to_ascii_lowercase() })
    });
    Some(Release { version: tag.trim_start_matches('v').into(), page: page.into(), installer })
}

fn curl(ua: &str) -> std::process::Command {
    let mut c = platform::tool("curl");
    c.args(["-fsSL", "--proto", "=https", "--proto-redir", "=https", "-A"]).arg(ua);
    c
}

fn run(mut cmd: std::process::Command) -> Result<Vec<u8>, String> {
    let out = cmd.output().map_err(|e| format!("curl could not be started: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { "could not reach GitHub".into() } else { err })
    }
}

pub fn fetch_latest(ua: &str) -> Result<Release, String> {
    let mut cmd = curl(ua);
    cmd.args(["--max-time", "20", "-H", "Accept: application/vnd.github+json", API_LATEST]);
    let v: Value = serde_json::from_slice(&run(cmd)?).map_err(|e| format!("unreadable answer from GitHub: {e}"))?;
    parse_release(&v).ok_or_else(|| "the latest GitHub release was not recognised".into())
}

/// Unduh installer ke `dir` dan pastikan SHA-256-nya sama dengan yang dicatat GitHub.
pub fn download(a: &Asset, dir: &Path, ua: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(&a.name);
    let mut cmd = curl(ua);
    cmd.args(["--max-time", "900", "-o"]).arg(&path).arg(&a.url);
    run(cmd)?;
    let got = sha256_file(&path).map_err(|e| e.to_string())?;
    if got != a.sha256 {
        let _ = std::fs::remove_file(&path);
        return Err("installer checksum does not match the GitHub release; update cancelled".into());
    }
    Ok(path)
}

/// Jalankan installer Inno Setup tanpa pertanyaan (jendela progres tetap tampak). Installer menutup instance
/// lama lewat Restart Manager bila masih jalan, dan membuka Kara DM lagi setelah selesai.
pub fn launch_installer(path: &Path) -> Result<(), String> {
    std::process::Command::new(path)
        .args(["/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("the installer could not be started: {e}"))
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.hex())
}

/// SHA-256 (FIPS 180-4) kecil, cukup untuk memeriksa satu installer tanpa menambah dependensi.
struct Sha256 {
    h: [u32; 8],
    buf: Vec<u8>,
    len: u64,
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

impl Sha256 {
    fn new() -> Self {
        Sha256 {
            h: [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19],
            buf: Vec::with_capacity(64),
            len: 0,
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.len += data.len() as u64;
        while !data.is_empty() {
            let take = (64 - self.buf.len()).min(data.len());
            self.buf.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.buf.len() == 64 {
                let block: [u8; 64] = self.buf[..].try_into().expect("64 byte");
                self.block(&block);
                self.buf.clear();
            }
        }
    }

    fn block(&mut self, b: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, c) in b.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*c);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = self.h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (h, x) in self.h.iter_mut().zip(v) {
            *h = h.wrapping_add(x);
        }
    }

    fn hex(mut self) -> String {
        let bits = self.len.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        pad.resize((119 - (self.len % 64) as usize) % 64 + 1, 0);
        pad.extend_from_slice(&bits.to_be_bytes());
        let len = self.len;
        self.update(&pad);
        self.len = len;
        self.h.iter().map(|x| format!("{x:08x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(data: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(data);
        h.hex()
    }

    #[test]
    fn sha256_matches_known_vectors() {
        assert_eq!(sha(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(sha(long), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        let mut h = Sha256::new();
        for _ in 0..1000 {
            h.update(&[b'a'; 1000]);
        }
        assert_eq!(h.hex(), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    /// Rilis sungguhan di GitHub: `cargo test -- --ignored live_github` (butuh jaringan, unduh ±40 MB).
    #[test]
    #[ignore]
    fn live_github() {
        let r = fetch_latest("kara-dm-test").unwrap();
        let a = r.installer.expect("rilis terbaru punya installer Windows dengan digest");
        let dir = std::env::temp_dir().join(format!("kara-dm-update-{}", crate::model::new_id()));
        let p = download(&a, &dir, "kara-dm-test").unwrap();
        assert_eq!(sha256_file(&p).unwrap(), a.sha256);
        let mut bad = a.clone();
        bad.sha256 = "0".repeat(64);
        assert!(download(&bad, &dir, "kara-dm-test").unwrap_err().contains("checksum"));
        assert!(!dir.join(&a.name).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn versions() {
        assert!(is_newer("v0.3.1", "0.3.0"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(!is_newer("v0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
        assert!(!is_newer("nightly", "0.3.0"));
        assert!(!is_newer("1.2.3.4", "0.3.0"));
    }

    #[test]
    fn release_json() {
        let digest = format!("sha256:{}", "A".repeat(64));
        let mut v = serde_json::json!({
            "tag_name": "v0.4.0",
            "html_url": "https://github.com/kamisama-v0/kara-dm/releases/tag/v0.4.0",
            "draft": false, "prerelease": false,
            "assets": [
                { "name": "kara-dm-linux-x86_64.flatpak", "browser_download_url": "https://github.com/kamisama-v0/kara-dm/releases/download/v0.4.0/kara-dm-linux-x86_64.flatpak", "digest": digest },
                { "name": "kara-dm-0.4.0-windows-x64-setup.exe", "browser_download_url": "https://github.com/kamisama-v0/kara-dm/releases/download/v0.4.0/kara-dm-0.4.0-windows-x64-setup.exe", "digest": digest }
            ]
        });
        let r = parse_release(&v).unwrap();
        assert_eq!(r.version, "0.4.0");
        let a = r.installer.unwrap();
        assert_eq!(a.name, "kara-dm-0.4.0-windows-x64-setup.exe");
        assert_eq!(a.sha256, "a".repeat(64));
        // Installer dari tempat lain atau tanpa digest ditolak; rilis pra-rilis diabaikan.
        v["assets"][1]["browser_download_url"] = "https://evil.example/kara-dm-0.4.0-windows-x64-setup.exe".into();
        assert_eq!(parse_release(&v).unwrap().installer, None);
        v["prerelease"] = true.into();
        assert_eq!(parse_release(&v), None);
    }
}
