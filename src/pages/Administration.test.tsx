import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { EntreeJournal } from "../bindings/EntreeJournal";
import type { Permission } from "../bindings/Permission";
import { rendreConnecte, PARAMETRES, session, simulerRust, utilisateur } from "../test/outils";
import { Coquille } from "./Coquille";
import { Detail } from "./Journal";
import { TableauDeBord } from "./TableauDeBord";
import { Utilisateurs } from "./Utilisateurs";

const entree = (seq: number, action: string, sensible = false): EntreeJournal => ({
  seq,
  date: "2026-03-10T14:00:00.000Z",
  utilisateur_id: "u2",
  utilisateur_nom: "Marie",
  action,
  entite: null,
  entite_id: null,
  avant: null,
  apres: null,
  sensible,
});

describe("Menu selon les permissions", () => {
  const menu = async (permissions: Permission[]) => {
    simulerRust({ parametres: () => PARAMETRES });
    rendreConnecte(<Coquille nomHotel="Hôtel Test" />, session(permissions, { role_id: "role-x" }));
    const nav = await screen.findByRole("navigation", { name: "Menu principal" });
    return within(nav)
      .getAllByRole("button")
      .map((b) => b.textContent);
  };

  it("une réceptionniste ne voit pas l'administration", async () => {
    expect(await menu(["chambre.voir", "reservation.voir", "client.voir"])).toEqual([
      "Accueil",
      "Réservations",
      "Clients",
      "Chambres",
      "BeltranMon compte, code PIN",
      "Se déconnecter",
    ]);
  });

  it("une réceptionniste qui encaisse voit l'onglet Caisse (pour y lire qu'elle n'a pas accès aux montants)", async () => {
    expect(await menu(["chambre.voir", "reservation.voir", "client.voir", "paiement.encaisser"])).toEqual([
      "Accueil",
      "Réservations",
      "Clients",
      "Chambres",
      "Caisse",
      "BeltranMon compte, code PIN",
      "Se déconnecter",
    ]);
  });

  it("un gérant avec les dépenses voit la caisse", async () => {
    expect(await menu(["depense.voir"])).toContain("Caisse");
  });

  it("une permission individuelle suffit à faire apparaître l'entrée", async () => {
    const entrees = await menu(["journal.voir"]);
    expect(entrees).toContain("Historique");
    expect(entrees).not.toContain("Employés et accès");
  });

  it("change d'écran au clic et indique l'écran actif", async () => {
    simulerRust({ parametres: () => PARAMETRES, lister_types_chambre: () => [], lister_chambres: () => [] });
    rendreConnecte(<Coquille nomHotel="Hôtel Test" />, session(["chambre.voir"]));
    await userEvent.click(await screen.findByRole("button", { name: "Chambres" }));
    expect(screen.getByRole("button", { name: "Chambres" })).toHaveAttribute("aria-current", "page");
    expect(await screen.findByRole("heading", { name: "Chambres", level: 1 })).toBeInTheDocument();
  });
});

