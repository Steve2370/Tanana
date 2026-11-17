import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { App } from "../App";
import { FournisseurSession } from "../lib/session";
import { erreurRust, PARAMETRES, session, simulerRust } from "../test/outils";

const ETAT = { statut: "prete", initialise: true, nom_hotel: "Hôtel du Lac", message: null, version: "0.1.0" };
const TABLEAU_DE_BORD = {
  parametres: () => PARAMETRES,
  verifier_integrite_journal: () => ({ intacte: true, entrees_verifiees: 3, premiere_rupture: null }),
  journal: () => [],
  sessions_actives: () => [],
};

function demarrer() {
  return render(
    <FournisseurSession>
      <App />
    </FournisseurSession>,
  );
}

beforeEach(() => localStorage.clear());

describe("Connexion", () => {
  it("connecte avec le mot de passe et ouvre l'application", async () => {
    const appels = simulerRust({ etat_application: () => ETAT, connexion: () => session(), ...TABLEAU_DE_BORD });
    demarrer();
    expect(await screen.findByRole("heading", { name: "Hôtel du Lac" })).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Tanana" })).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Identifiant"), "admin");
    await userEvent.type(screen.getByLabelText("Mot de passe"), "admin2026");
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));

    expect(await screen.findByRole("navigation", { name: "Menu principal" })).toBeInTheDocument();
    expect(appels.find((a) => a.commande === "connexion")?.args).toEqual({ identifiant: "admin", mot_de_passe: "admin2026" });
  });

  it("retient l'identifiant (jamais le mot de passe) pour la prochaine fois", async () => {
    simulerRust({ etat_application: () => ETAT, connexion: () => session(), ...TABLEAU_DE_BORD });
    const { unmount } = demarrer();
    await userEvent.type(await screen.findByLabelText("Identifiant"), "  marie ");
    await userEvent.type(screen.getByLabelText("Mot de passe"), "secret123");
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));
    await screen.findByRole("navigation");
    unmount();

    expect(localStorage.getItem("tanana.dernier-identifiant")).toBe("marie");
    expect(JSON.stringify(localStorage)).not.toContain("secret123");
    demarrer();
    expect(await screen.findByLabelText("Identifiant")).toHaveValue("marie");
    expect(screen.getByLabelText("Mot de passe")).toHaveValue("");
  });

  it("explique quoi faire quand le mot de passe est faux", async () => {
    simulerRust({ etat_application: () => ETAT, connexion: () => erreurRust({ code: "IDENTIFIANTS_INVALIDES" }) });
    demarrer();
    await userEvent.type(await screen.findByLabelText("Identifiant"), "admin");
    await userEvent.type(screen.getByLabelText("Mot de passe"), "faux");
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Vérifiez l'orthographe et la touche Majuscule");
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
  });

  it("demande de remplir les champs vides sans appeler Rust", async () => {
    const appels = simulerRust({ etat_application: () => ETAT });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: "Se connecter" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Écrivez votre identifiant.");
    expect(appels.some((a) => a.commande === "connexion")).toBe(false);
  });

  it("prévient quand la touche Majuscule est activée", async () => {
    simulerRust({ etat_application: () => ETAT });
    demarrer();
    const champ = await screen.findByLabelText("Mot de passe");
    await userEvent.click(champ);
    await userEvent.keyboard("{CapsLock}A");
    expect(await screen.findByText(/La touche Majuscule \(Verr. Maj.\) est activée/)).toBeInTheDocument();
  });

  it("permet d'afficher le mot de passe pour vérifier sa saisie", async () => {
    simulerRust({ etat_application: () => ETAT });
    demarrer();
    const champ = await screen.findByLabelText("Mot de passe");
    expect(champ).toHaveAttribute("type", "password");
    await userEvent.click(screen.getByRole("button", { name: "Afficher" }));
    expect(champ).toHaveAttribute("type", "text");
    await userEvent.click(screen.getByRole("button", { name: "Masquer" }));
    expect(champ).toHaveAttribute("type", "password");
  });

  it("affiche l'heure de fin du blocage temporaire", async () => {
    simulerRust({
      etat_application: () => ETAT,
      connexion: () => erreurRust({ code: "COMPTE_BLOQUE", details: { jusqu_a: "2026-03-10T14:15:00.000Z" } }),
    });
    demarrer();
    await userEvent.type(await screen.findByLabelText("Identifiant"), "admin");
    await userEvent.type(screen.getByLabelText("Mot de passe"), "x");
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/Trop d'essais incorrects.*bloqué jusqu'à \d+ h \d\d/);
  });

  it("connexion rapide par PIN avec le clavier à l'écran", async () => {
    const appels = simulerRust({
      etat_application: () => ETAT,
      connexion_pin: () => session(["chambre.voir"], { role_id: "role-reception", role_nom: "Réceptionniste" }),
      parametres: () => PARAMETRES,
    });
    demarrer();
    await userEvent.click(await screen.findByRole("tab", { name: "Code PIN" }));
    await userEvent.type(screen.getByLabelText("Identifiant"), "marie");
    for (const c of ["4", "8", "2", "9"]) await userEvent.click(screen.getByRole("button", { name: c }));
    await userEvent.click(screen.getByRole("button", { name: "← Retour" }));
    await userEvent.click(screen.getByRole("button", { name: "1" }));
    expect(screen.getByLabelText("4 chiffres saisis")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));
    await waitFor(() => expect(appels.some((a) => a.commande === "connexion_pin")).toBe(true));
    expect(appels.find((a) => a.commande === "connexion_pin")?.args).toEqual({ identifiant: "marie", pin: "4821" });
    expect(await screen.findByRole("navigation")).toBeInTheDocument();
  });

  it("le PIN se tape aussi au clavier de l'ordinateur", async () => {
    const appels = simulerRust({ etat_application: () => ETAT, connexion_pin: () => erreurRust({ code: "IDENTIFIANTS_INVALIDES" }) });
    localStorage.setItem("tanana.dernier-identifiant", "marie");
    demarrer();
    await userEvent.click(await screen.findByRole("tab", { name: "Code PIN" }));
    await userEvent.keyboard("1234");
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));
    await waitFor(() => expect(appels.find((a) => a.commande === "connexion_pin")?.args).toEqual({ identifiant: "marie", pin: "1234" }));
  });

  it("Entrée au clavier valide le PIN", async () => {
    const appels = simulerRust({ etat_application: () => ETAT, connexion_pin: () => erreurRust({ code: "IDENTIFIANTS_INVALIDES" }) });
    localStorage.setItem("tanana.dernier-identifiant", "marie");
    demarrer();
    await userEvent.click(await screen.findByRole("tab", { name: "Code PIN" }));
    await userEvent.keyboard("4071{Enter}");
    await waitFor(() => expect(appels.find((a) => a.commande === "connexion_pin")?.args).toEqual({ identifiant: "marie", pin: "4071" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("incorrect");
  });

  it("impose de choisir un mot de passe personnel, avec les règles visibles", async () => {
    let doitChanger = true;
    const appels = simulerRust({
      etat_application: () => ETAT,
      connexion: () => session(undefined, { doit_changer_mdp: true }),
      changer_mon_mot_de_passe: () => {
        doitChanger = false;
        return null;
      },
      ma_session: () => session(undefined, { doit_changer_mdp: doitChanger }),
      ...TABLEAU_DE_BORD,
    });
    demarrer();
    await userEvent.type(await screen.findByLabelText("Identifiant"), "paul");
    await userEvent.type(screen.getByLabelText("Mot de passe"), "temp1234");
    await userEvent.click(screen.getByRole("button", { name: "Se connecter" }));

    expect(await screen.findByText(/mot de passe provisoire/)).toBeInTheDocument();
    expect(appels.some((a) => a.commande === "parametres")).toBe(false);

    await userEvent.type(screen.getByLabelText("Mot de passe provisoire"), "temp1234");
    await userEvent.type(screen.getByLabelText("Nouveau mot de passe"), "court");
    expect(screen.getByText("Au moins 8 caractères").closest("li")).toHaveTextContent("pas encore");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer mon mot de passe" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("ne respecte pas encore toutes les règles");

    await userEvent.clear(screen.getByLabelText("Nouveau mot de passe"));
    await userEvent.type(screen.getByLabelText("Nouveau mot de passe"), "nouveau99");
    expect(screen.getByText("Au moins 8 caractères").closest("li")).toHaveTextContent("respecté");
    await userEvent.type(screen.getByLabelText("Retapez le nouveau mot de passe"), "autre9999");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer mon mot de passe" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("ne sont pas identiques");
    expect(appels.some((a) => a.commande === "changer_mon_mot_de_passe")).toBe(false);

    await userEvent.clear(screen.getByLabelText("Retapez le nouveau mot de passe"));
    await userEvent.type(screen.getByLabelText("Retapez le nouveau mot de passe"), "nouveau99");
    expect(screen.getByText("Les deux mots de passe sont identiques")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer mon mot de passe" }));
    expect(await screen.findByRole("navigation")).toBeInTheDocument();
  });

  it("récupération de l'accès admin par le code de secours", async () => {
    const appels = simulerRust({ etat_application: () => ETAT, recuperer_acces_admin: () => "NOUVO-CODE2-XXXXX-YYYYY" });
    demarrer();
    await userEvent.click(await screen.findByRole("button", { name: /oublié/ }));
    await userEvent.type(screen.getByLabelText("Code de récupération"), "ABCDE-FGHJK-MNPQR-STUVW");
    await userEvent.type(screen.getByLabelText("Identifiant administrateur"), "admin");
    await userEvent.type(screen.getByLabelText("Nouveau mot de passe"), "nouveau123");
    await userEvent.click(screen.getByRole("button", { name: "Enregistrer le nouveau mot de passe" }));
    expect(await screen.findByText("NOUVO-CODE2-XXXXX-YYYYY")).toBeInTheDocument();
    expect(appels.at(-1)?.args).toEqual({
      code: "ABCDE-FGHJK-MNPQR-STUVW",
      identifiant: "admin",
      nouveau_mot_de_passe: "nouveau123",
    });
  });
});
