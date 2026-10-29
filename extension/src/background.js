// Kara DM — background (event page di Firefox, service worker di Chromium).
// Menangkap unduhan browser (downloads API), mengumpulkan media per tab (webRequest, non-blocking), lalu
// mengirimnya ke Kara Download Manager lewat WebSocket protokol NDM.

if (typeof importScripts === "function" && !globalThis.KDM) importScripts("common.js");
const api = globalThis.browser ?? globalThis.chrome;
const K = globalThis.KDM;

let settings = { ...K.DEFAULTS };
const ready = api.storage.local.get(K.DEFAULTS).then((s) => (settings = { ...K.DEFAULTS, ...s }));
api.storage.onChanged.addListener((changes, area) => {
  if (area !== "local") return;
  for (const [k, { newValue }] of Object.entries(changes)) settings[k] = newValue;
});

// ---------- koneksi ke aplikasi ----------

let socket = null;

function connect(timeoutMs = 1500) {
  if (socket && socket.readyState === WebSocket.OPEN) return Promise.resolve(socket);
  return new Promise((resolve, reject) => {
    let ws;
    try {
      ws = new WebSocket(K.WS_URL, K.WS_PROTOCOL);
    } catch (e) {
      reject(e);
      return;
    }
    const timer = setTimeout(() => {
      ws.close();
      reject(new Error("Kara DM is not responding"));
    }, timeoutMs);
    ws.onopen = () => {
      clearTimeout(timer);
      socket = ws;
      resolve(ws);
    };
    ws.onerror = () => {
      clearTimeout(timer);
      reject(new Error("Kara DM is not running"));
    };
    ws.onclose = () => {
      if (socket === ws) socket = null;
    };
  });
}

async function send(req) {
  const msg = K.buildMessage(req);
  (await connect()).send(msg);
}

// Cookie first-party URL itu, seperti yang dikirim browser untuk unduhan biasa. Bukan semua partisi: Firefox juga
// menyimpan salinan cookie situs ini yang dipartisi di bawah situs lain (embed, login pihak ketiga), dan
// mencampurnya bisa mengirim sesi yang salah (mis. Google Drive lalu menolak token unduhannya).
async function cookiesFor(url) {
  try {
    return K.cookieHeader(await api.cookies.getAll({ url }));
  } catch {
    return "";
  }
}

function notify(title, message) {
  if (!settings.notify) return;
  api.notifications.create({ type: "basic", iconUrl: api.runtime.getURL("icons/128.png"), title, message });
}

async function activeTab() {
  const [tab] = await api.tabs.query({ active: true, lastFocusedWindow: true });
  return tab;
}

// ---------- body POST (untuk unduhan hasil kirim formulir) ----------

const posts = new Map();

function rememberPost(url, data) {
  const now = Date.now();
  for (const [k, v] of posts) if (now - v.t > 60000) posts.delete(k);
  posts.set(url, { ...posts.get(url), ...data, t: now });
  if (posts.size > 30) posts.delete(posts.keys().next().value);
}

api.webRequest.onBeforeRequest.addListener(
  (d) => {
    if (d.method !== "POST") return;
    const enc = K.encodeRequestBody(d.requestBody);
    if (enc) rememberPost(d.url, enc);
  },
  { urls: ["<all_urls>"], types: ["main_frame", "sub_frame"] },
  ["requestBody"],
);
// Header Cookie yang benar-benar dikirim browser untuk permintaan yang lalu menjadi unduhan: sesi yang persis
// sama dengan milik browser, termasuk dari iframe. Hanya di memori, dibuang setelah 60 detik.
const sentCookies = new Map();

function rememberCookie(url, cookie) {
  const now = Date.now();
  for (const [k, v] of sentCookies) if (now - v.t > 60000) sentCookies.delete(k);
  sentCookies.set(url, { cookie, t: now });
  if (sentCookies.size > 30) sentCookies.delete(sentCookies.keys().next().value);
}

function onSendHeaders(d) {
  const header = (name) => (d.requestHeaders || []).find((h) => h.name.toLowerCase() === name);
  const cookie = header("cookie");
  if (cookie && cookie.value) rememberCookie(d.url, cookie.value);
  if (d.method !== "POST" || !posts.has(d.url)) return;
  const ct = header("content-type");
  if (ct && !posts.get(d.url).contentType) rememberPost(d.url, { contentType: ct.value });
}

