import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { EtatDistance } from "../../bindings/EtatDistance";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Appel, type Gestionnaire } from "../../test/outils";
import { ReglagesDistance } from "./ReglagesDistance";

const INACTIF: EtatDistance = {
  active: false,
  relais: null,
  hotel_id: null,
  en_attente: 0,
  active_le: null,
  dernier_envoi_le: null,
  derniere_tentative_le: null,
  derniere_erreur: null,
};

const CODES = { relais: "https://relais.test", hotel_id: "HK7Q-F29X", cle_secrete: "IVNG-I2WV-JNBA-4R3C-LQU7-ZI2E-SI" };

const actif = (e: Partial<EtatDistance> = {}): EtatDistance => ({
  ...INACTIF,
  active: true,
  relais: CODES.relais,
  hotel_id: CODES.hotel_id,
  active_le: new Date().toISOString(),
  dernier_envoi_le: new Date().toISOString(),
  ...e,
});

function simuler(depart: EtatDistance, autres: Record<string, Gestionnaire> = {}) {
  const etat = { e: depart };
  const appels = simulerRust({
    parametres: () => PARAMETRES,
    ma_session: () => session(),
    etat_distance: () => etat.e,
    ...autres,
  });
  return { appels, etat };
}

const appelsDe = (appels: Appel[], c: string) => appels.filter((a) => a.commande === c);

