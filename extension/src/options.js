const api = globalThis.browser ?? globalThis.chrome;
const K = globalThis.KDM;
const $ = (id) => document.getElementById(id);
const list = (text, sep) => text.split(sep).map((s) => s.trim()).filter(Boolean);

let timer;
function saved() {
  $("saved").classList.add("show");
  clearTimeout(timer);
  timer = setTimeout(() => $("saved").classList.remove("show"), 1200);
}

api.storage.local.get(K.DEFAULTS).then((s) => {
  s = { ...K.DEFAULTS, ...s };
  for (const id of ["capture", "videoButton", "notify"]) {
    $(id).checked = s[id];
    $(id).onchange = (e) => api.storage.local.set({ [id]: e.target.checked }).then(saved);
  }
  $("excludedSites").value = s.excludedSites.join("\n");
  $("excludedSites").onchange = (e) => api.storage.local.set({ excludedSites: list(e.target.value, /\n/) }).then(saved);
  $("ignoredExtensions").value = s.ignoredExtensions.join(", ");
  $("ignoredExtensions").onchange = (e) =>
    api.storage.local.set({ ignoredExtensions: list(e.target.value, /[,\s]+/).map((x) => x.replace(/^\./, "").toLowerCase()) }).then(saved);
  $("minSizeMB").value = s.minSizeMB;
  $("minSizeMB").onchange = (e) => api.storage.local.set({ minSizeMB: Math.max(0, Number(e.target.value) || 0) }).then(saved);
});
