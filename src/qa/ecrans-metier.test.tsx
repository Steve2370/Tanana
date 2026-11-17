/**
 * Tests ajoutés par la passe QA (série 2 : écrans métier). Voir .qa/mutants2.md.
 */
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Bouton, Champ, useConfirmation } from "../components/ui";
import { aujourdhui } from "../lib/format";
import { Caisse } from "../pages/caisse/Caisse";
import { PARAMETRES, rendreConnecte, simulerRust, type Appel } from "../test/outils";

const DE = (appels: Appel[], c: string) => appels.filter((a) => a.commande === c);

describe("QA · caisse", () => {
  it("une dépense de 0 est refusée avant toute confirmation (N07)", async () => {
    const appels = simulerRust({
      parametres: () => PARAMETRES,
      rapport_caisse: () => ({
        debut: aujourdhui(),
        fin: aujourdhui(),
        encaissements: 0,
        encaissements_par_mode: [],
        remboursements: 0,
        depenses: 0,
        net: 0,
      }),
      lister_paiements: () => [],
      lister_depenses: () => [],
      saisir_depense: () => null,
    });
    rendreConnecte(<Caisse />);
    await userEvent.click(await screen.findByRole("button", { name: "Ajouter une dépense" }));
    const formulaire = screen.getByRole("group", { name: "Nouvelle dépense" });
    for (const zero of ["0", "0,00", "000"]) {
      await userEvent.clear(within(formulaire).getByLabelText("Montant (XAF)"));
      await userEvent.type(within(formulaire).getByLabelText("Montant (XAF)"), zero);
      await userEvent.click(within(formulaire).getByRole("button", { name: "Enregistrer la dépense" }));
      expect(await within(formulaire).findByRole("alert")).toHaveTextContent("Le montant n'est pas lisible");
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    }
    expect(DE(appels, "saisir_depense")).toHaveLength(0);
  });
});

/** Un écran minimal qui demande une confirmation, comme le font les pages. */
function AvecConfirmation() {
  const { confirmer, fenetre } = useConfirmation();
  return (
    <>
      <Bouton onClick={() => void confirmer({ titre: "Supprimer ?", message: "Sûr ?", libelleConfirmer: "Oui" })}>
        Demander
      </Bouton>
      <Bouton variante="secondaire">Après</Bouton>
      {fenetre}
    </>
  );
}

describe("QA · fenêtre de confirmation au clavier", () => {
  it("Tab et Maj+Tab restent dans la fenêtre (N14)", async () => {
    render(<AvecConfirmation />);
    await userEvent.click(screen.getByRole("button", { name: "Demander" }));
    const fenetre = screen.getByRole("alertdialog");
    for (let i = 0; i < 5; i++) {
      await userEvent.tab();
      expect(fenetre).toContainElement(document.activeElement as HTMLElement);
    }
    for (let i = 0; i < 5; i++) {
      await userEvent.tab({ shift: true });
      expect(fenetre).toContainElement(document.activeElement as HTMLElement);
    }
  });
  it("une fois fermée, le focus revient sur le bouton qui l'a ouverte", async () => {
    render(<AvecConfirmation />);
    const demander = screen.getByRole("button", { name: "Demander" });
    await userEvent.click(demander);
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(document.activeElement).toBe(demander);

    await userEvent.click(demander);
    await userEvent.click(screen.getByRole("button", { name: "Oui" }));
    expect(document.activeElement).toBe(demander);
  });
});

describe("QA · champs", () => {
  it("un champ en erreur est signalé aux lecteurs d'écran, avec son message (N15)", () => {
    render(<Champ libelle="Courriel" erreur="Adresse incomplète" />);
    const champ = screen.getByLabelText("Courriel");
    expect(champ).toHaveAttribute("aria-invalid", "true");
    expect(champ).toHaveAccessibleDescription(/Adresse incomplète/);
  });
});
