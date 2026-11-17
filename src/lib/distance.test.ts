import { LONGUEUR_CLE, caracteresCle, depuis, nettoyerAdresse, nettoyerIdentifiantHotel } from "./distance";

describe("consultation à distance : saisies tolérantes", () => {
  it("nettoie l'adresse du serveur", () => {
    expect(nettoyerAdresse("  relais.tanana.app/ ")).toBe("https://relais.tanana.app");
    expect(nettoyerAdresse("http://127.0.0.1:8080//")).toBe("http://127.0.0.1:8080");
    expect(nettoyerAdresse("HTTPS://Relais.exemple.com")).toBe("HTTPS://Relais.exemple.com");
    expect(nettoyerAdresse("   ")).toBe("");
    // Serveur d'essai sur le même ordinateur : http
    expect(nettoyerAdresse("127.0.0.1:8787")).toBe("http://127.0.0.1:8787");
    expect(nettoyerAdresse("localhost:8787/")).toBe("http://localhost:8787");
    expect(nettoyerAdresse("localhost.exemple.com")).toBe("https://localhost.exemple.com");
  });

  it("remet l'identifiant de l'hôtel en forme, avec ou sans tiret", () => {
    expect(nettoyerIdentifiantHotel("hk7qf29x")).toBe("HK7Q-F29X");
    expect(nettoyerIdentifiantHotel(" HK7Q F29X ")).toBe("HK7Q-F29X");
    expect(nettoyerIdentifiantHotel("hk7q-f29x")).toBe("HK7Q-F29X");
    expect(nettoyerIdentifiantHotel("abc")).toBe("ABC");
    // Zéro tapé pour la lettre O, un pour I, huit pour B
    expect(nettoyerIdentifiantHotel("af0j wrfx")).toBe("AFOJ-WRFX");
    expect(nettoyerIdentifiantHotel("1b8c-defg")).toBe("IBBC-DEFG");
  });

  it("compte les caractères utiles de la clé secrète", () => {
    expect(caracteresCle("IVNG-I2WV-JNBA-4R3C-LQU7-ZI2E-SI")).toBe(LONGUEUR_CLE);
    expect(caracteresCle("k7qf 29xm abcd")).toBe(12);
  });

  it("dit depuis quand en mots simples", () => {
    const t = Date.parse("2026-11-14T12:00:00Z");
    expect(depuis("2026-11-14T11:59:45Z", t)).toBe("à l'instant");
    expect(depuis("2026-11-14T11:59:00Z", t)).toBe("il y a 1 minute");
    expect(depuis("2026-11-14T11:35:00Z", t)).toBe("il y a 25 minutes");
    expect(depuis("2026-11-14T09:00:00Z", t)).toBe("il y a 3 heures");
    expect(depuis("2026-11-10T12:00:00Z", t)).toBe("il y a 4 jours");
    expect(depuis("pas une date", t)).toBe("");
  });
});