describe("Accueil administrateur", () => {
  const sessionsActives = [
    { session_id: "s1", utilisateur_id: "u-admin", utilisateur_nom: "Beltran", debut: "2026-03-10T13:00:00.000Z", derniere_activite: "2026-03-10T14:00:00.000Z" },
    { session_id: "s2", utilisateur_id: "u2", utilisateur_nom: "Marie", debut: "2026-03-10T13:00:00.000Z", derniere_activite: "2026-03-10T14:00:00.000Z" },
  ];

  it("affiche l'historique protégé et les actions à vérifier", async () => {
    simulerRust({
      parametres: () => PARAMETRES,
      verifier_integrite_journal: () => ({ intacte: true, entrees_verifiees: 42, premiere_rupture: null }),
      journal: ({ filtre }) =>
        (filtre as { sensibles_seulement: boolean }).sensibles_seulement
          ? [entree(9, "paiement.remboursement", true)]
          : [entree(10, "reservation.creation"), entree(9, "paiement.remboursement", true)],
      sessions_actives: () => sessionsActives,
    });
    rendreConnecte(<TableauDeBord allerA={() => {}} />);
    expect(await screen.findByText("Intact")).toBeInTheDocument();
    expect(screen.getAllByText("Remboursement")).toHaveLength(2);
    expect(screen.getByText("Réservation créée")).toBeInTheDocument();
    expect(screen.getByText("C'est vous")).toBeInTheDocument();
  });

  it("demande confirmation avant de déconnecter quelqu'un", async () => {
    const appels = simulerRust({
      parametres: () => PARAMETRES,
      verifier_integrite_journal: () => ({ intacte: true, entrees_verifiees: 1, premiere_rupture: null }),
      journal: () => [],
      sessions_actives: () => sessionsActives,
      fermer_session: () => null,
    });
    rendreConnecte(<TableauDeBord allerA={() => {}} />);

    // Un seul bouton : on ne peut pas se déconnecter soi-même ici
    const boutons = await screen.findAllByRole("button", { name: "Déconnecter" });
    expect(boutons).toHaveLength(1);

    // Annuler ne fait rien
    await userEvent.click(boutons[0]!);
    const dialogue = screen.getByRole("alertdialog", { name: "Déconnecter Marie ?" });
    expect(within(dialogue).getByRole("button", { name: "Annuler" })).toHaveFocus();
    await userEvent.click(within(dialogue).getByRole("button", { name: "Annuler" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(appels.some((a) => a.commande === "fermer_session")).toBe(false);

    // Échap ferme aussi sans rien faire
    await userEvent.click(screen.getByRole("button", { name: "Déconnecter" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();

    // Confirmer agit
    await userEvent.click(screen.getByRole("button", { name: "Déconnecter" }));
    await userEvent.click(screen.getByRole("button", { name: "Oui, déconnecter" }));
    await waitFor(() => expect(appels.some((a) => a.commande === "fermer_session" && a.args.session_id === "s2")).toBe(true));
  });

  it("alerte clairement quand l'historique a été falsifié", async () => {
    simulerRust({
      parametres: () => PARAMETRES,
      verifier_integrite_journal: () => ({ intacte: false, entrees_verifiees: 7, premiere_rupture: 8 }),
      journal: () => [],
      sessions_actives: () => [],
    });
    rendreConnecte(<TableauDeBord allerA={() => {}} />);
    const alerte = await screen.findByRole("alert");
    expect(alerte).toHaveTextContent("Attention : l'historique a été modifié");
    expect(alerte).toHaveTextContent("n° 8");
    expect(screen.getByText("Modifié !")).toBeInTheDocument();
  });
});

describe("Historique : détail lisible", () => {
  it("montre l'avant et l'après en clair, et met en évidence ce qui a changé", () => {
    render(<Detail avant={{ id: "r1", prix_total: 50000, notes: null }} apres={{ id: "r1", prix_total: 25000, notes: null }} />);
    const ligne = screen.getByText("Prix total").closest("tr")!;
    expect(ligne).toHaveTextContent("Prix total5000025000");
    expect(ligne.className).toContain("bg-or-clair");
    expect(screen.getByText("Notes").closest("tr")!.className).not.toContain("bg-or-clair");
    expect(screen.queryByText("Id")).not.toBeInTheDocument();
  });

  it("une liste s'affiche en clair, et une chambre seule n'a pas de ligne « groupe » vide", () => {
    render(<Detail avant={null} apres={{ chambres: ["101", "102"], solde_groupe: null, groupe: null, solde: 5000 }} />);
    expect(screen.getByText("Chambres").closest("tr")).toHaveTextContent("Chambres101, 102");
    expect(screen.queryByText("Solde groupe")).not.toBeInTheDocument();
    expect(screen.queryByText("Groupe")).not.toBeInTheDocument();
    expect(screen.getByText("Solde")).toBeInTheDocument();
  });

  it("traduit les valeurs techniques en mots", () => {
    render(<Detail avant={{ statut: "actif" }} apres={{ statut: "desactive" }} />);
    expect(screen.getByText("Statut").closest("tr")).toHaveTextContent("Statutactifdésactivé");
  });
});

describe("Employés et accès", () => {
  const marie = utilisateur({ id: "u2", identifiant: "marie", nom: "Marie", role_id: "role-reception", role_nom: "Réceptionniste" });
  const roles = [
    { id: "role-admin", nom: "Administrateur", systeme: true, permissions: [] },
    { id: "role-gerant", nom: "Gérant", systeme: false, permissions: [] },
    { id: "role-reception", nom: "Réceptionniste", systeme: false, permissions: ["chambre.voir"] },
  ];

  function simuler(extra: Record<string, (a: Record<string, unknown>) => unknown> = {}) {
    return simulerRust({
      parametres: () => PARAMETRES,
      lister_utilisateurs: () => [utilisateur(), marie],
      lister_roles: () => roles,
      permissions_perso: () => [],
      permissions_utilisateur: () => ["chambre.voir", "paiement.encaisser"],
      ...extra,
    });
  }

  it("suspend un employé après confirmation", async () => {
    const appels = simuler({ changer_statut_utilisateur: () => ({ ...marie, statut: "suspendu" }) });
    rendreConnecte(<Utilisateurs />);
    await userEvent.click(await screen.findByRole("button", { name: /Marie/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Suspendre" }));
    expect(screen.getByRole("alertdialog", { name: "Suspendre le compte de Marie ?" })).toHaveTextContent("déconnectée tout de suite");
    expect(appels.some((a) => a.commande === "changer_statut_utilisateur")).toBe(false);

    await userEvent.click(screen.getByRole("button", { name: "Oui, suspendre" }));
    expect(await screen.findByText(/Le compte de Marie est suspendu/)).toBeInTheDocument();
    expect(appels.find((a) => a.commande === "changer_statut_utilisateur")?.args).toMatchObject({
      utilisateur_id: "u2",
      statut: "suspendu",
    });
  });

  it("change le rôle seulement après confirmation", async () => {
    const appels = simuler({ changer_role: () => ({ ...marie, role_id: "role-gerant", role_nom: "Gérant" }) });
    rendreConnecte(<Utilisateurs />);
    await userEvent.click(await screen.findByRole("button", { name: /Marie/ }));
    await userEvent.selectOptions(await screen.findByLabelText("Rôle"), "role-gerant");
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Marie deviendra « Gérant »");
    await userEvent.click(screen.getByRole("button", { name: "Annuler" }));
    expect(appels.some((a) => a.commande === "changer_role")).toBe(false);
    expect(screen.getByLabelText("Rôle")).toHaveValue("role-reception");

    await userEvent.selectOptions(screen.getByLabelText("Rôle"), "role-gerant");
    await userEvent.click(screen.getByRole("button", { name: "Oui, changer le rôle" }));
    await waitFor(() => expect(appels.find((a) => a.commande === "changer_role")?.args).toMatchObject({ utilisateur_id: "u2", role_id: "role-gerant" }));
  });

  it("interdit une permission à une seule personne", async () => {
    const appels = simuler({ definir_permission_perso: () => [{ permission: "paiement.encaisser", accorde: false }] });
    rendreConnecte(<Utilisateurs />);
    await userEvent.click(await screen.findByRole("button", { name: /Marie/ }));
    await userEvent.selectOptions(await screen.findByLabelText("Exception pour Encaisser acomptes et soldes"), "false");
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "definir_permission_perso")?.args).toMatchObject({
        utilisateur_id: "u2",
        permission: "paiement.encaisser",
        accorde: false,
      }),
    );
  });

  it("ne propose aucune action sur son propre compte", async () => {
    simuler();
    rendreConnecte(<Utilisateurs />);
    await userEvent.click(await screen.findByRole("button", { name: /Beltran/ }));
    expect(await screen.findByText(/C'est votre propre compte/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Suspendre" })).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Rôle")).not.toBeInTheDocument();
  });

  it("crée un compte avec un mot de passe provisoire valide", async () => {
    const appels = simuler({ creer_utilisateur: () => marie });
    rendreConnecte(<Utilisateurs />);
    await userEvent.click(await screen.findByRole("button", { name: "Ajouter un employé" }));
    await userEvent.type(screen.getByLabelText("Nom complet"), "Paul");
    await userEvent.type(screen.getByLabelText("Identifiant"), "paul");
    await userEvent.type(screen.getByLabelText("Mot de passe provisoire"), "temp");
    await userEvent.click(screen.getByRole("button", { name: "Créer le compte" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("ne respecte pas encore toutes les règles");
    expect(appels.some((a) => a.commande === "creer_utilisateur")).toBe(false);

    await userEvent.type(screen.getByLabelText("Mot de passe provisoire"), "1234");
    await userEvent.click(screen.getByRole("button", { name: "Créer le compte" }));
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "creer_utilisateur")?.args).toMatchObject({
        donnees: { identifiant: "paul", nom: "Paul", mot_de_passe_temporaire: "temp1234", role_id: "role-reception" },
      }),
    );
    // Le formulaire se ferme et la fiche du nouveau compte s'ouvre
    await waitFor(() => expect(screen.queryByRole("button", { name: "Créer le compte" })).not.toBeInTheDocument());
    expect(await screen.findByRole("heading", { name: "Marie" })).toBeInTheDocument();
    expect(screen.getByText("Le compte de Marie est créé.")).toBeInTheDocument();
  });

  it("réinitialise un mot de passe oublié après confirmation", async () => {
    const appels = simuler({ reinitialiser_mot_de_passe: () => null });
    rendreConnecte(<Utilisateurs />);
    await userEvent.click(await screen.findByRole("button", { name: /Marie/ }));
    await userEvent.type(await screen.findByLabelText("Nouveau mot de passe provisoire"), "provisoire9");
    await userEvent.click(screen.getByRole("button", { name: "Réinitialiser le mot de passe" }));
    await userEvent.click(screen.getByRole("button", { name: "Oui, réinitialiser" }));
    expect(await screen.findByText(/Donnez-le à Marie/)).toBeInTheDocument();
    expect(appels.find((a) => a.commande === "reinitialiser_mot_de_passe")?.args).toMatchObject({
      utilisateur_id: "u2",
      mot_de_passe_temporaire: "provisoire9",
    });
  });
});
