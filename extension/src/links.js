// Kara DM — unduh massal: daftar tautan halaman (dibaca content script lewat background), saring per jenis berkas,
// centang, lalu kirim semuanya ke aplikasi sebagai unduhan biasa (aria2, multi-koneksi).
const api = globalThis.browser ?? globalThis.chrome;
const K = globalThis.KDM;
const $ = (id) => document.getElementById(id);
const tabId = Number(new URLSearchParams(location.search).get("tab"));

let links = [];
let filter = "files"; // "files" = semua yang berjenis berkas, "all" = semua tautan, atau id kategori
const chosen = new Set();
const sent = new Set();

function el(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

function ask(msg) {
  return api.runtime.sendMessage(msg).then((r) => {
    if (r && r.error) throw new Error(r.error);
    return r;
  });
}

function matches(l) {
  if (filter === "files" && l.cat === "other") return false;
  if (filter !== "files" && filter !== "all" && l.cat !== filter) return false;
  const q = $("search").value.trim().toLowerCase();
  return !q || l.url.toLowerCase().includes(q) || l.name.toLowerCase().includes(q) || l.text.toLowerCase().includes(q);
}

function renderChips() {
  const count = (pred) => links.filter(pred).length;
  const chips = [
    ["files", "Files", count((l) => l.cat !== "other")],
    ...K.CATEGORIES.map((c) => [c.id, c.label, count((l) => l.cat === c.id)]).filter(([, , n]) => n),
    ["all", "All links", links.length],
  ];
  $("chips").replaceChildren(
    ...chips.map(([id, label, n]) => {
      const b = el("button", "chip", label);
      b.append(el("span", "n", String(n)));
      b.setAttribute("aria-pressed", String(id === filter));
      b.onclick = () => {
        filter = id;
        renderChips();
        renderList();
      };
      return b;
    }),
  );
}

function renderList() {
  const shown = links.filter(matches);
  if (!shown.length) {
    $("list").replaceChildren(el("div", "empty", links.length ? "No matching links. Try another type or \"All links\"." : "No links on this page."));
  } else {
    $("list").replaceChildren(
      ...shown.map((l) => {
        const row = el("label", "row");
        row.title = l.url;
        const box = el("input");
        box.type = "checkbox";
        box.checked = chosen.has(l.url);
        box.onchange = () => {
          box.checked ? chosen.add(l.url) : chosen.delete(l.url);
          updateBar();
        };
        const grow = el("span", "grow");
        grow.append(el("div", "ellipsis", l.name), el("div", "sub ellipsis", l.text && l.text !== l.name ? l.text : K.hostOf(l.url)));
        const state = el("span", "sub");
        if (sent.has(l.url)) {
          state.className = "sub done";
          state.textContent = "Sent";
        }
        row.append(box, el("span", "ext", l.ext || "—"), grow, state);
        return row;
      }),
    );
  }
  updateBar();
}

function updateBar() {
  const n = chosen.size;
  $("count").textContent = n ? n + " selected" : "Tick the files to download";
  $("count").className = "grow sub";
  $("go").disabled = !n;
  $("go").textContent = n ? "Download " + n + (n === 1 ? " file" : " files") : "Download";
}

function setVisible(on) {
  for (const l of links.filter(matches)) on ? chosen.add(l.url) : chosen.delete(l.url);
  renderList();
}

async function download() {
  const urls = [...chosen];
  $("go").disabled = true;
  $("go").textContent = "Sending…";
  try {
    const { sent: n, failed } = await ask({ type: "batch", tabId, urls });
    for (const u of urls) sent.add(u);
    chosen.clear();
    renderList();
    $("count").textContent = n + (n === 1 ? " file" : " files") + " sent to Kara DM" + (failed ? ", " + failed + " failed" : "");
    $("count").className = "grow sub " + (failed ? "fail" : "done");
  } catch (e) {
    updateBar();
    $("count").textContent = /not (running|responding)/.test(e.message) ? "Kara Download Manager is not running. Open it and try again." : e.message;
    $("count").className = "grow sub fail";
  }
}

async function main() {
  if (!Number.isInteger(tabId)) throw new Error("Open this page from the Kara DM popup or the right-click menu.");
  const r = await ask({ type: "links", tabId });
  links = r.links;
  document.title = "Download links — " + (r.title || K.hostOf(r.url));
  $("page").textContent = (r.title ? r.title + " · " : "") + K.hostOf(r.url);
  if (!links.some((l) => l.cat !== "other")) filter = "all";
  $("search").oninput = renderList;
  $("all").onclick = () => setVisible(true);
  $("none").onclick = () => setVisible(false);
  $("go").onclick = download;
  renderChips();
  renderList();
}

main().catch((e) => {
  $("page").textContent = "";
  $("list").replaceChildren(el("div", "empty", e.message));
});
