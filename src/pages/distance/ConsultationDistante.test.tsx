import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import type { EtatCopie } from "../../bindings/EtatCopie";
import type { Permission } from "../../bindings/Permission";
import { definirModeDistant } from "../../lib/api";
import { FournisseurSession } from "../../lib/session";
import { erreurRust, PARAMETRES, session, simulerRust, type Appel, type Gestionnaire } from "../../test/outils";

/** Ce que Rust donne au poste à distance quand l'hôtel n'a pas répondu (voir PERMISSIONS_DISTANTES). */
const PERMISSIONS_DISTANTES: Permission[] = [
  "chambre.voir",
  "client.voir",
  "reservation.voir",
  "depense.voir",
  "rapport.financier.voir",
  "journal.voir",
  "utilisateur.gerer",
];

const SESSION_COPIE = session(PERMISSIONS_DISTANTES, {
  id: "distant",
  identifiant: "distant",
  nom: "Consultation à distance",
  role_id: "distant",
  role_nom: "Lecture seule",
});

/** L'hôtel a répondu : la vraie session de l'administratrice (tous les droits), sur le jeton de la copie. */
const SESSION_HOTEL = session(undefined, { nom: "Hanta", identifiant: "hanta" });

const etatApp = (e: object) => ({
  statut: "prete",
  initialise: false,
  consultation_distance: false,
  poste_remplace: false,
  nom_hotel: null,
  message: null,
  version: "0.2.0",
  ...e,
});

const COPIE = (e: Partial<EtatCopie> = {}): EtatCopie => ({
  relais: "https://relais.monkadran.com",
  hotel_id: "HK7Q-F29X",
  nom_hotel: "Hôtel du Lac",
  donnees_du: new Date(Date.now() - 2 * 60_000).toISOString(),
  dernier_contact: new Date(Date.now() - 60_000).toISOString(),
  mise_a_jour_le: new Date().toISOString(),
  ...e,
});

function demarrer() {
  return render(
    <FournisseurSession>
      <App />
    </FournisseurSession>,
  );
}

const appelsDe = (appels: Appel[], c: string) => appels.filter((a) => a.commande === c);

beforeEach(() => definirModeDistant(false));

async function remplirIdentifiants(titre: string, bouton: string, identifiant = "hanta", mdp = "Madagascar2026") {
  const f = within(await screen.findByRole("form", { name: titre }));
  expect(f.getByText("Serveur : relais.monkadran.com (changer)")).toBeInTheDocument();
  await userEvent.type(f.getByLabelText("Identifiant"), identifiant);
  await userEvent.type(f.getByLabelText("Mot de passe"), mdp);
  await userEvent.click(f.getByRole("button", { name: bouton }));
}

describe("Ordinateur de l'hôtel : retrouver mon hôtel", () => {
  it("vérifie l'identifiant et le mot de passe, prévient, puis installe l'hôtel", async () => {
    let installe = false;
    const appels = simulerRust({
      etat_application: () => etatApp(installe ? { initialise: true, nom_hotel: "Hôtel du Lac" } : {}),
      retrouver_hotel: ({ mot_de_passe }) =>
        mot_de_passe === "Madagascar2026"
          ? { copie_du: "2026-11-20T09:30:00Z" }
          : erreurRust({ code: "IDENTIFIANTS_INVALIDES" }),
      confirmer_reprise: () => {
        installe = true;
        return null;
      },
    });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: /Retrouver mon hôtel/ }));
    await remplirIdentifiants("Retrouver mon hôtel", "Retrouver mon hôtel", "hanta", "mauvais");
    expect(await screen.findByRole("alert")).toHaveTextContent("Identifiant ou mot de passe incorrect");

    await userEvent.clear(screen.getByLabelText("Mot de passe"));
    await userEvent.type(screen.getByLabelText("Mot de passe"), "Madagascar2026");
    await userEvent.click(screen.getByRole("button", { name: "Retrouver mon hôtel" }));
    expect(await screen.findByText("Votre hôtel a été trouvé")).toBeInTheDocument();
    expect(screen.getByText(/ouvrez-y Tanana une minute avec Internet/)).toBeInTheDocument();
    expect(appelsDe(appels, "retrouver_hotel").at(-1)?.args).toEqual({
      adresse: "https://relais.monkadran.com",
      identifiant: "hanta",
      mot_de_passe: "Madagascar2026",
    });
    expect(appelsDe(appels, "confirmer_reprise")).toHaveLength(0);

    await userEvent.click(screen.getByRole("button", { name: "Installer l'hôtel sur cet ordinateur" }));
    expect(await screen.findByText("Votre hôtel est sur cet ordinateur")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Continuer" }));
    expect(await screen.findByRole("button", { name: "Se connecter" })).toBeInTheDocument();
  });

  it("explique quand l'autre ordinateur n'a encore rien envoyé", async () => {
    simulerRust({
      etat_application: () => etatApp({}),
      retrouver_hotel: () => erreurRust({ code: "INSTANTANE_ABSENT" }),
    });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: /Retrouver mon hôtel/ }));
    await remplirIdentifiants("Retrouver mon hôtel", "Retrouver mon hôtel");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Ouvrez Tanana quelques minutes avec Internet sur l'ordinateur où vous avez créé l'hôtel",
    );
  });
});

