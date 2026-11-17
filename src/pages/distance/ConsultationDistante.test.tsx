import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../../App";
import type { EtatCopie } from "../../bindings/EtatCopie";
import type { Permission } from "../../bindings/Permission";
import { FournisseurSession } from "../../lib/session";
import { erreurRust, PARAMETRES, session, simulerRust, type Appel, type Gestionnaire } from "../../test/outils";

/** Ce que Rust donne à un poste de consultation (voir PERMISSIONS_DISTANTES). */
const PERMISSIONS_DISTANTES: Permission[] = [
  "chambre.voir",
  "client.voir",
  "reservation.voir",
  "depense.voir",
  "rapport.financier.voir",
  "journal.voir",
  "utilisateur.gerer",
];

const SESSION_DISTANTE = session(PERMISSIONS_DISTANTES, {
  id: "distant",
  identifiant: "distant",
  nom: "Consultation à distance",
  role_id: "distant",
  role_nom: "Consultation à distance",
});

const etatApp = (e: object) => ({
  statut: "prete",
  initialise: false,
  consultation_distance: false,
  nom_hotel: null,
  message: null,
  version: "0.1.0",
  ...e,
});

const COPIE = (e: Partial<EtatCopie> = {}): EtatCopie => ({
  relais: "https://relais.test",
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
const CLE = "ivng i2wv jnba 4r3c lqu7 zi2e si";

describe("Poste du propriétaire : rattacher l'hôtel", () => {
  async function remplir(cle = CLE, mdp = "proprio2026", confirmation = mdp) {
    await userEvent.click(await screen.findByRole("button", { name: "Consulter un hôtel à distance" }));
    const f = within(screen.getByRole("form", { name: "Consulter un hôtel à distance" }));
    await userEvent.type(f.getByLabelText("Adresse du serveur"), "relais.test");
    await userEvent.type(f.getByLabelText("Identifiant de l'hôtel"), "hk7qf29x");
    await userEvent.type(f.getByLabelText("Clé secrète"), cle);
    await userEvent.type(f.getByLabelText("Mot de passe"), mdp);
    await userEvent.type(f.getByLabelText("Retapez le mot de passe"), confirmation);
    await userEvent.click(f.getByRole("button", { name: "Consulter l'hôtel" }));
  }

  it("depuis le premier lancement, vérifie la clé puis ouvre la connexion par mot de passe seul", async () => {
    let rattache = false;
    const appels = simulerRust({
      etat_application: () =>
        etatApp(rattache ? { initialise: true, consultation_distance: true, nom_hotel: "Hôtel du Lac" } : {}),
      consulter_hotel_distant: () => {
        rattache = true;
        return null;
      },
    });
    demarrer();
    await remplir();

    expect(await screen.findByText("Cet ordinateur peut maintenant consulter l'hôtel.")).toBeInTheDocument();
    expect(appelsDe(appels, "consulter_hotel_distant")[0]?.args).toEqual({
      adresse: "https://relais.test",
      hotel_id: "HK7Q-F29X",
      cle_secrete: CLE,
      mot_de_passe: "proprio2026",
    });
    await userEvent.click(screen.getByRole("button", { name: "Continuer" }));

    expect(await screen.findByRole("heading", { name: "Hôtel du Lac" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Identifiant")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Mot de passe")).toBeInTheDocument();
  });

  it("prévient si les données n'ont pas encore pu être téléchargées", async () => {
    simulerRust({
      etat_application: () => etatApp({}),
      consulter_hotel_distant: () => "Impossible de joindre le serveur.",
    });
    demarrer();
    await remplir();
    expect(await screen.findByText("Les données n'ont pas encore pu être téléchargées")).toBeInTheDocument();
    expect(screen.getByText(/Tanana réessaiera en arrière-plan/)).toBeInTheDocument();
  });

  it("compte les caractères de la clé avant d'appeler Rust", async () => {
    const appels = simulerRust({ etat_application: () => etatApp({}) });
    demarrer();
    await remplir("K7QF-29XM-ABCD");
    expect(await screen.findByRole("alert")).toHaveTextContent("Vous en avez tapé 12");
    expect(appelsDe(appels, "consulter_hotel_distant")).toHaveLength(0);
  });

  it("refuse deux mots de passe différents", async () => {
    const appels = simulerRust({ etat_application: () => etatApp({}) });
    demarrer();
    await remplir(CLE, "proprio2026", "proprio2027");
    expect(await screen.findByRole("alert")).toHaveTextContent("ne sont pas identiques");
    expect(appelsDe(appels, "consulter_hotel_distant")).toHaveLength(0);
  });

  it("explique une clé qui ne correspond pas", async () => {
    simulerRust({
      etat_application: () => etatApp({}),
      consulter_hotel_distant: () => erreurRust({ code: "CLE_DISTANCE_INCORRECTE" }),
    });
    demarrer();
    await remplir();
    expect(await screen.findByRole("alert")).toHaveTextContent("La clé secrète ne correspond pas à cet hôtel");
  });

  it("le bouton Retour revient à la création d'hôtel", async () => {
    simulerRust({ etat_application: () => etatApp({}) });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: "Consulter un hôtel à distance" }));
    await userEvent.click(screen.getByRole("button", { name: "Retour" }));
    expect(await screen.findByRole("button", { name: "Créer l'hôtel" })).toBeInTheDocument();
  });
});

