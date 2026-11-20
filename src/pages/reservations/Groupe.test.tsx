import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Groupe } from "../../bindings/Groupe";
import type { NouveauGroupe } from "../../bindings/NouveauGroupe";
import type { Paiement } from "../../bindings/Paiement";
import type { Reservation } from "../../bindings/Reservation";
import type { Taxe } from "../../bindings/Taxe";
import { formatJourMoyen, formatJourLong } from "../../lib/dates";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Gestionnaire } from "../../test/outils";
import { CHAMBRES, chambre, detailSimule, j, J, MBARGA, paiement, resa } from "../../test/reservations";
import { FicheReservation } from "./FicheReservation";
import { listeNumeros } from "./NouvelleReservation";
import { Reservations } from "./Reservations";

const C201 = chambre("201", { prix_nuit_defaut: 60_000, type_nom: "Suite" });
const LIBRES = [CHAMBRES[0]!, CHAMBRES[1]!, C201];

/** Trois chambres du même groupe, du jour J+1 au jour J+3 (2 nuits). */
function chambresDuGroupe(): Reservation[] {
  const commun = { client_nom: "Mbarga Aline", date_arrivee: j(1), date_depart: j(3), groupe_id: "g1" };
  return [
    resa({ ...commun, id: "r1", chambre_id: "c101", chambre_numero: "101", prix_total: 50_000 }),
    resa({ ...commun, id: "r2", chambre_id: "c102", chambre_numero: "102", prix_total: 50_000 }),
    resa({ ...commun, id: "r3", chambre_id: "c201", chambre_numero: "201", prix_total: 120_000 }),
  ];
}

/** Faux serveur : le compte se tient au niveau du groupe, comme dans Rust. */
function simulerGroupe(extra: Record<string, Gestionnaire> = {}, paiementsInitiaux: Paiement[] = []) {
  const etat = { chambres: chambresDuGroupe(), paiements: paiementsInitiaux };
  const groupe = (): Groupe => {
    const total = etat.chambres.filter((r) => r.statut !== "annulee").reduce((t, r) => t + r.total_a_payer, 0);
    const paye = etat.paiements.reduce((t, p) => t + (p.type_paiement === "remboursement" ? -p.montant : p.montant), 0);
    return { id: "g1", nom: "Équipe SOGEA", client_id: "cl1", client_nom: "Mbarga Aline", reservations: etat.chambres, total_a_payer: total, total_encaisse: paye, solde: total - paye, cree_le: `${J}T10:00:00.000Z` };
  };
  const appels = simulerRust({
    parametres: () => PARAMETRES,
    obtenir_reservation: ({ id }) => etat.chambres.find((r) => r.id === id),
    obtenir_groupe: () => groupe(),
    paiements_reservation: () => etat.paiements,
    documents_reservation: () => [],
    taxes_reservation: () => detailSimule(50_000, 2, []),
    chambres_disponibles: () => [CHAMBRES[2] ?? CHAMBRES[0]!],
    encaisser: ({ donnees }) => {
      const d = donnees as { montant: number; reservation_id: string };
      const p = paiement({ id: `p${etat.paiements.length + 1}`, montant: d.montant, reservation_id: d.reservation_id });
      etat.paiements = [...etat.paiements, p];
      return p;
    },
    annuler_reservation: ({ id }) => {
      etat.chambres = etat.chambres.map((r) => (r.id === id ? { ...r, statut: "annulee" } : r));
      return etat.chambres.find((r) => r.id === id);
    },
    ajouter_au_groupe: ({ chambre_id, prix_total }) => {
      etat.chambres = [
        ...etat.chambres,
        resa({ id: "r4", chambre_id: chambre_id as string, chambre_numero: "103", date_arrivee: j(1), date_depart: j(3), groupe_id: "g1", prix_total: prix_total as number }),
      ];
      return groupe();
    },
    ...extra,
  });
  return { appels, etat };
}

const confirmer = (nom: string) => userEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: nom }));
const chiffre = (libelle: string) => screen.getByText(libelle).parentElement!;

async function ouvrirFiche(id = "r2", onOuvrir?: (id: string) => void) {
  rendreConnecte(<FicheReservation id={id} onRetour={() => {}} onOuvrir={onOuvrir} />);
  await screen.findByRole("heading", { name: "Mbarga Aline", level: 1 });
  await screen.findByRole("list", { name: "Chambres du groupe" });
}

