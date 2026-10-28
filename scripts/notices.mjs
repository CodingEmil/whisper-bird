// Erzeugt THIRD-PARTY-NOTICES.txt: Lizenzen der Modelle, Rust-Crates, npm-Pakete
// und von llama.cpp. Der Installer liefert die Datei mit, die Hilfe verlinkt sie.
// Aufruf: bun run notices  (nach neuen Abhängigkeiten oder Modellen)
// Braucht Netz und cargo-about: cargo install cargo-about --locked --features cli
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriDir = join(root, "src-tauri");
const read = (...p) => readFileSync(join(...p), "utf8").replace(/\r\n/g, "\n");
const canonical = (id) => read(root, "scripts", "licenses", `${id}.txt`).trim();

// MIT nennt den Rechteinhaber im Text, die Hugging-Face-Metadaten nicht.
const MIT_HOLDERS = [
  [/^openai\//, "2022 OpenAI"],
  [/silero/, "2020-present Silero Team"],
];
const mitText = (holder) => canonical("MIT").replace("<year> <copyright holders>", holder);

// ---------- Lizenztexte, dedupliziert und nummeriert ----------

const texts = [];
const byText = new Map();
/** Gibt die Referenz [Ln] zurück; gleiche Texte (bis auf Leerraum) teilen sich eine Nummer. */
function ref(title, text, user) {
  const key = text.replace(/\s+/g, " ").trim();
  let entry = byText.get(key);
  if (!entry) {
    entry = { n: texts.length + 1, title, text: text.trim(), users: [] };
    texts.push(entry);
    byText.set(key, entry);
  }
  if (!entry.users.includes(user)) entry.users.push(user);
  return `[L${entry.n}]`;
}

// ---------- 1. Modelle ----------

const MODEL_LICENSES = {
  "cc-by-4.0": { title: "Creative Commons Attribution 4.0 International (CC-BY-4.0)", text: () => canonical("CC-BY-4.0") },
  "apache-2.0": { title: "Apache License 2.0", text: () => canonical("Apache-2.0") },
  mit: {
    title: "MIT License",
    text: (source) => {
      const holder = MIT_HOLDERS.find(([re]) => re.test(source))?.[1];
      if (!holder) throw new Error(`MIT-Rechteinhaber für ${source} fehlt in MIT_HOLDERS`);
      return mitText(holder);
    },
  },
  "nvidia-open-model-license": {
    title: "NVIDIA Open Model License",
    text: () => canonical("NVIDIA-Open-Model-License"),
    notice: "Licensed by NVIDIA Corporation under the NVIDIA Open Model License",
  },
};

async function models() {
  const catalog = read(tauriDir, "src", "catalog.rs");
  const entries = [...catalog.matchAll(/ModelInfo \{([\s\S]*?)\n    \},/g)].map(([, body]) =>
    Object.fromEntries([...body.matchAll(/(\w+): "([^"]*)"/g)].map(([, k, v]) => [k, v])),
  );
  const out = [
    "Whisper Bird does not include these models. It downloads them from Hugging Face",
    "when you choose them. Each file is pinned to a revision and checked by SHA-256.",
  ];
  const cardOf = async (repo, revision = "main") => {
    const res = await fetch(`https://huggingface.co/api/models/${repo}/revision/${revision}`);
    if (!res.ok) throw new Error(`${repo}: HTTP ${res.status}`);
    const card = (await res.json()).cardData ?? {};
    return { ...card, license: card.license === "other" ? card.license_name : card.license };
  };
  for (const m of entries) {
    const card = await cardOf(m.repo, m.revision);
    const base = [card.base_model].flat()[0];
    // Maßgeblich ist das Original: ältere GGUF-Stände haben teils falsche oder
    // unvollständige Lizenzangaben (z. B. parakeet-unified-en vor dem 16.10.2026).
    const licenseId = base ? (await cardOf(base)).license : card.license;
    if (card.license && card.license !== licenseId) {
      console.warn(`${m.repo}@${m.revision.slice(0, 7)}: GGUF nennt "${card.license}", Original "${licenseId}"`);
    }
    const lic = MODEL_LICENSES[licenseId];
    if (!lic) throw new Error(`${m.repo}: unbekannte Lizenz "${licenseId}" – Text in scripts/licenses ergänzen`);

    const format = m.filename.match(/-([A-Z]\w*)\.gguf$/)?.[1];
    const org = m.repo.split("/")[0];
    out.push("", m.name);
    if (base) out.push(`  Original:  https://huggingface.co/${base}`);
    out.push(`  Download:  https://huggingface.co/${m.repo} (revision ${m.revision.slice(0, 7)}), ${m.filename}`);
    out.push(`  License:   ${lic.title} ${ref(lic.title, lic.text(base ?? m.id), m.name)}`);
    if (lic.notice) out.push(`  Notice:    ${lic.notice}`);
    out.push(`  Changes:   Converted to GGUF${format ? ` (${format})` : ""} by ${org}.`);
  }
  return out;
}

// ---------- 2. Rust-Crates ----------

function rustCrates() {
  // cargo-about verweigert unter Windows die Ausgabe in eine Pipe, daher -o.
  const tmp = mkdtempSync(join(tmpdir(), "notices-"));
  const file = join(tmp, "about.json");
  execFileSync("cargo", ["about", "generate", "--format", "json", "-c", "about.toml", "-o", file], {
    cwd: tauriDir,
    stdio: "inherit",
  });
  const about = JSON.parse(readFileSync(file, "utf8"));
  rmSync(tmp, { recursive: true });
  const refs = new Map();
  const add = (key, r) => refs.set(key, [...(refs.get(key) ?? []), r]);
  for (const lic of about.licenses) {
    for (const { crate } of lic.used_by) {
      const key = `${crate.name} ${crate.version}`;
      add(key, ref(lic.name, lic.text.replace(/\r\n/g, "\n"), key));
    }
  }

  const out = [
    "Compiled into Whisper Bird. Crates under MPL-2.0 are used unmodified; their",
    "source code is available at the crates.io address given next to them.",
    "",
  ];
  const crates = about.crates.map((c) => c.package).sort((a, b) => a.name.localeCompare(b.name));
  for (const p of crates) {
    const key = `${p.name} ${p.version}`;
    const r = refs.get(key);
    if (!r) throw new Error(`${key}: kein Lizenztext von cargo-about`);
    // NOTICE-Dateien (Apache-2.0 §4d) und Listen mitgelieferter Fremdbibliotheken.
    const dir = dirname(p.manifest_path);
    for (const f of readdirSync(dir).filter((f) => /^(NOTICE|THIRD[-_]PARTY)/i.test(f)).sort()) {
      r.push(ref(`${p.name}: ${f}`, read(dir, f), key));
    }
    const mpl = /MPL-2\.0/.test(p.license ?? "") ? `  https://crates.io/crates/${p.name}/${p.version}` : "";
    out.push(`${key}  ${p.license ?? "see license file"}  ${[...new Set(r)].join(" ")}${mpl}`);
  }
  return out;
}

// ---------- 3. npm-Pakete der Oberfläche ----------

function npmPackages() {
  const seen = new Map();
  const visit = (name) => {
    if (seen.has(name)) return;
    const dir = join(root, "node_modules", name);
    const pkg = JSON.parse(read(dir, "package.json"));
    seen.set(name, { pkg, dir });
    Object.keys(pkg.dependencies ?? {}).forEach(visit);
  };
  Object.keys(JSON.parse(read(root, "package.json")).dependencies).forEach(visit);

  const out = ["Bundled into the user interface.", ""];
  for (const { pkg, dir } of [...seen.values()].sort((a, b) => a.pkg.name.localeCompare(b.pkg.name))) {
    const key = `${pkg.name} ${pkg.version}`;
    let files = readdirSync(dir).filter((f) => /^(LICEN[CS]E|COPYING|NOTICE)/i.test(f) && !/\.spdx$/i.test(f));
    // Bei "MIT OR Apache-2.0" reicht die MIT-Datei.
    if (/\bMIT\b/.test(pkg.license) && files.some((f) => /MIT/i.test(f))) files = files.filter((f) => /MIT|NOTICE/i.test(f));
    let r = files.map((f) => ref(pkg.license, read(dir, f), key));
    if (!r.length) {
      // Tauri-Plugins liefern nur LICENSE.spdx; der Rechteinhaber steht dort.
      const spdx = existsSync(join(dir, "LICENSE.spdx")) ? read(dir, "LICENSE.spdx") : "";
      const holder = spdx.match(/PackageCopyrightText: (.+)/)?.[1] ?? pkg.author?.name ?? pkg.author;
      if (!/\bMIT\b/.test(pkg.license) || !holder) throw new Error(`${key}: kein Lizenztext gefunden`);
      r = [ref("MIT License", mitText(holder), key)];
    }
    out.push(`${key}  ${pkg.license}  ${r.join(" ")}`);
  }
  return out;
}

// ---------- 4. llama.cpp ----------

async function llama() {
  const tag = read(tauriDir, "src", "enhance", "llama.rs").match(/LLAMA_TAG: &str = "([^"]+)"/)[1];
  const res = await fetch(`https://raw.githubusercontent.com/ggml-org/llama.cpp/${tag}/LICENSE`);
  if (!res.ok) throw new Error(`llama.cpp LICENSE: HTTP ${res.status}`);
  return [
    "Whisper Bird downloads llama-server for Enhance from the llama.cpp releases",
    "on GitHub when you set it up. The archive contains its own license files.",
    "",
    `llama.cpp ${tag}  MIT  ${ref("MIT License", await res.text(), `llama.cpp ${tag}`)}  https://github.com/ggml-org/llama.cpp/releases/tag/${tag}`,
  ];
}

// ---------- Zusammensetzen ----------

const sections = [
  ["Speech and language models", await models()],
  ["Rust crates", rustCrates()],
  ["JavaScript packages", npmPackages()],
  ["llama.cpp", await llama()],
];

const rule = "=".repeat(78);
const lines = [
  "Whisper Bird – Third-Party Notices",
  rule,
  "",
  "Whisper Bird uses the third-party software and models listed below under their",
  "own licenses. The full license texts follow at the end, referenced as [L1], [L2] …",
  "",
  "Whisper Bird nutzt die unten aufgeführte Software und Modelle Dritter unter deren",
  "eigenen Lizenzen. Die vollständigen Lizenztexte stehen am Ende, verwiesen als [L1], [L2] …",
  "",
  "Generated by scripts/notices.mjs.",
];
sections.forEach(([title, body], i) => lines.push("", "", rule, `${i + 1}. ${title}`, rule, "", ...body));
lines.push("", "", rule, `${sections.length + 1}. License texts`, rule);
for (const t of texts) {
  lines.push("", "", `[L${t.n}] ${t.title}`, "-".repeat(78), `Used by: ${t.users.join(", ")}`, "", t.text);
}

const target = join(root, "THIRD-PARTY-NOTICES.txt");
writeFileSync(target, lines.join("\n") + "\n");
console.log(`${target}: ${texts.length} Lizenztexte, ${(lines.join("\n").length / 1024).toFixed(0)} KB`);
