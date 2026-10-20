//! Protokol ekstensi "NeatDownloadManager Extension" (diamati dari bg.js v1.9.9 dan string NeatDM.exe 1.4.24).
//!
//! Ekstensi membuka `ws://127.0.0.1:10007/download` dengan subprotocol `neatextension.v1` lalu mengirim satu
//! pesan teks per unduhan: baris `kunci:nilai\r\n`. Kunci angka: 1 metode, 2 URL, 3 URL audio (DASH, digabung
//! jadi MKV), 4 judul halaman, 5 URL halaman, 6 jenis (normal|media|hls), 7 ukuran, 8 mime, 9 label tampilan.
//! Header HTTP ikut sebagai baris biasa (Origin, Referer, Cookie, Content-Type, x-*). Body POST ditaruh di
//! akhir setelah penanda `__0NeatPostData9__:`. Aplikasi boleh mengirim `waiting`/`nowaiting` dan
//! `ShowPanelFox=0|1` (panel tangkap video di halaman).
//!
//! Tambahan khusus Kara DM (diabaikan NDM): `10` kualitas yt-dlp (`2160`/`1440`/`1080`/`720`/`480`/`360` = tinggi
//! maksimum, `audio` = audio saja; tanpa kunci = terbaik), `11:1` bagian dari unduhan massal (tanpa jendela
//! progres/notifikasi per berkas).

pub const PORT: u16 = 10007;
pub const SUBPROTOCOL: &str = "neatextension.v1";
const POST_MARK: &str = "__0NeatPostData9__:";

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Request {
    pub method: String,
    pub url: String,
    pub url2: Option<String>,
    pub title: String,
    pub page: String,
    pub kind: String,
    pub size: u64,
    pub mime: String,
    pub label: String,
    /// Referer, Origin, Cookie dan header x-* yang harus ikut dikirim ke server.
    pub headers: Vec<(String, String)>,
    pub content_type: Option<String>,
    pub body: Option<String>,
    /// Kualitas yt-dlp yang sudah divalidasi (lihat [`quality`]); kosong = terbaik.
    pub quality: String,
    pub batch: bool,
}

impl Request {
    pub fn from_url(url: &str) -> Result<Request, String> {
        let url = url.trim();
        if !valid_url(url) {
            return Err(format!("invalid URL: {url}"));
        }
        Ok(Request { method: "GET".into(), url: url.into(), kind: "normal".into(), ..Default::default() })
    }

    pub fn is_hls(&self) -> bool {
        self.kind == "hls" || url_path(&self.url).to_ascii_lowercase().ends_with(".m3u8")
    }
}

pub fn parse(text: &str) -> Result<Request, String> {
    let (head, body) = match text.find(POST_MARK) {
        Some(i) => (&text[..i], Some(text[i + POST_MARK.len()..].to_string())),
        None => (text, None),
    };
    let mut r = Request { method: "GET".into(), kind: "normal".into(), body, ..Default::default() };
    for line in head.split("\r\n") {
        let Some((k, v)) = line.split_once(':') else { continue };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "1" => r.method = v.to_ascii_uppercase(),
            "2" => r.url = v.into(),
            "3" if !v.is_empty() => r.url2 = Some(v.into()),
            "4" => r.title = v.into(),
            "5" => r.page = v.into(),
            "6" if !v.is_empty() => r.kind = v.to_ascii_lowercase(),
            "7" => r.size = v.parse().unwrap_or(0),
            "8" => r.mime = v.into(),
            "9" => r.label = v.into(),
            "10" => r.quality = quality(v).unwrap_or_default().into(),
            "11" => r.batch = v == "1",
            _ => {
                let l = k.to_ascii_lowercase();
                if l == "content-type" {
                    r.content_type = Some(v.into());
                } else if !v.is_empty()
                    && (matches!(l.as_str(), "referer" | "origin" | "cookie") || l.starts_with("x-"))
                {
                    r.headers.push((k.into(), v.into()));
                }
            }
        }
    }
    if !valid_url(&r.url) {
        return Err(format!("invalid URL: {:?}", r.url));
    }
    if r.method != "GET" && r.method != "POST" {
        return Err(format!("unsupported method: {}", r.method));
    }
    if r.url2.as_deref().is_some_and(|u| !valid_url(u)) {
        r.url2 = None;
    }
    Ok(r)
}