describe("Poste du propriétaire : consulter", () => {
  function simulerCopie(autres: Record<string, Gestionnaire> = {}) {
    return simulerRust({
      etat_application: () => etatApp({ initialise: true, consultation_distance: true, nom_hotel: "Hôtel du Lac" }),
      connexion_copie: ({ mot_de_passe }) =>
        mot_de_passe === "proprio2026" ? SESSION_DISTANTE : erreurRust({ code: "IDENTIFIANTS_INVALIDES" }),
      parametres: () => ({ ...PARAMETRES, nom_hotel: "Hôtel du Lac" }),
      ma_session: () => SESSION_DISTANTE,
      etat_copie: () => COPIE(),
      mettre_a_jour_copie: () => 0,
      journal: () => [],
      lister_utilisateurs: () => [],
      lister_roles: () => [],
      sessions_actives: () => [],
      planning: () => [],
      lister_chambres: () => [],
      ...autres,
    });
  }

  async function seConnecter(mdp = "proprio2026") {
    await userEvent.type(await screen.findByLabelText("Mot de passe"), mdp);
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));
  }

  it("refuse un mauvais mot de passe", async () => {
    simulerCopie();
    demarrer();
    await seConnecter("mauvais99");
    expect(await screen.findByRole("alert")).toHaveTextContent("Identifiant ou mot de passe incorrect");
  });

  it("montre l'hôtel en lecture seule, avec la date des informations, et met à jour à l'ouverture", async () => {
    const appels = simulerCopie();
    demarrer();
    await seConnecter();

    const bandeau = await screen.findByRole("region", { name: "Consultation à distance" });
    expect(bandeau).toHaveTextContent("rien ne peut être modifié d'ici");
    await waitFor(() => expect(bandeau).toHaveTextContent(/Informations de l'hôtel du aujourd'hui à/));
    expect(bandeau).toHaveTextContent("Dernier signe de l'hôtel il y a 1 minute.");
    expect(appelsDe(appels, "mettre_a_jour_copie").length).toBeGreaterThanOrEqual(1);

    // Accueil : pas de « Bonsoir Consultation à distance », pas de « 0 connecté » trompeur
    expect(await screen.findByRole("heading", { level: 1, name: /^Bon(jour|soir)$/ })).toBeInTheDocument();
    expect(await screen.findByText("Dernières actions")).toBeInTheDocument();
    expect(screen.queryByText("Personnes connectées")).not.toBeInTheDocument();
    expect(appelsDe(appels, "sessions_actives")).toHaveLength(0);

    const menu = screen.getByRole("navigation", { name: "Menu principal" });
    expect(within(menu).getByRole("button", { name: "Caisse" })).toBeInTheDocument();
    expect(within(menu).queryByRole("button", { name: "Réglages" })).not.toBeInTheDocument();
  });

  it("les employés se consultent sans boutons de modification", async () => {
    simulerCopie({
      lister_utilisateurs: () => [
        {
          ...SESSION_DISTANTE.utilisateur,
          id: "u-marie",
          identifiant: "marie",
          nom: "Marie",
          role_id: "role-reception",
          role_nom: "Réception",
          cree_le: "2026-03-10T14:00:00.000Z",
        },
      ],
      lister_roles: () => [{ id: "role-reception", nom: "Réception", permissions: [], systeme: true }],
      permissions_perso: () => [],
      permissions_utilisateur: () => ["reservation.voir"],
    });
    demarrer();
    await seConnecter();
    await userEvent.click(await screen.findByRole("button", { name: "Employés et accès" }));
    await userEvent.click(await screen.findByRole("button", { name: /Marie/ }));

    expect(await screen.findByText("Rôle : Réception.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Ajouter un employé" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Suspendre" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Réinitialiser le mot de passe" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Rôle")).not.toBeInTheDocument();
    expect(screen.queryAllByRole("combobox")).toHaveLength(0);
  });

  it("prévient quand l'hôtel ne donne plus de nouvelles", async () => {
    simulerCopie({ etat_copie: () => COPIE({ dernier_contact: new Date(Date.now() - 5 * 3600_000).toISOString() }) });
    demarrer();
    await seConnecter();
    expect(await screen.findByText(/L'hôtel n'a pas donné de nouvelles depuis/)).toBeInTheDocument();
    expect(screen.getByText(/éteint, ou Internet est coupé là-bas/)).toBeInTheDocument();
  });

  it("« Mettre à jour » recharge la page ; une erreur réseau s'affiche sans bloquer", async () => {
    let reponse: () => number = () => 0;
    const appels = simulerCopie({ mettre_a_jour_copie: () => reponse() });
    demarrer();
    await seConnecter();
    await screen.findByRole("region", { name: "Consultation à distance" });
    await waitFor(() => expect(appelsDe(appels, "mettre_a_jour_copie")).toHaveLength(1));
    const avant = appelsDe(appels, "journal").length + appelsDe(appels, "sessions_actives").length;

    reponse = () => 2;
    await userEvent.click(screen.getByRole("button", { name: "Mettre à jour" }));
    await waitFor(() =>
      expect(appelsDe(appels, "journal").length + appelsDe(appels, "sessions_actives").length).toBeGreaterThan(avant),
    );

    reponse = () => erreurRust({ code: "RELAIS_INJOIGNABLE", details: "délai dépassé" });
    await userEvent.click(screen.getByRole("button", { name: "Mettre à jour" }));
    expect(await screen.findByText(/Impossible de joindre le serveur de consultation à distance/)).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Menu principal" })).toBeInTheDocument();
  });

  it("dit clairement que l'hôtel a arrêté la consultation à distance", async () => {
    simulerCopie({ mettre_a_jour_copie: () => erreurRust({ code: "HOTEL_INCONNU_DU_RELAIS" }) });
    demarrer();
    await seConnecter();
    expect(await screen.findByText(/L'hôtel a arrêté la consultation à distance/)).toBeInTheDocument();
    expect(screen.getByText(/Celles affichées ici sont celles du dernier envoi/)).toBeInTheDocument();
  });

  it("les nouveautés reçues automatiquement attendent que la personne les demande", async () => {
    const appels = simulerCopie({ mettre_a_jour_copie: () => 3 });
    demarrer();
    await seConnecter();
    const afficher = await screen.findByRole("button", { name: "Afficher les nouveautés" });
    const avant = appelsDe(appels, "journal").length + appelsDe(appels, "sessions_actives").length;
    await userEvent.click(afficher);
    expect(screen.queryByRole("button", { name: "Afficher les nouveautés" })).not.toBeInTheDocument();
    await waitFor(() =>
      expect(appelsDe(appels, "journal").length + appelsDe(appels, "sessions_actives").length).toBeGreaterThan(avant),
    );
  });
});

describe("Poste du propriétaire : mot de passe oublié ou clé perdue", () => {
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

    // « Retour » ne touche à rien
    await userEvent.click(screen.getByRole("button", { name: "Retour" }));
    expect(appelsDe(appels, "oublier_hotel_distant")).toHaveLength(0);

    await userEvent.click(await screen.findByRole("button", { name: "Mot de passe oublié ?" }));
    await userEvent.click(screen.getByRole("button", { name: "Rattacher l'hôtel à nouveau" }));
    expect(await screen.findByRole("button", { name: "Consulter un hôtel à distance" })).toBeInTheDocument();
    expect(appelsDe(appels, "oublier_hotel_distant")).toHaveLength(1);
  });

  it("clé de protection perdue : pas de clé de 64 caractères à chercher, on rattache à nouveau", async () => {
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
    expect(await screen.findByText("Cet ordinateur doit être rattaché à nouveau")).toBeInTheDocument();
    expect(screen.queryByLabelText("Clé (64 caractères)")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Rattacher l'hôtel à nouveau" }));
    expect(await screen.findByRole("button", { name: "Consulter un hôtel à distance" })).toBeInTheDocument();
  });
});