describe("Nouvelle réservation : plusieurs chambres pour un client", () => {
  function simulerCreation(extra: Record<string, Gestionnaire> = {}) {
    const { appels, etat } = simulerGroupe({
      lister_chambres: () => LIBRES,
      planning: () => [],
      chambres_disponibles: () => LIBRES,
      rechercher_clients: () => [MBARGA],
      creer_groupe: ({ donnees }) => {
        const d = donnees as NouveauGroupe;
        return { id: "g1", nom: d.nom, client_id: "cl1", client_nom: "Mbarga Aline", reservations: etat.chambres, total_a_payer: 220_000, total_encaisse: 0, solde: 220_000, cree_le: "" };
      },
      ...extra,
    });
    return appels;
  }

  async function ouvrirFormulaire() {
    rendreConnecte(<Reservations />);
    await userEvent.click(await screen.findByRole("button", { name: "Nouvelle réservation" }));
    await screen.findByRole("heading", { name: "Nouvelle réservation" });
  }

  async function cocherEtChoisirClient(...numeros: string[]) {
    for (const n of numeros) await userEvent.click(await screen.findByRole("checkbox", { name: new RegExp(`Chambre ${n}`) }));
    await userEvent.type(screen.getByLabelText("Chercher un client déjà venu"), "mba");
    await userEvent.click(await screen.findByRole("button", { name: /Mbarga Aline/ }));
  }

  it("deux chambres cochées : un groupe, une seule facture, puis la fiche du groupe s'ouvre", async () => {
    const appels = simulerCreation();
    await ouvrirFormulaire();
    await userEvent.click(screen.getByRole("button", { name: "Une nuit de plus" })); // 2 nuits
    await cocherEtChoisirClient("101", "201");

    expect(screen.getByText("2 chambres pour le même client : 101 et 201.")).toBeInTheDocument();
    const prix = screen.getByRole("list", { name: "Prix par chambre" });
    expect(prix).toHaveTextContent(/Chambre 101 : 2 nuits × 25\s000\sFCFA50\s000\sFCFA/);
    expect(prix).toHaveTextContent(/Chambre 201 : 2 nuits × 60\s000\sFCFA120\s000\sFCFA/);
    expect(screen.getByText("Total des 2 chambres").parentElement).toHaveTextContent(/170\s000\sFCFA/);
    expect(screen.getByText(/Mbarga Aline — chambres 101 et 201 —/)).toHaveTextContent(`du ${formatJourMoyen(J)} au ${formatJourLong(j(2))} (2 nuits)`);

    await userEvent.type(screen.getByLabelText("Nom du groupe (facultatif)"), " Équipe SOGEA ");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les 2 chambres" }));
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "creer_groupe")?.args.donnees).toEqual({
        client_id: "cl1",
        chambres: [
          { chambre_id: "c101", prix_total: null },
          { chambre_id: "c201", prix_total: null },
        ],
        date_arrivee: J,
        date_depart: j(2),
        nom: "Équipe SOGEA",
        notes: null,
      }),
    );
    expect(appels.some((a) => a.commande === "creer_reservation")).toBe(false);
    expect(await screen.findByText(/La réservation de groupe de Mbarga Aline est enregistrée \(2 chambres, 2 nuits\)/)).toBeInTheDocument();
    expect(await screen.findByRole("list", { name: "Chambres du groupe" })).toBeInTheDocument();
  });

  it("décocher revient à une seule chambre ; un prix convenu illisible est signalé pour la bonne chambre", async () => {
    const appels = simulerCreation();
    await ouvrirFormulaire();
    await cocherEtChoisirClient("101", "102");
    await userEvent.click(screen.getByRole("checkbox", { name: /Chambre 102/ }));
    expect(screen.queryByLabelText("Nom du groupe (facultatif)")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enregistrer la réservation" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("checkbox", { name: /Chambre 102/ }));
    await userEvent.click(screen.getByLabelText(/Appliquer un autre prix/));
    await userEvent.type(screen.getByLabelText("Prix convenu pour la chambre 101"), "40000");
    await userEvent.type(screen.getByLabelText("Prix convenu pour la chambre 102"), "beaucoup");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les 2 chambres" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Le prix de la chambre 102 n'est pas lisible");
    expect(appels.some((a) => a.commande === "creer_groupe")).toBe(false);

    await userEvent.clear(screen.getByLabelText("Prix convenu pour la chambre 102"));
    await userEvent.type(screen.getByLabelText("Prix convenu pour la chambre 102"), "35 000");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les 2 chambres" }));
    await waitFor(() =>
      expect((appels.find((a) => a.commande === "creer_groupe")?.args.donnees as NouveauGroupe).chambres).toEqual([
        { chambre_id: "c101", prix_total: 40_000 },
        { chambre_id: "c102", prix_total: 35_000 },
      ]),
    );
  });

  it("les taxes des chambres s'additionnent dans le total à payer", async () => {
    const taxes: Taxe[] = [{ libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2_000, comprise: false }];
    simulerCreation({
      taxes_hotel: () => taxes,
      calculer_taxes: ({ prix, nuits, taxes }) => detailSimule(prix as number, nuits as number, taxes as Taxe[]),
    });
    await ouvrirFormulaire();
    await cocherEtChoisirClient("101", "102", "201");
    // 1 nuit : 25 000 + 25 000 + 60 000, plus 3 × 2 000 de taxe de séjour
    const liste = await screen.findByRole("list", { name: "Taxes du séjour" });
    await waitFor(() => expect(liste.parentElement).toHaveTextContent(/Total à payer116\s000\sFCFA/));
    expect(liste).toHaveTextContent(/6\s000\sFCFA/);
  });

  it("affiche le refus de Rust si une chambre vient d'être prise", async () => {
    simulerCreation({ creer_groupe: () => erreurRust({ code: "CHAMBRE_DEJA_RESERVEE", details: { reservation_en_conflit: "r9" } }) });
    await ouvrirFormulaire();
    await cocherEtChoisirClient("101", "102");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les 2 chambres" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("déjà réservée");
    expect(screen.getByRole("heading", { name: "Nouvelle réservation" })).toBeInTheDocument();
  });
});

