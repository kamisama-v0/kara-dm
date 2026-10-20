const api = globalThis.browser ?? globalThis.chrome;
const K = globalThis.KDM;
const $ = (id) => document.getElementById(id);

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

const empty = (text) => el("div", "empty", text);

function ask(msg) {
  return api.runtime.sendMessage(msg).then((r) => {
    if (r && r.error) throw new Error(r.error);
    return r;
  });
}

async function main() {
  const [tab] = await api.tabs.query({ active: true, currentWindow: true });
  const settings = { ...K.DEFAULTS, ...(await api.storage.local.get(K.DEFAULTS)) };
  const host = tab && /^https?:/i.test(tab.url || "") ? K.hostOf(tab.url) : "";

  $("capture").checked = settings.capture;
  $("capture").onchange = (e) => api.storage.local.set({ capture: e.target.checked });
  if (host) {
    $("site").textContent = host.replace(/^www\./, "");
    $("siteOff").checked = K.siteMatches(host, settings.excludedSites);
    $("siteOff").onchange = async (e) => {
      const { excludedSites } = await api.storage.local.get({ excludedSites: [] });
      const bare = host.replace(/^www\./, "");
      const list = excludedSites.filter((s) => !K.siteMatches(host, [s]));
      if (e.target.checked) list.push(bare);
      api.storage.local.set({ excludedSites: list });
    };
  } else {
    $("siteRow").hidden = true;
  }
  const streamSub = (it) => (it.page ? "Video + audio · " : it.label + " · ") + K.qualityLabel(settings.videoQuality);
  for (const [v, label] of K.QUALITIES) $("quality").append(new Option(label, v, false, v === settings.videoQuality));
  $("quality").onchange = (e) => {
    settings.videoQuality = e.target.value;
    api.storage.local.set({ videoQuality: e.target.value });
    for (const s of document.querySelectorAll("[data-stream]")) s.textContent = streamSub(JSON.parse(s.dataset.stream));
  };
  if (host) {
    $("links").onclick = () => ask({ type: "openLinks", tabId: tab.id }).then(() => window.close());
  } else {
    $("links").hidden = true;
  }
  $("options").onclick = () => api.runtime.openOptionsPage();
  $("help").onclick = () => api.tabs.create({ url: api.runtime.getURL("options.html#about") });

  ask({ type: "status" }).then(
    ({ connected }) => {
      $("dot").className = "dot " + (connected ? "on" : "off");
      $("status").textContent = connected ? "Connected" : "App not running";
    },
    () => ($("status").textContent = "Unknown"),
  );

  const box = $("media");
  if (!host) {
    box.replaceChildren(empty("No media on this page."));
    return;
  }
  const { items } = await ask({ type: "media", tabId: tab.id });
  if (!items.length) {
    box.replaceChildren(empty("No media found yet. Play the video for a moment, or right-click the page → \"Download this page's video\"."));
    return;
  }
  box.textContent = "";
  for (const it of items) {
    const row = document.createElement("div");
    row.className = "row";
    row.tabIndex = 0;
    const name = it.page ? K.pageTitle(it) : it.url.split("?")[0].split("/").pop() || it.label;
    const sub = it.kind === "hls" ? streamSub(it) : [it.label, K.formatSize(it.size)].filter(Boolean).join(" · ");
    const grow = el("span", "grow");
    const title = el("div", "ellipsis");
    const subEl = el("div", "sub ellipsis", sub);
    if (it.kind === "hls") subEl.dataset.stream = JSON.stringify({ page: !!it.page, label: it.label });
    grow.append(title, subEl);
    const state = el("span", "state");
    state.append(K.svgIcon(document, "download"));
    row.append(grow, state);
    let shown = name;
    try {
      shown = decodeURIComponent(name);
    } catch {}
    title.textContent = shown;
    row.title = it.url;
    const go = async () => {
      try {
        await ask({ type: "download", tabId: tab.id, item: it });
        state.className = "state done";
        state.textContent = "Sent";
      } catch (e) {
        state.className = "state fail";
        state.textContent = "Failed";
        row.title = e.message;
      }
    };
    row.onclick = go;
    row.onkeydown = (e) => (e.key === "Enter" || e.key === " ") && go();
    box.appendChild(row);
  }
}

main().catch((e) => {
  $("media").replaceChildren(empty(e.message));
});
