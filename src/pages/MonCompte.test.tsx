import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { erreurRust, PARAMETRES, rendreConnecte, session, simulerRust, type Appel } from "../test/outils";
import { Coquille } from "./Coquille";
import { MonCompte } from "./MonCompte";

const DE = (appels: Appel[], c: string) => appels.filter((a) => a.commande === c);
const RECEPTION = session(["reservation.voir"], { id: "u-marie", identifiant: "marie", nom: "Marie", role_nom: "Réception" });

function simuler() {
  let aUnPin = false;
  return simulerRust({
    parametres: () => PARAMETRES,
    ma_session: () => ({ ...RECEPTION, utilisateur: { ...RECEPTION.utilisateur, a_un_pin: aUnPin } }),
    definir_mon_pin: ({ mot_de_passe }) => {
      if (mot_de_passe !== "marie2026") erreurRust({ code: "IDENTIFIANTS_INVALIDES" });
      aUnPin = true;
      return null;
    },
    changer_mon_mot_de_passe: ({ actuel }) => (actuel === "marie2026" ? null : erreurRust({ code: "IDENTIFIANTS_INVALIDES" })),
    planning: () => [],
    lister_chambres: () => [],
  });
}

describe("Mon compte", () => {
  it("chaque employé y accède depuis le bas du menu", async () => {
    simuler();
    rendreConnecte(<Coquille nomHotel="Hôtel Test" />, RECEPTION);
    await userEvent.click(await screen.findByRole("button", { name: "Mon compte (mot de passe, PIN)" }));
    expect(await screen.findByRole("heading", { name: "Mon compte" })).toBeInTheDocument();
    expect(screen.getByText(/identifiant « marie »/)).toBeInTheDocument();
  });

  it("choisir son code PIN : chiffres seulement, identiques, pas trop simple, mot de passe exigé", async () => {
    const appels = simuler();
    rendreConnecte(<MonCompte />, RECEPTION);
    const f = within(await screen.findByRole("form", { name: "Code PIN" }));
    expect(f.getByText(/Vous n'avez pas encore de code PIN/)).toBeInTheDocument();
    const pin = f.getByLabelText("Nouveau code PIN");
    const conf = f.getByLabelText("Retapez le code PIN");
    const mdp = f.getByLabelText("Pour confirmer, votre mot de passe");
    const valider = f.getByRole("button", { name: "Enregistrer mon code PIN" });

    await userEvent.type(pin, "12a3456789");
    expect(pin).toHaveValue("123456");
    await userEvent.type(conf, "123456");
    await userEvent.click(valider);
    expect(await f.findByRole("alert")).toHaveTextContent("trop facile à deviner");

    await userEvent.clear(pin);
    await userEvent.type(pin, "4071");
    await userEvent.clear(conf);
    await userEvent.type(conf, "4072");
    await userEvent.click(valider);
    expect(await f.findByRole("alert")).toHaveTextContent("ne sont pas identiques");

    await userEvent.clear(conf);
    await userEvent.type(conf, "4071");
    await userEvent.click(valider);
    expect(await f.findByRole("alert")).toHaveTextContent("tapez votre mot de passe");
    await userEvent.type(mdp, "mauvais1");
    await userEvent.click(valider);
    expect(await f.findByRole("alert")).toHaveTextContent("Identifiant ou mot de passe incorrect");
    expect(DE(appels, "definir_mon_pin")).toHaveLength(1);

    await userEvent.clear(mdp);
    await userEvent.type(mdp, "marie2026");
    await userEvent.click(valider);
    expect(await f.findByText(/vous pouvez maintenant vous connecter avec votre code PIN/)).toBeInTheDocument();
    expect(DE(appels, "definir_mon_pin").at(-1)?.args).toEqual({ token: "jeton-test", mot_de_passe: "marie2026", pin: "4071" });
    await waitFor(() => expect(f.getByText(/Vous avez déjà un code PIN/)).toBeInTheDocument());
    expect(pin).toHaveValue("");
  });

  it("changer son mot de passe", async () => {
    const appels = simuler();
    rendreConnecte(<MonCompte />, RECEPTION);
    const f = within(await screen.findByRole("form", { name: "Changer mon mot de passe" }));
    await userEvent.type(f.getByLabelText("Mot de passe actuel"), "marie2026");
    await userEvent.type(f.getByLabelText("Nouveau mot de passe"), "soleil2027");
    await userEvent.type(f.getByLabelText("Retapez le nouveau mot de passe"), "soleil2028");
    await userEvent.click(f.getByRole("button", { name: "Enregistrer mon mot de passe" }));
    expect(await f.findByRole("alert")).toHaveTextContent("ne sont pas identiques");

    await userEvent.clear(f.getByLabelText("Retapez le nouveau mot de passe"));
    await userEvent.type(f.getByLabelText("Retapez le nouveau mot de passe"), "soleil2027");
    await userEvent.click(f.getByRole("button", { name: "Enregistrer mon mot de passe" }));
    expect(await f.findByText(/Utilisez votre nouveau mot de passe/)).toBeInTheDocument();
    expect(DE(appels, "changer_mon_mot_de_passe")[0]?.args).toEqual({
      token: "jeton-test",
      actuel: "marie2026",
      nouveau: "soleil2027",
    });
  });
});
