/**
 * Tests de régression fixes, issus des défauts trouvés par la QA
 * (mutations et fuzzing). Chacun échoue sur le code d'avant la correction.
 */
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { entreePourValider } from "../components/ui";
import { jetonSession } from "../lib/api";
import { Reservations } from "../pages/reservations/Reservations";
import { PARAMETRES, rendreConnecte, simulerRust } from "../test/outils";
import { CHAMBRES } from "../test/reservations";

beforeEach(() => jetonSession.definir(null));

describe("Constat · Entrée appuyée plusieurs fois pendant un envoi", () => {
  it("le nouveau client n'est créé qu'une fois, même avec Entrée tapée trois fois", async () => {
    const appels = simulerRust({
      parametres: () => PARAMETRES,
      lister_chambres: () => CHAMBRES,
      planning: () => [],
      chambres_disponibles: () => [CHAMBRES[0]],
      rechercher_clients: () => [],
      creer_client: () => new Promise(() => {}), // Rust met du temps à répondre
    });
    rendreConnecte(<Reservations />);
    await userEvent.click(await screen.findByRole("button", { name: "Nouvelle réservation" }));
    await userEvent.type(screen.getByLabelText("Chercher un client déjà venu"), "Ngono");
    await userEvent.click(await screen.findByRole("button", { name: "C'est un nouveau client" }));
    await userEvent.type(screen.getByLabelText("Prénom"), "Paul{Enter}{Enter}{Enter}");
    expect(appels.filter((a) => a.commande === "creer_client")).toHaveLength(1);
  });

  it("une touche Entrée maintenue (répétition du clavier) ne valide qu'une fois", () => {
    const valider = vi.fn();
    render(
      <div role="group" onKeyDown={entreePourValider(valider)}>
        <input aria-label="Champ" />
      </div>,
    );
    const champ = screen.getByLabelText("Champ");
    champ.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    champ.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, repeat: true }));
    champ.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, repeat: true }));
    expect(valider).toHaveBeenCalledTimes(1);
  });
});
