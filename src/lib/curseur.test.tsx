import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { Coquille } from "../pages/Coquille";
import { PARAMETRES, rendreConnecte, simulerRust } from "../test/outils";
import { estSaisie, installerCurseurAuto, premierChamp } from "./curseur";

let desinstaller: () => void = () => {};
beforeEach(() => {
  desinstaller = installerCurseurAuto();
});
afterEach(() => desinstaller());

function AvecFormulaire({ autreChamp = false }: { autreChamp?: boolean }) {
  const [ouvert, setOuvert] = useState(false);
  return (
    <div>
      {autreChamp && <input aria-label="Recherche" />}
      <button type="button" onClick={() => setOuvert(true)}>
        Nouveau
      </button>
      {ouvert && (
        <form aria-label="Formulaire">
          <input type="checkbox" aria-label="Case" />
          <input aria-label="Nom" />
          <input aria-label="Téléphone" />
        </form>
      )}
    </div>
  );
}

describe("curseur dans les champs", () => {
  it("un formulaire qui s'ouvre reçoit le curseur dans son premier champ de texte", async () => {
    render(<AvecFormulaire />);
    await userEvent.click(screen.getByRole("button", { name: "Nouveau" }));
    await waitFor(() => expect(screen.getByLabelText("Nom")).toHaveFocus());
    // On tape directement, sans cliquer
    await userEvent.keyboard("Rakoto");
    expect(screen.getByLabelText("Nom")).toHaveValue("Rakoto");
  });

  it("ne prend jamais la main à quelqu'un qui tape déjà dans un champ", async () => {
    render(<AvecFormulaire autreChamp />);
    const recherche = screen.getByLabelText("Recherche");
    await userEvent.click(recherche);
    // Le formulaire s'ouvre pendant que la personne est dans la recherche (ouverture au clavier)
    screen.getByRole("button", { name: "Nouveau" }).click();
    await screen.findByRole("form", { name: "Formulaire" });
    await new Promise((r) => setTimeout(r, 20));
    expect(recherche).toHaveFocus();
  });

  it("ignore les champs désactivés, cachés ou en lecture seule", () => {
    document.body.innerHTML = `
      <div id="r">
        <input aria-label="a" disabled />
        <input aria-label="b" readonly />
        <div hidden><input aria-label="c" /></div>
        <input type="checkbox" aria-label="d" />
        <textarea aria-label="e"></textarea>
      </div>`;
    expect(premierChamp(document.getElementById("r")!)?.getAttribute("aria-label")).toBe("e");
    expect(estSaisie(document.querySelector('[aria-label="d"]'))).toBe(false);
    expect(estSaisie(document.querySelector("textarea"))).toBe(true);
    document.body.innerHTML = "";
  });

  it("changer d'écran par le menu met le curseur dans le premier champ du nouvel écran", async () => {
    simulerRust({
      parametres: () => PARAMETRES,
      logo_hotel: () => null,
      rechercher_clients: () => [],
      journal: () => [],
      sessions_actives: () => [],
      planning: () => [],
      lister_chambres: () => [],
      lister_types_chambre: () => [],
      infos_mise_a_jour: () => ({ version: "0.2.0", configuree: false }),
    });
    rendreConnecte(<Coquille nomHotel="Hôtel Test" />);
    await userEvent.click(await screen.findByRole("button", { name: "Clients" }));
    const champ = await screen.findByPlaceholderText("Nom, prénom, téléphone ou email");
    await waitFor(() => expect(champ).toHaveFocus());
  });
});
