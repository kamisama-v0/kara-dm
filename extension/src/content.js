// Kara DM — tombol kaca "Unduh video" di atas pemutar. Shadow DOM tertutup: gaya situs tidak masuk, gaya kita
// tidak bocor. Muncul saat kursor di atas <video> yang cukup besar, hilang 1,2 dtk setelah kursor pergi.
(() => {
  if (window.__karaDm) return;
  window.__karaDm = true;
  const api = globalThis.browser ?? globalThis.chrome;
  const K = globalThis.KDM;

  let enabled = true;
  let quality = K.DEFAULTS.videoQuality;
  api.storage.local.get({ videoButton: true, videoQuality: quality }).then((s) => {
    enabled = s.videoButton;
    quality = s.videoQuality;
  });
  api.storage.onChanged.addListener((c, area) => {
    if (area !== "local") return;
    if (c.videoQuality) quality = c.videoQuality.newValue;
    if (c.videoButton) {
      enabled = c.videoButton.newValue;
      if (!enabled) hide(true);
    }
  });

  // Tautan halaman untuk "Unduh semua tautan" (diminta background, hanya bingkai utama).
  api.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
    if (msg?.type !== "kara-links") return;
    const out = [];
    for (const a of document.querySelectorAll("a[href], area[href]")) out.push({ url: a.href, text: a.textContent || a.title || "" });
    for (const m of document.querySelectorAll("img[src], video[src], audio[src], source[src]")) {
      out.push({ url: m.currentSrc || m.src, text: m.alt || m.title || "" });
    }
    sendResponse(out.slice(0, 5000));
  });

  const CSS = `
    :host { all: initial; position: fixed; left: 0; top: 0; z-index: 2147483647; pointer-events: none; }
    * { box-sizing: border-box; }
    .wrap { position: fixed; pointer-events: auto; font: 500 12.5px/1 system-ui, -apple-system, "SF Pro Text", "Inter", sans-serif;
      color: #fff; opacity: 0; transform: translateY(-4px) scale(0.98); transition: opacity .18s ease, transform .22s cubic-bezier(.2,.8,.2,1); }
    .wrap.show { opacity: 1; transform: none; }
    .glass { -webkit-backdrop-filter: blur(18px) saturate(180%); backdrop-filter: blur(18px) saturate(180%); }
    .pill { display: inline-flex; align-items: center; gap: 6px; height: 30px; padding: 0 11px 0 9px; border-radius: 999px; cursor: pointer;
      background: rgba(255,255,255,.14); border: 1px solid rgba(255,255,255,.40); text-shadow: 0 1px 1px rgba(0,0,0,.25);
      box-shadow: inset 0 1px 0 rgba(255,255,255,.50), inset 0 -1px 0 rgba(255,255,255,.08), 0 6px 18px rgba(0,0,0,.28);
      transition: background .15s; user-select: none; }
    .pill:hover { background: rgba(255,255,255,.24); }
    .pill svg { filter: drop-shadow(0 1px 1px rgba(0,0,0,.25)); }
    .menu { position: absolute; top: 38px; left: 0; min-width: 250px; max-width: 340px; padding: 6px; border-radius: 18px;
      background: rgba(22,24,23,.45); border: 1px solid rgba(255,255,255,.22);
      box-shadow: inset 0 1px 0 rgba(255,255,255,.26), 0 14px 34px rgba(0,0,0,.38); display: none; }
    .menu.open { display: block; }
    .item { display: flex; align-items: center; gap: 10px; padding: 8px 10px; border-radius: 12px; cursor: pointer; }
    .item:hover, .item:focus-visible { background: rgba(255,255,255,.13); outline: none; }
    .grow { flex: 1; min-width: 0; }
    .name, .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .sub { margin-top: 3px; font-weight: 400; font-size: 11.5px; color: rgba(255,255,255,.68); }
    .state { font-size: 11.5px; color: rgba(255,255,255,.8); }
    .ok { color: #7ee2a4; } .err { color: #ff9a90; }
    .note { padding: 9px 10px; font-weight: 400; color: rgba(255,255,255,.72); line-height: 1.4; }
    .quality { display: flex; align-items: center; gap: 8px; padding: 6px 10px 8px; font-weight: 400; color: rgba(255,255,255,.72); }
    .quality select { flex: 1; font: inherit; color: #fff; background: rgba(255,255,255,.12); border: 1px solid rgba(255,255,255,.22);
      border-radius: 10px; padding: 5px 8px; color-scheme: dark; }
  `;

  let host, wrap, menu, video, hideTimer, raf;

  function build() {
    host = document.createElement("kara-dm-overlay");
    const root = host.attachShadow({ mode: "closed" });
    const style = document.createElement("style");
    style.textContent = CSS;
    wrap = document.createElement("div");
    wrap.className = "wrap";
    const pill = document.createElement("div");
    pill.className = "pill glass";
    pill.setAttribute("role", "button");
    pill.tabIndex = 0;
    pill.append(K.svgIcon(document, "download", 15), "Download video", K.svgIcon(document, "chevron", 12, 1.8));
    menu = document.createElement("div");
    menu.className = "menu glass";
    wrap.append(pill, menu);
    root.append(style, wrap);
    pill.addEventListener("click", toggleMenu);
    pill.addEventListener("keydown", (e) => (e.key === "Enter" || e.key === " ") && toggleMenu(e));
    wrap.addEventListener("mouseenter", () => clearTimeout(hideTimer));
    wrap.addEventListener("mouseleave", scheduleHide);
  }

  function mount() {
    if (!host) build();
    const parent = document.fullscreenElement || document.documentElement;
    if (host.parentNode !== parent) parent.appendChild(host);
  }

  function place() {
    if (!video || !wrap) return;
    const r = video.getBoundingClientRect();
    if (!r.width || r.bottom < 0 || r.top > innerHeight) return hide(true);
    wrap.style.left = Math.max(8, Math.min(r.left + 12, innerWidth - 160)) + "px";
    wrap.style.top = Math.max(8, r.top + 12) + "px";
  }

  function show(v) {
    if (!enabled) return;
    clearTimeout(hideTimer);
    if (video !== v) {
      video = v;
      menu && menu.classList.remove("open");
    }
    mount();
    place();
    wrap.classList.add("show");
  }

  function hide(now) {
    clearTimeout(hideTimer);
    const go = () => {
      if (!wrap) return;
      wrap.classList.remove("show");
      menu.classList.remove("open");
      video = null;
    };
    if (now) go();
    else hideTimer = setTimeout(go, 1200);
  }

  function scheduleHide() {
    if (!menu || !menu.classList.contains("open")) hide(false);
  }

  function videoAt(x, y) {
    for (const v of document.getElementsByTagName("video")) {
      const r = v.getBoundingClientRect();
      if (r.width >= 240 && r.height >= 135 && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) return v;
    }
    return null;
  }

  document.addEventListener(
    "pointermove",
    (e) => {
      if (raf || !enabled) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        const v = videoAt(e.clientX, e.clientY);
        if (v) show(v);
        else if (video && !(menu && menu.classList.contains("open"))) scheduleHide();
      });
    },
    { passive: true, capture: true },
  );
  addEventListener("scroll", () => video && place(), { passive: true, capture: true });
  addEventListener("resize", () => video && place(), { passive: true });
  document.addEventListener("fullscreenchange", () => host && host.isConnected && (mount(), place()));
  document.addEventListener("pointerdown", (e) => host && !e.composedPath().includes(host) && menu.classList.remove("open"), true);

  function row(name, sub, onClick) {
    const el = document.createElement("div");
    el.className = "item";
    el.tabIndex = 0;
    const g = document.createElement("div");
    g.className = "grow";
    const n = document.createElement("div");
    n.className = "name";
    n.textContent = name;
    const s = document.createElement("div");
    s.className = "sub";
    s.textContent = sub;
    g.append(n, s);
    const st = document.createElement("span");
    st.className = "state";
    st.append(K.svgIcon(document, "download", 15));
    el.append(g, st);
    if (onClick) {
      el.addEventListener("click", () => onClick(st));
      el.addEventListener("keydown", (e) => (e.key === "Enter" || e.key === " ") && onClick(st));
    }
    return el;
  }

  async function toggleMenu(e) {
    e.stopPropagation();
    if (menu.classList.toggle("open") === false) return;
    menu.replaceChildren(row("Looking for media…", "", null));
    const extra = [];
    const src = video && (video.currentSrc || video.src);
    if (src && /^https?:/i.test(src)) extra.push({ kind: "media", label: "Video pemutar", url: src, size: 0 });
    let items = [];
    try {
      const r = await api.runtime.sendMessage({ type: "media", hasVideo: true, extra });
      if (r && r.error) throw new Error(r.error);
      items = r.items;
    } catch (err) {
      menu.replaceChildren(note("The extension is not responding: " + err.message));
      return;
    }
    // Baris yt-dlp (halaman / HLS / DASH) menampilkan kualitas pilihan; berkas langsung tidak punya pilihan.
    const streams = [];
    const streamSub = (it) => (it.page ? "Video + audio · " : it.label + " · ") + K.qualityLabel(quality);
    const rows = items.map((it) => {
      const name = it.page ? it.label : decodeName(it.url);
      const sub = it.kind === "hls" ? streamSub(it) : [it.label, K.formatSize(it.size)].filter(Boolean).join(" · ");
      const el = row(name, sub, async (st) => {
        st.textContent = "…";
        try {
          const r = await api.runtime.sendMessage({ type: "download", item: it });
          if (r && r.error) throw new Error(r.error);
          st.className = "state ok";
          st.textContent = "Sent";
          setTimeout(() => hide(true), 900);
        } catch (err) {
          st.className = "state err";
          st.textContent = /not (running|responding)/.test(err.message) ? "Kara DM is off" : "Failed";
          st.title = err.message;
        }
      });
      if (it.kind === "hls") streams.push([it, el.querySelector(".sub")]);
      return el;
    });
    menu.replaceChildren(...(streams.length ? [qualityPicker(() => streams.forEach(([it, s]) => (s.textContent = streamSub(it))))] : []), ...rows);
    if (!items.length) menu.append(note("No media found yet. Play the video for a moment, then open this again."));
  }

  function qualityPicker(onChange) {
    const wrap = document.createElement("label");
    wrap.className = "quality";
    const sel = document.createElement("select");
    for (const [v, label] of K.QUALITIES) sel.append(new Option(label, v, false, v === quality));
    sel.addEventListener("change", () => {
      quality = sel.value;
      api.storage.local.set({ videoQuality: quality });
      onChange();
    });
    wrap.append("Quality", sel);
    return wrap;
  }

  function note(text) {
    const n = document.createElement("div");
    n.className = "note";
    n.textContent = text;
    return n;
  }

  function decodeName(url) {
    const last = url.split("?")[0].split("/").pop() || url;
    try {
      return decodeURIComponent(last);
    } catch {
      return last;
    }
  }
})();
