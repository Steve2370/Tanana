import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Client } from "../../bindings/Client";
import type { Reservation } from "../../bindings/Reservation";
import { formatSejour } from "../../lib/dates";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Gestionnaire } from "../../test/outils";
import { CHAMBRES, j, MBARGA, resa } from "../../test/reservations";
import { Clients, doublonsPossibles } from "./Clients";

const NGONO: Client = { ...MBARGA, id: "cl2", nom: "Ngono", prenom: "Paul", telephone: "+237 677 12 34 56", email: "paul@exemple.cm" };

const SEJOURS: Reservation[] = [
  resa({ id: "r3", date_arrivee: j(10), date_depart: j(12), prix_total: 50_000, total_encaisse: 20_000 }),
  resa({ id: "r2", date_arrivee: j(-20), date_depart: j(-18), prix_total: 50_000, total_encaisse: 50_000, statut: "partie" }),
  resa({ id: "r1", date_arrivee: j(-40), date_depart: j(-39), prix_total: 25_000, total_encaisse: 0, statut: "annulee" }),
];

const NOUVELLE = resa({ id: "r9", date_arrivee: j(0), date_depart: j(1) });

/** Faux serveur : la liste de clients évolue comme dans Rust. */
function simuler(extra: Record<string, Gestionnaire> = {}) {
  const etat = { clients: [MBARGA, NGONO] };
  const appels = simulerRust({
    parametres: () => PARAMETRES,
    rechercher_clients: ({ recherche }) => {
      const q = String(recherche).toLowerCase();
      const chiffres = q.replace(/\D/g, "");
      const tel = chiffres.length >= 6 ? chiffres.slice(-8) : null;
      return etat.clients.filter(
        (c) =>
          [c.nom, c.prenom, c.telephone, c.email].some((v) => v?.toLowerCase().includes(q)) ||
          (tel !== null && (c.telephone ?? "").replace(/\D/g, "").includes(tel)),
      );
    },
    obtenir_client: ({ id }) => etat.clients.find((c) => c.id === id) ?? erreurRust({ code: "INTROUVABLE", details: "client" }),
    creer_client: ({ donnees }) => {
      const c = { ...MBARGA, ...(donnees as Partial<Client>), id: "cl9" };
      etat.clients = [...etat.clients, c];
      return c;
    },
    modifier_client: ({ id, donnees }) => {
      etat.clients = etat.clients.map((c) => (c.id === id ? { ...c, ...(donnees as Partial<Client>) } : c));
      return etat.clients.find((c) => c.id === id);
    },
    reservations_du_client: () => SEJOURS,
    obtenir_reservation: ({ id }) => [...SEJOURS, NOUVELLE].find((r) => r.id === id),
    paiements_reservation: () => [],
    chambres_disponibles: () => [CHAMBRES[0]],
    creer_reservation: () => NOUVELLE,
    ...extra,
  });
  return { appels, etat };
}

const appel = (appels: { commande: string; args: Record<string, unknown> }[], commande: string) =>
  appels.filter((a) => a.commande === commande).at(-1)?.args;
const listeClients = () => within(screen.getByRole("list", { name: "Clients" })).getAllByRole("button");

async function ouvrirFiche(nom: RegExp) {
  await userEvent.click(await screen.findByRole("button", { name: nom }));
  return screen.findByRole("heading", { level: 1, name: nom });
}

