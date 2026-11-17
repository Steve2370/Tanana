import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { jetonSession } from "../lib/api";
import { chercherMiseAJour, redemarrer, type NouvelleVersion } from "../lib/miseAJour";
import { Coquille } from "../pages/Coquille";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Appel, type Gestionnaire } from "../test/outils";
import { BandeauMiseAJour, CarteMisesAJour } from "./miseAJour";

vi.mock("../lib/miseAJour", () => ({
  chercherMiseAJour: vi.fn(),
  redemarrer: vi.fn(),
  PREMIERE_RECHERCHE_MS: 0,
  RECHERCHE_MS: 1_000_000_000,
}));

const chercher = vi.mocked(chercherMiseAJour);
const relancer = vi.mocked(redemarrer);
const DE = (appels: Appel[], c: string) => appels.filter((a) => a.commande === c);

function version(installer: NouvelleVersion["installer"] = async (p) => p(100)): NouvelleVersion {
  return { version: "0.2.0", notes: "Restauration des copies de sécurité.", installer };
}

function simuler(configuree: boolean, autres: Record<string, Gestionnaire> = {}) {
  return simulerRust({
    parametres: () => PARAMETRES,
    ma_session: () => session(),
    infos_mise_a_jour: () => ({ version: "0.1.0", configuree }),
    sauvegarder_avant_mise_a_jour: () => "/s/tanana-avant-mise-a-jour-0.2.0.db",
    ...autres,
  });
}

beforeEach(() => {
  chercher.mockReset();
  relancer.mockReset();
  jetonSession.definir(null);
});