describe("Poste à distance : relier cet ordinateur", () => {
  it("identifiant et mot de passe suffisent, puis connexion par mot de passe", async () => {
    let relie = false;
    const appels = simulerRust({
      etat_application: () =>
        etatApp(relie ? { initialise: true, consultation_distance: true, nom_hotel: "Hôtel du Lac" } : {}),
      travailler_a_distance: () => {
        relie = true;
        return "Impossible de joindre le serveur.";
      },
    });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: /Travailler à distance/ }));
    await remplirIdentifiants("Travailler à distance", "Relier cet ordinateur à l'hôtel");
    expect(await screen.findByText("C'est prêt")).toBeInTheDocument();
    expect(screen.getByText(/n'ont pas encore pu être téléchargées/)).toBeInTheDocument();
    expect(appelsDe(appels, "travailler_a_distance")[0]?.args).toEqual({
      adresse: "https://relais.monkadran.com",
      identifiant: "hanta",
      mot_de_passe: "Madagascar2026",
    });
    await userEvent.click(screen.getByRole("button", { name: "Continuer" }));
    expect(await screen.findByText("Travail à distance", { selector: "p" })).toBeInTheDocument();
    expect(screen.getByLabelText("Mot de passe")).toBeInTheDocument();
    expect(screen.queryByLabelText("Identifiant")).not.toBeInTheDocument();
  });

  it("demande l'identifiant avant d'appeler Rust, et l'adresse peut être changée", async () => {
    const appels = simulerRust({ etat_application: () => etatApp({}), travailler_a_distance: () => null });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: /Travailler à distance/ }));
    const f = within(await screen.findByRole("form", { name: "Travailler à distance" }));
    await userEvent.click(f.getByRole("button", { name: "Relier cet ordinateur à l'hôtel" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Écrivez votre identifiant");
    await userEvent.click(f.getByRole("button", { name: /\(changer\)/ }));
    const adresse = f.getByLabelText("Adresse du serveur");
    await userEvent.clear(adresse);
    await userEvent.type(adresse, "relais.test/");
    await userEvent.type(f.getByLabelText("Identifiant"), "hanta");
    await userEvent.type(f.getByLabelText("Mot de passe"), "Madagascar2026");
    await userEvent.click(f.getByRole("button", { name: "Relier cet ordinateur à l'hôtel" }));
    await waitFor(() =>
      expect(appelsDe(appels, "travailler_a_distance")[0]?.args).toMatchObject({ adresse: "https://relais.test" }),
    );
  });
});

