#!/usr/bin/env node
/**
 * Essai de bout en bout de la page web de consultation, dans un vrai Chrome.
 * Il faut un relais de démonstration qui sert la page :
 *   npm run build:web
 *   cargo run -p tanana-relais --example demo -- --pages dist-web     (affiche identifiant et clé)
 *   node scripts/qa-consultation.mjs <identifiant> <clé> [adresse=http://127.0.0.1:8787]
 * QA_CAPTURES=dossier : enregistre une capture par étape.
 */
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [identifiant, cle, adresse = "http://127.0.0.1:8787"] = process.argv.slice(2);
if (!identifiant || !cle) {
  console.error("Usage : node scripts/qa-consultation.mjs <identifiant> <clé> [adresse]");
  process.exit(2);
}
const PORT_CHROME = 9334;
const CHROMES = [process.env.CHROME, "/usr/bin/google-chrome", "/usr/bin/chromium",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"].filter(Boolean);
const attendre = (ms) => new Promise((r) => setTimeout(r, ms));
const captures = process.env.QA_CAPTURES;
if (captures) mkdirSync(captures, { recursive: true });

async function joindre(url) {
  for (let i = 0; i < 60; i++) {
    try {
      const r = await fetch(url);
      if (r.ok) return r;
    } catch {}
    await attendre(250);
  }
  throw new Error(`injoignable : ${url}`);
}

const chrome = CHROMES.find((c) => existsSync(c));
const nav = spawn(chrome, ["--headless=new", "--no-sandbox", "--disable-gpu", `--remote-debugging-port=${PORT_CHROME}`,
  `--user-data-dir=${mkdtempSync(join(tmpdir(), "tanana-e2e-"))}`, "about:blank"], { stdio: "ignore" });
const constats = [];
const erreursJs = [];
try {
  const cibles = await (await joindre(`http://127.0.0.1:${PORT_CHROME}/json/list`)).json();
  const ws = new WebSocket(cibles.find((c) => c.type === "page").webSocketDebuggerUrl);
  await new Promise((ok) => ws.addEventListener("open", ok));
  let id = 0;
  const attentes = new Map();
  ws.addEventListener("message", (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && attentes.has(msg.id)) {
      attentes.get(msg.id)(msg);
      attentes.delete(msg.id);
    } else if (msg.method === "Runtime.exceptionThrown") erreursJs.push(msg.params.exceptionDetails.exception?.description);
    else if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error")
      erreursJs.push(msg.params.args.map((a) => a.value ?? a.description).join(" "));
    else if (msg.method === "Log.entryAdded" && msg.params.entry.level === "error" && !/favicon/.test(msg.params.entry.url ?? ""))
      erreursJs.push(`${msg.params.entry.text} ${msg.params.entry.url ?? ""}`);
  });
  const envoyer = (method, params = {}) =>
    new Promise((ok) => {
      attentes.set(++id, ok);
      ws.send(JSON.stringify({ id, method, params }));
    });
  const evaluer = async (expression) => {
    const r = await envoyer("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description);
    return r.result?.result?.value;
  };
  const capture = async (nom) => {
    if (!captures) return;
    const r = await envoyer("Page.captureScreenshot", { format: "png", captureBeyondViewport: true });
    writeFileSync(join(captures, `${nom}.png`), Buffer.from(r.result.data, "base64"));
  };
  const texte = () => evaluer("document.body.innerText");
  const attendreTexte = async (motif, ms = 8000) => {
    for (let t = 0; t < ms; t += 200) {
      if (new RegExp(motif).test(await texte())) return true;
      await attendre(200);
    }
    return false;
  };
  const saisir = (libelle, valeur) =>
    evaluer(`(() => {
      const l = [...document.querySelectorAll("label")].find((x) => x.textContent.trim() === ${JSON.stringify(libelle)});
      const champ = document.getElementById(l.htmlFor);
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(champ, ${JSON.stringify(valeur)});
      champ.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
  const cliquer = (nom) =>
    evaluer(`(() => {
      const b = [...document.querySelectorAll("button")].find((x) => x.textContent.trim() === ${JSON.stringify(nom)});
      if (!b) throw new Error("bouton introuvable : " + ${JSON.stringify(nom)});
      b.click();
    })()`);
  const verifier = async (etape, motif) => {
    if (!(await attendreTexte(motif))) constats.push(`${etape} : « ${motif} » n'apparaît pas`);
  };

  await envoyer("Runtime.enable");
  await envoyer("Log.enable");
  await envoyer("Page.enable");
  await envoyer("Emulation.setDeviceMetricsOverride", { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false });
  await envoyer("Page.navigate", { url: adresse });
  await verifier("ouverture", "Consulter mon hôtel");
  await capture("1-saisie");

  // Mauvaise clé : message clair, rien d'ouvert
  await saisir("Identifiant de l'hôtel", identifiant.toLowerCase().replace("-", " "));
  await saisir("Clé secrète", "AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AA");
  await cliquer("Voir mon hôtel");
  await verifier("mauvaise clé", "ne correspond pas à cet hôtel");
  await capture("2-mauvaise-cle");
  // Le refus (403) de la mauvaise clé est voulu : le navigateur le note, ce n'est pas une erreur.
  erreursJs.length = 0;

  // Bonne clé, recopiée en minuscules sans tirets
  await saisir("Clé secrète", cle.toLowerCase().replace(/-/g, ""));
  await cliquer("Voir mon hôtel");
  await verifier("hôtel ouvert", "Consultation à distance \\(lecture seule\\)");
  await verifier("aujourd'hui", "Chambres occupées");
  await capture("3-aujourdhui");
  for (const [onglet, motif] of [
    ["Réservations", "En cours et à venir"],
    ["Caisse", "Reste en caisse"],
    ["Historique", "qui a fait quoi"],
  ]) {
    await cliquer(onglet);
    await verifier(onglet, motif);
    await capture(`4-${onglet}`);
  }
  await cliquer("Actualiser");
  await attendre(800);
  await cliquer("Fermer");
  await verifier("fermeture", "Consulter mon hôtel");
  // Mobile
  await envoyer("Emulation.setDeviceMetricsOverride", { width: 390, height: 844, deviceScaleFactor: 2, mobile: true });
  await envoyer("Page.navigate", { url: adresse });
  await verifier("mobile", "Consulter mon hôtel");
  const debord = await evaluer("document.documentElement.scrollWidth > document.documentElement.clientWidth");
  if (debord) constats.push("mobile : défilement horizontal sur la page de saisie");
  await capture("5-mobile");
  ws.close();
} finally {
  nav.kill();
}
for (const e of erreursJs) constats.push(`erreur dans la page : ${e}`);
if (constats.length) {
  console.log(`${constats.length} défaut(s) :`);
  for (const c of constats) console.log(`  - ${c}`);
  process.exitCode = 1;
} else console.log("Page de consultation : tout fonctionne (saisie, mauvaise clé, 4 rubriques, actualiser, fermer, mobile).");