// Chromium hanya memperlihatkan header Cookie dengan "extraHeaders"; Firefox selalu memperlihatkannya dan
// menolak opsi itu.
try {
  api.webRequest.onSendHeaders.addListener(onSendHeaders, { urls: ["<all_urls>"], types: ["main_frame", "sub_frame", "other"] }, [
    "requestHeaders",
    "extraHeaders",
  ]);
} catch {
  api.webRequest.onSendHeaders.addListener(onSendHeaders, { urls: ["<all_urls>"], types: ["main_frame", "sub_frame", "other"] }, [
    "requestHeaders",
  ]);
}

// ---------- tangkap unduhan browser ----------

// URL yang sengaja diunduh ulang oleh browser (fallback) agar tidak ditangkap lagi.
const bypass = new Set();

api.downloads.onCreated.addListener((item) => {
  handleDownload(item).catch((e) => console.warn("Kara DM:", e));
});

async function handleDownload(item) {
  await ready;
  const url = item.finalUrl || item.url;
  if (bypass.has(url) || bypass.has(item.url)) return;
  if (K.captureDecision(item, settings)) return;
  const name = (item.filename || "").split(/[\\/]/).pop() || url;
  try {
    await connect();
  } catch {
    // Aplikasi tidak jalan: biarkan browser yang mengunduh (berkas tidak hilang).
    notify("Kara DM is not running", "Downloaded by the browser: " + name);
    return;
  }
  try {
    await api.downloads.cancel(item.id);
  } catch {}
  const [now] = await api.downloads.search({ id: item.id });
  if (now && now.state === "complete") return; // selesai sebelum sempat diambil alih
  try {
    await api.downloads.erase({ id: item.id });
  } catch {}
  const post = posts.get(url) || posts.get(item.url);
  const tab = await activeTab().catch(() => null);
  const req = {
    method: post ? "POST" : "GET",
    url,
    page: item.referrer || "",
    title: tab && item.referrer && tab.url === item.referrer ? tab.title : "",
    cookies: (sentCookies.get(url) || sentCookies.get(item.url))?.cookie ?? (await cookiesFor(url)),
    size: item.totalBytes > 0 ? item.totalBytes : 0,
    mime: item.mime || "",
    body: post?.body,
    contentType: post?.contentType,
  };
  try {
    await send(req);
  } catch (e) {
    notify("Could not send to Kara DM", e.message + ". Downloaded again by the browser.");
    if (req.method === "GET") {
      bypass.add(url);
      setTimeout(() => bypass.delete(url), 10000);
      await api.downloads.download({ url });
    }
  }
}

// ---------- menu klik kanan ----------

api.runtime.onInstalled.addListener(async () => {
  await api.contextMenus.removeAll();
  api.contextMenus.create({ id: "kdm-link", title: "Download with Kara DM", contexts: ["link"] });
  api.contextMenus.create({ id: "kdm-media", title: "Download this media with Kara DM", contexts: ["video", "audio", "image"] });
  api.contextMenus.create({ id: "kdm-page", title: "Download this page's video (yt-dlp)", contexts: ["page", "video"] });
  api.contextMenus.create({ id: "kdm-links", title: "Download all links with Kara DM…", contexts: ["page"] });
});

api.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "kdm-links") return void openLinksPage(tab.id);
  sendFromMenu(info, tab).catch((e) => notify("Could not send to Kara DM", e.message));
});

function openLinksPage(tabId) {
  api.tabs.create({ url: api.runtime.getURL("links.html?tab=" + tabId) });
}

async function sendFromMenu(info, tab) {
  let url = info.menuItemId === "kdm-link" ? info.linkUrl : info.menuItemId === "kdm-media" ? info.srcUrl : tab.url;
  let kind = info.menuItemId === "kdm-page" ? "hls" : info.menuItemId === "kdm-media" && info.mediaType !== "image" ? "media" : "normal";
  if (!/^(https?|ftp):/i.test(url || "")) {
    // Media blob:/MediaSource tidak bisa diunduh langsung; serahkan halaman ke yt-dlp.
    url = tab.url;
    kind = "hls";
  }
  const quality = kind === "hls" ? settings.videoQuality : "";
  await send({ url, kind, quality, title: tab.title, page: tab.url, cookies: await cookiesFor(url) });
}

