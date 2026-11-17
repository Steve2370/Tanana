import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Depense } from "../../bindings/Depense";
import type { PaiementDetaille } from "../../bindings/PaiementDetaille";
import type { RapportCaisse } from "../../bindings/RapportCaisse";
import { ajouterJours, debutMois, debutMoisSuivant, debutSemaine, formatJourLong } from "../../lib/dates";
import { aujourdhui, decalageLocal } from "../../lib/format";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Gestionnaire } from "../../test/outils";
import { documentEnregistre } from "../../test/reservations";
import { Caisse, calculerPeriode } from "./Caisse";

const J = aujourdhui();

function depense(p: Partial<Depense> = {}): Depense {
  return { id: "d1", categorie: "Eau", montant: 8_000, description: "Facture d'octobre", date: J, utilisateur_nom: "Paul", annulee: false, motif_annulation: null, ...p };
}

function paiement(p: Partial<PaiementDetaille> = {}): PaiementDetaille {
  return {
    id: "p1",
    reservation_id: "r1",
    client_nom: "Mbarga Aline",
    chambre_numero: "101",
    montant: 30_000,
    type_paiement: "acompte",
    mode: "mobile",
    date: new Date().toISOString(),
    utilisateur_nom: "Marie",
    note: null,
    ...p,
  };
}

const RAPPORT: RapportCaisse = {
  debut: J,
  fin: ajouterJours(J, 1),
  encaissements: 80_000,
  encaissements_par_mode: [
    { mode: "especes", montant: 50_000 },
    { mode: "mobile", montant: 30_000 },
  ],
  remboursements: 5_000,
  depenses: 8_000,
  net: 67_000,
};

const PAIEMENTS = [
  paiement({ id: "p3", type_paiement: "remboursement", montant: 5_000, utilisateur_nom: "Beltran", note: "geste commercial" }),
  paiement({ id: "p2", type_paiement: "solde", montant: 50_000, mode: "especes", client_nom: "Ngono Paul", chambre_numero: "102", utilisateur_nom: "Beltran" }),
  paiement(),
];

/** Petit faux serveur dont les dépenses évoluent. */
function simuler(extra: Record<string, Gestionnaire> = {}, depensesInitiales: Depense[] = [depense()]) {
  const etat = { depenses: depensesInitiales };
  const appels = simulerRust({
    parametres: () => PARAMETRES,
    rapport_caisse: () => RAPPORT,
    lister_paiements: () => PAIEMENTS,
    lister_depenses: () => etat.depenses,
    saisir_depense: ({ donnees }) => {
      const d = depense({ ...(donnees as Partial<Depense>), id: `d${etat.depenses.length + 1}`, utilisateur_nom: "Beltran" });
      etat.depenses = [d, ...etat.depenses];
      return d;
    },
    annuler_depense: ({ id, motif }) => {
      etat.depenses = etat.depenses.map((d) => (d.id === id ? { ...d, annulee: true, motif_annulation: motif as string } : d));
      return etat.depenses.find((d) => d.id === id);
    },
    ...extra,
  });
  return { appels, etat };
}

const derniers = (appels: { commande: string; args: Record<string, unknown> }[], commande: string) =>
  appels.filter((a) => a.commande === commande).at(-1)?.args;
const total = (libelle: string) => within(screen.getByRole("group", { name: "Totaux de la période" })).getByText(libelle).parentElement!;
const confirmer = (nom: string) => userEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: nom }));

