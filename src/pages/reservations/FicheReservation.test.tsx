import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Paiement } from "../../bindings/Paiement";
import type { Reservation } from "../../bindings/Reservation";
import { formatJourLong, formatSejour } from "../../lib/dates";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Gestionnaire } from "../../test/outils";
import { CHAMBRES, detailSimule, documentEmis, documentEnregistre, j, J, paiement, resa } from "../../test/reservations";
import type { Taxe } from "../../bindings/Taxe";
import { FicheReservation } from "./FicheReservation";

/** Petit faux serveur : la réservation et ses paiements évoluent comme dans Rust. */
function simuler(depart: Reservation, paiementsInitiaux: Paiement[] = [], extra: Record<string, Gestionnaire> = {}) {
  const etat = { r: depart, paiements: paiementsInitiaux };
  const payer = (montant: number, signe: 1 | -1) => {
    const total = etat.r.total_encaisse + signe * montant;
    etat.r = { ...etat.r, total_encaisse: total, solde: etat.r.total_a_payer - total };
  };
  const appels = simulerRust({
    parametres: () => PARAMETRES,
    obtenir_reservation: () => etat.r,
    paiements_reservation: () => etat.paiements,
    lister_chambres: () => CHAMBRES,
    chambres_disponibles: () => [CHAMBRES[1]],
    encaisser: ({ donnees }) => {
      const d = donnees as { montant: number; type_paiement: Paiement["type_paiement"]; mode: Paiement["mode"] };
      payer(d.montant, 1);
      const p = paiement({ id: `p${etat.paiements.length + 1}`, montant: d.montant, type_paiement: d.type_paiement, mode: d.mode });
      etat.paiements = [...etat.paiements, p];
      return p;
    },
    rembourser: ({ montant, mode }) => {
      payer(montant as number, -1);
      const p = paiement({ id: "pr", montant: montant as number, type_paiement: "remboursement", mode: mode as Paiement["mode"] });
      etat.paiements = [...etat.paiements, p];
      return p;
    },
    enregistrer_arrivee: () => (etat.r = { ...etat.r, statut: "arrivee" }),
    enregistrer_depart: () => (etat.r = { ...etat.r, statut: "partie" }),
    annuler_reservation: () => (etat.r = { ...etat.r, statut: "annulee" }),
    modifier_reservation: ({ donnees }) => {
      const d = donnees as { date_arrivee: string; date_depart: string; prix_total: number };
      return (etat.r = resa({ ...etat.r, ...d, solde: d.prix_total - etat.r.total_encaisse }));
    },
    ...extra,
  });
  return { appels, etat };
}

const ouvrir = async (s = session()) => {
  const onRetour = vi.fn();
  rendreConnecte(<FicheReservation id="r1" onRetour={onRetour} />, s);
  await screen.findByRole("heading", { name: "Mbarga Aline", level: 1 });
  return onRetour;
};
const appel = (appels: { commande: string; args: Record<string, unknown> }[], commande: string) => appels.find((a) => a.commande === commande)?.args;
const confirmer = (nom: string) => userEvent.click(within(screen.getByRole("alertdialog")).getByRole("button", { name: nom }));
const chiffre = (libelle: string) => screen.getByText(libelle).parentElement!;

