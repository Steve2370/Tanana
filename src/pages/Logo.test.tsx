import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { vi } from "vitest";

import type { Logo } from "../bindings/Logo";
import type { NouveauLogo } from "../bindings/NouveauLogo";
import { FournisseurLogo, ImageRefusee, preparerLogo, tailleReduite, urlLogo } from "../lib/logoHotel";
import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust } from "../test/outils";
import { Coquille } from "./Coquille";
import { Parametres } from "./Parametres";

// Le dessin sur canvas n'existe pas dans les tests : on remplace la préparation de l'image.
vi.mock("../lib/logoHotel", async (original) => ({
  ...(await original<typeof import("../lib/logoHotel")>()),
  preparerLogo: vi.fn(),
}));
const preparer = vi.mocked(preparerLogo);

const PNG: NouveauLogo = { format: "png", donnees: "iVBORw0KGgo=" };
const LOGO: Logo = { ...PNG, largeur: 300, hauteur: 200 };

function simuler(depart: Logo | null = null) {
  const etat = { logo: depart };
  const appels = simulerRust({
    parametres: () => PARAMETRES,
    ma_session: () => session(),
    taxes_hotel: () => [],
    etat_distance: () => ({ active: false, relais: null, hotel_id: null, en_attente: 0, active_le: null, dernier_envoi_le: null, derniere_tentative_le: null, derniere_erreur: null }),
    logo_hotel: () => etat.logo,
    definir_logo: ({ image }) => {
      const i = image as NouveauLogo | null;
      if (i?.donnees === "abime") erreurRust({ code: "VALIDATION", details: "cette image est illisible : choisissez un fichier PNG ou JPEG" });
      return (etat.logo = i ? { ...i, largeur: 300, hauteur: 200 } : null);
    },
  });
  return { appels, etat };
}

const carte = async () => (await screen.findByText("Logo de l'hôtel")).closest("section")!;
const fichier = (nom = "logo.png", type = "image/png") => new File(["x"], nom, { type });

function rendre() {
  rendreConnecte(
    <FournisseurLogo>
      <Parametres />
    </FournisseurLogo>,
  );
}

