/**
 * Tests ajoutés par la passe QA (mutations) : chacun tue un bug réaliste que
 * les tests existants laissaient passer. Voir .qa/mutants.md.
 */
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../App";
import { libelleCopie } from "../components/restauration";
import { jetonSession } from "../lib/api";
import { FournisseurSession } from "../lib/session";
import { Coquille } from "../pages/Coquille";
import { MonCompte } from "../pages/MonCompte";
import { Parametres } from "../pages/Parametres";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Appel } from "../test/outils";

const DE = (appels: Appel[], c: string) => appels.filter((a) => a.commande === c);

describe("QA · restauration", () => {
  it("« Remettre cette copie » ne part qu'une fois, même cliqué deux fois pendant le traitement (M04)", async () => {
    let finir: (v: unknown) => void = () => {};
    const appels = simulerRust({
      parametres: () => PARAMETRES,
      ma_session: () => session(),
      lister_sauvegardes: () => [
        { chemin: "/s/tanana-sauvegarde-1.db", nom: "tanana-sauvegarde-1.db", date: new Date().toISOString(), octets: 1 },
      ],
      examiner_sauvegarde: () => ({
        nom_hotel: "Hôtel du Lac",
        devise: "XAF",
        derniere_activite: null,
        reservations: 0,
        clients: 0,
        comptes: 1,
        journal_intact: true,
      }),
      restaurer_sauvegarde: () => new Promise((r) => (finir = r)),
    });
    rendreConnecte(<Parametres />);
    await userEvent.click(await screen.findByRole("button", { name: "Remettre une copie…" }));
    await userEvent.click(await screen.findByRole("button", { name: /^Copie automatique/ }));
    await userEvent.type(await screen.findByLabelText("Pour confirmer, retapez votre mot de passe"), "admin2026");
    const bouton = screen.getByRole("button", { name: "Remettre cette copie" });
    await userEvent.click(bouton);
    await userEvent.click(bouton);
    await userEvent.click(bouton);
    expect(DE(appels, "restaurer_sauvegarde")).toHaveLength(1);
    finir({ copie_avant: null, distance_arretee: false });
  });

  it("chaque sorte de copie se présente par son nom (M06)", () => {
    const c = (nom: string) => libelleCopie({ chemin: `/s/${nom}`, nom, date: "2026-11-10T14:00:00.000Z", octets: 1 });
    expect(c("tanana-sauvegarde-2026-11-10.db")).toMatch(/^Copie automatique, /);
    expect(c("tanana-avant-restauration-2026-11-10-09h00.db")).toMatch(/^État d'avant une restauration, /);
    expect(c("tanana-avant-mise-a-jour-0.2.0-2026-11-10.db")).toMatch(/^État d'avant une mise à jour, /);
    expect(c("tanana-illisible-2026-11-10.db")).toMatch(/^Données illisibles mises de côté, /);
  });
});

describe("QA · mon compte", () => {
  function simuler() {
    return simulerRust({
      parametres: () => PARAMETRES,
      ma_session: () => session(),
      changer_mon_mot_de_passe: () => null,
      definir_mon_pin: () => null,
    });
  }

  async function changerMotDePasse() {
    const f = within(await screen.findByRole("form", { name: "Changer mon mot de passe" }));
    await userEvent.type(f.getByLabelText("Mot de passe actuel"), "marie2026");
    await userEvent.type(f.getByLabelText("Nouveau mot de passe"), "soleil2027");
    await userEvent.type(f.getByLabelText("Retapez le nouveau mot de passe"), "soleil2027");
    await userEvent.click(f.getByRole("button", { name: "Enregistrer mon mot de passe" }));
    await f.findByText(/Utilisez votre nouveau mot de passe/);
    return f;
  }

  it("après l'enregistrement, aucun mot de passe ne reste dans les champs (M13)", async () => {
    simuler();
    rendreConnecte(<MonCompte />);
    const f = await changerMotDePasse();
    expect(f.getByLabelText("Mot de passe actuel")).toHaveValue("");
    expect(f.getByLabelText("Nouveau mot de passe")).toHaveValue("");
    expect(f.getByLabelText("Retapez le nouveau mot de passe")).toHaveValue("");
  });

  it("« C'est enregistré » disparaît dès qu'on retape, pour ne pas tromper (M12)", async () => {
    simuler();
    rendreConnecte(<MonCompte />);
    const f = await changerMotDePasse();
    await userEvent.type(f.getByLabelText("Mot de passe actuel"), "x");
    expect(f.queryByText(/Utilisez votre nouveau mot de passe/)).not.toBeInTheDocument();
  });

  it("le bouton « Mon compte » est annoncé comme la page courante quand on y est (M15)", async () => {
    simuler();
    rendreConnecte(<Coquille nomHotel="Hôtel Test" />, session(["reservation.voir"]));
    const bouton = await screen.findByRole("button", { name: "Mon compte (mot de passe, PIN)" });
    expect(bouton).not.toHaveAttribute("aria-current");
    await userEvent.click(bouton);
    await screen.findByRole("heading", { name: "Mon compte" });
    expect(screen.getByRole("button", { name: "Mon compte (mot de passe, PIN)" })).toHaveAttribute("aria-current", "page");
  });
});

describe("QA · connexion par PIN", () => {
  beforeEach(() => jetonSession.definir(null));

  it("Entrée sans PIN affiche « Tapez votre code PIN » et le message reste (M14)", async () => {
    const appels = simulerRust({
      etat_application: () => ({
        statut: "prete",
        initialise: true,
        consultation_distance: false,
        nom_hotel: "Hôtel Test",
        message: null,
        version: "0.1.0",
      }),
      connexion_pin: () => erreurRust({ code: "IDENTIFIANTS_INVALIDES" }),
    });
    localStorage.setItem("tanana.dernier-identifiant", "marie");
    render(
      <FournisseurSession>
        <App />
      </FournisseurSession>,
    );
    await userEvent.click(await screen.findByRole("tab", { name: "Code PIN" }));
    await userEvent.keyboard("{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Tapez votre code PIN.");
    await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Tapez votre code PIN."));
    expect(DE(appels, "connexion_pin")).toHaveLength(0);
  });
});