describe("Fiche réservation : paiements", () => {
  it("montre le prix, ce qui est payé et ce qui reste", async () => {
    simuler(resa({ total_encaisse: 30_000 }), [paiement()]);
    await ouvrir();
    expect(chiffre("Prix du séjour")).toHaveTextContent("75 000 FCFA");
    expect(chiffre("Déjà payé")).toHaveTextContent("30 000 FCFA");
    expect(chiffre("Reste à payer")).toHaveTextContent("45 000 FCFA");
    expect(screen.getByText(/^Chambre/)).toHaveTextContent(`Chambre 101 · ${formatSejour(j(1), j(4))} · 3 nuits`);
    expect(screen.getByText("Réservée")).toBeInTheDocument();
    const ligne = within(screen.getByRole("list", { name: "Paiements enregistrés" })).getByRole("listitem");
    expect(ligne).toHaveTextContent(/Acompte · Mobile money · aujourd'hui à \d+ h \d\d · par Marie/);
    expect(ligne).toHaveTextContent("30 000 FCFA");
  });

  it("encaisse un acompte après confirmation", async () => {
    const { appels } = simuler(resa());
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    const montant = screen.getByLabelText("Montant (XAF)");
    expect(montant).toHaveValue("75000"); // le reste à payer est proposé
    await userEvent.clear(montant);
    await userEvent.type(montant, "30 000");
    await userEvent.click(screen.getByRole("radio", { name: "Mobile money" }));
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));

    const dialogue = screen.getByRole("alertdialog");
    expect(dialogue).toHaveTextContent("Encaisser 30 000 FCFA ?");
    expect(dialogue).toHaveTextContent("Mobile money");
    expect(appel(appels, "encaisser")).toBeUndefined();
    await confirmer("Oui, encaisser");

    expect(await screen.findByText(/Paiement de 30\s000\sFCFA enregistré\. Il reste 45\s000\sFCFA à payer\./)).toBeInTheDocument();
    expect(appel(appels, "encaisser")).toEqual({
      token: "jeton-test",
      donnees: { reservation_id: "r1", montant: 30_000, type_paiement: "acompte", mode: "mobile", note: null },
    });
    expect(chiffre("Reste à payer")).toHaveTextContent("45 000 FCFA");
    expect(screen.queryByLabelText("Montant (XAF)")).not.toBeInTheDocument();
  });

  it("payer tout le reste est enregistré comme solde", async () => {
    const { appels } = simuler(resa({ total_encaisse: 30_000 }), [paiement()]);
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    expect(screen.getByLabelText("Montant (XAF)")).toHaveValue("45000");
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    await confirmer("Oui, encaisser");
    expect(await screen.findByText(/Tout est payé\./)).toBeInTheDocument();
    expect(appel(appels, "encaisser")).toMatchObject({ donnees: { montant: 45_000, type_paiement: "solde", mode: "especes" } });
    expect(screen.queryByRole("button", { name: "Encaisser un paiement" })).not.toBeInTheDocument();
  });

  it("refuse un montant illisible ou plus grand que le reste, sans appeler Rust", async () => {
    const { appels } = simuler(resa());
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    const montant = screen.getByLabelText("Montant (XAF)");
    await userEvent.clear(montant);
    await userEvent.type(montant, "80000");
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Le client ne doit plus que 75 000 FCFA");
    await userEvent.clear(montant);
    await userEvent.type(montant, "dix mille");
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Le montant n'est pas lisible");
    await userEvent.clear(montant);
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(appel(appels, "encaisser")).toBeUndefined();

    await userEvent.click(screen.getByRole("button", { name: /Remettre 75\s000\sFCFA/ }));
    expect(montant).toHaveValue("75000");
  });

  it("la touche Entrée dans le montant demande la confirmation, sans encaisser toute seule", async () => {
    const { appels } = simuler(resa());
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    // Une seule chose à la fois : les actions du séjour sont masquées pendant l'encaissement
    expect(screen.queryByRole("button", { name: "Le client est arrivé" })).not.toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Montant (XAF)"), "{Enter}");
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Encaisser 75 000 FCFA ?");
    // « Annuler » a le focus : un second Entrée par réflexe n'encaisse pas
    await userEvent.keyboard("{Enter}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(appel(appels, "encaisser")).toBeUndefined();
  });

  it("« Annuler » dans la confirmation n'encaisse rien", async () => {
    const { appels } = simuler(resa());
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    await confirmer("Annuler");
    expect(appel(appels, "encaisser")).toBeUndefined();
    expect(screen.getByLabelText("Montant (XAF)")).toBeInTheDocument();
  });

  it("rembourse avec une raison obligatoire, sans dépasser ce qui a été payé", async () => {
    const { appels } = simuler(resa({ total_encaisse: 30_000, statut: "annulee" }), [paiement()]);
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Rembourser" }));
    const montant = screen.getByLabelText("Montant (XAF)");
    expect(montant).toHaveValue("30000");
    await userEvent.click(within(screen.getByRole("group", { name: "Rembourser le client" })).getByRole("button", { name: "Rembourser" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Écrivez la raison du remboursement.");

    await userEvent.type(screen.getByLabelText("Raison du remboursement"), "Annulation par le client");
    await userEvent.clear(montant);
    await userEvent.type(montant, "30001");
    await userEvent.click(within(screen.getByRole("group", { name: "Rembourser le client" })).getByRole("button", { name: "Rembourser" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Le client a payé 30 000 FCFA en tout");

    await userEvent.clear(montant);
    await userEvent.type(montant, "25000");
    await userEvent.click(within(screen.getByRole("group", { name: "Rembourser le client" })).getByRole("button", { name: "Rembourser" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Rembourser 25 000 FCFA à Mbarga Aline ?");
    await confirmer("Oui, rembourser");
    expect(await screen.findByText(/Remboursement de 25\s000\sFCFA enregistré\./)).toBeInTheDocument();
    expect(appel(appels, "rembourser")).toEqual({
      token: "jeton-test",
      reservation_id: "r1",
      montant: 25_000,
      mode: "especes",
      motif: "Annulation par le client",
    });
    expect(screen.getByRole("list", { name: "Paiements enregistrés" })).toHaveTextContent("− 25 000 FCFA");
  });

  it("signale un trop-payé et propose de rendre la différence", async () => {
    simuler(resa({ prix_total: 50_000, total_encaisse: 75_000, statut: "arrivee", date_arrivee: J, date_depart: j(2) }), [paiement({ montant: 75_000 })]);
    await ouvrir();
    expect(chiffre("Trop payé, à rendre")).toHaveTextContent("25 000 FCFA");
    await userEvent.click(screen.getByRole("button", { name: "Rembourser" }));
    expect(screen.getByLabelText("Montant (XAF)")).toHaveValue("25000");
  });
});

describe("Fiche réservation : séjour", () => {
  it("enregistre l'arrivée après confirmation", async () => {
    const { appels } = simuler(resa({ date_arrivee: J, date_depart: j(3) }));
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Le client est arrivé" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("Le client s'installe dans la chambre 101.");
    await confirmer("Oui, le client est arrivé");
    expect(await screen.findByText("L'arrivée de Mbarga Aline est enregistrée.")).toBeInTheDocument();
    expect(appel(appels, "enregistrer_arrivee")).toEqual({ token: "jeton-test", id: "r1" });
    expect(screen.getByText("Client présent")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Annuler la réservation" })).not.toBeInTheDocument();
  });

  it("le départ attend que tout soit payé", async () => {
    const { appels } = simuler(resa({ statut: "arrivee", date_arrivee: j(-2), date_depart: J, prix_total: 50_000, total_encaisse: 30_000 }), [paiement()]);
    await ouvrir();
    expect(screen.getByRole("button", { name: "Le client est parti" })).toBeDisabled();
    expect(screen.getByText(/Avant d'enregistrer le départ, encaissez le reste à payer/)).toHaveTextContent("20 000 FCFA");

    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    await confirmer("Oui, encaisser");
    await screen.findByText(/Tout est payé\./);

    await userEvent.click(screen.getByRole("button", { name: "Le client est parti" }));
    await confirmer("Oui, le client est parti");
    expect(await screen.findByText("Le départ de Mbarga Aline est enregistré. La chambre 101 est libre.")).toBeInTheDocument();
    expect(appel(appels, "enregistrer_depart")).toEqual({ token: "jeton-test", id: "r1" });
    expect(screen.getByText("Le séjour est terminé.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Modifier" })).not.toBeInTheDocument();
  });

  it("affiche le refus de Rust pour une arrivée trop en avance", async () => {
    simuler(resa({ date_arrivee: j(10), date_depart: j(12) }), [], {
      enregistrer_arrivee: () => erreurRust({ code: "VALIDATION", details: "arrivée prévue plus tard : modifiez d'abord les dates" }),
    });
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Le client est arrivé" }));
    await confirmer("Oui, le client est arrivé");
    expect(await screen.findByRole("alert")).toHaveTextContent("Arrivée prévue plus tard : modifiez d'abord les dates.");
    expect(screen.getByText("Réservée")).toBeInTheDocument();
  });

  it("annule avec une raison, en prévenant que l'acompte n'est pas rendu tout seul", async () => {
    const { appels } = simuler(resa({ total_encaisse: 30_000 }), [paiement()]);
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Annuler la réservation" }));
    const formulaire = screen.getByRole("group", { name: "Annuler la réservation" });
    await userEvent.click(within(formulaire).getByRole("button", { name: "Annuler la réservation" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Écrivez la raison de l'annulation.");

    await userEvent.type(screen.getByLabelText("Raison de l'annulation"), "Client malade");
    await userEvent.click(within(formulaire).getByRole("button", { name: "Annuler la réservation" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("les 30 000 FCFA déjà payés ne sont pas rendus automatiquement");
    expect(appel(appels, "annuler_reservation")).toBeUndefined();
    await confirmer("Oui, annuler la réservation");

    expect(await screen.findByText(/Réservation annulée\. Le client avait payé 30\s000\sFCFA/)).toBeInTheDocument();
    expect(appel(appels, "annuler_reservation")).toEqual({ token: "jeton-test", id: "r1", motif: "Client malade" });
    expect(screen.getByText("Cette réservation a été annulée.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Encaisser un paiement" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rembourser" })).toBeInTheDocument();
  });

  it("« Ne pas annuler » referme le formulaire", async () => {
    const { appels } = simuler(resa());
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Annuler la réservation" }));
    await userEvent.click(screen.getByRole("button", { name: "Ne pas annuler" }));
    expect(screen.queryByLabelText("Raison de l'annulation")).not.toBeInTheDocument();
    expect(appel(appels, "annuler_reservation")).toBeUndefined();
  });

  it("prolonge le séjour : le prix est recalculé puis reste corrigeable", async () => {
    const { appels } = simuler(resa());
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Modifier" }));
    const prix = screen.getByLabelText("Prix du séjour (XAF)");
    expect(prix).toHaveValue("75000");
    expect(screen.getByText("3 nuits")).toBeInTheDocument();
    await waitFor(() => expect(appels.some((a) => a.commande === "lister_chambres")).toBe(true));

    await userEvent.click(screen.getByRole("button", { name: "Une nuit de plus" }));
    await waitFor(() => expect(prix).toHaveValue("100000"));
    expect(screen.getByText(/Prix recalculé avec le tarif normal/)).toBeInTheDocument();
    expect(screen.getByText(formatJourLong(j(5)))).toBeInTheDocument();
    await userEvent.clear(prix);
    await userEvent.type(prix, "90000");

    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les changements" }));
    expect(screen.getByRole("alertdialog")).toHaveTextContent("prix 90 000 FCFA");
    await confirmer("Oui, enregistrer");
    expect(await screen.findByText("Les changements sont enregistrés.")).toBeInTheDocument();
    expect(appel(appels, "modifier_reservation")).toEqual({
      token: "jeton-test",
      id: "r1",
      donnees: { chambre_id: "c101", date_arrivee: j(1), date_depart: j(5), prix_total: 90_000, notes: null },
    });
    expect(chiffre("Prix du séjour")).toHaveTextContent("90 000 FCFA");
  });

  it("propose la chambre actuelle et les chambres libres ; le jour d'arrivée est figé si le client est là", async () => {
    const { appels } = simuler(resa({ statut: "arrivee", date_arrivee: J, date_depart: j(2), prix_total: 50_000, total_encaisse: 50_000 }));
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Modifier" }));
    const options = within(await screen.findByLabelText("Chambre")).getAllByRole("option");
    await waitFor(() => expect(within(screen.getByLabelText("Chambre")).getAllByRole("option")).toHaveLength(2));
    expect(options[0]).toHaveTextContent("Chambre 101 (actuelle)");
    expect(screen.getByLabelText("Jour d'arrivée : jour")).toBeDisabled();
    expect(screen.getByText("Le client est déjà arrivé : ce jour ne change plus.")).toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Chambre"), "c102");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer les changements" }));
    await confirmer("Oui, enregistrer");
    await waitFor(() => expect(appel(appels, "modifier_reservation")).toMatchObject({ donnees: { chambre_id: "c102", date_arrivee: J } }));
  });

  it("affiche la remarque et revient au tableau", async () => {
    simuler(resa({ notes: "Arrivée tardive vers 23 h" }));
    const onRetour = await ouvrir();
    expect(screen.getByText("Arrivée tardive vers 23 h")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retour au tableau" }));
    expect(onRetour).toHaveBeenCalledOnce();
  });

  it("n'affiche que les actions permises au compte", async () => {
    simuler(resa({ total_encaisse: 30_000 }), [paiement()]);
    await ouvrir(session(["reservation.voir", "paiement.encaisser", "sejour.gerer"], { role_id: "role-reception" }));
    expect(screen.getByRole("button", { name: "Encaisser un paiement" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Le client est arrivé" })).toBeInTheDocument();
    for (const interdit of ["Rembourser", "Modifier", "Annuler la réservation"]) {
      expect(screen.queryByRole("button", { name: interdit })).not.toBeInTheDocument();
    }
  });

  it("explique quand la réservation est introuvable", async () => {
    simuler(resa(), [], { obtenir_reservation: () => erreurRust({ code: "INTROUVABLE", details: "réservation" }) });
    rendreConnecte(<FicheReservation id="r1" onRetour={() => {}} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Élément introuvable (réservation)");
    expect(screen.getByRole("button", { name: "Retour au tableau" })).toBeInTheDocument();
  });
});

describe("Fiche réservation : reçus et factures", () => {
  const imprimer = (nom: RegExp | string) => userEvent.click(screen.getByRole("button", { name: nom }));

  it("imprime le reçu d'un paiement, dit quoi faire et où le retrouver", async () => {
    let docs: ReturnType<typeof documentEmis>[] = [];
    const { appels } = simuler(resa({ total_encaisse: 30_000 }), [paiement()], {
      documents_reservation: () => docs,
      imprimer_recu: () => {
        docs = [documentEmis()];
        return documentEnregistre();
      },
    });
    await ouvrir();
    expect(await screen.findByText(/Aucun reçu ni facture imprimé pour cette réservation/)).toBeInTheDocument();
    await imprimer(/^Imprimer le reçu : acompte de 30\s000\sFCFA$/);

    const avis = await screen.findByText("Le reçu R-2026-0001 est ouvert à l'écran.");
    const bloc = avis.closest('[role="status"]')!;
    expect(bloc).toHaveTextContent("Pour l'imprimer, appuyez sur « Imprimer » dans la fenêtre qui vient de s'ouvrir.");
    expect(bloc).toHaveTextContent("Une copie est rangée dans Documents › Tanana › Reçus.");
    expect(appel(appels, "imprimer_recu")).toEqual({
      token: "jeton-test",
      paiement_id: "p1",
      decalage_utc_minutes: -new Date().getTimezoneOffset(),
    });
    // La liste des documents se met à jour
    const liste = await screen.findByRole("list", { name: "Documents imprimés" });
    expect(liste).toHaveTextContent(/Reçu R-2026-0001 · aujourd'hui à \d+ h \d\d · par Marie/);
  });

  it("après un encaissement, propose d'imprimer le reçu tout de suite", async () => {
    const { appels } = simuler(resa(), [], { imprimer_recu: () => documentEnregistre() });
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Encaisser un paiement" }));
    await userEvent.click(screen.getByRole("button", { name: "Encaisser" }));
    await confirmer("Oui, encaisser");
    await userEvent.click(await screen.findByRole("button", { name: "Imprimer le reçu pour le client" }));
    expect(await screen.findByText("Le reçu R-2026-0001 est ouvert à l'écran.")).toBeInTheDocument();
    expect(appel(appels, "imprimer_recu")).toMatchObject({ paiement_id: "p1" });
    expect(screen.queryByRole("button", { name: "Imprimer le reçu pour le client" })).not.toBeInTheDocument();
  });

  it("après un remboursement aussi", async () => {
    const { appels } = simuler(resa({ total_encaisse: 30_000 }), [paiement()], {
      imprimer_recu: () => documentEnregistre({ numero: "R-2026-0002", paiement_id: "pr" }),
    });
    await ouvrir();
    await userEvent.click(screen.getByRole("button", { name: "Rembourser" }));
    await userEvent.type(screen.getByLabelText("Raison du remboursement"), "Panne");
    await userEvent.click(screen.getByRole("button", { name: "Rembourser" }));
    await confirmer("Oui, rembourser");
    await userEvent.click(await screen.findByRole("button", { name: "Imprimer le reçu pour le client" }));
    await screen.findByText("Le reçu R-2026-0002 est ouvert à l'écran.");
    expect(appel(appels, "imprimer_recu")).toMatchObject({ paiement_id: "pr" });
  });

  it("une réimpression est annoncée comme duplicata", async () => {
    simuler(resa({ total_encaisse: 30_000 }), [paiement()], {
      imprimer_recu: () => documentEnregistre({}, { duplicata: true }),
    });
    await ouvrir();
    await imprimer(/Imprimer le reçu/);
    expect(await screen.findByText(/Ce reçu avait déjà été imprimé : cette copie porte la mention « Duplicata »\./)).toBeInTheDocument();
  });

  it("imprime la facture, et explique quand elle en remplace une autre", async () => {
    const facture = { type_document: "facture" as const, numero: "F-2026-0002", paiement_id: null, remplace_numero: "F-2026-0001" };
    let docs: ReturnType<typeof documentEmis>[] = [documentEmis({ id: "f1", type_document: "facture", numero: "F-2026-0001", paiement_id: null })];
    const { appels } = simuler(resa(), [], {
      documents_reservation: () => docs,
      imprimer_facture: () => {
        docs = [documentEmis({ ...facture, id: "f2" }), ...docs];
        return documentEnregistre(facture);
      },
    });
    await ouvrir();
    await imprimer("Imprimer la facture");
    const avis = await screen.findByText("La facture F-2026-0002 est ouverte à l'écran.");
    expect(avis.closest('[role="status"]')).toHaveTextContent(
      "Elle remplace la facture F-2026-0001, car le séjour ou les paiements ont changé depuis.",
    );
    expect(avis.closest('[role="status"]')).toHaveTextContent("Documents › Tanana › Factures");
    expect(appel(appels, "imprimer_facture")).toMatchObject({ reservation_id: "r1" });
    // L'ancienne facture est marquée « Remplacée »
    const lignes = within(await screen.findByRole("list", { name: "Documents imprimés" })).getAllByRole("listitem");
    await waitFor(() => expect(lignes.length).toBeGreaterThan(0));
    const ancienne = within(screen.getByRole("list", { name: "Documents imprimés" }))
      .getAllByRole("listitem")
      .find((l) => l.textContent?.includes("F-2026-0001"));
    expect(ancienne).toHaveTextContent("Remplacée");
  });

  it("si le PDF ne s'ouvre pas tout seul, dit où il est et ouvre le dossier", async () => {
    const { appels } = simuler(resa(), [], {
      imprimer_facture: () =>
        documentEnregistre({ type_document: "facture", numero: "F-2026-0001", paiement_id: null }, { ouvert: false }),
      ouvrir_dossier_documents: () => null,
    });
    await ouvrir();
    await imprimer("Imprimer la facture");
    const avis = await screen.findByText("La facture F-2026-0001 est prête.");
    expect(avis.closest('[role="status"]')).toHaveTextContent(
      "Le fichier n'a pas pu s'ouvrir tout seul. Vous le trouverez dans Documents › Tanana › Factures.",
    );
    await userEvent.click(screen.getByRole("button", { name: "Ouvrir le dossier des factures" }));
    await waitFor(() => expect(appel(appels, "ouvrir_dossier_documents")).toEqual({ type_document: "facture" }));
  });

  it("affiche clairement un échec d'enregistrement", async () => {
    simuler(resa(), [], {
      imprimer_facture: () => erreurRust({ code: "ENREGISTREMENT_IMPOSSIBLE", details: "disque plein" }),
    });
    await ouvrir();
    await imprimer("Imprimer la facture");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Le fichier n'a pas pu être enregistré sur cet ordinateur. Vérifiez qu'il reste de la place, puis réessayez.",
    );
  });

  it("réservation annulée : plus de facture, mais les reçus restent possibles", async () => {
    simuler(resa({ statut: "annulee", total_encaisse: 30_000 }), [paiement()]);
    await ouvrir();
    expect(screen.queryByRole("button", { name: "Imprimer la facture" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Imprimer le reçu/ })).toBeInTheDocument();
  });

  it("sans le droit d'encaisser : pas de bouton d'impression, mais la liste des documents reste visible", async () => {
    simuler(resa({ total_encaisse: 30_000 }), [paiement()], { documents_reservation: () => [documentEmis()] });
    await ouvrir(session(["reservation.voir"], { role_id: "role-x" }));
    expect(await screen.findByRole("list", { name: "Documents imprimés" })).toHaveTextContent("Reçu R-2026-0001");
    expect(screen.queryByRole("button", { name: /Imprimer/ })).not.toBeInTheDocument();
  });
});

describe("Fiche réservation : taxes", () => {
  const SEJOUR: Taxe = { libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2_000, comprise: false };
  const TVA: Taxe = { libelle: "TVA", calcul: "pourcentage", valeur: 2_000, comprise: true };

  /** Faux serveur où les taxes changent vraiment le total à payer. */
  function simulerAvecTaxes(taxesInitiales: Taxe[], extra: Record<string, Gestionnaire> = {}) {
    const etat = { taxes: taxesInitiales };
    const avec = (r: ReturnType<typeof resa>) => {
      const d = detailSimule(r.prix_total, r.nuits, etat.taxes);
      return resa({ ...r, taxes_ajoutees: d.total_ajoutees, total_a_payer: d.total_a_payer, solde: undefined });
    };
    const base = resa({ total_encaisse: 30_000 });
    const res = simuler(avec(base), [paiement()], {
      taxes_reservation: () => detailSimule(75_000, 3, etat.taxes),
      calculer_taxes: ({ prix, nuits, taxes }) => detailSimule(prix as number, nuits as number, taxes as Taxe[]),
      modifier_taxes_reservation: ({ taxes }) => {
        etat.taxes = taxes as Taxe[];
        return (res.etat.r = avec(base));
      },
      ...extra,
    });
    return res;
  }

  it("montre les taxes et le total à payer avec la taxe ajoutée", async () => {
    simulerAvecTaxes([TVA, SEJOUR]);
    await ouvrir();
    expect(chiffre("Total à payer")).toHaveTextContent(/81\s000\sFCFA/);
    expect(chiffre("Total à payer")).toHaveTextContent(/Chambre 75\s000\sFCFA \+ taxes 6\s000\sFCFA/);
    expect(chiffre("Reste à payer")).toHaveTextContent(/51\s000\sFCFA/);
    const liste = await screen.findByRole("list", { name: "Taxes du séjour" });
    expect(liste).toHaveTextContent("TVA (20 %) · comprise dans le prix");
    expect(liste).toHaveTextContent(/Taxe de séjour \(2\s000\sFCFA par nuit\) · ajoutée au prix\+ 6\s000\sFCFA/);
    expect(screen.getByText("La facture reprend les taxes indiquées dans « Paiement », plus haut.")).toBeInTheDocument();
  });

  it("ajoute une taxe à ce séjour : prévient du nouveau total, puis l'enregistre", async () => {
    const { appels } = simulerAvecTaxes([]);
    await ouvrir();
    expect(await screen.findByText("Aucune taxe sur ce séjour.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Modifier les taxes" }));
    const f = within(screen.getByRole("form", { name: "Taxes de ce séjour" }));
    await userEvent.click(f.getByRole("button", { name: "Ajouter une taxe" }));
    await userEvent.type(f.getByLabelText("Nom de la taxe"), "Taxe de séjour");
    await userEvent.click(f.getByRole("radio", { name: "Montant par nuit" }));
    await userEvent.type(f.getByLabelText("Montant (XAF)"), "2000");
    expect(await f.findByRole("group", { name: "Résultat du calcul" })).toHaveTextContent(/Total à payer81\s000\sFCFA/);
    await userEvent.click(f.getByRole("button", { name: "Enregistrer les taxes" }));

    const dialogue = await screen.findByRole("alertdialog");
    expect(dialogue).toHaveTextContent(/Mbarga Aline devra payer 81\s000\sFCFA en tout, au lieu de 75\s000\sFCFA\./);
    expect(appel(appels, "modifier_taxes_reservation")).toBeUndefined();
    await confirmer("Oui, enregistrer");

    expect(await screen.findByText(/Les taxes sont enregistrées\. Le client doit 81\s000\sFCFA en tout, il reste 51\s000\sFCFA à payer\./)).toBeInTheDocument();
    expect(appel(appels, "modifier_taxes_reservation")).toEqual({
      token: "jeton-test",
      reservation_id: "r1",
      taxes: [{ libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2000, comprise: false }],
    });
    expect(chiffre("Total à payer")).toHaveTextContent(/81\s000\sFCFA/);
  });

  it("une taxe comprise ne change pas le total : pas de question inutile", async () => {
    const { appels } = simulerAvecTaxes([]);
    await ouvrir();
    await userEvent.click(await screen.findByRole("button", { name: "Modifier les taxes" }));
    const f = within(screen.getByRole("form", { name: "Taxes de ce séjour" }));
    await userEvent.click(f.getByRole("button", { name: "Ajouter une taxe" }));
    await userEvent.type(f.getByLabelText("Nom de la taxe"), "TVA");
    await userEvent.type(f.getByLabelText("Pourcentage (%)"), "20");
    await userEvent.click(f.getByRole("radio", { name: "Déjà comprise dans le prix" }));
    await userEvent.click(f.getByRole("button", { name: "Enregistrer les taxes" }));
    await screen.findByText(/Les taxes sont enregistrées/);
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(appel(appels, "modifier_taxes_reservation")).toMatchObject({ taxes: [{ libelle: "TVA", valeur: 2000, comprise: true }] });
  });

  it("« Fermer sans enregistrer » ne change rien", async () => {
    const { appels } = simulerAvecTaxes([SEJOUR]);
    await ouvrir();
    await userEvent.click(await screen.findByRole("button", { name: "Modifier les taxes" }));
    await userEvent.click(screen.getByRole("button", { name: "Retirer cette taxe" }));
    await userEvent.click(screen.getByRole("button", { name: "Fermer sans enregistrer" }));
    expect(appel(appels, "modifier_taxes_reservation")).toBeUndefined();
    expect(await screen.findByRole("list", { name: "Taxes du séjour" })).toHaveTextContent("Taxe de séjour");
  });

  it("sans le droit de modifier les réservations : les taxes se voient mais ne se changent pas", async () => {
    simulerAvecTaxes([SEJOUR]);
    await ouvrir(session(["reservation.voir", "paiement.encaisser"], { role_id: "role-x" }));
    expect(await screen.findByRole("list", { name: "Taxes du séjour" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Modifier les taxes" })).not.toBeInTheDocument();
  });
});
