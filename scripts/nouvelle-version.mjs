#!/usr/bin/env node
/**
 * Change le numéro de version partout où il est écrit, en une fois :
 * package.json, package-lock.json, src-tauri/tauri.conf.json, Cargo.toml (et Cargo.lock).
 *
 *   npm run version:nouvelle -- 0.2.0
 *
 * Puis : ajouter les nouveautés dans NOUVEAUTES.md, committer, et poser l'étiquette :
 *   git commit -am "Version 0.2.0" && git tag v0.2.0 && git push && git push origin v0.2.0
 *
 * Une mise à jour n'est proposée aux hôtels que si le numéro augmente.
 */
import { readFileSync, writeFileSync } from "node:fs";

export function comparer(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pa[i] - pb[i];
  return 0;
}

export function changerVersion(fichiers, nouvelle) {
  if (!/^\d+\.\d+\.\d+$/.test(nouvelle)) throw new Error(`« ${nouvelle} » n'est pas un numéro de version (ex. 0.2.0)`);
  const actuelle = JSON.parse(fichiers["package.json"]).version;
  if (comparer(nouvelle, actuelle) <= 0)
    throw new Error(`La nouvelle version (${nouvelle}) doit être plus grande que l'actuelle (${actuelle}), sinon les hôtels ne recevront pas la mise à jour.`);
  const sortie = { ...fichiers };
  const json = (nom, modifier) => {
    const d = JSON.parse(fichiers[nom]);
    modifier(d);
    sortie[nom] = JSON.stringify(d, null, 2) + "\n";
  };
  json("package.json", (d) => (d.version = nouvelle));
  // tauri.conf.json a sa propre mise en forme : on ne touche qu'à la ligne de version
  const conf = fichiers["src-tauri/tauri.conf.json"];
  if (!/^ {2}"version": "[^"]+"/m.test(conf)) throw new Error("tauri.conf.json : version introuvable");
  sortie["src-tauri/tauri.conf.json"] = conf.replace(/^( {2}"version": )"[^"]+"/m, `$1"${nouvelle}"`);
  if (fichiers["package-lock.json"])
    json("package-lock.json", (d) => {
      d.version = nouvelle;
      if (d.packages?.[""]) d.packages[""].version = nouvelle;
    });
  const cargo = fichiers["Cargo.toml"];
  const section = cargo.indexOf("[workspace.package]");
  if (section < 0) throw new Error("Cargo.toml : section [workspace.package] introuvable");
  sortie["Cargo.toml"] =
    cargo.slice(0, section) + cargo.slice(section).replace(/^version = "[^"]+"/m, `version = "${nouvelle}"`);
  if (fichiers["Cargo.lock"])
    sortie["Cargo.lock"] = fichiers["Cargo.lock"].replace(
      /(name = "tanana(?:-[a-z]+)?"\nversion = )"[^"]+"/g,
      `$1"${nouvelle}"`,
    );
  return { actuelle, sortie };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const nouvelle = process.argv[2];
  const noms = ["package.json", "package-lock.json", "src-tauri/tauri.conf.json", "Cargo.toml", "Cargo.lock"];
  const fichiers = Object.fromEntries(noms.map((n) => [n, readFileSync(n, "utf8")]));
  try {
    const { actuelle, sortie } = changerVersion(fichiers, nouvelle ?? "");
    for (const n of noms) if (sortie[n] !== fichiers[n]) writeFileSync(n, sortie[n]);
    console.log(`Version ${actuelle} → ${nouvelle}. Pensez à NOUVEAUTES.md, puis :`);
    console.log(`  git commit -am "Version ${nouvelle}" && git tag v${nouvelle} && git push && git push origin v${nouvelle}`);
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}