describe("calcul des périodes", () => {
  it("un seul jour : aujourd'hui, hier, ou un jour choisi", () => {
    expect(calculerPeriode("aujourdhui", "2026-01-01", "2026-11-13")).toEqual({ debut: "2026-11-13", fin: "2026-11-14", titre: "vendredi 13 novembre 2026" });
    expect(calculerPeriode("hier", "2026-01-01", "2026-11-01")).toMatchObject({ debut: "2026-10-31", fin: "2026-11-01" });
    expect(calculerPeriode("jour", "2026-03-05", "2026-11-13")).toMatchObject({ debut: "2026-03-05", fin: "2026-03-06" });
  });

  it("la semaine va du lundi au dimanche", () => {
    expect(calculerPeriode("semaine", "", "2026-11-13")).toEqual({
      debut: "2026-11-09",
      fin: "2026-11-16",
      titre: "Du lundi 9 novembre au dimanche 15 novembre 2026",
    });
    expect(debutSemaine("2026-11-09")).toBe("2026-11-09"); // un lundi
    expect(debutSemaine("2026-11-15")).toBe("2026-11-09"); // un dimanche
    expect(debutSemaine("2027-01-01")).toBe("2026-12-28"); // à cheval sur deux années
  });

  it("le mois va du 1er au dernier jour, y compris en décembre", () => {
    expect(calculerPeriode("mois", "", "2026-11-13")).toMatchObject({ debut: "2026-11-01", fin: "2026-12-01", titre: "Du dimanche 1 novembre au lundi 30 novembre 2026" });
    expect(debutMois("2026-12-25")).toBe("2026-12-01");
    expect(debutMoisSuivant("2026-12-25")).toBe("2027-01-01");
    expect(debutMoisSuivant("2028-02-29")).toBe("2028-03-01");
  });
});

describe("Caisse : totaux et paiements", () => {
  it("affiche les totaux du jour, le détail par moyen de paiement et par employé", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />);
    expect(await screen.findByRole("heading", { name: formatJourLong(J) })).toBeInTheDocument();
    await waitFor(() => expect(total("Argent entré")).toHaveTextContent("80 000 FCFA"));
    expect(total("Rendu aux clients")).toHaveTextContent("5 000 FCFA");
    expect(total("Dépenses")).toHaveTextContent("8 000 FCFA");
    expect(total("Entrées moins sorties")).toHaveTextContent("67 000 FCFA");

    const periode = { debut: J, fin: ajouterJours(J, 1), decalage_utc_minutes: decalageLocal() };
    expect(derniers(appels, "rapport_caisse")).toMatchObject(periode);
    expect(derniers(appels, "lister_paiements")).toMatchObject(periode);
    expect(derniers(appels, "lister_depenses")).toMatchObject({ debut: J, fin: ajouterJours(J, 1) });

    const modes = screen.getByRole("list", { name: "Argent entré par moyen de paiement" });
    expect(modes).toHaveTextContent("Espèces : 50 000 FCFA");
    expect(modes).toHaveTextContent("Mobile money : 30 000 FCFA");

    const lignes = within(screen.getByRole("list", { name: "Paiements des clients" })).getAllByRole("listitem");
    expect(lignes).toHaveLength(3);
    expect(lignes[0]).toHaveTextContent(/Mbarga Aline · chambre 101.*Remboursement · Mobile money · aujourd'hui à \d+ h \d\d · par Beltran · « geste commercial »/);
    expect(lignes[0]).toHaveTextContent("− 5 000 FCFA");
    expect(lignes[1]).toHaveTextContent("Ngono Paul · chambre 102");
    expect(lignes[1]).toHaveTextContent("+ 50 000 FCFA");
    // Les remboursements ne comptent pas dans « encaissé par »
    const parEmploye = screen.getByLabelText("Argent encaissé par employé");
    expect(parEmploye).toHaveTextContent("Encaissé par : Beltran 50 000 FCFA · Marie 30 000 FCFA");
  });

  it("change de période", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />);
    await screen.findByRole("list", { name: "Paiements des clients" });
    expect(screen.getByRole("button", { name: "Aujourd'hui" })).toHaveAttribute("aria-pressed", "true");

    await userEvent.click(screen.getByRole("button", { name: "Hier" }));
    await waitFor(() => expect(derniers(appels, "rapport_caisse")).toMatchObject({ debut: ajouterJours(J, -1), fin: J }));
    expect(screen.getByRole("heading", { name: formatJourLong(ajouterJours(J, -1)) })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Cette semaine" }));
    await waitFor(() => expect(derniers(appels, "lister_depenses")).toMatchObject({ debut: debutSemaine(J), fin: ajouterJours(debutSemaine(J), 7) }));

    await userEvent.click(screen.getByRole("button", { name: "Ce mois" }));
    await waitFor(() => expect(derniers(appels, "lister_paiements")).toMatchObject({ debut: debutMois(J), fin: debutMoisSuivant(J) }));
    expect(screen.getByRole("heading", { level: 2, name: /^Du .* au / })).toBeInTheDocument();
  });

  it("« Un autre jour » laisse choisir la date", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />);
    await screen.findByRole("list", { name: "Paiements des clients" });
    expect(screen.queryByLabelText("Jour à afficher : jour")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Un autre jour" }));
    const [annee, mois] = J.split("-");
    const autre = J.endsWith("-01") ? "02" : "01";
    await userEvent.selectOptions(screen.getByLabelText("Jour à afficher : jour"), String(Number(autre)));
    const choisi = `${annee}-${mois}-${autre}`;
    await waitFor(() => expect(derniers(appels, "rapport_caisse")).toMatchObject({ debut: choisi, fin: ajouterJours(choisi, 1) }));
  });

  it("met en évidence un résultat négatif, et le dit quand il n'y a rien", async () => {
    simuler({
      rapport_caisse: () => ({ ...RAPPORT, encaissements: 0, encaissements_par_mode: [], remboursements: 0, net: -8_000 }),
      lister_paiements: () => [],
    });
    rendreConnecte(<Caisse />);
    expect(await screen.findByText("Aucun paiement sur cette période.")).toBeInTheDocument();
    // Les totaux arrivent de leur côté : on les attend
    await waitFor(() => expect(total("Entrées moins sorties")).toHaveTextContent("-8 000 FCFA"));
    expect(total("Entrées moins sorties").className).toContain("text-danger");
    expect(screen.queryByRole("list", { name: "Argent entré par moyen de paiement" })).not.toBeInTheDocument();
  });

  it("affiche le refus de Rust", async () => {
    simuler({ rapport_caisse: () => erreurRust({ code: "BASE", details: "disque plein" }) });
    rendreConnecte(<Caisse />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Un problème technique est survenu");
  });
});

