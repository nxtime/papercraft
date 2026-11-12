// Bundle the open icon sets for the browser: one `<out>/<set>.json` per set,
// `{"license": "<licence text>", "icons": {name: svg}}`. Run at image build time:
//   node deploy/icon-sets.mjs <out dir>
// The ids, packages and folders mirror SETS in src/icons.rs (what `papercraft-cli icons add`
// installs); versions are pinned here, bump them on purpose.
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, existsSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SETS = [
  ["lucide", "lucide-static@1.72.0", "icons"],
  ["tabler", "@tabler/icons@3.51.0", "icons/outline"],
  ["phosphor", "@phosphor-icons/core@2.1.1", "assets/regular"],
  ["heroicons", "heroicons@2.2.0", "24/outline"],
  ["material", "@material-symbols/svg-400@0.49.0", "outlined"],
];
const out = process.argv[2];
if (!out) throw new Error("usage: node icon-sets.mjs <out dir>");
mkdirSync(out, { recursive: true });

for (const [id, pkg, dir] of SETS) {
  const tmp = mkdtempSync(join(tmpdir(), `icons-${id}-`));
  const tgz = execFileSync("npm", ["pack", pkg, "--silent"], { cwd: tmp }).toString().trim().split("\n").pop();
  execFileSync("tar", ["-xzf", tgz], { cwd: tmp });
  const root = join(tmp, "package");
  const icons = {};
  for (const f of readdirSync(join(root, dir)).sort()) {
    if (f.endsWith(".svg")) icons[f.slice(0, -4)] = readFileSync(join(root, dir, f), "utf8");
  }
  const lic = ["LICENSE", "LICENSE.md", "LICENSE.txt", "license"].map((n) => join(root, n)).find(existsSync);
  if (!lic || !Object.keys(icons).length) throw new Error(`${id}: no licence or no icons in ${pkg}/${dir}`);
  writeFileSync(join(out, `${id}.json`), JSON.stringify({ license: readFileSync(lic, "utf8"), icons }));
  console.log(`${id}: ${Object.keys(icons).length} icons`);
}
