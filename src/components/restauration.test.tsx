import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../App";
import type { ApercuSauvegarde } from "../bindings/ApercuSauvegarde";
import type { SauvegardeDisponible } from "../bindings/SauvegardeDisponible";
import { jetonSession } from "../lib/api";
import { FournisseurSession } from "../lib/session";
import { Parametres } from "../pages/Parametres";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Appel, type Gestionnaire } from "../test/outils";

const COPIES: SauvegardeDisponible[] = [
  { chemin: "/s/tanana-sauvegarde-2026-11-14.db", nom: "tanana-sauvegarde-2026-11-14.db", date: new Date().toISOString(), octets: 90_000 },
  {
    chemin: "/s/tanana-avant-restauration-2026-11-10-09h00.db",
    nom: "tanana-avant-restauration-2026-11-10-09h00.db",
    date: "2026-11-10T14:00:00.000Z",
    octets: 80_000,
  },
];

const APERCU: ApercuSauvegarde = {
  nom_hotel: "Hôtel du Lac",
  devise: "XAF",
  derniere_activite: new Date().toISOString(),
  reservations: 12,
  clients: 8,
  comptes: 3,
  journal_intact: true,
};

const CLE_AUTRE = "a".repeat(64);
const DE = (appels: Appel[], c: string) => appels.filter((a) => a.commande === c);
const MAUVAISE_CLE = {
  code: "VALIDATION",
  details: "cette copie ne s'ouvre pas avec cette clé, ou ce n'est pas une copie de sécurité de Tanana",
} as const;