/// Nilai kualitas yang dikenal; selain itu (termasuk `best`) = terbaik.
pub fn quality(v: &str) -> Option<&'static str> {
    ["2160", "1440", "1080", "720", "480", "360", "audio"].into_iter().find(|q| *q == v)
}

/// Argumen pemilihan format yt-dlp untuk kualitas tersebut.
pub fn ytdlp_format_args(quality: &str) -> Vec<String> {
    match quality {
        "audio" => vec!["-f".into(), "bestaudio[ext=m4a]/bestaudio/best".into()],
        h if !h.is_empty() => vec!["-S".into(), format!("res:{h}")],
        _ => vec![],
    }
}

/// Judul jadi nama berkas ringkas: deretan selain huruf/angka menjadi satu `-`, mis. "niki - Tsunami (live)" ->
/// "niki-Tsunami-live". Huruf non-Latin tetap dipertahankan.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').chars().take(150).collect()
}

/// Halaman "Google Drive can't scan this file for viruses" yang dikirim Google alih-alih berkas besar. Token `at`
/// di URL dari browser hanya berlaku sekali (sudah dipakai Firefox), jadi permintaan aria2 mendapat halaman ini
/// lagi berisi token baru. Kembalikan URL unduhan dari formulirnya, hanya bila tujuannya tetap Google Drive.
pub fn drive_confirm_url(html: &str) -> Option<String> {
    let form_start = html.find("id=\"download-form\"")?;
    let tag_open = html[..form_start].rfind("<form")?;
    let form = &html[tag_open..];
    let form = &form[..form.find("</form>").unwrap_or(form.len())];
    let action = html_unescape(attr(&form[..form.find('>')?], "action")?);
    if !action.starts_with("https://drive.usercontent.google.com/") || action.contains(['?', '#']) {
        return None;
    }
    let mut query = vec![];
    for input in form.split("<input").skip(1) {
        let tag = &input[..input.find('>').unwrap_or(input.len())];
        if let (Some(name), Some(value)) = (attr(tag, "name"), attr(tag, "value")) {
            query.push(format!("{}={}", percent_encode(&html_unescape(name)), percent_encode(&html_unescape(value))));
        }
    }
    (!query.is_empty()).then(|| format!("{action}?{}", query.join("&")))
}

/// Awal berkas berupa halaman HTML (bukan berkas biner/teks yang memang diminta).
pub fn is_html(text: &str) -> bool {
    let head: String = text.trim_start().chars().take(64).collect::<String>().to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html")
}

/// Pesan untuk halaman error Google Drive, dari judulnya (mis. "Quota exceeded").
pub fn drive_page_error(html: &str) -> String {
    let title = html
        .find("<title>")
        .and_then(|s| html[s + 7..].find("</title>").map(|e| &html[s + 7..s + 7 + e]))
        .map(|t| html_unescape(t.trim().trim_start_matches("Google Drive - ")))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "unknown page".into());
    let hint = if title.to_ascii_lowercase().contains("quota") {
        ". The file was downloaded too often; try again later (up to 24 hours) or download a copy of it (Make a copy)"
    } else {
        ""
    };
    format!("Google Drive refused the download: {title}{hint}")
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    Some(&tag[start..start + tag[start..].find('"')?])
}

fn html_unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn valid_url(u: &str) -> bool {
    let l = u.to_ascii_lowercase();
    (l.starts_with("http://") || l.starts_with("https://") || l.starts_with("ftp://"))
        && u.len() > 8
        && !u.chars().any(|c| c.is_whitespace() || c.is_control())
}

pub fn url_host(u: &str) -> &str {
    let rest = u.split_once("://").map(|x| x.1).unwrap_or(u);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    host.split(':').next().unwrap_or(host)
}

