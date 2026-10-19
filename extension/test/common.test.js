const test = require("node:test");
const assert = require("node:assert/strict");
const K = require("../src/common.js");

test("buildMessage GET mengikuti format protokol NDM", () => {
  const msg = K.buildMessage({
    url: "https://cdn.example.com/a.zip",
    title: "Halaman\r\nunduh",
    page: "https://example.com/p#bagian",
    cookies: "a=1; b=2",
    size: 1234,
    mime: "application/zip",
  });
  assert.equal(
    msg,
    "1:GET\r\n2:https://cdn.example.com/a.zip\r\n6:normal\r\n4:Halaman unduh\r\nOrigin: https://example.com\r\n" +
      "Referer: https://example.com/p\r\n5:https://example.com/p#bagian\r\nCookie: a=1; b=2\r\n7:1234\r\n8:application/zip\r\n",
  );
});

test("buildMessage POST menaruh body setelah penanda", () => {
  const msg = K.buildMessage({ method: "post", url: "https://x.org/dl", body: "a=1&b=2", contentType: "application/x-www-form-urlencoded" });
  assert.ok(msg.startsWith("1:POST\r\n2:https://x.org/dl\r\n6:normal\r\n"));
  assert.ok(msg.endsWith("Content-Type: application/x-www-form-urlencoded\r\n__0NeatPostData9__:a=1&b=2"));
  assert.throws(() => K.buildMessage({ url: "https://x.org/" + "a".repeat(200000) }));
});

test("captureDecision menghormati pengaturan", () => {
  const item = { url: "https://dl.example.com/f.iso", referrer: "https://example.com/", filename: "/home/k/Downloads/f.iso", totalBytes: 5e8 };
  assert.equal(K.captureDecision(item, {}), null);
  assert.equal(K.captureDecision(item, { capture: false }), "tangkapan dimatikan");
  assert.equal(K.captureDecision({ ...item, incognito: true }, {}), "jendela privat");
  assert.equal(K.captureDecision({ ...item, url: "blob:https://x/1" }, {}), "bukan http/https/ftp");
  assert.equal(K.captureDecision(item, { excludedSites: ["example.com"] }), "situs dikecualikan");
  assert.equal(K.captureDecision(item, { excludedSites: ["other.org"] }), null);
  assert.equal(K.captureDecision(item, { ignoredExtensions: [".ISO"] }), "jenis berkas diabaikan");
  assert.equal(K.captureDecision({ ...item, totalBytes: 1000 }, { minSizeMB: 1 }), "di bawah ukuran minimum");
});

test("siteMatches mencocokkan sub-domain, bukan akhiran sembarang", () => {
  assert.ok(K.siteMatches("www.github.com", ["github.com"]));
  assert.ok(K.siteMatches("github.com", ["https://github.com/"]));
  assert.ok(!K.siteMatches("notgithub.com", ["github.com"]));
});

const resp = (url, ct, extra = {}) => ({
  url,
  tabId: 3,
  responseHeaders: [{ name: "Content-Type", value: ct }, ...Object.entries(extra).map(([name, value]) => ({ name, value }))],
});

test("classifyMedia mengenali stream dan berkas media, mengabaikan potongan", () => {
  assert.equal(K.classifyMedia(resp("https://v.x/master.m3u8?t=1", "application/vnd.apple.mpegurl")).kind, "hls");
  assert.equal(K.classifyMedia(resp("https://v.x/a.mpd", "application/dash+xml")).label, "Stream DASH");
  const mp4 = K.classifyMedia(resp("https://v.x/film.mp4", "video/mp4", { "Content-Length": "50000000" }));
  assert.deepEqual([mp4.kind, mp4.ext, mp4.size], ["media", "mp4", 50000000]);
  assert.equal(K.classifyMedia(resp("https://v.x/seg1.ts", "video/mp2t")), null);
  assert.equal(K.classifyMedia(resp("https://r1.googlevideo.com/videoplayback", "video/webm")), null);
  assert.equal(K.classifyMedia(resp("https://v.x/f.mp4", "video/mp4", { "Content-Range": "bytes 1000-2000/9000000" })), null);
  assert.equal(K.classifyMedia(resp("https://v.x/f.mp4", "video/mp4", { "Content-Range": "bytes 0-2000/9000000" })).size, 9000000);
  assert.equal(K.classifyMedia(resp("https://v.x/kecil.mp4", "video/mp4", { "Content-Length": "1000" })), null);
  assert.equal(K.classifyMedia(resp("https://v.x/page", "text/html")), null);
  assert.equal(K.classifyMedia({ ...resp("https://v.x/a.m3u8", ""), tabId: -1 }), null);
});

test("cookieHeader dan encodeRequestBody", () => {
  assert.equal(K.cookieHeader([{ name: "a", value: "1" }, { name: "b", value: "2" }, { name: "a", value: "9" }]), "a=1; b=2");
  assert.deepEqual(K.encodeRequestBody({ formData: { q: ["a b"], id: ["1", "2"] } }), {
    body: "q=a+b&id=1&id=2",
    contentType: "application/x-www-form-urlencoded",
  });
  assert.equal(K.encodeRequestBody({ raw: [{ bytes: new Uint8Array([104, 105]).buffer }] }).body, "hi");
  assert.equal(K.encodeRequestBody(null), null);
  assert.ok(K.isVideoSite("https://www.youtube.com/watch?v=x"));
  assert.ok(!K.isVideoSite("https://example.com/"));
});
