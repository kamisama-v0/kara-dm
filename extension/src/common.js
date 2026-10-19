// Logika murni Kara DM (tanpa API browser): dipakai background.js dan diuji dengan `node --test`.
// Protokol ke aplikasi = protokol ekstensi NDM (lihat ../../src/protocol.rs): satu pesan teks per unduhan,
// baris "kunci:nilai\r\n" lewat ws://127.0.0.1:10007/download dengan subprotocol neatextension.v1.

(function (root) {
  const WS_URL = "ws://127.0.0.1:10007/download";
  const WS_PROTOCOL = "neatextension.v1";
  const POST_MARK = "__0NeatPostData9__:";
  const MAX_MESSAGE = 118784;

  const DEFAULTS = {
    capture: true,
    excludedSites: [],
    ignoredExtensions: [],
    minSizeMB: 0,
    videoButton: true,
    notify: true,
    videoQuality: "best",
  };

  // Kualitas video untuk yt-dlp (halaman video, HLS, DASH): tinggi maksimum atau audio saja.
  const QUALITIES = [
    ["best", "Terbaik"],
    ["2160", "4K (2160p)"],
    ["1440", "1440p"],
    ["1080", "1080p"],
    ["720", "720p"],
    ["480", "480p"],
    ["360", "360p"],
    ["audio", "Audio saja"],
  ];

  function qualityLabel(q) {
    return (QUALITIES.find(([v]) => v === q) || QUALITIES[0])[1];
  }

  // Kategori tautan untuk unduh massal, menurut ekstensi berkas di URL.
  const CATEGORIES = [
    ["video", "Video", "mp4 mkv webm mov avi m4v flv wmv 3gp mpg mpeg"],
    ["audio", "Audio", "mp3 m4a flac wav ogg opus aac wma"],
    ["image", "Gambar", "jpg jpeg png gif webp avif bmp tif tiff heic svg"],
    ["document", "Dokumen", "pdf doc docx xls xlsx ppt pptx odt ods odp txt csv rtf epub"],
    ["archive", "Arsip", "zip rar 7z tar gz tgz bz2 xz zst iso dmg img"],
    ["program", "Program", "exe msi apk deb rpm appimage flatpak pkg jar"],
  ].map(([id, label, exts]) => ({ id, label, exts: new Set(exts.split(" ")) }));

  function linkCategory(url) {
    const ext = extOf(url);
    return (CATEGORIES.find((c) => c.exts.has(ext)) || { id: "other" }).id;
  }

  // Rapikan tautan mentah dari halaman: hanya http/https/ftp, tanpa #fragmen, tanpa duplikat dan tanpa tautan ke
  // halaman itu sendiri. raw: [{url, text}] -> [{url, text, name, ext, cat}]
  function prepareLinks(raw, pageUrl) {
    const self = String(pageUrl || "").split("#")[0];
    const seen = new Set();
    const out = [];
    for (const r of raw || []) {
      const url = String(r.url || "").split("#")[0];
      if (!isCapturableUrl(url) || url === self || seen.has(url)) continue;
      seen.add(url);
      let name = url.split("?")[0].split("/").pop() || hostOf(url);
      try {
        name = decodeURIComponent(name);
      } catch {}
      const text = String(r.text || "").replace(/\s+/g, " ").trim().slice(0, 120);
      out.push({ url, text, name, ext: extOf(url), cat: linkCategory(url) });
    }
    return out;
  }

  // Situs yang videonya lebih baik diunduh yt-dlp lewat URL halaman (stream terpisah / ditandatangani).
  const PAGE_SITES = /(^|\.)(youtube\.com|youtu\.be|vimeo\.com|x\.com|twitter\.com|instagram\.com|facebook\.com|tiktok\.com|twitch\.tv|dailymotion\.com|reddit\.com|bilibili\.com)$/i;

  function clean(v) {
    return String(v ?? "").replace(/[\r\n]+/g, " ").trim();
  }

  function hostOf(url) {
    try {
      return new URL(url).hostname.toLowerCase();
    } catch {
      return "";
    }
  }

  function extOf(url) {
    try {
      const last = new URL(url).pathname.split("/").pop() || "";
      const i = last.lastIndexOf(".");
      return i > 0 ? last.slice(i + 1).toLowerCase() : "";
    } catch {
      return "";
    }
  }

  function isCapturableUrl(url) {
    return /^(https?|ftp):\/\//i.test(url || "");
  }

  // "example.com" juga mencocokkan sub-domain (www.example.com, cdn.example.com).
  function siteMatches(host, sites) {
    host = (host || "").toLowerCase();
    return (sites || []).some((s) => {
      s = String(s).trim().toLowerCase().replace(/^\*\./, "").replace(/^https?:\/\//, "").replace(/\/.*$/, "");
      return s && (host === s || host.endsWith("." + s));
    });
  }

  function formatSize(n) {
    if (!(n > 0)) return "";
    const u = ["B", "KB", "MB", "GB", "TB"];
    let i = 0;
    while (n >= 1024 && i < u.length - 1) {
      n /= 1024;
      i++;
    }
    return (i ? n.toFixed(n < 10 ? 1 : 0) : n) + " " + u[i];
  }

  // Putuskan apakah unduhan browser diambil alih. Kembalikan alasan menolak, atau null kalau diambil.
  function captureDecision(item, settings) {
    const s = { ...DEFAULTS, ...settings };
    const url = item.finalUrl || item.url;
    if (!s.capture) return "tangkapan dimatikan";
    if (item.incognito) return "jendela privat";
    if (!isCapturableUrl(url)) return "bukan http/https/ftp";
    if (siteMatches(hostOf(url), s.excludedSites) || siteMatches(hostOf(item.referrer), s.excludedSites)) return "situs dikecualikan";
    const name = (item.filename || "").split(/[\\/]/).pop();
    const ext = (name.includes(".") ? name.split(".").pop() : extOf(url)).toLowerCase();
    if (ext && s.ignoredExtensions.map((e) => String(e).trim().replace(/^\./, "").toLowerCase()).includes(ext)) return "jenis berkas diabaikan";
    if (s.minSizeMB > 0 && item.totalBytes > 0 && item.totalBytes < s.minSizeMB * 1048576) return "di bawah ukuran minimum";
    return null;
  }

  function header(headers, name) {
    const h = (headers || []).find((x) => x.name.toLowerCase() === name);
    return h ? h.value || "" : "";
  }

  // Kenali respons media dari onHeadersReceived. Kembalikan {kind, label, url, size} atau null.
  function classifyMedia(details) {
    const url = details.url;
    if (!/^https?:\/\//i.test(url) || details.tabId < 0) return null;
    const ct = header(details.responseHeaders, "content-type").split(";")[0].trim().toLowerCase();
    const ext = extOf(url);
    const host = hostOf(url);
    if (/mpegurl/.test(ct) || ext === "m3u8") return { kind: "hls", label: "Stream HLS", ext: "m3u8", url, size: 0 };
    if (ct === "application/dash+xml" || ext === "mpd") return { kind: "hls", label: "Stream DASH", ext: "mpd", url, size: 0 };
    const media = /^(video|audio)\//.test(ct) || ["mp4", "webm", "mkv", "mov", "m4a", "mp3", "ogg", "opus", "flac"].includes(ext);
    if (!media) return null;
    // Potongan stream (segmen HLS/DASH, range YouTube) bukan berkas utuh.
    if (ct === "video/mp2t" || ["ts", "m4s", "m4f", "aac"].includes(ext) || /googlevideo\.com$/.test(host)) return null;
    const range = header(details.responseHeaders, "content-range");
    if (range && !/^bytes 0-/i.test(range)) return null;
    const total = range ? parseInt(range.split("/")[1], 10) : parseInt(header(details.responseHeaders, "content-length"), 10);
    const size = Number.isFinite(total) ? total : 0;
    if (size > 0 && size < 512 * 1024) return null;
    const kindExt = ext || (ct.split("/")[1] || "").replace(/^x-/, "") || "mp4";
    return { kind: "media", label: kindExt.toUpperCase() + (ct.startsWith("audio/") ? " audio" : " video"), ext: kindExt, url, size };
  }

  function pageEntry(pageUrl, title) {
    if (!/^https?:\/\//i.test(pageUrl || "")) return null;
    return { kind: "hls", page: true, label: "Video halaman ini (yt-dlp)", ext: "", url: pageUrl, size: 0, title };
  }

  function isVideoSite(pageUrl) {
    return PAGE_SITES.test(hostOf(pageUrl));
  }

  // Bentuk pesan protokol NDM. req: {method,url,url2,kind,title,page,referrer,cookies,size,mime,label,headers,body,
  // contentType,quality,batch}. Kunci 10 (kualitas) dan 11 (unduhan massal) khusus Kara DM; NDM mengabaikannya.
  function buildMessage(req) {
    const lines = [];
    const add = (k, v) => {
      v = clean(v);
      if (v) lines.push(k + v);
    };
    add("1:", (req.method || "GET").toUpperCase());
    add("2:", req.url);
    add("3:", req.url2);
    add("6:", req.kind || "normal");
    add("4:", req.title);
    const page = req.page || req.referrer || "";
    if (page) {
      try {
        add("Origin: ", new URL(page).origin);
      } catch {}
      add("Referer: ", page.split("#")[0]);
    }
    add("5:", page);
    add("Cookie: ", req.cookies);
    add("7:", req.size > 0 ? String(req.size) : "");
    add("8:", req.mime);
    add("9:", req.label);
    add("10:", QUALITIES.some(([v]) => v === req.quality) && req.quality !== "best" ? req.quality : "");
    add("11:", req.batch ? "1" : "");
    for (const [k, v] of Object.entries(req.headers || {})) {
      if (/^x-[a-z0-9-]+$/i.test(k)) add(k + ": ", v);
    }
    let msg = lines.join("\r\n") + "\r\n";
    if ((req.method || "").toUpperCase() === "POST") {
      if (req.contentType) msg += "Content-Type: " + clean(req.contentType) + "\r\n";
      msg += req.body != null ? POST_MARK + req.body : "Content-Length: 0\r\n";
    }
    if (msg.length > MAX_MESSAGE) throw new Error("pesan terlalu besar (" + msg.length + " byte)");
    return msg;
  }

  function cookieHeader(cookies) {
    const seen = new Set();
    return (cookies || [])
      .filter((c) => !seen.has(c.name) && seen.add(c.name))
      .map((c) => c.name + "=" + c.value)
      .join("; ");
  }

  // requestBody dari webRequest.onBeforeRequest -> string body (form urlencoded / raw teks).
  function encodeRequestBody(rb) {
    if (!rb) return null;
    if (rb.formData) {
      const p = new URLSearchParams();
      for (const [k, vals] of Object.entries(rb.formData)) for (const v of vals) p.append(k, v);
      return { body: p.toString(), contentType: "application/x-www-form-urlencoded" };
    }
    if (rb.raw && rb.raw.length && rb.raw.every((r) => r.bytes)) {
      const parts = rb.raw.map((r) => new Uint8Array(r.bytes));
      let s = "";
      for (const u of parts) for (let i = 0; i < u.length; i++) s += String.fromCharCode(u[i]);
      return { body: s, contentType: "" };
    }
    return null;
  }

  const ICONS = { download: "M8 2.5v8M4.5 7 8 10.5 11.5 7M3 13.5h10", chevron: "m4 6 4 4 4-4" };

  // Ikon garis 16x16 dibuat lewat DOM (tanpa innerHTML).
  function svgIcon(doc, name, size = 16, stroke = 1.7) {
    const ns = "http://www.w3.org/2000/svg";
    const svg = doc.createElementNS(ns, "svg");
    for (const [k, v] of Object.entries({
      width: size, height: size, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor",
      "stroke-width": stroke, "stroke-linecap": "round", "stroke-linejoin": "round", "aria-hidden": "true",
    })) svg.setAttribute(k, v);
    const p = doc.createElementNS(ns, "path");
    p.setAttribute("d", ICONS[name]);
    svg.append(p);
    return svg;
  }

  const KDM = {
    svgIcon,
    WS_URL,
    WS_PROTOCOL,
    DEFAULTS,
    QUALITIES,
    qualityLabel,
    CATEGORIES,
    linkCategory,
    prepareLinks,
    hostOf,
    extOf,
    siteMatches,
    formatSize,
    captureDecision,
    classifyMedia,
    pageEntry,
    isVideoSite,
    buildMessage,
    cookieHeader,
    encodeRequestBody,
  };
  if (typeof module !== "undefined" && module.exports) module.exports = KDM;
  else root.KDM = KDM;
})(typeof globalThis !== "undefined" ? globalThis : this);
