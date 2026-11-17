/**
 * Le « singe » : 200 actions au hasard dans toute l'application (clics, saisies
 * hostiles, Échap, Tab, Entrée, listes, cases), avec une graine reproductible.
 * Après chaque action : pas d'erreur, rien d'injecté, le menu est toujours là,
 * et une fenêtre fermée ne laisse pas le focus perdu en haut de la page.
 * Rejouer : FUZZ_SEED=<graine> npx vitest run src/qa/fuzz/singe
 */
import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { jetonSession } from "../../lib/api";
import { Coquille } from "../../pages/Coquille";
import { rendreConnecte, simulerRust } from "../../test/outils";
import { fauxRust } from "./fauxRust";
import { aleatoire, graine, HOSTILES, injection, surveiller } from "./outils";

const SEED = graine();
const ACTIONS = Number(process.env.FUZZ_ACTIONS) || 200;
/** Ces boutons terminent la session : le singe les évite pour rester dans l'application. */
const EVITER = /déconnecter|Quitter/i;

const visibles = <T extends Element>(selecteur: string) =>
  [...document.querySelectorAll<T>(selecteur)].filter(
    (e) => !(e as unknown as HTMLButtonElement).disabled && !e.closest("[hidden], [aria-hidden=true]"),
  );

beforeEach(() => jetonSession.definir(null));

it(`QA · singe de ${ACTIONS} actions sur toute l'application (FUZZ_SEED=${SEED})`, async () => {
  const h = aleatoire(SEED);
  const faux = fauxRust();
  simulerRust(faux.gestionnaires);
  const veille = surveiller();
  const journal: string[] = [];
  const constats: string[] = [];
  const user = userEvent.setup({ delay: null });
  try {
    rendreConnecte(<Coquille nomHotel="Hôtel du Lac" />);
    await screen.findByRole("navigation", { name: "Menu principal" });
    for (let i = 0; i < ACTIONS; i++) {
      const dialogueAvant = document.querySelector('[role="alertdialog"]');
      const tirage = h.entier(0, 9);
      let action = "";
      if (tirage <= 4) {
        const boutons = visibles<HTMLButtonElement>("button").filter((b) => !EVITER.test(b.textContent ?? ""));
        const b = boutons.length ? h.parmi(boutons) : null;
        if (b) {
          action = `clic « ${(b.textContent || b.getAttribute("aria-label") || "?").trim().slice(0, 40)} »`;
          await user.click(b);
        }
      } else if (tirage <= 6) {
        const champs = visibles<HTMLInputElement>("input:not([type=checkbox]):not([type=radio]), textarea");
        const c = champs.length ? h.parmi(champs) : null;
        if (c) {
          const v = h.parmi(HOSTILES);
          action = `saisie ${JSON.stringify(v.slice(0, 25))} dans ${c.getAttribute("aria-label") || c.id}`;
          fireEvent.change(c, { target: { value: v } });
        }
      } else if (tirage === 7) {
        const touche = h.parmi(["{Escape}", "{Tab}", "{Shift>}{Tab}{/Shift}", "{Enter}"]);
        action = `touche ${touche}`;
        await user.keyboard(touche);
      } else if (tirage === 8) {
        const listes = visibles<HTMLSelectElement>("select");
        const l = listes.length ? h.parmi(listes) : null;
        if (l && l.options.length) {
          const o = h.parmi([...l.options]);
          action = `liste ${l.id} ← ${o.value}`;
          await user.selectOptions(l, o.value);
        }
      } else {
        const cases = visibles<HTMLInputElement>("input[type=checkbox], input[type=radio]");
        const c = cases.length ? h.parmi(cases) : null;
        if (c) {
          action = `case ${c.getAttribute("aria-label") || c.id}`;
          await user.click(c);
        }
      }
      await new Promise((r) => setTimeout(r, 5));
      journal.push(`${i} ${action || "(rien à faire)"}`);
      const ou = `action ${i} (${action}) — rejouer FUZZ_SEED=${SEED}`;
      const injecte = injection();
      if (injecte) constats.push(`${injecte} après ${ou}`);
      if (!screen.queryByRole("navigation", { name: "Menu principal" })) {
        constats.push(`l'application a disparu après ${ou}`);
        break;
      }
      const dialogueApres = document.querySelector('[role="alertdialog"]');
      if (dialogueAvant && !dialogueApres && document.activeElement === document.body) {
        constats.push(`focus perdu en haut de la page à la fermeture d'une fenêtre, ${ou}`);
      }
      if (veille.erreurs.length) {
        constats.push(...veille.erreurs.splice(0).map((e) => `${e} après ${ou}`));
      }
    }
  } finally {
    veille.arreter();
  }
  if (process.env.FUZZ_JOURNAL) console.log(journal.join("\n"));
  if (faux.inconnues.size) constats.push(`commandes non simulées : ${[...faux.inconnues].join(", ")}`);
  expect(constats, `${journal.slice(-15).join("\n")}`).toEqual([]);
}, 120_000);
