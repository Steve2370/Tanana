import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { formatJourLong, formatJourMoyen } from "../../lib/dates";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Gestionnaire } from "../../test/outils";
import { CHAMBRES, detailSimule, j, J, MBARGA, resa } from "../../test/reservations";
import type { Taxe } from "../../bindings/Taxe";
import { Reservations } from "./Reservations";

function simuler(extra: Record<string, Gestionnaire> = {}) {
  return simulerRust({
    parametres: () => PARAMETRES,
    lister_chambres: () => CHAMBRES,
    planning: () => [],
    chambres_disponibles: () => [CHAMBRES[0], CHAMBRES[1]],
    rechercher_clients: () => [MBARGA],
    creer_reservation: () => resa({ date_arrivee: J, date_depart: j(2), prix_total: 50_000 }),
    obtenir_reservation: () => resa({ date_arrivee: J, date_depart: j(2), prix_total: 50_000 }),
    paiements_reservation: () => [],
    ...extra,
  });
}

async function ouvrirFormulaire() {
  rendreConnecte(<Reservations />);
  await userEvent.click(await screen.findByRole("button", { name: "Nouvelle réservation" }));
  await screen.findByRole("heading", { name: "Nouvelle réservation" });
}

const choisirClient = async () => {
  await userEvent.type(screen.getByLabelText("Chercher un client déjà venu"), "mba");
  await userEvent.click(await screen.findByRole("button", { name: /Mbarga Aline/ }));
};

