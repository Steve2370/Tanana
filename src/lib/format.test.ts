import { aujourdhui, decalageLocal, formatHeure, formatHorodatage, formatJour, formatMontant, lireMontant, resteDe, resteDesSejours, saisieMontant } from "./format";

/** Intl utilise des espaces insécables : on les normalise pour comparer. */
const n = (s: string) => s.replace(/[  ]/g, " ");

describe("formatMontant", () => {
  it("affiche le franc CFA sans décimales", () => {
    expect(n(formatMontant(25_000, { code: "XAF", decimales: 0 }))).toBe("25 000 FCFA");
  });

  it("convertit les centimes pour le dollar canadien", () => {
    expect(n(formatMontant(125_050, { code: "CAD", decimales: 2 }))).toBe("1 250,50 $CA");
  });

  it("gère zéro et les montants négatifs (trop-perçu)", () => {
    expect(n(formatMontant(0, { code: "XAF", decimales: 0 }))).toBe("0 FCFA");
    expect(n(formatMontant(-5_000, { code: "XAF", decimales: 0 }))).toBe("-5 000 FCFA");
  });

  it("ne plante pas sur un code de devise inconnu", () => {
    expect(formatMontant(1234, { code: "ZZ!", decimales: 2 })).toBe("12.34 ZZ!");
  });
});

describe("lireMontant", () => {
  it.each([
    ["25000", 0, 25_000],
    ["25 000", 0, 25_000],
    ["1 250,50", 2, 125_050],
    ["1250.5", 2, 125_050],
    ["12", 2, 1_200],
    ["0", 0, 0],
  ])("« %s » avec %i décimales → %i", (saisie, decimales, attendu) => {
    expect(lireMontant(saisie, decimales)).toBe(attendu);
  });

  it.each([["abc"], [""], ["-5"], ["12,345"], ["1e5"], ["12,5"]])("refuse « %s »", (saisie) => {
    const decimales = saisie === "12,5" ? 0 : 2;
    expect(lireMontant(saisie, decimales)).toBeNull();
  });

  it("fait l'aller-retour avec saisieMontant", () => {
    expect(saisieMontant(25_000, 0)).toBe("25000");
    expect(saisieMontant(125_050, 2)).toBe("1250,50");
    for (const [m, d] of [[0, 0], [70_000, 0], [18_950, 2], [5, 2]] as const) {
      expect(lireMontant(saisieMontant(m, d), d)).toBe(m);
    }
  });

  it("refuse les montants hors de la précision des nombres JavaScript", () => {
    expect(lireMontant("99999999999999999999", 0)).toBeNull();
  });
});

describe("dates", () => {
  it("formate une date de séjour sans décalage de fuseau", () => {
    expect(formatJour("2026-03-10")).toBe("10 mars 2026");
    expect(formatJour("2026-01-01")).toBe("1 janvier 2026");
    expect(formatJour("n'importe quoi")).toBe("n'importe quoi");
  });

  it("écrit l'heure à la française", () => {
    expect(formatHeure(new Date(2026, 2, 10, 14, 5))).toBe("14 h 05");
    expect(formatHeure(new Date(2026, 2, 10, 9, 30))).toBe("9 h 30");
  });

  it("écrit les horodatages en langage courant", () => {
    const maintenant = new Date(2026, 2, 12, 10, 0); // jeudi 12 mars 2026
    const iso = (j: number, h: number, m: number) => new Date(2026, 2, j, h, m).toISOString();
    expect(formatHorodatage(iso(12, 8, 49), maintenant)).toBe("aujourd'hui à 8 h 49");
    expect(formatHorodatage(iso(11, 17, 2), maintenant)).toBe("hier à 17 h 02");
    expect(formatHorodatage(iso(9, 8, 0), maintenant)).toBe("lundi 9 mars à 8 h 00");
    expect(formatHorodatage(iso(1, 14, 15), maintenant)).toBe("1 mars 2026 à 14 h 15");
    expect(formatHorodatage("pas une date", maintenant)).toBe("pas une date");
  });

  it("donne la date locale du jour au format ISO", () => {
    expect(aujourdhui(new Date(2026, 2, 5, 23, 59))).toBe("2026-03-05");
  });

  it("calcule le décalage horaire local (signe inversé de getTimezoneOffset)", () => {
    const d = new Date();
    expect(decalageLocal(d)).toBe(-d.getTimezoneOffset());
  });
});

describe("reste dû avec les groupes", () => {
  const seule = (solde: number) => ({ solde, solde_groupe: null, groupe_id: null });
  const duGroupe = (id: string, solde: number, solde_groupe: number) => ({ solde, solde_groupe, groupe_id: id });

  it("une chambre de groupe : c'est le reste du groupe qui compte", () => {
    expect(resteDe(seule(20_000))).toBe(20_000);
    expect(resteDe(duGroupe("g1", -35_000, 40_000))).toBe(40_000);
    expect(resteDe(duGroupe("g1", 75_000, 0))).toBe(0);
  });

  it("plusieurs séjours : chaque groupe une fois, les trop-payés ne retirent rien", () => {
    expect(resteDesSejours([])).toBe(0);
    expect(resteDesSejours([seule(20_000), seule(-5_000)])).toBe(20_000);
    expect(resteDesSejours([duGroupe("g1", -25_000, 50_000), duGroupe("g1", 75_000, 50_000), seule(20_000)])).toBe(70_000);
    expect(resteDesSejours([duGroupe("g1", 0, 50_000), duGroupe("g2", 0, 30_000)])).toBe(80_000);
    expect(resteDesSejours([duGroupe("g1", 10_000, -5_000)])).toBe(0);
  });
});
