import { erreurRust, session, simulerRust } from "../test/outils";
import { api, FILTRE_JOURNAL_DEFAUT, jetonSession, quandSessionPerdue } from "./api";
import { ErreurTanana } from "./erreurs";

beforeEach(() => {
  jetonSession.definir(null);
  quandSessionPerdue(null);
});

describe("api", () => {
  it("envoie les arguments en snake_case, sans jeton pour la connexion", async () => {
    const appels = simulerRust({ connexion: () => session() });
    await api.connexion("marie", "secret123");
    expect(appels).toEqual([{ commande: "connexion", args: { identifiant: "marie", mot_de_passe: "secret123" } }]);
  });

  it("ajoute le jeton de session aux commandes protégées", async () => {
    const appels = simulerRust({ changer_statut_utilisateur: () => ({}) });
    jetonSession.definir("abc");
    await api.changerStatutUtilisateur("u1", "suspendu");
    expect(appels[0]).toEqual({
      commande: "changer_statut_utilisateur",
      args: { token: "abc", utilisateur_id: "u1", statut: "suspendu" },
    });
  });

  it("complète le filtre du journal avec les valeurs par défaut", async () => {
    const appels = simulerRust({ journal: () => [] });
    jetonSession.definir("abc");
    await api.journal({ sensibles_seulement: true });
    expect(appels[0]!.args.filtre).toEqual({ ...FILTRE_JOURNAL_DEFAUT, sensibles_seulement: true });
  });

  it("transforme les erreurs Rust en ErreurTanana avec message français", async () => {
    simulerRust({ connexion: () => erreurRust({ code: "IDENTIFIANTS_INVALIDES" }) });
    const e = await api.connexion("x", "y").catch((e: unknown) => e);
    expect(e).toBeInstanceOf(ErreurTanana);
    expect((e as ErreurTanana).code).toBe("IDENTIFIANTS_INVALIDES");
    expect((e as ErreurTanana).message).toMatch(/^Identifiant ou mot de passe incorrect\. Vérifiez/);
  });

  it("refuse une commande protégée sans session, sans appeler Rust", async () => {
    const appels = simulerRust({});
    const perdue = vi.fn();
    quandSessionPerdue(perdue);
    await expect(api.listerChambres()).rejects.toMatchObject({ code: "NON_AUTHENTIFIE" });
    expect(appels).toHaveLength(0);
    expect(perdue).toHaveBeenCalledOnce();
  });

  it("oublie le jeton et prévient quand la session expire côté Rust", async () => {
    simulerRust({ lister_chambres: () => erreurRust({ code: "SESSION_EXPIREE" }) });
    const perdue = vi.fn();
    quandSessionPerdue(perdue);
    jetonSession.definir("abc");
    await expect(api.listerChambres()).rejects.toMatchObject({ code: "SESSION_EXPIREE" });
    expect(jetonSession.lire()).toBeNull();
    expect(perdue).toHaveBeenCalledWith(expect.objectContaining({ code: "SESSION_EXPIREE" }));
  });

  it("garde la session sur une simple permission refusée", async () => {
    simulerRust({ lister_utilisateurs: () => erreurRust({ code: "PERMISSION_REFUSEE", details: { permission: "utilisateur.gerer" } }) });
    const perdue = vi.fn();
    quandSessionPerdue(perdue);
    jetonSession.definir("abc");
    await expect(api.listerUtilisateurs()).rejects.toMatchObject({ code: "PERMISSION_REFUSEE" });
    expect(jetonSession.lire()).toBe("abc");
    expect(perdue).not.toHaveBeenCalled();
  });
});