describe("Réglages : logo de l'hôtel", () => {
  beforeEach(() => preparer.mockReset());

  it("sans logo : le dit, propose d'en choisir un, montre l'aperçu puis l'enregistre", async () => {
    const { appels } = simuler();
    preparer.mockResolvedValue(PNG);
    rendre();
    const c = await carte();
    expect(within(c).getByText("Aucun logo")).toBeInTheDocument();
    expect(within(c).queryByRole("button", { name: /Retirer le logo/ })).not.toBeInTheDocument();

    await userEvent.upload(within(c).getByLabelText("Image du logo"), fichier());
    expect(preparer).toHaveBeenCalledTimes(1);
    const apercu = await within(c).findByRole("img", { name: "Nouveau logo, pas encore enregistré" });
    expect(apercu).toHaveAttribute("src", urlLogo(PNG));
    expect(appels.some((a) => a.commande === "definir_logo")).toBe(false); // rien d'enregistré avant le clic

    await userEvent.click(within(c).getByRole("button", { name: "Enregistrer ce logo" }));
    expect(await within(c).findByText(/Le logo est enregistré/)).toBeInTheDocument();
    expect(appels.find((a) => a.commande === "definir_logo")?.args.image).toEqual(PNG);
    expect(within(c).getByRole("img", { name: "Logo actuel de l'hôtel" })).toHaveAttribute("src", urlLogo(LOGO));
    expect(within(c).getByRole("button", { name: /Changer le logo/ })).toBeInTheDocument();
  });

  it("« Annuler » abandonne l'image choisie sans rien enregistrer", async () => {
    const { appels } = simuler();
    preparer.mockResolvedValue(PNG);
    rendre();
    const c = await carte();
    await userEvent.upload(within(c).getByLabelText("Image du logo"), fichier());
    await userEvent.click(await within(c).findByRole("button", { name: "Annuler" }));
    expect(within(c).getByText("Aucun logo")).toBeInTheDocument();
    expect(appels.some((a) => a.commande === "definir_logo")).toBe(false);
  });

  it("une image refusée par l'interface ou par Rust : message clair, rien de changé", async () => {
    const { etat } = simuler();
    preparer.mockRejectedValueOnce(new ImageRefusee("Ce fichier n'est pas une image PNG ou JPEG. Choisissez une autre image."));
    rendre();
    const c = await carte();
    await userEvent.upload(within(c).getByLabelText("Image du logo"), fichier("photo.png"));
    expect(await within(c).findByText(/n'est pas une image PNG ou JPEG/)).toBeInTheDocument();

    preparer.mockResolvedValueOnce({ format: "png", donnees: "abime" });
    await userEvent.upload(within(c).getByLabelText("Image du logo"), fichier());
    expect(within(c).queryByText(/n'est pas une image PNG ou JPEG/)).not.toBeInTheDocument(); // l'ancien message part
    await userEvent.click(await within(c).findByRole("button", { name: "Enregistrer ce logo" }));
    expect(await within(c).findByText(/illisible/)).toBeInTheDocument();
    expect(etat.logo).toBeNull();
  });

  it("retire le logo après confirmation ; « Non » ne change rien", async () => {
    const { appels } = simuler(LOGO);
    rendre();
    const c = await carte();
    expect(await within(c).findByRole("img", { name: "Logo actuel de l'hôtel" })).toBeInTheDocument();

    await userEvent.click(within(c).getByRole("button", { name: "Retirer le logo" }));
    const dialogue = await screen.findByRole("alertdialog");
    await userEvent.click(within(dialogue).getByRole("button", { name: "Annuler" }));
    expect(appels.some((a) => a.commande === "definir_logo")).toBe(false);

    await userEvent.click(within(c).getByRole("button", { name: "Retirer le logo" }));
    await userEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Oui, retirer le logo" }));
    expect(await within(c).findByText("Le logo est retiré.")).toBeInTheDocument();
    expect(appels.find((a) => a.commande === "definir_logo")?.args.image).toBeNull();
    expect(within(c).getByText("Aucun logo")).toBeInTheDocument();
  });

  it("n'apparaît pas sans le droit de modifier les réglages", async () => {
    simuler(LOGO);
    rendreConnecte(<Parametres />, session(["sauvegarde.faire"], { role_id: "role-gerant" }));
    await screen.findByText("Copie de sécurité");
    expect(screen.queryByText("Logo de l'hôtel")).not.toBeInTheDocument();
  });
});

describe("Menu : logo en haut à gauche", () => {
  const nav = () => screen.findByRole("navigation", { name: "Menu principal" });

  it("montre le logo de l'hôtel à côté de son nom", async () => {
    simuler(LOGO);
    rendreConnecte(<Coquille nomHotel="Hôtel du Lac" />);
    const menu = await nav();
    await waitFor(() => expect(menu.querySelector("img[src^='data:']")).toHaveAttribute("src", urlLogo(LOGO)));
    expect(within(menu).getByText("Hôtel du Lac")).toBeInTheDocument();
  });

  it("sans logo, garde le symbole de Tanana ; une erreur de chargement ne casse rien", async () => {
    simulerRust({ parametres: () => PARAMETRES, logo_hotel: () => erreurRust({ code: "BASE", details: "panne" }) });
    rendreConnecte(<Coquille nomHotel="Hôtel du Lac" />);
    const menu = await nav();
    expect(within(menu).getByText("Hôtel du Lac")).toBeInTheDocument();
    expect(menu.querySelector("img[src^='data:']")).toBeNull();
  });

  it("change tout de suite quand l'administrateur enregistre un nouveau logo", async () => {
    simuler();
    preparer.mockResolvedValue(PNG);
    rendreConnecte(<Coquille nomHotel="Hôtel du Lac" />);
    await userEvent.click(await screen.findByRole("button", { name: "Réglages" }));
    const c = await carte();
    await userEvent.upload(within(c).getByLabelText("Image du logo"), fichier());
    await userEvent.click(await within(c).findByRole("button", { name: "Enregistrer ce logo" }));
    const menu = await nav();
    await waitFor(() => expect(menu.querySelector("img[src^='data:']")).toHaveAttribute("src", urlLogo(LOGO)));
  });
});

describe("Préparation de l'image", () => {
  it("réduit au plus grand côté sans déformer, et n'agrandit jamais", () => {
    expect(tailleReduite(1200, 600)).toEqual({ largeur: 600, hauteur: 300 });
    expect(tailleReduite(300, 1500)).toEqual({ largeur: 120, hauteur: 600 });
    expect(tailleReduite(200, 100)).toEqual({ largeur: 200, hauteur: 100 });
    expect(tailleReduite(5000, 1)).toEqual({ largeur: 600, hauteur: 1 });
    expect(tailleReduite(0, 0)).toEqual({ largeur: 1, hauteur: 1 });
  });

  it("refuse un fichier qui n'est pas une image", async () => {
    const { preparerLogo: vrai } = await vi.importActual<typeof import("../lib/logoHotel")>("../lib/logoHotel");
    await expect(vrai(fichier("lettre.pdf", "application/pdf"))).rejects.toThrow(/pas une image PNG ou JPEG/);
    await expect(vrai(fichier("image.gif", "image/gif"))).rejects.toBeInstanceOf(Error);
  });
});