describe("Clients : liste et recherche", () => {
  it("liste tous les clients, puis filtre en tapant", async () => {
    const { appels } = simuler();
    rendreConnecte(<Clients />);
    await waitFor(() => expect(listeClients()).toHaveLength(2));
    expect(screen.getByText("Tous les clients, par ordre alphabétique")).toBeInTheDocument();
    expect(appel(appels, "rechercher_clients")).toMatchObject({ recherche: "" });
    expect(listeClients()[1]).toHaveTextContent("Ngono Paul+237 677 12 34 56");

    await userEvent.type(screen.getByLabelText("Chercher un client"), "ngo");
    await waitFor(() => expect(listeClients()).toHaveLength(1));
    expect(screen.getByText("1 client trouvé")).toBeInTheDocument();
  });

  it("explique quoi faire quand rien ne correspond", async () => {
    simuler();
    rendreConnecte(<Clients />);
    await userEvent.type(await screen.findByLabelText("Chercher un client"), "Zorro");
    expect(await screen.findByText(/Aucun client ne correspond à « Zorro »\. Vérifiez l'orthographe/)).toBeInTheDocument();
  });

  it("dit comment ajouter le premier client", async () => {
    simuler({ rechercher_clients: () => [] });
    rendreConnecte(<Clients />);
    expect(await screen.findByText(/Aucun client pour le moment/)).toBeInTheDocument();
  });

  it("prévient quand la liste est coupée à 200", async () => {
    const beaucoup = Array.from({ length: 200 }, (_, i) => ({ ...MBARGA, id: `c${i}`, nom: `Client ${i}` }));
    simuler({ rechercher_clients: () => beaucoup });
    rendreConnecte(<Clients />);
    expect(await screen.findByText(/Seuls les 200 premiers sont affichés/)).toBeInTheDocument();
  });
});

describe("Clients : nouveau client", () => {
  it("crée un client et ouvre sa fiche", async () => {
    const { appels } = simuler();
    rendreConnecte(<Clients />);
    await userEvent.click(await screen.findByRole("button", { name: "Nouveau client" }));
    const formulaire = screen.getByRole("form", { name: "Nouveau client" });
    await userEvent.click(within(formulaire).getByRole("button", { name: "Enregistrer le client" }));
    expect(await within(formulaire).findByRole("alert")).toHaveTextContent("Écrivez le nom du client.");

    await userEvent.type(within(formulaire).getByLabelText("Nom"), " Essomba ");
    await userEvent.type(within(formulaire).getByLabelText("Prénom (facultatif)"), "Rose");
    await userEvent.type(within(formulaire).getByLabelText("Téléphone (facultatif)"), "699 11 22 33{Enter}");

    expect(await screen.findByRole("heading", { level: 1, name: "Essomba Rose" })).toBeInTheDocument();
    expect(screen.getByText("La fiche de Essomba Rose est enregistrée.")).toBeInTheDocument();
    expect(appel(appels, "creer_client")).toEqual({
      token: "jeton-test",
      donnees: { nom: "Essomba", prenom: "Rose", telephone: "699 11 22 33", email: null, piece_identite: null, notes: null },
    });
  });

  it("reprend ce qui a été cherché : un nom devient le nom, un numéro devient le téléphone", async () => {
    simuler();
    rendreConnecte(<Clients />);
    await userEvent.type(await screen.findByLabelText("Chercher un client"), "Tchoumi");
    await userEvent.click(screen.getByRole("button", { name: "Nouveau client" }));
    expect(screen.getByLabelText("Nom")).toHaveValue("Tchoumi");
    await userEvent.click(screen.getByRole("button", { name: "Fermer sans enregistrer" }));

    await userEvent.clear(screen.getByLabelText("Chercher un client"));
    await userEvent.type(screen.getByLabelText("Chercher un client"), "655 00 11 22");
    await userEvent.click(screen.getByRole("button", { name: "Nouveau client" }));
    expect(screen.getByLabelText("Nom")).toHaveValue("");
    expect(screen.getByLabelText("Téléphone (facultatif)")).toHaveValue("655 00 11 22");
  });

  it("prévient si le client semble déjà enregistré, puis laisse choisir", async () => {
    const defiler = vi.fn();
    Element.prototype.scrollIntoView = defiler;
    const { appels } = simuler();
    rendreConnecte(<Clients />);
    await userEvent.click(await screen.findByRole("button", { name: "Nouveau client" }));
    // Même téléphone que Ngono Paul, écrit autrement
    await userEvent.type(screen.getByLabelText("Nom"), "Ngonno");
    await userEvent.type(screen.getByLabelText("Téléphone (facultatif)"), "677123456");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer le client" }));

    const avertissement = await screen.findByText("Ce client est peut-être déjà enregistré");
    // L'avertissement est amené à l'écran : sinon, en bas d'un long formulaire, on ne le verrait pas
    await waitFor(() => expect(defiler).toHaveBeenCalled());
    expect(avertissement.closest('[role="status"]')).toHaveTextContent("Ngono Paul");
    expect(appel(appels, "creer_client")).toBeUndefined();

    // Ouvrir la fiche existante plutôt que créer un doublon
    await userEvent.click(screen.getByRole("button", { name: /Ngono Paul.*Ouvrir sa fiche/ }));
    expect(await screen.findByRole("heading", { level: 1, name: "Ngono Paul" })).toBeInTheDocument();
    expect(appel(appels, "creer_client")).toBeUndefined();
  });

  it("« Enregistrer quand même » crée bien une deuxième fiche", async () => {
    const { appels } = simuler();
    rendreConnecte(<Clients />);
    await userEvent.click(await screen.findByRole("button", { name: "Nouveau client" }));
    await userEvent.type(screen.getByLabelText("Nom"), "mbarga");
    await userEvent.type(screen.getByLabelText("Prénom (facultatif)"), "Aliné");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer le client" }));
    expect(await screen.findByText("Ce client est peut-être déjà enregistré")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer quand même" }));
    await waitFor(() => expect(appel(appels, "creer_client")).toMatchObject({ donnees: { nom: "mbarga", prenom: "Aliné" } }));
  });
});

describe("détection des doublons", () => {
  const d = (nom: string, prenom: string | null, telephone: string | null) => ({
    nom,
    prenom,
    telephone,
    email: null,
    piece_identite: null,
    notes: null,
  });

  it("même nom complet, sans tenir compte des accents ni des majuscules", () => {
    expect(doublonsPossibles(d("MBARGA", "aliné", null), [MBARGA, NGONO])).toEqual([MBARGA]);
    expect(doublonsPossibles(d("Mbarga", "Jean", null), [MBARGA, NGONO])).toEqual([]);
  });

  it("même téléphone, avec ou sans indicatif et espaces", () => {
    expect(doublonsPossibles(d("X", null, "677 12 34 56"), [MBARGA, NGONO])).toEqual([NGONO]);
    expect(doublonsPossibles(d("X", null, "00237677123456"), [MBARGA, NGONO])).toEqual([NGONO]);
    expect(doublonsPossibles(d("X", null, "12"), [MBARGA, NGONO])).toEqual([]); // trop court pour comparer
  });
});

describe("Clients : fiche", () => {
  it("affiche les coordonnées et le résumé des séjours", async () => {
    simuler();
    rendreConnecte(<Clients />);
    await ouvrirFiche(/Mbarga Aline/);
    expect(screen.getByText("Téléphone").nextSibling).toHaveTextContent("690 00 00 00");
    expect(screen.getByText("Courriel").nextSibling).toHaveTextContent("Non renseigné");

    const resume = await screen.findByRole("group", { name: "Résumé des séjours" });
    expect(within(resume).getByText("Séjours").nextSibling).toHaveTextContent("2"); // l'annulé ne compte pas
    expect(within(resume).getByText("Total payé").nextSibling).toHaveTextContent("70 000 FCFA");
    expect(within(resume).getByText("Reste à payer").nextSibling).toHaveTextContent("30 000 FCFA");

    const sejours = within(screen.getByRole("list", { name: "Séjours du client" })).getAllByRole("button");
    expect(sejours).toHaveLength(3);
    expect(sejours[0]).toHaveTextContent(formatSejour(j(10), j(12)));
    expect(sejours[0]).toHaveTextContent("Chambre 101 · 2 nuits · 50 000 FCFA");
    expect(sejours[0]).toHaveTextContent("Reste 30 000 FCFA");
    expect(sejours[0]).toHaveTextContent("Réservée");
    expect(sejours[1]).toHaveTextContent("Séjour terminé");
    expect(sejours[2]).toHaveTextContent("Annulée");
    expect(sejours[2]).not.toHaveTextContent("Reste");
  });

  it("modifie la fiche, et « Fermer » ne change rien", async () => {
    const { appels } = simuler();
    rendreConnecte(<Clients />);
    await ouvrirFiche(/Ngono Paul/);
    await userEvent.click(screen.getByRole("button", { name: "Modifier" }));
    expect(screen.getByLabelText("Courriel (facultatif)")).toHaveValue("paul@exemple.cm");
    await userEvent.clear(screen.getByLabelText("Courriel (facultatif)"));
    await userEvent.click(screen.getByRole("button", { name: "Fermer sans enregistrer" }));
    expect(appel(appels, "modifier_client")).toBeUndefined();
    expect(screen.getByText("Courriel").nextSibling).toHaveTextContent("paul@exemple.cm");

    await userEvent.click(screen.getByRole("button", { name: "Modifier" }));
    await userEvent.clear(screen.getByLabelText("Téléphone (facultatif)"));
    await userEvent.type(screen.getByLabelText("Téléphone (facultatif)"), "655 99 88 77");
    await userEvent.type(screen.getByLabelText(/Remarques/), "Vient chaque mois");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les changements" }));

    expect(await screen.findByText("Les changements sont enregistrés.")).toBeInTheDocument();
    expect(appel(appels, "modifier_client")).toEqual({
      token: "jeton-test",
      id: "cl2",
      donnees: { nom: "Ngono", prenom: "Paul", telephone: "655 99 88 77", email: "paul@exemple.cm", piece_identite: null, notes: "Vient chaque mois" },
    });
    expect(screen.getByText("Téléphone").nextSibling).toHaveTextContent("655 99 88 77");
  });

  it("affiche le refus de Rust (courriel invalide) sans fermer le formulaire", async () => {
    simuler({ modifier_client: () => erreurRust({ code: "VALIDATION", details: "courriel invalide" }) });
    rendreConnecte(<Clients />);
    await ouvrirFiche(/Ngono Paul/);
    await userEvent.click(screen.getByRole("button", { name: "Modifier" }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les changements" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Courriel invalide.");
    expect(screen.getByRole("form", { name: "Modifier la fiche" })).toBeInTheDocument();
  });

  it("ouvre un séjour, puis revient à la fiche du client", async () => {
    simuler();
    rendreConnecte(<Clients />);
    await ouvrirFiche(/Mbarga Aline/);
    await userEvent.click(within(await screen.findByRole("list", { name: "Séjours du client" })).getAllByRole("button")[1]!);
    expect(await screen.findByText("Le séjour est terminé.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retour à la fiche du client" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Mbarga Aline" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retour à la liste des clients" }));
    expect(await screen.findByLabelText("Chercher un client")).toBeInTheDocument();
  });

  it("garde la recherche en revenant d'une fiche", async () => {
    simuler();
    rendreConnecte(<Clients />);
    await userEvent.type(await screen.findByLabelText("Chercher un client"), "ngo");
    await ouvrirFiche(/Ngono Paul/);
    await userEvent.click(screen.getByRole("button", { name: "Retour à la liste des clients" }));
    expect(await screen.findByLabelText("Chercher un client")).toHaveValue("ngo");
  });

  it("réserve pour ce client : il est déjà choisi dans le formulaire", async () => {
    const { appels } = simuler();
    rendreConnecte(<Clients />);
    await ouvrirFiche(/Mbarga Aline/);
    await userEvent.click(screen.getByRole("button", { name: "Réserver pour ce client" }));
    expect(await screen.findByRole("heading", { name: "Nouvelle réservation" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Changer de client" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Chercher un client déjà venu")).not.toBeInTheDocument();

    await userEvent.click(await screen.findByRole("radio", { name: /Chambre 101/ }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la réservation" }));
    await waitFor(() => expect(appel(appels, "creer_reservation")).toMatchObject({ donnees: { client_id: "cl1", chambre_id: "c101" } }));
    expect(await screen.findByText(/La réservation de Mbarga Aline est enregistrée/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retour à la fiche du client" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Mbarga Aline" })).toBeInTheDocument();
  });

  it("« Retour » depuis la nouvelle réservation revient à la fiche sans rien créer", async () => {
    const { appels } = simuler();
    rendreConnecte(<Clients />);
    await ouvrirFiche(/Mbarga Aline/);
    await userEvent.click(screen.getByRole("button", { name: "Réserver pour ce client" }));
    await userEvent.click(await screen.findByRole("button", { name: "Retour à la fiche du client" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Mbarga Aline" })).toBeInTheDocument();
    expect(appel(appels, "creer_reservation")).toBeUndefined();
  });

  it("dit quand le client n'a encore aucun séjour", async () => {
    simuler({ reservations_du_client: () => [] });
    rendreConnecte(<Clients />);
    await ouvrirFiche(/Ngono Paul/);
    expect(await screen.findByText("Aucun séjour pour le moment.")).toBeInTheDocument();
  });
});

describe("Clients : selon les droits du compte", () => {
  it("en lecture seule : pas de création, de modification, ni de réservation ; pas de séjours sans le droit", async () => {
    const { appels } = simuler();
    rendreConnecte(<Clients />, session(["client.voir"], { role_id: "role-x" }));
    await ouvrirFiche(/Ngono Paul/);
    expect(screen.getByText("Téléphone").nextSibling).toHaveTextContent("+237 677 12 34 56");
    for (const nom of ["Modifier", "Réserver pour ce client"]) expect(screen.queryByRole("button", { name: nom })).not.toBeInTheDocument();
    expect(screen.queryByText("Séjours")).not.toBeInTheDocument();
    expect(appel(appels, "reservations_du_client")).toBeUndefined();
    await userEvent.click(screen.getByRole("button", { name: "Retour à la liste des clients" }));
    await screen.findByLabelText("Chercher un client");
    expect(screen.queryByRole("button", { name: "Nouveau client" })).not.toBeInTheDocument();
  });
});