describe("Nouvelle réservation", () => {
  it("enregistre une réservation en 4 étapes, puis ouvre sa fiche", async () => {
    const appels = simuler();
    await ouvrirFormulaire();

    // 1. Dates : aujourd'hui par défaut, 1 nuit → on passe à 2 nuits
    expect(screen.getByText(formatJourLong(J))).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Une nuit de plus" }));
    expect(screen.getByText(formatJourLong(j(2)))).toBeInTheDocument();
    await waitFor(() =>
      expect(appels.filter((a) => a.commande === "chambres_disponibles").at(-1)?.args).toMatchObject({ arrivee: J, depart: j(2) }),
    );

    // 2. Chambre : seulement les chambres libres
    expect(await screen.findByText("2 chambres libres à ces dates.")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: /Chambre 103/ })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("checkbox", { name: /Chambre 101/ }));

    // 3. Client
    await choisirClient();
    expect(screen.getByText("Téléphone : 690 00 00 00")).toBeInTheDocument();

    // 4. Prix calculé tout seul
    expect(screen.getByText(/2 nuits × 25\s000\sFCFA =/)).toHaveTextContent("50 000 FCFA");
    expect(screen.getByText(/Mbarga Aline - chambre 101 -/)).toHaveTextContent(`du ${formatJourMoyen(J)} au ${formatJourLong(j(2))} (2 nuits)`);

    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la réservation" }));
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "creer_reservation")?.args).toEqual({
        token: "jeton-test",
        donnees: { chambre_id: "c101", client_id: "cl1", date_arrivee: J, date_depart: j(2), prix_total: null, notes: null },
      }),
    );
    expect(await screen.findByRole("heading", { name: "Mbarga Aline", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("La réservation de Mbarga Aline est enregistrée (chambre 101, 2 nuits)");
  });

  it("depuis une case du tableau, la chambre et le jour sont déjà choisis", async () => {
    const appels = simuler();
    rendreConnecte(<Reservations />);
    await userEvent.click(await screen.findByRole("button", { name: `Réserver la chambre 102 pour la nuit du ${formatJourMoyen(j(3))}` }));
    expect(await screen.findByRole("checkbox", { name: /Chambre 102/ })).toBeChecked();
    expect(appels.filter((a) => a.commande === "chambres_disponibles").at(-1)?.args).toMatchObject({ arrivee: j(3), depart: j(4) });
    expect(screen.getByText(formatJourLong(j(4)))).toBeInTheDocument();
  });

  it("dit clairement ce qui manque, sans appeler Rust", async () => {
    const appels = simuler();
    await ouvrirFormulaire();
    await screen.findByRole("checkbox", { name: /Chambre 101/ });
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la réservation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choisissez une chambre (étape 2).");
    await userEvent.click(screen.getByRole("checkbox", { name: /Chambre 101/ }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la réservation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choisissez le client (étape 3).");
    expect(appels.some((a) => a.commande === "creer_reservation")).toBe(false);
    // Le message disparaît dès que le client est choisi
    await choisirClient();
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("prévient quand la chambre choisie n'est plus libre après un changement de dates", async () => {
    let libres = [CHAMBRES[0], CHAMBRES[1]];
    simuler({ chambres_disponibles: () => libres });
    await ouvrirFormulaire();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Chambre 101/ }));
    libres = [CHAMBRES[1]];
    await userEvent.click(screen.getByRole("button", { name: "Une nuit de plus" }));
    expect(await screen.findByText(/La chambre choisie n'est pas libre à ces dates/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la réservation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choisissez une chambre");
  });

  it("signale qu'aucune chambre n'est libre, et propose d'autres dates", async () => {
    simuler({ chambres_disponibles: () => [] });
    await ouvrirFormulaire();
    expect(await screen.findByText("Aucune chambre n'est libre à ces dates.")).toBeInTheDocument();
    expect(screen.getByText(/Essayez d'autres dates/)).toBeInTheDocument();
  });

  it("dans un hôtel sans chambre, dit d'aller d'abord dans « Chambres »", async () => {
    simuler({ chambres_disponibles: () => [], lister_chambres: () => [] });
    await ouvrirFormulaire();
    expect(await screen.findByText("L'hôtel n'a encore aucune chambre.")).toBeInTheDocument();
    expect(screen.getByText(/Ajoutez d'abord les chambres dans « Chambres »/)).toBeInTheDocument();
    expect(screen.queryByText(/Essayez d'autres dates/)).not.toBeInTheDocument();
  });

  it("quand toutes les chambres sont hors service, le dit", async () => {
    simuler({
      chambres_disponibles: () => [],
      lister_chambres: () => CHAMBRES.map((c, i) => ({ ...c, statut: i % 2 ? "maintenance" : "hors_service" })),
    });
    await ouvrirFormulaire();
    expect(await screen.findByText("Aucune chambre n'est en service.")).toBeInTheDocument();
  });

  it("sans le droit de voir les chambres, garde le message général", async () => {
    simuler({ chambres_disponibles: () => [], lister_chambres: () => erreurRust({ code: "PERMISSION_REFUSEE", details: { permission: "chambre.voir" } }) });
    await ouvrirFormulaire();
    expect(await screen.findByText("Aucune chambre n'est libre à ces dates.")).toBeInTheDocument();
  });

  it("crée un nouveau client sans quitter la réservation", async () => {
    const nouveau = { ...MBARGA, id: "cl9", nom: "Ngono", prenom: "Paul", telephone: null };
    const appels = simuler({ rechercher_clients: () => [], creer_client: () => nouveau });
    await ouvrirFormulaire();
    await userEvent.type(screen.getByLabelText("Chercher un client déjà venu"), "Ngono");
    expect(await screen.findByText("Aucun client trouvé pour « Ngono ».")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "C'est un nouveau client" }));
    expect(screen.getByLabelText("Nom")).toHaveValue("Ngono");
    await userEvent.type(screen.getByLabelText("Prénom"), "Paul{Enter}"); // Entrée valide le client
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "creer_client")?.args).toMatchObject({
        donnees: { nom: "Ngono", prenom: "Paul", telephone: null },
      }),
    );
    expect(await screen.findByText("Ngono Paul")).toBeInTheDocument();
    // On peut encore changer d'avis
    await userEvent.click(screen.getByRole("button", { name: "Changer de client" }));
    expect(screen.getByLabelText("Chercher un client déjà venu")).toBeInTheDocument();
  });

  it("un numéro cherché devient le téléphone du nouveau client, et le nom est exigé", async () => {
    const appels = simuler({ rechercher_clients: () => [] });
    await ouvrirFormulaire();
    await userEvent.type(screen.getByLabelText("Chercher un client déjà venu"), "690 11 22 33");
    await userEvent.click(await screen.findByRole("button", { name: "C'est un nouveau client" }));
    expect(screen.getByLabelText("Téléphone")).toHaveValue("690 11 22 33");
    expect(screen.getByLabelText("Nom")).toHaveValue("");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer ce client" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Écrivez le nom du client.");
    expect(appels.some((a) => a.commande === "creer_client")).toBe(false);
  });

  it("le nouveau client peut avoir un email, affiché une fois choisi", async () => {
    const nouveau = { ...MBARGA, id: "cl9", nom: "Ngono", prenom: null, telephone: null, email: "ngono@exemple.com" };
    const appels = simuler({ rechercher_clients: () => [], creer_client: () => nouveau });
    await ouvrirFormulaire();
    await userEvent.type(screen.getByLabelText("Chercher un client déjà venu"), "Ngono");
    await userEvent.click(await screen.findByRole("button", { name: "C'est un nouveau client" }));
    await userEvent.type(screen.getByLabelText("Email"), "  ngono@exemple.com ");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer ce client" }));
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "creer_client")?.args).toMatchObject({ donnees: { nom: "Ngono", email: "ngono@exemple.com" } }),
    );
    expect(await screen.findByText("Email : ngono@exemple.com")).toBeInTheDocument();
  });

  it("un email mal écrit est signalé avant d'enregistrer", async () => {
    const appels = simuler({ rechercher_clients: () => [] });
    await ouvrirFormulaire();
    await userEvent.type(screen.getByLabelText("Chercher un client déjà venu"), "Ngono");
    await userEvent.click(await screen.findByRole("button", { name: "C'est un nouveau client" }));
    await userEvent.type(screen.getByLabelText("Email"), "ngono exemple.com");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer ce client" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("L'email n'est pas valide. Écrivez-le comme ceci : nom@exemple.com");
    expect(appels.some((a) => a.commande === "creer_client")).toBe(false);
  });

  it("sans email, le nouveau client est créé avec un email vide", async () => {
    const nouveau = { ...MBARGA, id: "cl9", nom: "Ngono", prenom: null, telephone: null, email: null };
    const appels = simuler({ rechercher_clients: () => [], creer_client: () => nouveau });
    await ouvrirFormulaire();
    await userEvent.type(screen.getByLabelText("Chercher un client déjà venu"), "Ngono");
    await userEvent.click(await screen.findByRole("button", { name: "C'est un nouveau client" }));
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer ce client" }));
    await waitFor(() => expect(appels.find((a) => a.commande === "creer_client")?.args).toMatchObject({ donnees: { email: null } }));
    expect(screen.queryByText(/^Email :/)).not.toBeInTheDocument();
  });

  it("sans le droit de créer des clients, on ne peut que chercher", async () => {
    simuler();
    rendreConnecte(<Reservations />, session(["reservation.voir", "reservation.creer", "chambre.voir", "client.voir"], { role_id: "role-x" }));
    await userEvent.click(await screen.findByRole("button", { name: "Nouvelle réservation" }));
    expect(await screen.findByLabelText("Chercher un client déjà venu")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "C'est un nouveau client" })).not.toBeInTheDocument();
  });

  it("accepte un prix convenu différent et une remarque", async () => {
    const appels = simuler();
    await ouvrirFormulaire();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Chambre 101/ }));
    await choisirClient();
    await userEvent.click(screen.getByLabelText(/Appliquer un autre prix/));
    await userEvent.type(screen.getByLabelText("Prix total convenu"), "beaucoup");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la réservation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Le prix n'est pas lisible");
    expect(appels.some((a) => a.commande === "creer_reservation")).toBe(false);

    await userEvent.clear(screen.getByLabelText("Prix total convenu"));
    await userEvent.type(screen.getByLabelText("Prix total convenu"), "20 000");
    await userEvent.type(screen.getByLabelText(/Remarque/), "  Client fidèle ");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la réservation" }));
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "creer_reservation")?.args).toMatchObject({
        donnees: { prix_total: 20_000, notes: "Client fidèle" },
      }),
    );
  });

  it("affiche le refus de Rust si la chambre vient d'être prise par un autre poste", async () => {
    simuler({ creer_reservation: () => erreurRust({ code: "CHAMBRE_DEJA_RESERVEE", details: { reservation_en_conflit: "r9" } }) });
    await ouvrirFormulaire();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Chambre 101/ }));
    await choisirClient();
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer la réservation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Cette chambre est déjà réservée sur une partie de ces dates");
    expect(screen.getByRole("heading", { name: "Nouvelle réservation" })).toBeInTheDocument();
  });

  it("« Annuler » revient au tableau sans rien enregistrer", async () => {
    const appels = simuler();
    await ouvrirFormulaire();
    await userEvent.click(screen.getByRole("button", { name: "Annuler" }));
    expect(await screen.findByRole("table")).toBeInTheDocument();
    expect(appels.some((a) => a.commande === "creer_reservation")).toBe(false);
  });
});