fn url_path(u: &str) -> &str {
    let rest = u.split_once("://").map(|x| x.1).unwrap_or(u);
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    rest.find('/').map(|i| &rest[i..]).unwrap_or("")
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 3 <= b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Nama dari segmen terakhir path URL (sudah di-decode dan disanitasi); kosong kalau tidak ada.
pub fn url_basename(u: &str) -> String {
    let seg = url_path(u).rsplit('/').next().unwrap_or("");
    if seg.is_empty() { String::new() } else { sanitize(&percent_decode(seg)) }
}

/// Ekstensi (huruf kecil, tanpa titik) dari nama file, kalau masuk akal.
pub fn ext_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && (1..=5).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| ext.to_ascii_lowercase())
}

/// Nama file aman di Linux dan Windows: tanpa pemisah path, karakter kontrol atau `<>:"|?*`, tidak diawali titik,
/// bukan nama perangkat Windows (CON, NUL, COM1, ...), maks ±200 byte.
pub fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_control() || "/\\<>:\"|?*".contains(c) { '_' } else { c }).collect();
    let mut s = s.trim().trim_start_matches('.').trim_end_matches(['.', ' ']).trim().to_string();
    if s.is_empty() {
        return "download".into();
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if device {
        s.insert(0, '_');
    }
    if s.len() <= 200 {
        return s;
    }
    let ext = ext_of(&s).map(|e| format!(".{e}")).unwrap_or_default();
    let mut cut = 200 - ext.len();
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{}", s[..cut].trim_end(), ext)
}

/// Judul halaman tanpa akhiran situs yang umum, untuk dijadikan nama file media.
pub fn title_stem(title: &str) -> String {
    let mut t = title.trim();
    for suf in [" - YouTube", " | Vimeo", " on Vimeo"] {
        if let Some(x) = t.strip_suffix(suf) {
            t = x;
        }
    }
    sanitize(t)
}

/// "MP4 File 12 MB" -> mp4, "MKV File 1080p" -> mkv, "VTT Subtitles File" -> vtt.
pub fn ext_from_label(label: &str) -> Option<String> {
    let w = label.split_whitespace().next()?;
    ((1..=4).contains(&w.len()) && w.chars().all(|c| c.is_ascii_alphanumeric())).then(|| w.to_ascii_lowercase())
}