describe("Bandeau de mise à jour", () => {
  it("une version de développement ne cherche rien", async () => {
    const appels = simuler(false);
    rendreConnecte(<BandeauMiseAJour />);
    await waitFor(() => expect(DE(appels, "infos_mise_a_jour")).toHaveLength(1));
    await new Promise((r) => setTimeout(r, 20));
    expect(chercher).not.toHaveBeenCalled();
  });

  it("propose, fait une copie, installe avec la progression puis redémarre", async () => {
    let suite: (() => void) | null = null;
    chercher.mockResolvedValue(
      version(async (p) => {
        p(0);
        p(45);
        await new Promise<void>((r) => (suite = r));
        p(100);
      }),
    );
    const appels = simuler(true);
    rendreConnecte(<BandeauMiseAJour />);
    const bandeau = await screen.findByRole("region", { name: "Nouvelle version" });
    expect(bandeau).toHaveTextContent("Une nouvelle version de Tanana est prête : 0.2.0");
    expect(bandeau).toHaveTextContent("Restauration des copies de sécurité.");

    await userEvent.click(within(bandeau).getByRole("button", { name: "Installer maintenant" }));
    expect(bandeau).toHaveTextContent("personne ne doit être en train d'encaisser");
    await userEvent.click(within(bandeau).getByRole("button", { name: "Oui, installer et redémarrer" }));
    expect(await within(bandeau).findByText("Téléchargement de la nouvelle version : 45 %")).toBeInTheDocument();
    expect(DE(appels, "sauvegarder_avant_mise_a_jour")[0]?.args).toEqual({ token: "jeton-test", version: "0.2.0" });
    expect(relancer).not.toHaveBeenCalled();

    suite!();
    await waitFor(() => expect(relancer).toHaveBeenCalledOnce());
    expect(bandeau).toHaveTextContent("Tanana redémarre");
  });

  it("un échec ne change rien et se dit simplement", async () => {
    chercher.mockResolvedValue(
      version(async () => {
        throw new Error("connexion interrompue");
      }),
    );
    simuler(true);
    rendreConnecte(<BandeauMiseAJour />);
    const bandeau = await screen.findByRole("region", { name: "Nouvelle version" });
    await userEvent.click(within(bandeau).getByRole("button", { name: "Installer maintenant" }));
    await userEvent.click(within(bandeau).getByRole("button", { name: "Oui, installer et redémarrer" }));
    expect(await within(bandeau).findByText("La mise à jour n'a pas pu se faire")).toBeInTheDocument();
    expect(bandeau).toHaveTextContent("n'est pas authentique");
    expect(bandeau).toHaveTextContent("Rien n'a changé");
    expect(bandeau).toHaveTextContent("Détail pour le support : connexion interrompue");
    expect(relancer).not.toHaveBeenCalled();
  });

  it("si la copie de sécurité échoue, rien n'est téléchargé", async () => {
    const installer = vi.fn();
    chercher.mockResolvedValue(version(installer));
    simuler(true, {
      sauvegarder_avant_mise_a_jour: () => erreurRust({ code: "BASE", details: "disque plein" }),
    });
    rendreConnecte(<BandeauMiseAJour />);
    const bandeau = await screen.findByRole("region", { name: "Nouvelle version" });
    await userEvent.click(within(bandeau).getByRole("button", { name: "Installer maintenant" }));
    await userEvent.click(within(bandeau).getByRole("button", { name: "Oui, installer et redémarrer" }));
    expect(await within(bandeau).findByText("La mise à jour n'a pas pu se faire")).toBeInTheDocument();
    expect(bandeau).toHaveTextContent("disque plein");
    expect(installer).not.toHaveBeenCalled();
  });

  it("« Plus tard » cache le bandeau ; pas d'Internet : rien ne s'affiche", async () => {
    chercher.mockResolvedValueOnce(version());
    simuler(true);
    const { unmount } = rendreConnecte(<BandeauMiseAJour />);
    const bandeau = await screen.findByRole("region", { name: "Nouvelle version" });
    await userEvent.click(within(bandeau).getByRole("button", { name: "Plus tard" }));
    expect(screen.queryByRole("region", { name: "Nouvelle version" })).not.toBeInTheDocument();
    unmount();

    chercher.mockRejectedValueOnce(new Error("hors ligne"));
    simuler(true);
    rendreConnecte(<BandeauMiseAJour />);
    await waitFor(() => expect(chercher).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("region", { name: "Nouvelle version" })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("la réception n'est pas dérangée : seuls les administrateurs voient les mises à jour", async () => {
    chercher.mockResolvedValue(version());
    const appels = simuler(true, {
      ma_session: () => session(["reservation.voir"]),
      planning: () => [],
      lister_chambres: () => [],
    });
    rendreConnecte(<Coquille nomHotel="Hôtel Test" />, session(["reservation.voir"]));
    expect(await screen.findByRole("navigation", { name: "Menu principal" })).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 20));
    expect(DE(appels, "infos_mise_a_jour")).toHaveLength(0);
    expect(screen.queryByRole("region", { name: "Nouvelle version" })).not.toBeInTheDocument();
  });
});

describe("Réglages › Mises à jour", () => {
  it("version de développement : le dit simplement", async () => {
    simuler(false);
    rendreConnecte(<CarteMisesAJour />);
    expect(await screen.findByText(/Version installée/)).toHaveTextContent("0.1.0");
    expect(screen.getByText(/Version de développement/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Vérifier maintenant" })).not.toBeInTheDocument();
  });

  it("vérifie à la demande : à jour, pas d'Internet, ou nouvelle version", async () => {
    chercher.mockResolvedValueOnce(null).mockRejectedValueOnce(new Error("hors ligne")).mockResolvedValueOnce(version());
    simuler(true);
    render(<CarteMisesAJour />);
    const verifier = await screen.findByRole("button", { name: "Vérifier maintenant" });
    await userEvent.click(verifier);
    expect(await screen.findByText("Tanana est à jour.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Vérifier maintenant" }));
    expect(await screen.findByText(/Impossible de vérifier pour l'instant/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Vérifier maintenant" }));
    expect(await screen.findByText("Une nouvelle version de Tanana est prête : 0.2.0", { exact: false })).toBeInTheDocument();
  });
});