describe("Réglages › Travail à distance (ordinateur de l'hôtel)", () => {
  it("relie l'hôtel au serveur déjà rempli, sans clé à noter, puis montre l'état", async () => {
    const { appels, etat } = simuler(INACTIF, {
      activer_distance: () => {
        etat.e = actif({ dernier_envoi_le: null, en_attente: 1234 });
        return CODES;
      },
    });
    rendreConnecte(<ReglagesDistance />);
    const f = within(await screen.findByRole("form", { name: "Relier l'hôtel au serveur" }));
    expect(f.getByText("Serveur : relais.monkadran.com (changer)")).toBeInTheDocument();
    await userEvent.click(f.getByRole("button", { name: "Relier l'hôtel au serveur" }));

    expect(await screen.findByText(/L'hôtel est relié au serveur/)).toBeInTheDocument();
    expect(screen.queryByTestId("codes-distance")).not.toBeInTheDocument();
    expect(appelsDe(appels, "activer_distance")[0]).toMatchObject({
      args: { token: "jeton-test", adresse: "https://relais.monkadran.com", code_invitation: null },
    });
    expect(await screen.findByText("1234 changements")).toBeInTheDocument();
    expect(screen.getByText("pas encore")).toBeInTheDocument();
  });

  it("un autre serveur peut être choisi ; vide, il est demandé avant d'appeler Rust", async () => {
    const { appels } = simuler(INACTIF, { activer_distance: () => CODES });
    rendreConnecte(<ReglagesDistance />);
    await userEvent.click(await screen.findByRole("button", { name: /\(changer\)/ }));
    const champ = screen.getByLabelText("Adresse du serveur");
    await userEvent.clear(champ);
    await userEvent.click(screen.getByRole("button", { name: "Relier l'hôtel au serveur" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Écrivez l'adresse du serveur");
    expect(appelsDe(appels, "activer_distance")).toHaveLength(0);
    await userEvent.type(champ, " relais.test/ ");
    await userEvent.click(screen.getByRole("button", { name: "Relier l'hôtel au serveur" }));
    await waitFor(() => expect(appelsDe(appels, "activer_distance")[0]?.args).toMatchObject({ adresse: "https://relais.test" }));
  });

  it("explique simplement un refus du serveur et transmet le code d'invitation", async () => {
    const { appels } = simuler(INACTIF, {
      activer_distance: () => erreurRust({ code: "RELAIS_REFUSE", details: "code d'invitation inconnu" }),
    });
    rendreConnecte(<ReglagesDistance />);
    await userEvent.type(await screen.findByLabelText("Code d'invitation (facultatif)"), " BIENVENUE ");
    await userEvent.click(screen.getByRole("button", { name: "Relier l'hôtel au serveur" }));
    const alerte = await screen.findByRole("alert");
    expect(alerte).toHaveTextContent("Le code d'invitation n'est pas accepté par le serveur");
    expect(alerte).not.toHaveTextContent("identifiant"); // l'hôtel n'en a pas encore
    expect(screen.getByLabelText("Code d'invitation (facultatif)")).toHaveValue(" BIENVENUE "); // rien n'est effacé
    expect(appelsDe(appels, "activer_distance")[0]?.args).toMatchObject({ code_invitation: "BIENVENUE" });
  });

  it("rassure quand Internet manque : rien n'est perdu, Tanana réessaie", async () => {
    simuler(actif({ en_attente: 3, derniere_erreur: "Impossible de joindre le serveur." }));
    rendreConnecte(<ReglagesDistance />);
    expect(await screen.findByText("Le dernier envoi n'a pas pu se faire")).toBeInTheDocument();
    expect(screen.getByText(/rien n'est perdu, et Tanana réessaie toute seule/)).toBeInTheDocument();
    expect(screen.getByText("3 changements")).toBeInTheDocument();
  });

  it("« Envoyer maintenant » dit ce qui est parti", async () => {
    let lots = 1;
    simuler(actif(), {
      envoyer_distance: () => (lots-- > 0 ? { lots: 1, changements: 12 } : { lots: 0, changements: 0 }),
    });
    rendreConnecte(<ReglagesDistance />);
    expect(await screen.findByText("Le travail à distance fonctionne.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Envoyer maintenant" }));
    expect(await screen.findByText("C'est envoyé : 12 changements.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Envoyer maintenant" }));
    expect(await screen.findByText(/Rien de nouveau à envoyer/)).toBeInTheDocument();
  });

  it("réaffiche la clé de la page web après le mot de passe", async () => {
    const { appels } = simuler(actif(), {
      afficher_cle_distance: ({ mot_de_passe }) =>
        mot_de_passe === "admin2026" ? CODES : erreurRust({ code: "IDENTIFIANTS_INVALIDES" }),
    });
    rendreConnecte(<ReglagesDistance />);
    await userEvent.click(await screen.findByRole("button", { name: "Clé de la page web" }));
    await userEvent.click(screen.getByRole("button", { name: "Afficher la clé" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Écrivez votre mot de passe.");
    expect(appelsDe(appels, "afficher_cle_distance")).toHaveLength(0);

    await userEvent.type(screen.getByLabelText("Par sécurité, retapez votre mot de passe"), "mauvais");
    await userEvent.click(screen.getByRole("button", { name: "Afficher la clé" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Identifiant ou mot de passe incorrect");

    await userEvent.type(screen.getByLabelText("Par sécurité, retapez votre mot de passe"), "admin2026");
    await userEvent.click(screen.getByRole("button", { name: "Afficher la clé" }));
    expect(await screen.findByTestId("codes-distance")).toHaveTextContent("IVNG-I2WV-JNBA-4R3C-LQU7-ZI2E-SI");
  });

  it("« Tout renvoyer » demande confirmation", async () => {
    const { appels } = simuler(actif(), { renvoyer_tout_distance: () => 540 });
    rendreConnecte(<ReglagesDistance />);
    await userEvent.click(await screen.findByRole("button", { name: "Tout renvoyer" }));
    await userEvent.click(screen.getByRole("button", { name: "Annuler" }));
    expect(appelsDe(appels, "renvoyer_tout_distance")).toHaveLength(0);

    await userEvent.click(screen.getByRole("button", { name: "Tout renvoyer" }));
    await userEvent.click(screen.getByRole("button", { name: "Oui, tout renvoyer" }));
    expect(await screen.findByText(/Envoi complet préparé \(540 informations\)/)).toBeInTheDocument();
  });

  it("arrête, et prévient si le serveur n'a pas pu effacer les données", async () => {
    const { etat } = simuler(actif(), {
      desactiver_distance: () => {
        etat.e = INACTIF;
        return false;
      },
    });
    rendreConnecte(<ReglagesDistance />);
    await userEvent.click(await screen.findByRole("button", { name: "Arrêter" }));
    await userEvent.click(screen.getByRole("button", { name: "Oui, arrêter" }));
    await waitFor(() => expect(screen.getByRole("form", { name: "Relier l'hôtel au serveur" })).toBeInTheDocument());
    expect(screen.getByText(/Le serveur n'a pas pu être joint pour effacer les données/)).toBeInTheDocument();
  });
});