describe("Caisse : dépenses", () => {
  it("liste les dépenses de la période", async () => {
    simuler();
    rendreConnecte(<Caisse />);
    const ligne = within(await screen.findByRole("list", { name: "Dépenses" })).getByRole("listitem");
    expect(ligne).toHaveTextContent("Eau");
    expect(ligne).toHaveTextContent("Facture d'octobre");
    expect(ligne).toHaveTextContent("par Paul");
    expect(ligne).toHaveTextContent("8 000 FCFA");
  });

  it("ajoute une dépense après vérification et confirmation", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />);
    await userEvent.click(await screen.findByRole("button", { name: "Ajouter une dépense" }));
    const formulaire = screen.getByRole("group", { name: "Nouvelle dépense" });

    // Montant illisible : rien n'est envoyé
    await userEvent.type(screen.getByLabelText("Montant (XAF)"), "cher");
    await userEvent.click(within(formulaire).getByRole("button", { name: "Enregistrer la dépense" }));
    expect(await within(formulaire).findByRole("alert")).toHaveTextContent("Le montant n'est pas lisible");
    expect(appels.some((a) => a.commande === "saisir_depense")).toBe(false);

    await userEvent.selectOptions(screen.getByLabelText("Type de dépense"), "Salaires");
    await userEvent.clear(screen.getByLabelText("Montant (XAF)"));
    await userEvent.type(screen.getByLabelText("Montant (XAF)"), "45 000");
    await userEvent.type(screen.getByLabelText("Détail (facultatif)"), " Veilleur de nuit ");
    await userEvent.click(within(formulaire).getByRole("button", { name: "Enregistrer la dépense" }));

    const dialogue = screen.getByRole("alertdialog");
    expect(dialogue).toHaveTextContent("Enregistrer une dépense de 45 000 FCFA ?");
    expect(dialogue).toHaveTextContent(`Salaires, le ${formatJourLong(J)}.`);
    expect(appels.some((a) => a.commande === "saisir_depense")).toBe(false);
    await confirmer("Oui, enregistrer");

    expect(await screen.findByText(/Dépense de 45\s000\sFCFA enregistrée \(Salaires\)\./)).toBeInTheDocument();
    expect(derniers(appels, "saisir_depense")).toEqual({
      token: "jeton-test",
      donnees: { categorie: "Salaires", montant: 45_000, description: "Veilleur de nuit", date: J },
    });
    // Le formulaire se ferme et la liste est relue
    expect(screen.queryByRole("group", { name: "Nouvelle dépense" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("list", { name: "Dépenses" })).getAllByRole("listitem")).toHaveLength(2);
  });

  it("« Autre » demande de préciser la dépense", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />);
    await userEvent.click(await screen.findByRole("button", { name: "Ajouter une dépense" }));
    expect(screen.queryByLabelText("Précisez la dépense")).not.toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Type de dépense"), "Autre");
    await userEvent.type(screen.getByLabelText("Montant (XAF)"), "2000{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Écrivez de quelle dépense il s'agit.");
    await userEvent.type(screen.getByLabelText("Précisez la dépense"), "Taxi{Enter}");
    await confirmer("Oui, enregistrer");
    await waitFor(() => expect(derniers(appels, "saisir_depense")).toMatchObject({ donnees: { categorie: "Taxi", montant: 2_000, description: null } }));
  });

  it("« Annuler » dans la confirmation n'enregistre rien et garde la saisie", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />);
    await userEvent.click(await screen.findByRole("button", { name: "Ajouter une dépense" }));
    await userEvent.type(screen.getByLabelText("Montant (XAF)"), "80000");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la dépense" }));
    await confirmer("Annuler");
    expect(appels.some((a) => a.commande === "saisir_depense")).toBe(false);
    expect(screen.getByLabelText("Montant (XAF)")).toHaveValue("80000");
  });

  it("annule une dépense avec une raison ; elle reste visible, barrée", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />);
    await userEvent.click(await screen.findByRole("button", { name: /Annuler la dépense Eau de 8\s000\sFCFA/ }));
    const formulaire = screen.getByRole("group", { name: "Annuler la dépense" });
    await userEvent.click(within(formulaire).getByRole("button", { name: "Annuler cette dépense" }));
    expect(await within(formulaire).findByRole("alert")).toHaveTextContent("Écrivez la raison de l'annulation.");

    await userEvent.type(screen.getByLabelText("Raison de l'annulation"), "Un zéro en trop");
    await userEvent.click(within(formulaire).getByRole("button", { name: "Annuler cette dépense" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Elle restera visible, barrée, mais ne comptera plus dans les totaux.");
    expect(appels.some((a) => a.commande === "annuler_depense")).toBe(false);
    await confirmer("Oui, annuler cette dépense");

    expect(await screen.findByText(/La dépense de 8\s000\sFCFA est annulée\./)).toBeInTheDocument();
    expect(derniers(appels, "annuler_depense")).toEqual({ token: "jeton-test", id: "d1", motif: "Un zéro en trop" });
    const ligne = within(screen.getByRole("list", { name: "Dépenses" })).getByRole("listitem");
    expect(ligne).toHaveTextContent("Annulée");
    expect(ligne).toHaveTextContent("raison : Un zéro en trop");
    expect(within(ligne).queryByRole("button")).not.toBeInTheDocument();
    // Les totaux sont relus après l'annulation
    expect(appels.filter((a) => a.commande === "rapport_caisse").length).toBeGreaterThan(1);
  });

  it("« Ne pas annuler » referme sans rien changer", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />);
    await userEvent.click(await screen.findByRole("button", { name: /Annuler la dépense Eau/ }));
    await userEvent.click(screen.getByRole("button", { name: "Ne pas annuler" }));
    expect(screen.queryByLabelText("Raison de l'annulation")).not.toBeInTheDocument();
    expect(appels.some((a) => a.commande === "annuler_depense")).toBe(false);
  });

  it("dit quand il n'y a aucune dépense", async () => {
    simuler({}, []);
    rendreConnecte(<Caisse />);
    expect(await screen.findByText("Aucune dépense sur cette période.")).toBeInTheDocument();
  });
});