describe("Fiche d'une chambre de groupe", () => {
  it("montre le groupe, ses chambres et un seul compte pour tout le groupe", async () => {
    simulerGroupe({}, [paiement({ montant: 90_000 })]);
    const ouvrir = vi.fn();
    await ouvrirFiche("r2", ouvrir);
    expect(screen.getByText("Groupe « Équipe SOGEA » · 3 chambres")).toBeInTheDocument();
    const liste = screen.getByRole("list", { name: "Chambres du groupe" });
    expect(within(liste).getAllByRole("listitem")).toHaveLength(3);
    expect(within(liste).getByText("(cette fiche)").closest("li")).toHaveTextContent("Chambre 102");
    await userEvent.click(within(liste).getByRole("button", { name: "Ouvrir la chambre 201" }));
    expect(ouvrir).toHaveBeenCalledWith("r3");

    expect(screen.getByRole("heading", { name: "Paiement du groupe" })).toBeInTheDocument();
    expect(chiffre("Total du groupe")).toHaveTextContent(/220\s000\sFCFA/);
    expect(chiffre("Déjà payé")).toHaveTextContent(/90\s000\sFCFA/);
    expect(chiffre("Reste à payer")).toHaveTextContent(/130\s000\sFCFA/);
    expect(screen.getByRole("button", { name: "Imprimer la facture du groupe" })).toBeInTheDocument();
  });

  it("encaisse pour le groupe : le reste du groupe est proposé et sert de plafond", async () => {
    const { appels } = simulerGroupe({}, [paiement({ montant: 90_000 })]);
    await ouvrirFiche("r2");
    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    const montant = screen.getByLabelText("Montant (XAF)");
    expect(montant).toHaveValue("130000");
    await userEvent.clear(montant);
    await userEvent.type(montant, "130001");
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    expect(await screen.findByText(/Le groupe ne doit plus que 130\s000\sFCFA/)).toBeInTheDocument();
    await userEvent.clear(montant);
    await userEvent.type(montant, "130000");
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    await confirmer("Oui, encaisser");
    expect(await screen.findByText(/Paiement de 130\s000\sFCFA enregistré\. Tout est payé\./)).toBeInTheDocument();
    expect(appels.find((a) => a.commande === "encaisser")?.args.donnees).toMatchObject({ reservation_id: "r2", montant: 130_000, type_paiement: "solde" });
  });

  it("depuis une chambre annulée, l'argent va sur une chambre encore active", async () => {
    const { appels, etat } = simulerGroupe();
    etat.chambres = etat.chambres.map((r) => (r.id === "r1" ? { ...r, statut: "annulee" } : r));
    await ouvrirFiche("r1");
    expect(chiffre("Total du groupe")).toHaveTextContent(/170\s000\sFCFA/); // la chambre annulée ne compte plus
    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    await confirmer("Oui, encaisser");
    await waitFor(() => expect(appels.find((a) => a.commande === "encaisser")?.args.donnees).toMatchObject({ reservation_id: "r2" }));
  });

  it("annuler une chambre : l'argent reste sur le compte du groupe", async () => {
    simulerGroupe({}, [paiement({ montant: 200_000 })]);
    await ouvrirFiche("r3");
    await userEvent.click(screen.getByRole("button", { name: "Annuler la réservation" }));
    await userEvent.type(screen.getByLabelText("Raison de l'annulation"), "un collègue ne vient plus");
    await userEvent.click(within(screen.getByRole("group", { name: "Annuler la réservation" })).getByRole("button", { name: "Annuler la réservation" }));
    expect(within(screen.getByRole("alertdialog")).getByText(/L'argent déjà payé reste sur le compte du groupe/)).toBeInTheDocument();
    await confirmer("Oui, annuler la réservation");
    // 100 000 restent dus, 200 000 payés : 100 000 de trop
    expect(await screen.findByText(/Le groupe a trop payé 100\s000\sFCFA/)).toBeInTheDocument();
  });

  it("ajoute une chambre au groupe, aux dates du groupe", async () => {
    const { appels } = simulerGroupe();
    await ouvrirFiche("r2");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter une chambre" }));
    expect(await screen.findByText(/Aux dates du groupe/)).toHaveTextContent("(2 nuits)");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter la chambre" }));
    expect(await screen.findByText("Choisissez la chambre à ajouter.")).toBeInTheDocument();
    await userEvent.click(await screen.findByRole("radio", { name: /Chambre 103/ }));
    await userEvent.click(screen.getByRole("button", { name: "Ajouter la chambre" }));
    await confirmer("Oui, ajouter la chambre");
    expect(await screen.findByText(/La chambre 103 est ajoutée au groupe/)).toBeInTheDocument();
    expect(appels.find((a) => a.commande === "ajouter_au_groupe")?.args).toMatchObject({ groupe_id: "g1", chambre_id: "c103" });
    expect(within(screen.getByRole("list", { name: "Chambres du groupe" })).getAllByRole("listitem")).toHaveLength(4);
  });

  it("sans le droit de réserver, pas de bouton pour ajouter une chambre", async () => {
    simulerGroupe();
    rendreConnecte(<FicheReservation id="r2" onRetour={() => {}} />, session(["reservation.voir"], { role_id: "role-x" }));
    await screen.findByRole("list", { name: "Chambres du groupe" });
    expect(screen.queryByRole("button", { name: "Ajouter une chambre" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Ouvrir la chambre/ })).not.toBeInTheDocument(); // pas de navigation fournie
  });

  it("le départ attend que le groupe ait tout payé", async () => {
    const { etat } = simulerGroupe({}, [paiement({ montant: 50_000 })]);
    etat.chambres = etat.chambres.map((r) => ({ ...r, statut: "arrivee", solde: 0, total_encaisse: r.total_a_payer }));
    await ouvrirFiche("r1");
    expect(screen.getByText(/Avant d'enregistrer le départ, encaissez le reste à payer du groupe \(170\s000\sFCFA\)/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Le client est parti" })).toBeDisabled();
  });
});

describe("Aide", () => {
  it("liste les numéros de chambre en français", () => {
    expect(listeNumeros([])).toBe("");
    expect(listeNumeros([{ numero: "101" }])).toBe("101");
    expect(listeNumeros([{ numero: "101" }, { numero: "102" }])).toBe("101 et 102");
    expect(listeNumeros([{ numero: "101" }, { numero: "102" }, { numero: "201" }])).toBe("101, 102 et 201");
  });
});
