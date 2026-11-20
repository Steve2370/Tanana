#!/usr/bin/env node
/**
 * QA dans un vrai navigateur : chaque écran de Tanana, à chaque taille de
 * fenêtre (1024 à 1920 px) et avec le texte agrandi jusqu'à 200 %.
 * Vérifie : pas de défilement horizontal, aucun bouton ni champ hors de la
 * fenêtre ou caché sous un autre élément, aucun texte de bouton coupé,
 * aucune erreur JavaScript.
 *
 *   node scripts/qa-fenetres.mjs            (Chrome ou Chromium installé)
 *   CHROME=/chemin/vers/chrome node scripts/qa-fenetres.mjs
 *   QA_CAPTURES=dossier node scripts/qa-fenetres.mjs   (captures des écrans en défaut)
 *   QA_TEXTES=1,1.5,2 node scripts/qa-fenetres.mjs     (texte agrandi : exploration)
 *
 * Aucune dépendance : Vite (déjà là), Chrome et le protocole DevTools.
 */
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const PORT_VITE = 1431;
const PORT_CHROME = 9333;
const FENETRES = [
  [1024, 680], // taille minimale de la fenêtre Tanana
  [1280, 720],
  [1366, 768],
  [1440, 900],
  [1920, 1080],
];
// Taille du texte. Tanana n'a pas (encore) de réglage de taille du texte et le zoom
// est bloqué : seul 100 % existe vraiment. QA_TEXTES=1,1.5,2 pour explorer plus grand.
const TEXTES = (process.env.QA_TEXTES ?? "1").split(",").map(Number);
const ECRANS = [
  "Accueil",
  "Réservations",
  "Clients",
  "Chambres",
  "Caisse",
  "Employés et accès",
  "Historique",
  "Réglages",
  "Mon compte (mot de passe, PIN)",
];
/**
 * Parcours : des états qu'on n'atteint pas d'un seul clic dans le menu.
 * Chaque étape est exécutée dans la page ; `cliquer(texte)` clique le premier
 * bouton (ou case) dont le nom accessible commence par ce texte.
 */
const PARCOURS = [
  {
    nom: "Nouvelle réservation de groupe (2 chambres, prix convenus)",
    etapes: [
      'menu("Accueil")',
      'menu("Réservations")',
      'cliquer("Nouvelle réservation")',
      'cocher(/Chambre 101/)',
      'cocher(/Chambre 102/)',
      'cocher(/Appliquer un autre prix/)',
    ],
  },
  {
    nom: "Fiche d'une chambre de groupe",
    etapes: ['menu("Accueil")', 'menu("Réservations")', 'cliquer("Mbarga Aline, chambre 102")'],
  },
  {
    nom: "Ajouter une chambre au groupe",
    etapes: ['menu("Accueil")', 'menu("Réservations")', 'cliquer("Mbarga Aline, chambre 102")', 'cliquer("Ajouter une chambre")'],
  },
];
/** Aides disponibles dans les étapes des parcours. */
const AIDES = `
  const nomDe = (e) => (e.getAttribute("aria-label") || e.textContent || "").trim().replace(/\\s+/g, " ");
  const menu = (t) => [...document.querySelector('nav[aria-label="Menu principal"]').querySelectorAll("button")].find((b) => nomDe(b) === t).click();
  const cliquer = (t) => { const b = [...document.querySelectorAll("button")].find((b) => nomDe(b).startsWith(t)); if (!b) throw new Error("bouton introuvable : " + t); b.click(); };
  const cocher = (re) => { const l = [...document.querySelectorAll("label")].find((l) => re.test(l.textContent)); if (!l) throw new Error("case introuvable : " + re); l.click(); };
`;
const NOM_LONG = "Hôtel Résidence Internationale du Grand Lac de Kribi et des Chutes de la Lobé";