describe("Réglages › remettre une copie de sécurité", () => {
  function simuler(autres: Record<string, Gestionnaire> = {}) {
    return simulerRust({
      parametres: () => PARAMETRES,
      ma_session: () => session(),
      taxes_hotel: () => [],
      etat_distance: () => ({
        active: false,
        relais: null,
        hotel_id: null,
        en_attente: 0,
        active_le: null,
        dernier_envoi_le: null,
        derniere_tentative_le: null,
        derniere_erreur: null,
      }),
      lister_sauvegardes: () => COPIES,
      examiner_sauvegarde: () => APERCU,
      restaurer_sauvegarde: ({ mot_de_passe }) =>
        mot_de_passe === "admin2026"
          ? { copie_avant: "/s/tanana-avant-restauration-x.db", distance_arretee: true }
          : erreurRust({ code: "IDENTIFIANTS_INVALIDES" }),
      deconnexion: () => null,
      ...autres,
    });
  }

  async function ouvrir() {
    rendreConnecte(<Parametres />);
    await userEvent.click(await screen.findByRole("button", { name: "Remettre une copie…" }));
    return within(screen.getByRole("region", { name: "Remettre une copie de sécurité" }));
  }

  it("montre les copies, ce qu'elles contiennent, puis exige le mot de passe", async () => {
    const appels = simuler();
    const zone = await ouvrir();
    expect(await zone.findByRole("button", { name: /^Copie automatique, aujourd'hui à/ })).toBeInTheDocument();
    expect(zone.getByRole("button", { name: /^État d'avant une restauration/ })).toBeInTheDocument();

    await userEvent.click(zone.getByRole("button", { name: /^Copie automatique/ }));
    const apercu = await zone.findByTestId("apercu-copie");
    expect(apercu).toHaveTextContent("Hôtel du Lac");
    expect(apercu).toHaveTextContent("12 réservations, 8 clients, 3 comptes");
    expect(zone.getByText("Tout ce qui a été fait depuis sera remplacé")).toBeInTheDocument();
    expect(DE(appels, "examiner_sauvegarde")[0]?.args).toEqual({
      token: "jeton-test",
      chemin: "/s/tanana-sauvegarde-2026-11-14.db",
      cle: null,
    });

    await userEvent.click(zone.getByRole("button", { name: "Remettre cette copie" }));
    expect(await zone.findByRole("alert")).toHaveTextContent("Retapez votre mot de passe");
    expect(DE(appels, "restaurer_sauvegarde")).toHaveLength(0);

    await userEvent.type(zone.getByLabelText("Pour confirmer, retapez votre mot de passe"), "mauvais99");
    await userEvent.click(zone.getByRole("button", { name: "Remettre cette copie" }));
    expect(await zone.findByRole("alert")).toHaveTextContent("Identifiant ou mot de passe incorrect");

    await userEvent.type(zone.getByLabelText("Pour confirmer, retapez votre mot de passe"), "admin2026");
    await userEvent.click(zone.getByRole("button", { name: "Remettre cette copie" }));
    // Tout le monde doit se reconnecter
    await waitFor(() => expect(DE(appels, "deconnexion")).toHaveLength(1));
    expect(DE(appels, "restaurer_sauvegarde").at(-1)?.args).toEqual({
      token: "jeton-test",
      chemin: "/s/tanana-sauvegarde-2026-11-14.db",
      cle: null,
      mot_de_passe: "admin2026",
    });
  });

  it("une copie d'un autre ordinateur demande sa clé de 64 caractères", async () => {
    const appels = simuler({
      examiner_sauvegarde: ({ cle }) => (cle === CLE_AUTRE ? APERCU : erreurRust(MAUVAISE_CLE)),
      "plugin:dialog|open": () => "/Volumes/USB/tanana-sauvegarde-2026-11-01.db",
    });
    const zone = await ouvrir();
    await userEvent.click(await zone.findByRole("button", { name: "Choisir le fichier…" }));
    expect(await zone.findByText("Cette copie vient d'un autre ordinateur")).toBeInTheDocument();

    await userEvent.type(zone.getByLabelText("Clé de protection (64 caractères)"), "abc");
    await userEvent.click(zone.getByRole("button", { name: "Ouvrir la copie" }));
    expect(await zone.findByRole("alert")).toHaveTextContent("Vous en avez tapé 3");

    await userEvent.clear(zone.getByLabelText("Clé de protection (64 caractères)"));
    await userEvent.type(zone.getByLabelText("Clé de protection (64 caractères)"), CLE_AUTRE);
    await userEvent.click(zone.getByRole("button", { name: "Ouvrir la copie" }));
    expect(await zone.findByTestId("apercu-copie")).toHaveTextContent("Hôtel du Lac");

    await userEvent.type(zone.getByLabelText("Pour confirmer, retapez votre mot de passe"), "admin2026");
    await userEvent.click(zone.getByRole("button", { name: "Remettre cette copie" }));
    await waitFor(() => expect(DE(appels, "restaurer_sauvegarde")).toHaveLength(1));
    expect(DE(appels, "restaurer_sauvegarde")[0]?.args).toMatchObject({ cle: CLE_AUTRE });
  });

  it("explique une copie inutilisable sans rien remplacer", async () => {
    const appels = simuler({
      examiner_sauvegarde: () =>
        erreurRust({ code: "VALIDATION", details: "cette copie est vide : l'hôtel n'y avait pas encore été configuré" }),
    });
    const zone = await ouvrir();
    await userEvent.click(await zone.findByRole("button", { name: /^Copie automatique/ }));
    expect(await zone.findByRole("alert")).toHaveTextContent("Cette copie est vide");
    expect(DE(appels, "restaurer_sauvegarde")).toHaveLength(0);
    await userEvent.click(zone.getByRole("button", { name: "Annuler" }));
    expect(screen.queryByRole("region", { name: "Remettre une copie de sécurité" })).not.toBeInTheDocument();
  });

  it("réservé aux administrateurs", async () => {
    simuler();
    rendreConnecte(<Parametres />, session(["sauvegarde.faire"]));
    expect(await screen.findByRole("button", { name: "Faire une copie maintenant…" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remettre une copie…" })).not.toBeInTheDocument();
  });
});

describe("Ordinateur neuf ou données illisibles", () => {
  beforeEach(() => jetonSession.definir(null));
  const etatApp = (e: object) => ({
    statut: "prete",
    initialise: false,
    consultation_distance: false,
    nom_hotel: null,
    message: null,
    version: "0.1.0",
    ...e,
  });

  function demarrer() {
    render(
      <FournisseurSession>
        <App />
      </FournisseurSession>,
    );
  }

  it("au premier lancement, reprend l'hôtel depuis la clé USB avec la clé sur papier", async () => {
    let remis = false;
    const appels = simulerRust({
      etat_application: () => etatApp(remis ? { initialise: true, nom_hotel: "Hôtel du Lac" } : {}),
      lister_sauvegardes: () => [],
      "plugin:dialog|open": () => "/Volumes/USB/tanana-sauvegarde-2026-11-14.db",
      examiner_sauvegarde: ({ cle }) => (cle === CLE_AUTRE ? APERCU : erreurRust(MAUVAISE_CLE)),
      restaurer_sauvegarde: () => {
        remis = true;
        return { copie_avant: null, distance_arretee: false };
      },
    });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: "Remettre une copie de sécurité" }));
    expect(screen.getByText(/Branchez la clé USB/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Choisir le fichier…" }));
    await userEvent.type(await screen.findByLabelText("Clé de protection (64 caractères)"), CLE_AUTRE);
    await userEvent.click(screen.getByRole("button", { name: "Ouvrir la copie" }));
    expect(await screen.findByTestId("apercu-copie")).toBeInTheDocument();
    expect(screen.queryByLabelText(/retapez votre mot de passe/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Remettre cette copie" }));

    expect(await screen.findByText("L'hôtel est de retour")).toBeInTheDocument();
    expect(DE(appels, "restaurer_sauvegarde")[0]?.args).toEqual({
      token: null,
      chemin: "/Volumes/USB/tanana-sauvegarde-2026-11-14.db",
      cle: CLE_AUTRE,
      mot_de_passe: null,
    });
    await userEvent.click(screen.getByRole("button", { name: "Continuer" }));
    expect(await screen.findByRole("button", { name: "Se connecter" })).toBeInTheDocument();
  });

  it("depuis l'écran de clé manquante, propose les copies de cet ordinateur", async () => {
    let remis = false;
    simulerRust({
      etat_application: () =>
        etatApp(remis ? { initialise: true, nom_hotel: "Hôtel du Lac" } : { statut: "cle_manquante", initialise: true }),
      lister_sauvegardes: () => COPIES,
      examiner_sauvegarde: () => APERCU,
      restaurer_sauvegarde: () => {
        remis = true;
        return { copie_avant: "/s/tanana-illisible-x.db", distance_arretee: false };
      },
    });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: "Remettre une copie de sécurité" }));
    expect(screen.getByText(/ne sont pas effacées : elles sont mises de côté/)).toBeInTheDocument();
    await userEvent.click(await screen.findByRole("button", { name: /^Copie automatique/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Remettre cette copie" }));
    expect(await screen.findByRole("button", { name: "Se connecter" })).toBeInTheDocument();
  });

  it("une erreur grave de la base propose aussi une copie de sécurité", async () => {
    simulerRust({ etat_application: () => etatApp({ statut: "erreur", message: "disque illisible" }), lister_sauvegardes: () => [] });
    demarrer();
    expect(await screen.findByText("Tanana ne peut pas démarrer")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Remettre une copie de sécurité" }));
    expect(await screen.findByRole("button", { name: "Choisir le fichier…" })).toBeInTheDocument();
  });
});
