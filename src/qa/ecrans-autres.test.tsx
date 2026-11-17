/**
 * Tests ajoutés par la passe QA (série 3 : reste de l'application). Voir .qa/mutants3.md.
 */
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { App } from "../App";
import { ClavierPin } from "../components/ui";
import { jetonSession } from "../lib/api";
import { FournisseurSession } from "../lib/session";
import { Journal } from "../pages/Journal";
import { PARAMETRES, rendreConnecte, simulerRust, type Appel } from "../test/outils";

const DE = (appels: Appel[], c: string) => appels.filter((a) => a.commande === c);

describe("QA · clé manquante", () => {
  beforeEach(() => jetonSession.definir(null));

  it("« Ouvrir les données » reste grisé tant que la clé est vide (P04)", async () => {
    const appels = simulerRust({
      etat_application: () => ({
        statut: "cle_manquante",
        initialise: true,
        consultation_distance: false,
        nom_hotel: null,
        message: null,
        version: "0.1.0",
      }),
      fournir_cle: () => null,
    });
    render(
      <FournisseurSession>
        <App />
      </FournisseurSession>,
    );
    const ouvrir = await screen.findByRole("button", { name: "Ouvrir les données" });
    expect(ouvrir).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Clé (64 caractères)"), "   ");
    expect(ouvrir).toBeDisabled();
    await userEvent.click(ouvrir);
    expect(DE(appels, "fournir_cle")).toHaveLength(0);
  });
});

describe("QA · déconnexion automatique", () => {
  beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
  afterEach(() => vi.useRealTimers());

  it("un geste involontaire pendant l'avertissement ne repousse pas la déconnexion (P06)", async () => {
    const appels = simulerRust({ parametres: () => ({ ...PARAMETRES, delai_inactivite_minutes: 2 }), deconnexion: () => null, ma_session: () => null });
    rendreConnecte(<p>au travail</p>);
    expect(await screen.findByText("au travail")).toBeInTheDocument();
    await act(async () => vi.advanceTimersByTimeAsync(61_000));
    expect(screen.getByRole("alertdialog", { name: "Êtes-vous toujours là ?" })).toBeInTheDocument();
    fireEvent.pointerDown(window);
    fireEvent.keyDown(window, { key: "a" });
    await act(async () => vi.advanceTimersByTimeAsync(2_000));
    expect(screen.getByRole("alertdialog", { name: "Êtes-vous toujours là ?" })).toBeInTheDocument();
    // Seul « Oui, je suis là » compte : à l'heure dite, la session se ferme
    await act(async () => vi.advanceTimersByTimeAsync(60_000));
    expect(DE(appels, "deconnexion")).toHaveLength(1);
  });
});

describe("QA · historique", () => {
  it("« Seulement les actions à vérifier » demande bien les seules actions sensibles (P09)", async () => {
    const appels = simulerRust({
      parametres: () => PARAMETRES,
      journal: () => [],
      lister_utilisateurs: () => [],
      verifier_integrite_journal: () => ({ intacte: true, entrees_verifiees: 0, premiere_rupture: null }),
    });
    rendreConnecte(<Journal />);
    await userEvent.click(await screen.findByLabelText("Seulement les actions à vérifier"));
    await waitFor(() =>
      expect(DE(appels, "journal").at(-1)?.args).toMatchObject({ filtre: { sensibles_seulement: true } }),
    );
  });
});

describe("QA · pavé PIN", () => {
  function Pave() {
    const [v, setV] = useState("");
    return (
      <>
        <ClavierPin valeur={v} onChange={setV} />
        <output data-testid="pin">{v}</output>
      </>
    );
  }

  it("s'arrête à 6 chiffres, même si on continue de taper (P11)", async () => {
    render(<Pave />);
    for (const c of "12345678") await userEvent.click(screen.getByRole("button", { name: c }));
    expect(screen.getByTestId("pin")).toHaveTextContent(/^123456$/);
    expect(screen.getByLabelText("6 chiffres saisis")).toBeInTheDocument();
  });
});