pub fn ext_from_mime(mime: &str) -> Option<&'static str> {
    let m = mime.split(';').next()?.trim().to_ascii_lowercase();
    Some(match m.as_str() {
        "video/mp4" | "video/x-mp4" => "mp4",
        "video/webm" => "webm",
        "audio/webm" => "weba",
        "video/x-matroska" => "mkv",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" => "m4a",
        "application/pdf" => "pdf",
        "application/zip" | "application/x-zip-compressed" => "zip",
        "application/x-rar-compressed" | "application/vnd.rar" => "rar",
        "application/x-7z-compressed" => "7z",
        "application/gzip" | "application/x-gzip" => "gz",
        "application/x-msdownload" => "exe",
        "application/vnd.debian.binary-package" => "deb",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "text/vtt" => "vtt",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_get_from_extension() {
        let msg = "1:GET\r\n2:https://example.com/files/a%20b.zip?x=1\r\n6:normal\r\n4:Halaman\r\nOrigin: https://example.com\r\nReferer: https://example.com/page\r\n5:https://example.com/page\r\nCookie: a=1; b=2\r\n8:application/zip\r\n7:1234\r\n";
        let r = parse(msg).unwrap();
        assert_eq!(r.method, "GET");
        assert_eq!(r.url, "https://example.com/files/a%20b.zip?x=1");
        assert_eq!(r.size, 1234);
        assert_eq!(r.page, "https://example.com/page");
        assert_eq!(r.headers.len(), 3);
        assert!(r.headers.contains(&("Cookie".into(), "a=1; b=2".into())));
        assert_eq!(url_basename(&r.url), "a b.zip");
        assert_eq!(r.body, None);
    }

    #[test]
    fn parse_post_and_dash() {
        let msg = "1:POST\r\n2:https://x.org/dl\r\n6:normal\r\nContent-Type: application/x-www-form-urlencoded\r\nx-token: q\r\n__0NeatPostData9__:a=1&b=2\r\nlagi";
        let r = parse(msg).unwrap();
        assert_eq!(r.method, "POST");
        assert_eq!(r.body.as_deref(), Some("a=1&b=2\r\nlagi"));
        assert_eq!(r.headers, vec![("x-token".to_string(), "q".to_string())]);
        let d =
            parse("1:GET\r\n2:https://v.example/v\r\n3:https://v.example/a\r\n6:media\r\n9:MKV File 1080p, 3 min\r\n")
                .unwrap();
        assert_eq!(d.url2.as_deref(), Some("https://v.example/a"));
        assert_eq!(ext_from_label(&d.label).as_deref(), Some("mkv"));
    }

    #[test]
    fn parse_kara_quality_and_batch() {
        let r = parse("1:GET\r\n2:https://youtu.be/x\r\n6:hls\r\n10:720\r\n11:1\r\n").unwrap();
        assert_eq!((r.quality.as_str(), r.batch), ("720", true));
        assert_eq!(ytdlp_format_args(&r.quality), ["-S", "res:720"]);
        assert_eq!(ytdlp_format_args("audio"), ["-f", "bestaudio[ext=m4a]/bestaudio/best"]);
        let r = parse("1:GET\r\n2:https://youtu.be/x\r\n10:--exec rm\r\n").unwrap();
        assert_eq!((r.quality.as_str(), r.batch), ("", false));
        assert!(ytdlp_format_args(&r.quality).is_empty());
        assert!(r.headers.is_empty());
    }

    #[test]
    fn drive_virus_warning_page_gives_fresh_url() {
        let page = r#"<html><body>Google Drive can't scan this file for viruses.<form id="download-form" action="https://drive.usercontent.google.com/download" method="get"><input type="submit" id="uc-download-link" class="goog-inline-block jfk-button jfk-button-action" value="Download anyway"/><input type="hidden" name="id" value="10xD5abc"><input type="hidden" name="export" value="download"><input type="hidden" name="authuser" value="2"><input type="hidden" name="confirm" value="t"><input type="hidden" name="uuid" value="815d2b63-384e"><input type="hidden" name="at" value="AMrW+x/y:179&amp;z"></form></body></html>"#;
        assert_eq!(
            drive_confirm_url(page).as_deref(),
            Some(
                "https://drive.usercontent.google.com/download?id=10xD5abc&export=download&authuser=2&confirm=t&uuid=815d2b63-384e&at=AMrW%2Bx%2Fy%3A179%26z"
            )
        );
        assert_eq!(drive_confirm_url(&page.replace("drive.usercontent.google.com", "evil.example")), None);
        assert_eq!(drive_confirm_url("<html>berkas biasa</html>"), None);
        assert!(is_html(page) && is_html("  <!DOCTYPE html><html>") && !is_html("PK\u{3}\u{4}zip"));
        let quota = "<!DOCTYPE html><html><head><title>Google Drive - Quota exceeded</title></head><body>Too many users</body></html>";
        assert!(
            drive_page_error(quota)
                .starts_with("Google Drive refused the download: Quota exceeded. The file was downloaded too often")
        );
        assert_eq!(drive_confirm_url(quota), None);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse("1:GET\r\n2:javascript:alert(1)\r\n").is_err());
        assert!(parse("1:GET\r\n2:file:///etc/passwd\r\n").is_err());
        assert!(parse("1:DELETE\r\n2:https://a.b/c\r\n").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn names_are_safe() {
        assert_eq!(sanitize("../../etc/passwd"), "_.._etc_passwd");
        assert_eq!(sanitize(".bashrc"), "bashrc");
        assert_eq!(sanitize("  "), "download");
        assert_eq!(sanitize("a\nb"), "a_b");
        assert_eq!(sanitize("judul: bagian 1? <final>.mp4"), "judul_ bagian 1_ _final_.mp4");
        assert_eq!(sanitize("con.txt"), "_con.txt");
        assert_eq!(sanitize("COM3"), "_COM3");
        assert_eq!(sanitize("console.log"), "console.log");
        let long = format!("{}.zip", "x".repeat(400));
        let s = sanitize(&long);
        assert!(s.len() <= 200 && s.ends_with(".zip"));
        assert_eq!(url_basename("https://h/p/"), "");
        assert_eq!(url_basename("https://h/%2e%2e%2fx"), "_x");
        assert_eq!(title_stem("Lagu - YouTube"), "Lagu");
        assert_eq!(url_host("https://u:p@cdn.x.com:8443/a"), "cdn.x.com");
    }
}