describe("Poste à distance : travailler", () => {
  function simulerPoste(modifications: boolean, autres: Record<string, Gestionnaire> = {}) {
    const lien = { possible: modifications };
    const appels = simulerRust({
      etat_application: () => etatApp({ initialise: true, consultation_distance: true, nom_hotel: "Hôtel du Lac" }),
      connexion_distante: ({ mot_de_passe }) =>
        mot_de_passe === "Madagascar2026"
          ? {
              session: modifications ? SESSION_HOTEL : SESSION_COPIE,
              modifications_possibles: modifications,
              raison: modifications ? null : "L'ordinateur de l'hôtel n'est pas joignable",
            }
          : erreurRust({ code: "IDENTIFIANTS_INVALIDES" }),
      parametres: () => ({ ...PARAMETRES, nom_hotel: "Hôtel du Lac" }),
      ma_session: () => (lien.possible ? SESSION_HOTEL : SESSION_COPIE),
      lien_hotel: () => ({ modifications_possibles: lien.possible, session_hotel: lien.possible ? SESSION_HOTEL : null }),
      etat_copie: () => COPIE(),
      mettre_a_jour_copie: () => 0,
      journal: () => [],
      lister_utilisateurs: () => [],
      lister_roles: () => [],
      sessions_actives: () => [],
      planning: () => [],
      lister_chambres: () => [],
      rechercher_clients: () => [],
      ...autres,
    });
    return { appels, lien };
  }

  async function seConnecter(mdp = "Madagascar2026") {
    await userEvent.type(await screen.findByLabelText("Mot de passe"), mdp);
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));
  }

  it("refuse un mauvais mot de passe", async () => {
    simulerPoste(true);
    demarrer();
    await seConnecter("mauvais99");
    expect(await screen.findByRole("alert")).toHaveTextContent("Identifiant ou mot de passe incorrect");
  });

  it("hôtel joignable : les modifications partent à l'hôtel, les lectures restent sur la copie", async () => {
    const { appels } = simulerPoste(true, {
      commande_distante: ({ commande, args }) =>
        commande === "creer_client"
          ? { id: "cl9", ...(args as { donnees: object }).donnees, cree_le: "2026-11-23T10:00:00Z" }
          : erreurRust({ code: "BASE", details: "inattendu" }),
      obtenir_client: () => ({ id: "cl9", nom: "Andry", prenom: null, telephone: null, email: null, piece_identite: null, notes: null, cree_le: "2026-11-23T10:00:00Z" }),
      reservations_du_client: () => [],
    });
    demarrer();
    await seConnecter();
    const bandeau = await screen.findByRole("region", { name: "Travail à distance" });
    expect(bandeau).toHaveTextContent("vos changements partent directement à l'hôtel");
    await waitFor(() => expect(bandeau).toHaveTextContent(/Informations de l'hôtel du aujourd'hui à/));

    // Les réglages et « Mon compte » sont là : ce sont ses vrais droits d'administratrice
    const menu = screen.getByRole("navigation", { name: "Menu principal" });
    expect(within(menu).getByRole("button", { name: "Réglages" })).toBeInTheDocument();

    await userEvent.click(within(menu).getByRole("button", { name: "Clients" }));
    await userEvent.click(await screen.findByRole("button", { name: "Nouveau client" }));
    await userEvent.type(screen.getByLabelText("Nom"), "Andry");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer le client" }));
    await waitFor(() => expect(appelsDe(appels, "commande_distante")).toHaveLength(1));
    expect(appelsDe(appels, "commande_distante")[0]?.args).toMatchObject({
      token: "jeton-test",
      commande: "creer_client",
      args: { donnees: expect.objectContaining({ nom: "Andry" }) },
    });
    expect(appelsDe(appels, "creer_client")).toHaveLength(0);
    expect(appelsDe(appels, "rechercher_clients").length).toBeGreaterThan(0);
  });

  it("une modification refusée faute d'hôtel joignable s'explique simplement", async () => {
    simulerPoste(true, {
      commande_distante: () => erreurRust({ code: "HOTEL_INJOIGNABLE" }),
    });
    demarrer();
    await seConnecter();
    await userEvent.click(await screen.findByRole("button", { name: "Clients" }));
    await userEvent.click(await screen.findByRole("button", { name: "Nouveau client" }));
    await userEvent.type(screen.getByLabelText("Nom"), "Andry");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer le client" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("L'ordinateur de l'hôtel n'est pas joignable");
    expect(screen.getByRole("alert")).toHaveTextContent("Rien n'a été modifié");
  });

  it("hôtel injoignable : consultation seulement, puis les modifications reviennent toutes seules", async () => {
    const { lien } = simulerPoste(false);
    demarrer();
    await seConnecter();
    const bandeau = await screen.findByRole("region", { name: "Travail à distance" });
    expect(bandeau).toHaveTextContent("l'hôtel n'est pas joignable, consultation seulement");
    const menu = screen.getByRole("navigation", { name: "Menu principal" });
    expect(within(menu).queryByRole("button", { name: "Réglages" })).not.toBeInTheDocument();
    await userEvent.click(within(menu).getByRole("button", { name: "Clients" }));
    expect(screen.queryByRole("button", { name: "Nouveau client" })).not.toBeInTheDocument();

    // L'hôtel revient en ligne
    lien.possible = true;
    await userEvent.click(screen.getByRole("button", { name: "Mettre à jour" }));
    await waitFor(() => expect(bandeau).toHaveTextContent("vos changements partent directement à l'hôtel"));
    expect(await screen.findByRole("button", { name: "Nouveau client" })).toBeInTheDocument();
  });

  it("prévient quand l'hôtel ne donne plus de nouvelles", async () => {
    simulerPoste(false, { etat_copie: () => COPIE({ dernier_contact: new Date(Date.now() - 5 * 3600_000).toISOString() }) });
    demarrer();
    await seConnecter();
    expect(await screen.findByText(/L'hôtel n'a pas donné de nouvelles depuis/)).toBeInTheDocument();
    expect(screen.getByText(/éteint, ou Internet est coupé là-bas/)).toBeInTheDocument();
  });

  it("« Mettre à jour » recharge la page ; une erreur réseau s'affiche sans bloquer", async () => {
    let reponse: () => number = () => 0;
    const { appels } = simulerPoste(true, { mettre_a_jour_copie: () => reponse() });
    demarrer();
    await seConnecter();
    await screen.findByRole("region", { name: "Travail à distance" });
    const avant = appelsDe(appels, "journal").length + appelsDe(appels, "sessions_actives").length;
    reponse = () => 2;
    await userEvent.click(screen.getByRole("button", { name: "Mettre à jour" }));
    await waitFor(() =>
      expect(appelsDe(appels, "journal").length + appelsDe(appels, "sessions_actives").length).toBeGreaterThan(avant),
    );
    reponse = () => erreurRust({ code: "RELAIS_INJOIGNABLE", details: "délai dépassé" });
    await userEvent.click(screen.getByRole("button", { name: "Mettre à jour" }));
    expect(await screen.findByText(/Impossible de joindre le serveur de Tanana/)).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Menu principal" })).toBeInTheDocument();
  });

  it("les nouveautés reçues automatiquement attendent que la personne les demande", async () => {
    let donnees = COPIE().donnees_du;
    const { appels } = simulerPoste(true, { etat_copie: () => COPIE({ donnees_du: donnees }) });
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      demarrer();
      await seConnecter();
      await screen.findByRole("region", { name: "Travail à distance" });
      donnees = new Date().toISOString();
      await vi.advanceTimersByTimeAsync(16_000);
      const afficher = await screen.findByRole("button", { name: "Afficher les nouveautés" });
      const avant = appelsDe(appels, "journal").length + appelsDe(appels, "sessions_actives").length;
      await userEvent.click(afficher);
      expect(screen.queryByRole("button", { name: "Afficher les nouveautés" })).not.toBeInTheDocument();
      await waitFor(() =>
        expect(appelsDe(appels, "journal").length + appelsDe(appels, "sessions_actives").length).toBeGreaterThan(avant),
      );
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("Ancien ordinateur principal", () => {
  it("apprend qu'un autre ordinateur a repris l'hôtel et passe en travail à distance", async () => {
    let remplace = true;
    const appels = simulerRust({
      etat_application: () =>
        etatApp(remplace ? { initialise: true, poste_remplace: true, nom_hotel: "Hôtel du Lac" } : {}),
      abandonner_poste: () => {
        remplace = false;
        return "/Users/hanta/Library/sauvegardes/tanana-ancien-ordinateur-principal.db";
      },
    });
    demarrer();
    expect(await screen.findByText("L'hôtel est maintenant sur un autre ordinateur")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Se connecter" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Passer en travail à distance" }));
    expect(await screen.findByRole("button", { name: /Travailler à distance/ })).toBeInTheDocument();
    expect(appelsDe(appels, "abandonner_poste")).toHaveLength(1);
  });
});

