import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../App";
import { FournisseurSession } from "../lib/session";
import { erreurRust, simulerRust } from "../test/outils";

function demarrer() {
  return render(
    <FournisseurSession>
      <App />
    </FournisseurSession>,
  );
}

const etat = (e: object) => ({ statut: "prete", initialise: false, nom_hotel: null, message: null, version: "0.1.0", ...e });

describe("Premier lancement", () => {
  async function remplir(mdp: string, confirmation: string) {
    await userEvent.type(await screen.findByLabelText("Nom de l'hôtel"), "Hôtel Tanana");
    await userEvent.selectOptions(screen.getByLabelText("Monnaie utilisée"), "CAD");
    await userEvent.type(screen.getByLabelText("Votre nom"), "Beltran");
    await userEvent.type(screen.getByLabelText("Mot de passe"), mdp);
    await userEvent.type(screen.getByLabelText("Retapez le mot de passe"), confirmation);
    await userEvent.click(screen.getByRole("button", { name: "Créer l'hôtel" }));
  }

  it("crée l'hôtel puis montre le code de secours une seule fois", async () => {
    let initialise = false;
    const appels = simulerRust({
      etat_application: () => etat({ initialise, nom_hotel: initialise ? "Hôtel Tanana" : null }),
      initialiser: () => {
        initialise = true;
        return "ABCDE-FGHJK-MNPQR-STUVW";
      },
    });
    demarrer();
    await remplir("admin2026", "admin2026");

    expect(await screen.findByTestId("code-recuperation")).toHaveTextContent("ABCDE-FGHJK-MNPQR-STUVW");
    expect(appels.find((a) => a.commande === "initialiser")?.args).toEqual({
      configuration: {
        nom_hotel: "Hôtel Tanana",
        devise: "CAD",
        decimales_devise: 2,
        admin_identifiant: "admin",
        admin_nom: "Beltran",
        admin_mot_de_passe: "admin2026",
      },
    });

    const continuer = screen.getByRole("button", { name: "Continuer" });
    expect(continuer).toBeDisabled();
    await userEvent.click(screen.getByLabelText("J'ai noté ce code en lieu sûr"));
    await userEvent.click(continuer);
    expect(await screen.findByRole("button", { name: "Se connecter" })).toBeInTheDocument();
  });

  it("refuse des mots de passe différents sans appeler Rust", async () => {
    const appels = simulerRust({ etat_application: () => etat({}) });
    demarrer();
    await remplir("admin2026", "admin2027");
    expect(await screen.findByRole("alert")).toHaveTextContent("ne sont pas identiques");
    expect(appels.some((a) => a.commande === "initialiser")).toBe(false);
  });

  it("montre les règles du mot de passe au fur et à mesure", async () => {
    const appels = simulerRust({ etat_application: () => etat({}) });
    demarrer();
    await remplir("court", "court");
    const regles = screen.getByRole("list", { name: "Règles du mot de passe" });
    expect(regles).toHaveTextContent("Au moins 8 caractères : pas encore");
    expect(regles).toHaveTextContent("Au moins une lettre : respecté");
    expect(regles).toHaveTextContent("Au moins un chiffre : pas encore");
    expect(await screen.findByRole("alert")).toHaveTextContent("ne respecte pas encore toutes les règles");
    expect(appels.some((a) => a.commande === "initialiser")).toBe(false);
  });

  it("affiche le message de Rust s'il refuse quand même", async () => {
    simulerRust({
      etat_application: () => etat({}),
      initialiser: () => erreurRust({ code: "VALIDATION", details: "la devise : code ISO à 3 lettres" }),
    });
    demarrer();
    await remplir("admin2026", "admin2026");
    expect(await screen.findByRole("alert")).toHaveTextContent("La devise : code ISO à 3 lettres.");
  });
});

describe("Démarrage", () => {
  it("demande la clé quand les données ne peuvent pas être ouvertes", async () => {
    let statut = "cle_manquante";
    const appels = simulerRust({
      etat_application: () => etat({ statut, initialise: true }),
      fournir_cle: ({ cle }) => {
        if (cle !== "a".repeat(64)) erreurRust({ code: "VALIDATION", details: "cette clé n'ouvre pas la base de données" });
        statut = "prete";
        return null;
      },
    });
    demarrer();
    const champ = await screen.findByLabelText(/Clé/);
    await userEvent.type(champ, "mauvaise");
    await userEvent.click(screen.getByRole("button", { name: "Ouvrir les données" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Cette clé n'ouvre pas la base de données.");

    await userEvent.clear(champ);
    await userEvent.type(champ, "a".repeat(64));
    await userEvent.click(screen.getByRole("button", { name: "Ouvrir les données" }));
    expect(await screen.findByRole("button", { name: "Se connecter" })).toBeInTheDocument();
    expect(appels.filter((a) => a.commande === "fournir_cle")).toHaveLength(2);
  });

  it("explique une erreur grave au démarrage et propose de réessayer", async () => {
    simulerRust({ etat_application: () => etat({ statut: "erreur", message: "trousseau illisible : accès refusé" }) });
    demarrer();
    const alerte = await screen.findByRole("alert");
    expect(alerte).toHaveTextContent("Tanana ne peut pas démarrer");
    expect(alerte).toHaveTextContent("trousseau illisible");
    expect(screen.getByRole("button", { name: "Réessayer" })).toBeInTheDocument();
  });
});
