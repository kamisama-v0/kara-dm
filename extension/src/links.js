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
    ["files", "Berkas", count((l) => l.cat !== "other")],
    ...K.CATEGORIES.map((c) => [c.id, c.label, count((l) => l.cat === c.id)]).filter(([, , n]) => n),
    ["all", "Semua tautan", links.length],
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
    $("list").replaceChildren(el("div", "empty", links.length ? "Tidak ada tautan yang cocok. Coba jenis lain atau \"Semua tautan\"." : "Tidak ada tautan di halaman ini."));
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
          state.textContent = "Dikirim";
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
  $("count").textContent = n ? n + " dipilih" : "Centang berkas yang mau diunduh";
  $("count").className = "grow sub";
  $("go").disabled = !n;
  $("go").textContent = n ? "Unduh " + n + " berkas" : "Unduh";
}

function setVisible(on) {
  for (const l of links.filter(matches)) on ? chosen.add(l.url) : chosen.delete(l.url);
  renderList();
}

async function download() {
  const urls = [...chosen];
  $("go").disabled = true;
  $("go").textContent = "Mengirim…";
  try {
    const { sent: n, failed } = await ask({ type: "batch", tabId, urls });
    for (const u of urls) sent.add(u);
    chosen.clear();
    renderList();
    $("count").textContent = n + " berkas dikirim ke Kara DM" + (failed ? ", " + failed + " gagal" : "");
    $("count").className = "grow sub " + (failed ? "fail" : "done");
  } catch (e) {
    updateBar();
    $("count").textContent = /tidak (berjalan|menjawab)/.test(e.message) ? "Kara Download Manager tidak berjalan. Buka aplikasinya lalu coba lagi." : e.message;
    $("count").className = "grow sub fail";
  }
}

async function main() {
  if (!Number.isInteger(tabId)) throw new Error("Buka halaman ini dari popup Kara DM atau menu klik kanan.");
  const r = await ask({ type: "links", tabId });
  links = r.links;
  document.title = "Unduh tautan — " + (r.title || K.hostOf(r.url));
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