describe("Poste à distance : mot de passe oublié ou clé perdue", () => {
  it("mot de passe oublié : l'hôtel est oublié, puis on revient au premier lancement", async () => {
    let copie = true;
    const appels = simulerRust({
      etat_application: () =>
        etatApp(copie ? { initialise: true, consultation_distance: true, nom_hotel: "Hôtel du Lac" } : {}),
      oublier_hotel_distant: () => {
        copie = false;
        return null;
      },
    });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: "Mot de passe oublié ?" }));
    expect(screen.getByText(/rien n'est perdu/i)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retour" }));
    expect(appelsDe(appels, "oublier_hotel_distant")).toHaveLength(0);

    await userEvent.click(await screen.findByRole("button", { name: "Mot de passe oublié ?" }));
    await userEvent.click(screen.getByRole("button", { name: "Relier à nouveau" }));
    expect(await screen.findByRole("button", { name: /Travailler à distance/ })).toBeInTheDocument();
    expect(appelsDe(appels, "oublier_hotel_distant")).toHaveLength(1);
  });

  it("clé de protection perdue : pas de clé de 64 caractères à chercher, on relie à nouveau", async () => {
    let copie = true;
    simulerRust({
      etat_application: () =>
        etatApp(copie ? { statut: "cle_manquante", initialise: true, consultation_distance: true } : {}),
      oublier_hotel_distant: () => {
        copie = false;
        return null;
      },
    });
    demarrer();
    expect(await screen.findByText("Cet ordinateur doit être relié à nouveau")).toBeInTheDocument();
    expect(screen.queryByLabelText("Clé (64 caractères)")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Relier à nouveau" }));
    expect(await screen.findByRole("button", { name: /Travailler à distance/ })).toBeInTheDocument();
  });
});
