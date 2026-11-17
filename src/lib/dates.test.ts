import {
  ajouterJours,
  composer,
  decomposer,
  enTeteJour,
  estDateValide,
  formatJourLong,
  formatJourMoyen,
  formatSejour,
  joursDansLeMois,
  joursEntre,
  NOMS_DES_MOIS,
  plage,
  pluriel,
} from "./dates";

describe("calculs de dates", () => {
  it("ajoute des jours en passant les mois et les années", () => {
    expect(ajouterJours("2026-11-13", 1)).toBe("2026-11-14");
    expect(ajouterJours("2026-11-30", 1)).toBe("2026-12-01");
    expect(ajouterJours("2026-12-31", 1)).toBe("2027-01-01");
    expect(ajouterJours("2026-03-01", -1)).toBe("2026-02-28");
    expect(ajouterJours("2028-02-28", 1)).toBe("2028-02-29"); // année bissextile
  });

  it("ne se décale pas aux changements d'heure", () => {
    // Nuits du passage à l'heure d'été et d'hiver (Amérique du Nord et Europe)
    const cas = [
      ["2026-03-07", "2026-03-08"],
      ["2026-03-28", "2026-03-29"],
      ["2026-10-24", "2026-10-25"],
      ["2026-10-31", "2026-11-01"],
    ] as const;
    for (const [jour, lendemain] of cas) {
      expect(ajouterJours(jour, 1)).toBe(lendemain);
      expect(joursEntre(jour, ajouterJours(jour, 3))).toBe(3);
    }
  });

  it("compte les nuits entre deux dates", () => {
    expect(joursEntre("2026-11-13", "2026-11-15")).toBe(2);
    expect(joursEntre("2026-12-30", "2027-01-02")).toBe(3);
    expect(joursEntre("2026-11-15", "2026-11-13")).toBe(-2);
    expect(joursEntre("2026-11-13", "2026-11-13")).toBe(0);
  });

  it("donne une suite de jours", () => {
    expect(plage("2026-11-29", 4)).toEqual(["2026-11-29", "2026-11-30", "2026-12-01", "2026-12-02"]);
  });

  it("connaît la longueur des mois", () => {
    expect(joursDansLeMois(2026, 2)).toBe(28);
    expect(joursDansLeMois(2028, 2)).toBe(29);
    expect(joursDansLeMois(2026, 4)).toBe(30);
    expect(joursDansLeMois(2026, 12)).toBe(31);
  });

  it("recompose une date en corrigeant un jour impossible", () => {
    expect(decomposer("2026-11-03")).toEqual({ annee: 2026, mois: 11, jour: 3 });
    expect(composer(2026, 11, 3)).toBe("2026-11-03");
    expect(composer(2026, 2, 31)).toBe("2026-02-28");
    expect(composer(2028, 2, 31)).toBe("2028-02-29");
  });

  it("reconnaît une date valide", () => {
    expect(estDateValide("2026-11-13")).toBe(true);
    expect(estDateValide("2026-02-30")).toBe(false);
    expect(estDateValide("13/11/2026")).toBe(false);
    expect(estDateValide("")).toBe(false);
  });
});

describe("dates en toutes lettres", () => {
  it("écrit le jour complet", () => {
    expect(formatJourLong("2026-11-13")).toBe("vendredi 13 novembre 2026");
    expect(formatJourMoyen("2026-11-15")).toBe("dimanche 15 novembre");
    expect(formatJourLong("pas une date")).toBe("pas une date");
  });

  it("écrit un séjour", () => {
    expect(formatSejour("2026-11-13", "2026-11-15")).toBe("du vendredi 13 novembre au dimanche 15 novembre 2026");
  });

  it("donne l'en-tête d'une colonne", () => {
    expect(enTeteJour("2026-11-13")).toEqual({ semaine: "ven.", jour: 13, mois: "nov." });
  });

  it("liste les mois et accorde le pluriel", () => {
    expect(NOMS_DES_MOIS).toHaveLength(12);
    expect(NOMS_DES_MOIS[0]).toBe("janvier");
    expect(NOMS_DES_MOIS[11]).toBe("décembre");
    expect(pluriel(1, "nuit")).toBe("1 nuit");
    expect(pluriel(3, "nuit")).toBe("3 nuits");
  });
});
