import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { Chambre } from "../bindings/Chambre";
import { rendreConnecte, PARAMETRES, session, simulerRust } from "../test/outils";
import { Chambres } from "./Chambres";

const CHAMBRE: Chambre = {
  id: "c1",
  numero: "101",
  type_id: "t1",
  type_nom: "Standard",
  prix_nuit_defaut: 25_000,
  etage: 1,
  statut: "disponible",
  notes: null,
};

describe("Chambres", () => {
  it("affiche les prix dans la devise de l'hôtel", async () => {
    simulerRust({ parametres: () => PARAMETRES, lister_types_chambre: () => [], lister_chambres: () => [CHAMBRE] });
    rendreConnecte(<Chambres />);
    expect(await screen.findByText(/25\s000\sFCFA/)).toBeInTheDocument();
  });

  it("convertit le prix saisi en plus petite unité de la devise", async () => {
    const appels = simulerRust({
      parametres: () => ({ ...PARAMETRES, devise: "CAD", decimales_devise: 2 }),
      lister_types_chambre: () => [],
      lister_chambres: () => [],
      creer_type_chambre: () => ({ id: "t1", nom: "Suite", prix_nuit_defaut: 18_950, capacite: 2, actif: true }),
    });
    rendreConnecte(<Chambres />);
    await userEvent.type(await screen.findByLabelText("Nom du type"), "Suite");
    await userEvent.type(screen.getByLabelText("Prix par nuit"), "189,50");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter le type" }));
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "creer_type_chambre")?.args).toMatchObject({
        donnees: { nom: "Suite", prix_nuit_defaut: 18_950, capacite: 2 },
      }),
    );
  });

  it("refuse un prix illisible sans appeler Rust", async () => {
    const appels = simulerRust({ parametres: () => PARAMETRES, lister_types_chambre: () => [], lister_chambres: () => [] });
    rendreConnecte(<Chambres />);
    await userEvent.type(await screen.findByLabelText("Nom du type"), "Suite");
    await userEvent.type(screen.getByLabelText("Prix par nuit"), "beaucoup");
    await userEvent.click(screen.getByRole("button", { name: "Ajouter le type" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Le prix n'est pas lisible");
    expect(appels.some((a) => a.commande === "creer_type_chambre")).toBe(false);
  });

  it("met une chambre en maintenance", async () => {
    const appels = simulerRust({
      parametres: () => PARAMETRES,
      lister_types_chambre: () => [],
      lister_chambres: () => [CHAMBRE],
      modifier_chambre: () => ({ ...CHAMBRE, statut: "maintenance" }),
    });
    rendreConnecte(<Chambres />);
    await userEvent.selectOptions(await screen.findByLabelText("Statut de la chambre 101"), "maintenance");
    await waitFor(() =>
      expect(appels.find((a) => a.commande === "modifier_chambre")?.args).toMatchObject({
        id: "c1",
        donnees: { numero: "101", statut: "maintenance" },
      }),
    );
  });

  it("en lecture seule sans la permission de gérer", async () => {
    simulerRust({ parametres: () => PARAMETRES, lister_types_chambre: () => [], lister_chambres: () => [CHAMBRE] });
    rendreConnecte(<Chambres />, session(["chambre.voir"], { role_id: "role-reception" }));
    expect(await screen.findByText("Peut être louée")).toBeInTheDocument();
    expect(screen.queryByLabelText("Statut de la chambre 101")).not.toBeInTheDocument();
    expect(screen.queryByText("Nouveau type de chambre")).not.toBeInTheDocument();
  });
});
