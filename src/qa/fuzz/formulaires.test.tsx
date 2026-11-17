/**
 * Fuzzing des formulaires de tous les écrans, avec une graine reproductible.
 * Pour chaque écran : ouvrir les formulaires, remplir chaque champ avec une
 * saisie hostile, envoyer (deux fois, plus Entrée), puis vérifier :
 *  - aucune erreur JavaScript ni erreur React ;
 *  - le texte hostile reste du texte (rien d'injecté) ;
 *  - un envoi refusé montre un message (jamais de refus silencieux) ;
 *  - un double envoi n'appelle Rust qu'une seule fois.
 * Rejouer : FUZZ_SEED=<graine> npx vitest run src/qa/fuzz
 */
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { jetonSession } from "../../lib/api";
import { Coquille } from "../../pages/Coquille";
import { rendreConnecte, simulerRust } from "../../test/outils";
import { fauxRust } from "./fauxRust";
import { aleatoire, graine, HOSTILES, injection, surveiller } from "./outils";

const SEED = graine();
const TOURS = Number(process.env.FUZZ_TOURS) || 3;

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
] as const;

/** Boutons qui ouvrent un formulaire ou une fenêtre. */
const OUVRANTS = /^(Nouveau|Nouvelle|Ajouter|Encaisser|Saisir|Rembourser|Modifier|Créer|Changer|Définir|Annuler la réservation)/;

const pause = (ms = 30) => new Promise((r) => setTimeout(r, ms));

async function ouvrirEcran(nom: string) {
  const menu = await screen.findByRole("navigation", { name: "Menu principal" });
  await userEvent.click(within(menu).getByRole("button", { name: nom }));
  await pause(60);
}

function champsSaisissables(racine: ParentNode): (HTMLInputElement | HTMLTextAreaElement)[] {
  return [...racine.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>("input, textarea")].filter(
    (c) => !c.disabled && !c.readOnly && !["checkbox", "radio", "hidden", "file", "button", "submit"].includes(c.type),
  );
}

/** Un refus visible : message d'erreur, champ marqué invalide, ou contrôle natif du navigateur. */
function refusVisible(zone: HTMLElement): boolean {
  if (document.querySelector('[role="alert"], [aria-invalid="true"], [role="alertdialog"]')) return true;
  if (zone instanceof HTMLFormElement && zone.isConnected && !zone.checkValidity()) return true;
  const statut = [...document.querySelectorAll('[role="status"]')].some((s) => s.textContent?.trim());
  return statut;
}

beforeEach(() => jetonSession.definir(null));

describe(`QA · fuzzing des formulaires (FUZZ_SEED=${SEED})`, () => {
  for (const ecran of ECRANS) {
    it(`${ecran} : aucune saisie hostile ne casse l'écran`, async () => {
      const h = aleatoire(SEED ^ ecran.length * 7919);
      const constats: string[] = [];
      for (let tour = 0; tour < TOURS; tour++) {
        const attente = tour % 2 === 1; // un tour sur deux : Rust ne répond jamais (double envoi)
        const faux = fauxRust({ ecrituresEnAttente: attente });
        simulerRust(faux.gestionnaires);
        const veille = surveiller();
        try {
          rendreConnecte(<Coquille nomHotel={h.parmi(HOSTILES) || "Hôtel du Lac"} />);
          await ouvrirEcran(ecran);
          const ouvrants = screen.queryAllByRole("button").filter((b) => OUVRANTS.test(b.textContent?.trim() ?? ""));
          const ouvrant = ouvrants.length ? h.parmi(ouvrants) : null;
          if (ouvrant && !(ouvrant as HTMLButtonElement).disabled) {
            await userEvent.click(ouvrant);
            await pause();
          }
          // Formulaires classiques, et groupes validés par la touche Entrée
          const zones = [...document.querySelectorAll<HTMLElement>("form, [role=group]")].filter(
            (z) => champsSaisissables(z).length > 0 && (z.tagName === "FORM" || !z.parentElement?.closest("form")),
          );
          for (const form of zones) {
            const valeurs: string[] = [];
            for (const champ of champsSaisissables(form)) {
              const v = h.parmi(HOSTILES);
              valeurs.push(`${champ.name || champ.id}=${JSON.stringify(v.slice(0, 30))}`);
              fireEvent.change(champ, { target: { value: v } });
            }
            const boutons = [...form.querySelectorAll<HTMLButtonElement>("button")];
            const envoi =
              form.querySelector<HTMLButtonElement>('button[type="submit"]') ??
              boutons.filter((x) => !/Fermer|Annuler|Retour|Afficher/.test(x.textContent ?? "")).at(-1);
            if (!envoi || envoi.disabled) continue;
            const avant = faux.ecritures.length;
            // Une personne impatiente : clic, re-clic, puis Entrée dans un champ
            await userEvent.click(envoi);
            await userEvent.click(envoi);
            const champ = champsSaisissables(form).find((c) => c instanceof HTMLInputElement && c.isConnected);
            if (champ) {
              champ.focus();
              await userEvent.keyboard("{Enter}{Enter}");
            }
            // Une fenêtre de confirmation compte comme un refus visible : on la valide pour aller au bout
            const confirmation = screen.queryByRole("alertdialog");
            if (confirmation) {
              const oui = within(confirmation).getAllByRole("button").at(-1)!;
              await userEvent.click(oui);
            }
            await pause(60);
            const faites = faux.ecritures.slice(avant);
            const lieu = `${ecran}, tour ${tour}, ${valeurs.join(" ")}`;
            if (attente && new Set(faites).size < faites.length) {
              constats.push(`double envoi (${faites.join(", ")}) — ${lieu}`);
            }
            if (faites.length === 0 && !refusVisible(form)) {
              constats.push(`refus silencieux : rien envoyé, aucun message — ${lieu}`);
            }
          }
          const injecte = injection();
          if (injecte) constats.push(`${injecte} — ${ecran}, tour ${tour}`);
          await userEvent.keyboard("{Escape}");
          await pause();
        } finally {
          veille.arreter();
          cleanup();
        }
        for (const e of veille.erreurs) constats.push(`${e} — ${ecran}, tour ${tour}`);
        if (faux.inconnues.size) constats.push(`commandes non simulées : ${[...faux.inconnues].join(", ")}`);
      }
      expect(constats, `FUZZ_SEED=${SEED}`).toEqual([]);
    }, 60_000);
  }
});

describe("QA · le faux moteur couvre toute l'interface", () => {
  it("chaque écran se charge sans commande inconnue", async () => {
    const faux = fauxRust();
    simulerRust(faux.gestionnaires);
    rendreConnecte(<Coquille nomHotel="Hôtel du Lac" />);
    for (const ecran of ECRANS) await ouvrirEcran(ecran);
    await waitFor(() => expect(screen.getByRole("navigation", { name: "Menu principal" })).toBeInTheDocument());
    expect([...faux.inconnues]).toEqual([]);
  });
});