describe("Caisse : selon les droits du compte", () => {
  it("avec seulement les dépenses en lecture : pas de totaux, pas de paiements, pas de saisie", async () => {
    const { appels } = simuler({}, [depense(), depense({ id: "d2", montant: 80_000, annulee: true, motif_annulation: "erreur" })]);
    rendreConnecte(<Caisse />, session(["depense.voir"], { role_id: "role-x" }));
    // Le total est calculé sur place, sans la dépense annulée
    expect(await screen.findByRole("heading", { name: /Dépenses : 8\s000\sFCFA/ })).toBeInTheDocument();
    expect(screen.queryByText("Argent entré")).not.toBeInTheDocument();
    expect(screen.queryByText("Paiements des clients")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Ajouter une dépense" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Annuler la dépense/ })).not.toBeInTheDocument();
    expect(appels.some((a) => a.commande === "rapport_caisse" || a.commande === "lister_paiements")).toBe(false);
  });

  it("avec seulement le rapport : pas de carte Dépenses", async () => {
    const { appels } = simuler();
    rendreConnecte(<Caisse />, session(["rapport.financier.voir"], { role_id: "role-x" }));
    await screen.findByRole("list", { name: "Paiements des clients" });
    expect(total("Dépenses")).toHaveTextContent("8 000 FCFA"); // le total reste dans le rapport
    expect(screen.queryByRole("list", { name: "Dépenses" })).not.toBeInTheDocument();
    expect(appels.some((a) => a.commande === "lister_depenses")).toBe(false);
    expect(screen.queryByRole("button", { name: /Imprimer le reçu/ })).not.toBeInTheDocument();
  });
});

describe("Caisse : reçus", () => {
  it("réimprime le reçu d'un paiement de la liste", async () => {
    const { appels } = simuler({
      imprimer_recu: ({ paiement_id }) =>
        documentEnregistre({ numero: "R-2026-0007", paiement_id: paiement_id as string }, { duplicata: true }),
    });
    rendreConnecte(<Caisse />);
    await userEvent.click(await screen.findByRole("button", { name: /^Imprimer le reçu de Ngono Paul : 50\s000\sFCFA$/ }));
    const avis = await screen.findByText("Le reçu R-2026-0007 est ouvert à l'écran.");
    expect(avis.closest('[role="status"]')).toHaveTextContent("cette copie porte la mention « Duplicata »");
    expect(derniers(appels, "imprimer_recu")).toMatchObject({ paiement_id: "p2", decalage_utc_minutes: decalageLocal() });
  });

  it("affiche l'erreur si l'impression échoue", async () => {
    simuler({ imprimer_recu: () => erreurRust({ code: "PERMISSION_REFUSEE", details: { permission: "paiement.encaisser" } }) });
    rendreConnecte(<Caisse />);
    const boutons = await screen.findAllByRole("button", { name: /Imprimer le reçu/ });
    expect(boutons).toHaveLength(3);
    await userEvent.click(boutons[0]!);
    expect(await screen.findByRole("alert")).toHaveTextContent("Votre compte n'a pas le droit de faire cette action.");
  });
});