// ---------- media per tab ----------

const media = new Map();

async function getMedia(tabId) {
  if (!media.has(tabId)) {
    const key = "media:" + tabId;
    const saved = await api.storage.session.get(key).catch(() => ({}));
    media.set(tabId, saved[key] || []);
  }
  return media.get(tabId);
}

function setBadge(tabId, n) {
  api.action.setBadgeText({ tabId, text: n ? String(n) : "" }).catch(() => {});
}

async function addMedia(tabId, m) {
  const list = await getMedia(tabId);
  const key = m.kind === "hls" ? m.url : m.url.split("?")[0];
  if (list.some((x) => (x.kind === "hls" ? x.url : x.url.split("?")[0]) === key)) return;
  list.push(m);
  if (list.length > 40) list.shift();
  api.storage.session.set({ ["media:" + tabId]: list }).catch(() => {});
  setBadge(tabId, list.length);
}

function clearMedia(tabId) {
  media.set(tabId, []);
  api.storage.session.remove("media:" + tabId).catch(() => {});
  setBadge(tabId, 0);
}

api.webRequest.onHeadersReceived.addListener(
  (d) => {
    const m = K.classifyMedia(d);
    if (m) addMedia(d.tabId, m);
  },
  { urls: ["<all_urls>"], types: ["media", "xmlhttprequest", "object", "other"] },
  ["responseHeaders"],
);
api.tabs.onUpdated.addListener((tabId, change) => {
  if (change.url) clearMedia(tabId);
});
api.tabs.onRemoved.addListener((tabId) => {
  media.delete(tabId);
  api.storage.session.remove("media:" + tabId).catch(() => {});
});

// ---------- pesan dari popup & tombol video ----------

api.runtime.onMessage.addListener((msg, sender, sendResponse) => {
  handleMessage(msg, sender).then(sendResponse, (e) => sendResponse({ error: e.message }));
  return true;
});

async function handleMessage(msg, sender) {
  await ready;
  const tab = msg.tabId != null ? await api.tabs.get(msg.tabId) : sender.tab;
  switch (msg.type) {
    case "status":
      try {
        await connect(800);
        return { connected: true };
      } catch {
        return { connected: false };
      }
    case "media": {
      const items = [...(await getMedia(tab.id))];
      for (const extra of msg.extra || []) {
        if (/^https?:/i.test(extra.url) && !items.some((x) => x.url === extra.url)) items.push(extra);
      }
      // Video di iframe (mis. YouTube tertanam): yt-dlp butuh URL frame-nya, bukan URL tab.
      const pageUrl = msg.tabId == null && sender.url ? sender.url : tab.url;
      const page = (msg.hasVideo || K.isVideoSite(pageUrl)) && K.pageEntry(pageUrl, tab.title);
      return { items: page ? [page, ...items] : items };
    }
    case "download": {
      const it = msg.item;
      const quality = it.kind === "hls" ? settings.videoQuality : "";
      await send({ url: it.url, kind: it.kind, quality, title: tab?.title, page: tab?.url, cookies: await cookiesFor(it.url), size: it.size, label: it.label });
      return { ok: true };
    }
    case "openLinks":
      openLinksPage(tab.id);
      return { ok: true };
    case "links": {
      // Hanya bingkai utama: tautan di iframe iklan/pelacak jarang yang dicari.
      let raw;
      try {
        raw = await api.tabs.sendMessage(tab.id, { type: "kara-links" }, { frameId: 0 });
      } catch {
        throw new Error("Can't read this page yet. Reload it and try again.");
      }
      return { title: tab.title, url: tab.url, links: K.prepareLinks(raw, tab.url) };
    }
    case "batch": {
      const urls = (msg.urls || []).filter((u) => /^(https?|ftp):\/\//i.test(u));
      await connect(); // gagal cepat kalau aplikasi mati, sebelum mengambil cookie
      let sent = 0;
      for (const url of urls) {
        try {
          await send({ url, kind: "normal", batch: true, page: tab.url, cookies: await cookiesFor(url) });
          sent++;
        } catch {}
      }
      return { sent, failed: urls.length - sent };
    }
    default:
      throw new Error("unknown message: " + msg.type);
  }
}
