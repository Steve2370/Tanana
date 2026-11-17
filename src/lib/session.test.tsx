import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { rendreConnecte, PARAMETRES, session, simulerRust } from "../test/outils";
import { SiPermis, useSession } from "./session";

function Etat() {
  const { session: s, messageSortie } = useSession();
  return <p data-testid="etat">{s ? `connecté:${s.utilisateur.nom}` : `déconnecté:${messageSortie ?? ""}`}</p>;
}

describe("session", () => {
  it("SiPermis n'affiche que ce que la personne a le droit de voir", async () => {
    simulerRust({ parametres: () => PARAMETRES });
    rendreConnecte(
      <>
        <SiPermis permission="chambre.voir">visible</SiPermis>
        <SiPermis permission="journal.voir">caché</SiPermis>
      </>,
      session(["chambre.voir"]),
    );
    expect(await screen.findByText("visible")).toBeInTheDocument();
    expect(screen.queryByText("caché")).not.toBeInTheDocument();
  });

  describe("inactivité", () => {
    beforeEach(() => vi.useFakeTimers({ shouldAdvanceTime: true }));
    afterEach(() => vi.useRealTimers());

    const attendre = (ms: number) => act(async () => vi.advanceTimersByTimeAsync(ms));

    it("prévient une minute avant, puis déconnecte", async () => {
      const appels = simulerRust({ parametres: () => ({ ...PARAMETRES, delai_inactivite_minutes: 2 }), deconnexion: () => null });
      rendreConnecte(<Etat />);
      expect(await screen.findByText("connecté:Beltran")).toBeInTheDocument();

      await attendre(59_000);
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
      await attendre(2_000);
      const dialogue = screen.getByRole("alertdialog", { name: "Êtes-vous toujours là ?" });
      expect(dialogue).toHaveTextContent(/dans \d+ secondes/);
      // « Oui, je suis là » a le focus : Entrée par réflexe ne déconnecte pas
      expect(screen.getByRole("button", { name: "Oui, je suis là" })).toHaveFocus();

      await attendre(60_000);
      expect(screen.getByTestId("etat")).toHaveTextContent("déconnecté:Par sécurité, vous avez été déconnecté après 2 minutes");
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
      expect(appels.some((a) => a.commande === "deconnexion")).toBe(true);
    });

    it("« Oui, je suis là » repart pour un délai complet et prolonge la session côté Rust", async () => {
      const appels = simulerRust({
        parametres: () => ({ ...PARAMETRES, delai_inactivite_minutes: 2 }),
        ma_session: () => session(),
        deconnexion: () => null,
      });
      rendreConnecte(<Etat />);
      await screen.findByText("connecté:Beltran");
      await attendre(70_000);
      await userEvent.click(screen.getByRole("button", { name: "Oui, je suis là" }));
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
      expect(appels.some((a) => a.commande === "ma_session")).toBe(true);

      await attendre(100_000);
      expect(screen.getByTestId("etat")).toHaveTextContent("connecté:Beltran");
    });

    it("une activité au clavier ou à la souris repousse le délai", async () => {
      simulerRust({ parametres: () => ({ ...PARAMETRES, delai_inactivite_minutes: 2 }), ma_session: () => session(), deconnexion: () => null });
      rendreConnecte(<Etat />);
      await screen.findByText("connecté:Beltran");
      await attendre(50_000);
      window.dispatchEvent(new KeyboardEvent("keydown"));
      await attendre(50_000);
      window.dispatchEvent(new Event("pointerdown"));
      await attendre(50_000);
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
      expect(screen.getByTestId("etat")).toHaveTextContent("connecté:Beltran");
    });

    it("pendant le travail, la session Rust est prolongée sans rien demander", async () => {
      const appels = simulerRust({ parametres: () => ({ ...PARAMETRES, delai_inactivite_minutes: 15 }), ma_session: () => session() });
      rendreConnecte(<Etat />);
      await screen.findByText("connecté:Beltran");
      await attendre(5 * 60_000);
      window.dispatchEvent(new KeyboardEvent("keydown"));
      expect(appels.filter((a) => a.commande === "ma_session")).toHaveLength(1);
      window.dispatchEvent(new KeyboardEvent("keydown"));
      await attendre(10);
      expect(appels.filter((a) => a.commande === "ma_session")).toHaveLength(1);
    });
  });
});