const CHROMES = [process.env.CHROME, "/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "C:/Program Files/Google/Chrome/Application/chrome.exe"].filter(Boolean);

const attendre = (ms) => new Promise((r) => setTimeout(r, ms));

async function joindre(url, essais = 60) {
  for (let i = 0; i < essais; i++) {
    try {
      const r = await fetch(url);
      if (r.ok) return r;
    } catch {}
    await attendre(250);
  }
  throw new Error(`injoignable : ${url}`);
}

function cdp(ws) {
  let id = 0;
  const attentes = new Map();
  const ecouteurs = [];
  ws.addEventListener("message", (m) => {
    const msg = JSON.parse(m.data);
    if (msg.id && attentes.has(msg.id)) {
      const { ok, ko } = attentes.get(msg.id);
      attentes.delete(msg.id);
      msg.error ? ko(new Error(msg.error.message)) : ok(msg.result);
    } else if (msg.method) ecouteurs.forEach((f) => f(msg));
  });
  return {
    envoyer: (method, params = {}) =>
      new Promise((ok, ko) => {
        attentes.set(++id, { ok, ko });
        ws.send(JSON.stringify({ id, method, params }));
      }),
    ecouter: (f) => ecouteurs.push(f),
  };
}

/** Exécuté dans la page : relève les défauts de mise en page de l'écran affiché. */
function relever() {
  const defauts = [];
  const nomDe = (e) => (e.getAttribute("aria-label") || e.textContent || e.id || e.tagName).trim().replace(/\s+/g, " ").slice(0, 50);
  const doc = document.documentElement;
  if (doc.scrollWidth > doc.clientWidth + 1) defauts.push(`défilement horizontal (${doc.scrollWidth} px pour ${doc.clientWidth})`);
  for (const z of document.querySelectorAll("main, [data-defilement], .overflow-auto, .overflow-y-auto")) {
    if (z.scrollWidth > z.clientWidth + 1 && getComputedStyle(z).overflowX !== "hidden")
      defauts.push(`défilement horizontal dans une zone (${z.scrollWidth} px pour ${z.clientWidth})`);
  }
  const controles = [...document.querySelectorAll("button, input:not([type=hidden]), select, textarea, a[href]")].filter((e) => {
    const r = e.getBoundingClientRect();
    return r.width > 2 && r.height > 2 && !e.closest("[aria-hidden=true]") && getComputedStyle(e).visibility !== "hidden";
  });
  for (const e of controles) {
    e.scrollIntoView({ block: "center", inline: "nearest" });
    const r = e.getBoundingClientRect();
    if (r.right > innerWidth + 1 || r.left < -1) {
      defauts.push(`hors de la fenêtre : « ${nomDe(e)} »`);
      continue;
    }
    // Atteignable si au moins un des trois points (gauche, centre, droite) le touche.
    // Les cases et boutons radio sont souvent cachés sous leur étiquette : c'est voulu.
    const etiquette = e.labels?.[0];
    const y = Math.min(r.top + r.height / 2, innerHeight - 1);
    const dessous = [0.2, 0.5, 0.8].map((f) => document.elementFromPoint(Math.min(r.left + r.width * f, innerWidth - 1), y));
    const atteint = dessous.some((d) => d && (e.contains(d) || d.contains(e) || (etiquette && etiquette.contains(d))));
    if (!atteint) defauts.push(`caché sous un autre élément : « ${nomDe(e)} » (sous « ${nomDe(dessous[1] ?? document.body)} »)`);
    if (e.tagName === "BUTTON" && e.scrollWidth > e.clientWidth + 2) defauts.push(`texte de bouton coupé : « ${nomDe(e)} »`);
  }
  scrollTo(0, 0);
  return [...new Set(defauts)];
}

async function principal() {
  const chrome = CHROMES.find((c) => existsSync(c));
  if (!chrome) throw new Error("Chrome introuvable : indiquez-le avec CHROME=/chemin/vers/chrome");
  const vite = spawn("npx", ["vite", "--port", String(PORT_VITE), "--strictPort"], { stdio: "ignore", shell: process.platform === "win32" });
  const profil = mkdtempSync(join(tmpdir(), "tanana-qa-"));
  const nav = spawn(chrome, ["--headless=new", "--no-sandbox", "--disable-gpu", "--hide-scrollbars", `--remote-debugging-port=${PORT_CHROME}`, `--user-data-dir=${profil}`, "about:blank"], { stdio: "ignore" });
  const captures = process.env.QA_CAPTURES;
  if (captures) mkdirSync(captures, { recursive: true });
  const constats = [];
  const erreurs = [];
  try {
    await joindre(`http://localhost:${PORT_VITE}/src/qa/banc/index.html`);
    const cibles = await (await joindre(`http://127.0.0.1:${PORT_CHROME}/json/list`)).json();
    const page = cibles.find((c) => c.type === "page");
    const ws = new WebSocket(page.webSocketDebuggerUrl);
    await new Promise((ok) => ws.addEventListener("open", ok));
    const c = cdp(ws);
    c.ecouter((m) => {
      if (m.method === "Runtime.exceptionThrown") erreurs.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text);
      if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error")
        erreurs.push(m.params.args.map((a) => a.value ?? a.description).join(" ").slice(0, 300));
    });
    await c.envoyer("Runtime.enable");
    await c.envoyer("Page.enable");
    const evaluer = async (expression) => {
      const r = await c.envoyer("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
      return r.result.value;
    };
    for (const nom of ["Hôtel du Lac", NOM_LONG]) {
      for (const [largeur, hauteur] of FENETRES) {
        for (const texte of TEXTES) {
          await c.envoyer("Emulation.setDeviceMetricsOverride", { width: largeur, height: hauteur, deviceScaleFactor: 1, mobile: false });
          await c.envoyer("Page.navigate", { url: `http://localhost:${PORT_VITE}/src/qa/banc/index.html?nom=${encodeURIComponent(nom)}` });
          await attendre(900);
          await evaluer(`document.documentElement.style.fontSize = "${texte * 100}%"`);
          for (const ecran of ECRANS) {
            const trouve = await evaluer(`(() => {
              const menu = document.querySelector('nav[aria-label="Menu principal"]');
              const b = menu && [...menu.querySelectorAll("button")].find((b) => (b.getAttribute("aria-label") || b.textContent.trim()) === ${JSON.stringify(ecran)});
              if (!b) return false; b.click(); return true; })()`);
            const ou = `${ecran} · ${largeur}×${hauteur} · texte ${texte * 100} %${nom === NOM_LONG ? " · nom d'hôtel long" : ""}`;
            if (!trouve) {
              constats.push(`${ou} : écran introuvable dans le menu`);
              continue;
            }
            await attendre(350);
            const defauts = await evaluer(`(${relever.toString()})()`);
            for (const d of defauts) constats.push(`${ou} : ${d}`);
            if (defauts.length && captures) {
              const { data } = await c.envoyer("Page.captureScreenshot", { format: "png" });
              writeFileSync(join(captures, `${ecran.split(" ")[0]}-${largeur}-${texte * 100}${nom === NOM_LONG ? "-long" : ""}.png`), Buffer.from(data, "base64"));
            }
          }
          for (const [i, p] of PARCOURS.entries()) {
            const ou = `${p.nom} · ${largeur}×${hauteur} · texte ${texte * 100} %${nom === NOM_LONG ? " · nom d'hôtel long" : ""}`;
            try {
              for (const etape of p.etapes) {
                await evaluer(`(() => { ${AIDES}; ${etape}; })()`);
                await attendre(500);
              }
            } catch (e) {
              constats.push(`${ou} : parcours impossible (${e.message.split("\n")[0]})`);
              continue;
            }
            const defauts = await evaluer(`(${relever.toString()})()`);
            for (const d of defauts) constats.push(`${ou} : ${d}`);
            if (captures && (defauts.length || process.env.QA_TOUT)) {
              await evaluer(`document.querySelector("main")?.scrollTo(0, 0)`);
              const { data } = await c.envoyer("Page.captureScreenshot", { format: "png" });
              writeFileSync(join(captures, `parcours${i + 1}-${largeur}-${texte * 100}${nom === NOM_LONG ? "-long" : ""}.png`), Buffer.from(data, "base64"));
            }
          }
        }
      }
    }
    ws.close();
  } finally {
    nav.kill();
    vite.kill();
  }
  const uniques = [...new Set(erreurs)];
  for (const e of uniques) constats.push(`erreur JavaScript : ${e}`);
  const essais = 2 * FENETRES.length * TEXTES.length * (ECRANS.length + PARCOURS.length);
  console.log(
    `\n${essais} écrans vérifiés (${ECRANS.length} écrans et ${PARCOURS.length} parcours × ${FENETRES.length} fenêtres × ${TEXTES.length} tailles de texte × 2 noms d'hôtel)`,
  );
  if (constats.length === 0) {
    console.log("Aucun défaut.");
    return;
  }
  console.log(`${constats.length} défaut(s) :`);
  for (const k of constats) console.log(`  - ${k}`);
  process.exitCode = 1;
}

principal().catch((e) => {
  console.error(e);
  process.exitCode = 2;
});
