import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Parametres as P } from "../bindings/Parametres";
import type { Taxe } from "../bindings/Taxe";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust } from "../test/outils";
import { detailSimule } from "../test/reservations";
import { Parametres } from "./Parametres";

function simuler(depart: P = PARAMETRES, taxes: Taxe[] = []) {
  const etat = { p: depart, taxes };
  const appels = simulerRust({
    parametres: () => etat.p,
    taxes_hotel: () => etat.taxes,
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
    definir_taxes_hotel: ({ taxes }) => (etat.taxes = taxes as Taxe[]),
    calculer_taxes: ({ prix, nuits, taxes }) => detailSimule(prix as number, nuits as number, taxes as Taxe[]),
    ma_session: () => session(),
    modifier_parametres: ({ donnees }) => {
      const d = donnees as P;
      if (d.email_hotel && !d.email_hotel.includes("@")) erreurRust({ code: "VALIDATION", details: "email de l'hôtel invalide" });
      return (etat.p = d);
    },
  });
  return { appels, etat };
}

const formulaire = () => screen.findByRole("form", { name: "Informations de l'hôtel" });

describe("Réglages : informations imprimées sur les reçus et factures", () => {
  it("enregistre l'adresse, le téléphone, l'email et les mentions", async () => {
    const { appels } = simuler();
    rendreConnecte(<Parametres />);
    const f = within(await formulaire());
    expect(f.getByText("Sur les reçus et factures")).toBeInTheDocument();
    await userEvent.type(f.getByLabelText("Adresse (facultatif)"), "12 rue du Lac{Enter}Bonapriso, Douala");
    await userEvent.type(f.getByLabelText("Téléphone (facultatif)"), "+237 233 00 00 00");
    await userEvent.type(f.getByLabelText("Email (facultatif)"), "  accueil@hotel.cm ");
    await userEvent.type(f.getByLabelText("Mentions en bas de page (facultatif)"), "NIU : M0123");
    await userEvent.click(f.getByRole("button", { name: "Enregistrer" }));

    expect(await f.findByText(/C'est enregistré\. Les prochains reçus et factures porteront ces informations\./)).toBeInTheDocument();
    expect(appels.filter((a) => a.commande === "modifier_parametres").at(-1)?.args).toEqual({
      token: "jeton-test",
      donnees: {
        ...PARAMETRES,
        adresse_hotel: "12 rue du Lac\nBonapriso, Douala",
        telephone_hotel: "+237 233 00 00 00",
        email_hotel: "accueil@hotel.cm",
        mentions_documents: "NIU : M0123",
      },
    });
  });

  it("reprend les informations déjà enregistrées, et un champ vidé est envoyé comme absent", async () => {
    const { appels } = simuler({ ...PARAMETRES, telephone_hotel: "699 00 00 00", mentions_documents: "RCCM 1" });
    rendreConnecte(<Parametres />);
    const f = within(await formulaire());
    await waitFor(() => expect(f.getByLabelText("Téléphone (facultatif)")).toHaveValue("699 00 00 00"));
    await userEvent.clear(f.getByLabelText("Mentions en bas de page (facultatif)"));
    await userEvent.click(f.getByRole("button", { name: "Enregistrer" }));
    await f.findByText(/C'est enregistré/);
    expect(appels.filter((a) => a.commande === "modifier_parametres").at(-1)?.args).toMatchObject({
      donnees: { telephone_hotel: "699 00 00 00", mentions_documents: null, adresse_hotel: null },
    });
  });

  it("affiche le refus de Rust, et l'efface dès qu'on corrige", async () => {
    simuler();
    rendreConnecte(<Parametres />);
    const f = within(await formulaire());
    await userEvent.type(f.getByLabelText("Email (facultatif)"), "hotel.cm");
    await userEvent.click(f.getByRole("button", { name: "Enregistrer" }));
    expect(await f.findByRole("alert")).toHaveTextContent("Email de l'hôtel invalide.");
    await userEvent.type(f.getByLabelText("Email (facultatif)"), "x");
    expect(f.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("n'apparaît pas sans le droit de modifier les paramètres", async () => {
    simuler();
    rendreConnecte(<Parametres />, session(["sauvegarde.faire"], { role_id: "role-x" }));
    await screen.findByRole("heading", { name: "Réglages" });
    expect(screen.queryByRole("form", { name: "Informations de l'hôtel" })).not.toBeInTheDocument();
  });
});

describe("Réglages : taxes de l'hôtel", () => {
  const carte = async () => within(await screen.findByRole("form", { name: "Taxes de l'hôtel" }));

  it("ajoute une taxe de séjour par nuit et une TVA comprise, avec l'exemple calculé", async () => {
    const { appels } = simuler();
    rendreConnecte(<Parametres />);
    const f = await carte();
    expect(f.getByText("Aucune taxe : le prix de la chambre est le prix final.")).toBeInTheDocument();

    await userEvent.click(f.getByRole("button", { name: "Ajouter une taxe" }));
    const t1 = within(f.getByRole("group", { name: "Taxe 1" }));
    await userEvent.type(t1.getByLabelText("Nom de la taxe"), "Taxe de séjour");
    await userEvent.click(t1.getByRole("radio", { name: "Montant par nuit" }));
    await userEvent.type(t1.getByLabelText("Montant (XAF)"), "2000");

    await userEvent.click(f.getByRole("button", { name: "Ajouter une taxe" }));
    const t2 = within(f.getByRole("group", { name: "Taxe 2" }));
    await userEvent.type(t2.getByLabelText("Nom de la taxe"), "TVA");
    await userEvent.type(t2.getByLabelText("Pourcentage (%)"), "19,25");
    await userEvent.click(t2.getByRole("radio", { name: "Déjà comprise dans le prix" }));

    // L'exemple montre ce que paie le client
    const exemple = await f.findByRole("group", { name: "Résultat du calcul" });
    expect(exemple).toHaveTextContent(/Exemple : séjour de 2 nuits à 20\s000\sFCFA/);
    expect(exemple).toHaveTextContent(/Total à payer24\s000\sFCFA/);

    await userEvent.click(f.getByRole("button", { name: "Enregistrer les taxes" }));
    // Le message apparaît à côté du bouton (dans le formulaire), pas en haut de la carte
    expect(await (await carte()).findByText(/Les taxes sont enregistrées/)).toBeInTheDocument();
    expect(appels.filter((a) => a.commande === "definir_taxes_hotel").at(-1)?.args).toEqual({
      token: "jeton-test",
      taxes: [
        { libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2000, comprise: false },
        { libelle: "TVA", calcul: "pourcentage", valeur: 1925, comprise: true },
      ],
    });
  });

  it("reprend les taxes existantes, en retire une, et refuse une ligne incomplète sans rien envoyer", async () => {
    const { appels } = simuler(PARAMETRES, [
      { libelle: "TVA", calcul: "pourcentage", valeur: 2000, comprise: true },
      { libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2000, comprise: false },
    ]);
    rendreConnecte(<Parametres />);
    const f = await carte();
    expect(within(f.getByRole("group", { name: "Taxe 1" })).getByLabelText("Pourcentage (%)")).toHaveValue("20");
    await userEvent.click(within(f.getByRole("group", { name: "Taxe 1" })).getByRole("button", { name: "Retirer cette taxe" }));
    expect(f.queryByRole("group", { name: "Taxe 2" })).not.toBeInTheDocument();

    await userEvent.click(f.getByRole("button", { name: "Ajouter une taxe" }));
    await userEvent.click(f.getByRole("button", { name: "Enregistrer les taxes" }));
    expect(f.getByRole("alert")).toHaveTextContent("Taxe 2 : écrivez le nom de la taxe");
    expect(appels.some((a) => a.commande === "definir_taxes_hotel")).toBe(false);

    await userEvent.click(within(f.getByRole("group", { name: "Taxe 2" })).getByRole("button", { name: "Retirer cette taxe" }));
    expect(f.queryByRole("alert")).not.toBeInTheDocument();
    await userEvent.click(f.getByRole("button", { name: "Enregistrer les taxes" }));
    await waitFor(() =>
      expect(appels.filter((a) => a.commande === "definir_taxes_hotel").at(-1)?.args).toMatchObject({
        taxes: [{ libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2000, comprise: false }],
      }),
    );
  });
});

describe("Réglages : le travail à distance a sa carte", () => {
  it("est proposée à l'administrateur", async () => {
    simuler();
    rendreConnecte(<Parametres />);
    expect(await screen.findByRole("form", { name: "Relier l'hôtel au serveur" })).toBeInTheDocument();
  });
});

describe("Réglages : clé de protection des données", () => {
  it("s'affiche par groupes de 4 sur deux lignes, pour la recopier sans erreur", async () => {
    const cle = "3ad442fb0269ec164acf7f89fce12e84bd2df4482866cdc6c0fef3115fcd3c4f";
    simulerRust({
      parametres: () => PARAMETRES,
      ma_session: () => session(),
      taxes_hotel: () => [],
      exporter_cle_base: ({ mot_de_passe }) => (mot_de_passe === "admin2026" ? cle : erreurRust({ code: "IDENTIFIANTS_INVALIDES" })),
    });
    rendreConnecte(<Parametres />);
    const carte = (await screen.findByText("Clé de protection des données")).closest("section")!;
    await userEvent.type(within(carte).getByLabelText("Par sécurité, retapez votre mot de passe"), "admin2026");
    await userEvent.click(within(carte).getByRole("button", { name: "Afficher la clé" }));
    const affichee = await within(carte).findByTestId("cle-base");
    expect(affichee).toHaveTextContent("3ad4 42fb 0269 ec16 4acf 7f89 fce1 2e84");
    expect(affichee).toHaveTextContent("bd2d f448 2866 cdc6 c0fe f311 5fcd 3c4f");
  });
});
