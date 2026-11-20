import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { ChampDate, Compteur } from "../../components/saisie";
import { formatJourMoyen } from "../../lib/dates";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Gestionnaire } from "../../test/outils";
import { CHAMBRES, j, J, MBARGA, resa } from "../../test/reservations";
import { Reservations } from "./Reservations";

function simuler(extra: Record<string, Gestionnaire> = {}) {
  return simulerRust({
    parametres: () => PARAMETRES,
    lister_chambres: () => CHAMBRES,
    planning: () => [resa()],
    chambres_disponibles: () => [CHAMBRES[0], CHAMBRES[1]],
    rechercher_clients: () => [MBARGA],
    obtenir_reservation: () => resa(),
    paiements_reservation: () => [],
    ...extra,
  });
}

const caseLibre = (numero: string, jour: string) => `Réserver la chambre ${numero} pour la nuit du ${formatJourMoyen(jour)}`;

describe("Tableau des réservations", () => {
  it("affiche les chambres, les nuits réservées et les cases libres", async () => {
    const appels = simuler();
    rendreConnecte(<Reservations />);
    const tableau = await screen.findByRole("table", { name: "Tableau des réservations" });

    // On lit depuis la veille (pour les départs du jour) jusqu'à 7 jours plus tard
    await waitFor(() => expect(appels.find((a) => a.commande === "planning")?.args).toMatchObject({ debut: j(-1), fin: j(7) }));
    expect(within(tableau).getAllByRole("columnheader")).toHaveLength(8);
    expect(within(tableau).getByRole("columnheader", { name: `${formatJourMoyen(J)} (aujourd'hui)` })).toBeInTheDocument();

    // La réservation occupe les nuits J+1 à J+3 : pas de bouton « réserver » dessus
    const barre = await within(tableau).findByRole("button", { name: /Mbarga Aline, chambre 101/ });
    expect(barre).toHaveAccessibleName(/Réservée\. Reste 75\s000\sFCFA/);
    expect(screen.getByRole("button", { name: caseLibre("101", J) })).toBeInTheDocument();
    for (const n of [1, 2, 3]) expect(screen.queryByRole("button", { name: caseLibre("101", j(n)) })).not.toBeInTheDocument();
    // Le jour du départ est libre pour une nouvelle arrivée
    expect(screen.getByRole("button", { name: caseLibre("101", j(4)) })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: caseLibre("102", j(2)) })).toBeInTheDocument();
  });

  it("ne propose pas de réserver une chambre en maintenance", async () => {
    simuler();
    rendreConnecte(<Reservations />);
    const ligne = (await screen.findByRole("rowheader", { name: /103/ })).closest('[role="row"]') as HTMLElement;
    expect(ligne).toHaveTextContent("En maintenance");
    expect(within(ligne).queryAllByRole("button")).toHaveLength(0);
  });

  it("change de période et de nombre de jours", async () => {
    const appels = simuler();
    rendreConnecte(<Reservations />);
    await screen.findByRole("table");
    expect(screen.getByRole("button", { name: "Aujourd'hui" })).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "Jours suivants" }));
    await waitFor(() => expect(appels.filter((a) => a.commande === "planning").at(-1)?.args).toMatchObject({ debut: j(6), fin: j(14) }));
    await userEvent.click(screen.getByRole("button", { name: "Jours précédents" }));
    await userEvent.click(screen.getByRole("button", { name: "Jours précédents" }));
    await waitFor(() => expect(appels.filter((a) => a.commande === "planning").at(-1)?.args).toMatchObject({ debut: j(-8), fin: j(0) }));
    await userEvent.click(screen.getByRole("button", { name: "Aujourd'hui" }));

    await userEvent.click(screen.getByRole("button", { name: "2 semaines" }));
    await waitFor(() => expect(screen.getAllByRole("columnheader")).toHaveLength(15));
    expect(appels.filter((a) => a.commande === "planning").at(-1)?.args).toMatchObject({ debut: j(-1), fin: j(14) });
  });

  it("rappelle les arrivées et les départs du jour", async () => {
    simuler({
      planning: () => [
        resa({ id: "a", client_nom: "Arrive Aujourdhui", date_arrivee: J, date_depart: j(2) }),
        resa({ id: "d", client_nom: "Part Aujourdhui", chambre_id: "c102", chambre_numero: "102", date_arrivee: j(-2), date_depart: J, statut: "arrivee" }),
        resa({ id: "x", client_nom: "Deja Arrive", chambre_id: "c102", chambre_numero: "102", date_arrivee: J, date_depart: j(1), statut: "arrivee" }),
      ],
    });
    rendreConnecte(<Reservations />);
    const arrivees = (await screen.findByRole("heading", { name: "Arrivées prévues aujourd'hui" })).closest("section")!;
    expect(within(arrivees).getAllByRole("button")).toHaveLength(1);
    expect(within(arrivees).getByRole("button")).toHaveTextContent("Arrive AujourdhuiChambre 101");
    const departs = screen.getByRole("heading", { name: "Départs prévus aujourd'hui" }).closest("section")!;
    expect(within(departs).getByRole("button")).toHaveTextContent("Part AujourdhuiChambre 102");
    // Le séjour terminé ce matin ne prend plus de case dans le tableau
    expect(within(screen.getByRole("table")).queryByRole("button", { name: /Part Aujourdhui/ })).not.toBeInTheDocument();
  });

  it("coupe proprement un séjour qui dépasse la période affichée", async () => {
    simuler({ planning: () => [resa({ date_arrivee: j(-3), date_depart: j(10), statut: "arrivee", total_encaisse: 325_000, prix_total: 325_000 })] });
    rendreConnecte(<Reservations />);
    const barre = await screen.findByRole("button", { name: /Mbarga Aline, chambre 101/ });
    expect(barre.style.gridColumn).toBe("2 / 9");
    expect(barre).toHaveAccessibleName(/Client présent\. Payé/);
    expect(screen.queryByRole("button", { name: caseLibre("101", J) })).not.toBeInTheDocument();
  });

  it("une chambre de groupe montre le reste du groupe, pas celui de la chambre", async () => {
    // Tout l'acompte est sur la 101 (« trop payé » pour elle seule), le groupe doit encore 40 000
    simuler({
      planning: () => [
        resa({ groupe_id: "g1", total_encaisse: 110_000, solde: -35_000, solde_groupe: 40_000 }),
        resa({ id: "r2", chambre_id: "c102", chambre_numero: "102", groupe_id: "g1", solde_groupe: 40_000 }),
      ],
    });
    rendreConnecte(<Reservations />);
    for (const ch of ["101", "102"]) {
      const barre = await screen.findByRole("button", { name: new RegExp(`Mbarga Aline, chambre ${ch}`) });
      expect(barre).toHaveAccessibleName(/Groupe : reste 40\s000\sFCFA/);
      expect(barre).not.toHaveAccessibleName(/Trop payé/);
    }
  });

  it("sans le droit de réserver, le tableau reste consultable seulement", async () => {
    simuler();
    rendreConnecte(<Reservations />, session(["reservation.voir", "chambre.voir"], { role_id: "role-x" }));
    await screen.findByRole("button", { name: /Mbarga Aline, chambre 101/ });
    expect(screen.queryByRole("button", { name: "Nouvelle réservation" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Réserver la chambre/ })).not.toBeInTheDocument();
  });

  it("explique quoi faire quand il n'y a aucune chambre", async () => {
    simuler({ lister_chambres: () => [], planning: () => [] });
    rendreConnecte(<Reservations />);
    expect(await screen.findByText(/Ajoutez d'abord vos chambres/)).toBeInTheDocument();
  });

  it("affiche l'erreur si le tableau ne peut pas être lu", async () => {
    simuler({ planning: () => erreurRust({ code: "PERMISSION_REFUSEE", details: { permission: "reservation.voir" } }) });
    rendreConnecte(<Reservations />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Votre compte n'a pas le droit");
  });

  it("ouvre la fiche en cliquant sur un nom, et revient au tableau", async () => {
    simuler();
    rendreConnecte(<Reservations />);
    await userEvent.click(await screen.findByRole("button", { name: /Mbarga Aline, chambre 101/ }));
    expect(await screen.findByRole("heading", { name: "Mbarga Aline", level: 1 })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retour au tableau" }));
    expect(await screen.findByRole("table")).toBeInTheDocument();
  });
});

describe("Saisie d'une date et d'un nombre", () => {
  it("corrige un jour impossible et réécrit la date en toutes lettres", async () => {
    const onChange = vi.fn();
    const { rerender } = render(<ChampDate libelle="Jour d'arrivée" valeur="2026-01-31" onChange={onChange} />);
    expect(screen.getByText("samedi 31 janvier 2026")).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText("Jour d'arrivée : mois"), "février");
    expect(onChange).toHaveBeenLastCalledWith("2026-02-28");

    rerender(<ChampDate libelle="Jour d'arrivée" valeur="2026-02-28" onChange={onChange} />);
    expect(within(screen.getByLabelText("Jour d'arrivée : jour")).getAllByRole("option")).toHaveLength(28);
    await userEvent.selectOptions(screen.getByLabelText("Jour d'arrivée : jour"), "5");
    expect(onChange).toHaveBeenLastCalledWith("2026-02-05");
    const anProchain = new Date().getFullYear() + 1;
    await userEvent.selectOptions(screen.getByLabelText("Jour d'arrivée : année"), String(anProchain));
    expect(onChange).toHaveBeenLastCalledWith(`${anProchain}-02-28`);
  });

  it("le compteur respecte son minimum et accorde le mot", async () => {
    const onChange = vi.fn();
    const { rerender } = render(<Compteur libelle="Nombre de nuits" valeur={1} onChange={onChange} unite="nuit" />);
    expect(screen.getByText("1 nuit")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Une nuit de moins" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Une nuit de plus" }));
    expect(onChange).toHaveBeenCalledWith(2);
    rerender(<Compteur libelle="Nombre de nuits" valeur={2} onChange={onChange} unite="nuit" />);
    expect(screen.getByText("2 nuits")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Une nuit de moins" })).toBeEnabled();
  });
});