describe("Nouvelle réservation : taxes de l'hôtel", () => {
  it("montre les taxes qui s'appliqueront et le total à payer, qui suit le prix convenu", async () => {
    const taxes: Taxe[] = [{ libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2_000, comprise: false }];
    simuler({
      taxes_hotel: () => taxes,
      calculer_taxes: ({ prix, nuits, taxes }) => detailSimule(prix as number, nuits as number, taxes as Taxe[]),
    });
    await ouvrirFormulaire();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Chambre 101/ }));
    const liste = await screen.findByRole("list", { name: "Taxes du séjour" });
    expect(liste).toHaveTextContent(/Taxe de séjour \(2\s000\sFCFA par nuit\)/);
    expect(liste.parentElement).toHaveTextContent(/Total à payer27\s000\sFCFA/); // 1 nuit à 25 000 + 2 000
    expect(screen.getByText("Les taxes pourront être changées ensuite, sur la fiche de la réservation.")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("checkbox", { name: /Appliquer un autre prix/ }));
    await userEvent.type(screen.getByLabelText("Prix total convenu"), "20000");
    await waitFor(() =>
      expect(screen.getByRole("list", { name: "Taxes du séjour" }).parentElement).toHaveTextContent(/Total à payer22\s000\sFCFA/),
    );
  });

  it("sans taxe réglée, rien de plus ne s'affiche", async () => {
    simuler({ taxes_hotel: () => [] });
    await ouvrirFormulaire();
    await userEvent.click(await screen.findByRole("checkbox", { name: /Chambre 101/ }));
    expect(screen.queryByText("Taxes de l'hôtel")).not.toBeInTheDocument();
  });
});
