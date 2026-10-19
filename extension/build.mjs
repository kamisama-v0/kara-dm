// Bangun dua paket dari src/: dist/firefox (background scripts + gecko id) dan dist/chromium (service worker).
// Pemakaian: node build.mjs   (lalu lihat README.md untuk memasang / menandatangani)
import { cpSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const base = JSON.parse(readFileSync(join(here, "manifest.base.json"), "utf8"));

const targets = {
  firefox: {
    background: { scripts: ["common.js", "background.js"] },
    browser_specific_settings: {
      gecko: { id: "kara-dm@kamisama", strict_min_version: "140.0", data_collection_permissions: { required: ["none"] } },
      gecko_android: { strict_min_version: "142.0" },
    },
  },
  chromium: {
    background: { service_worker: "background.js" },
    minimum_chrome_version: "116",
  },
};

for (const [name, extra] of Object.entries(targets)) {
  const out = join(here, "dist", name);
  rmSync(out, { recursive: true, force: true });
  cpSync(join(here, "src"), out, { recursive: true, filter: (p) => !p.endsWith(".svg") });
  writeFileSync(join(out, "manifest.json"), JSON.stringify({ ...base, ...extra }, null, 2) + "\n");
  console.log("dist/" + name);
}
