import { formatPourcentage, libelleTaxe, lireTaxe, lireTaxes, versSaisie, type TaxeSaisie } from "./taxes";

const XAF = { code: "XAF", decimales: 0 };
const EUR = { code: "EUR", decimales: 2 };
const ligne = (p: Partial<TaxeSaisie>): TaxeSaisie => ({ cle: 1, libelle: "TVA", calcul: "pourcentage", saisie: "20", comprise: false, ...p });
const espaces = (s: string) => s.replace(/\s/g, " ");

describe("taxes : saisie et affichage", () => {
  it("lit un pourcentage avec ou sans virgule, en centièmes", () => {
    expect(lireTaxe(ligne({ saisie: "20" }), XAF)).toEqual({ taxe: { libelle: "TVA", calcul: "pourcentage", valeur: 2000, comprise: false } });
    expect(lireTaxe(ligne({ saisie: "19,25" }), XAF)).toMatchObject({ taxe: { valeur: 1925 } });
    expect(lireTaxe(ligne({ saisie: "5.5 %" }), XAF)).toMatchObject({ taxe: { valeur: 550 } });
  });

  it("lit un montant dans la devise de l'hôtel", () => {
    expect(lireTaxe(ligne({ libelle: " Taxe de séjour ", calcul: "par_nuit", saisie: "2 000" }), XAF)).toEqual({
      taxe: { libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2000, comprise: false },
    });
    expect(lireTaxe(ligne({ calcul: "par_sejour", saisie: "1,50" }), EUR)).toMatchObject({ taxe: { valeur: 150 } });
  });

  it("explique ce qui ne va pas, et dans quelle case", () => {
    expect(lireTaxe(ligne({ libelle: " " }), XAF)).toEqual({ erreur: "écrivez le nom de la taxe (par exemple : TVA, taxe de séjour)." });
    expect(lireTaxe(ligne({ saisie: "abc" }), XAF)).toEqual({ erreur: "écrivez le pourcentage, par exemple 18 ou 19,25." });
    expect(lireTaxe(ligne({ saisie: "0" }), XAF)).toHaveProperty("erreur");
    expect(lireTaxe(ligne({ saisie: "150" }), XAF)).toEqual({ erreur: "le pourcentage ne peut pas dépasser 100 %." });
    expect(lireTaxe(ligne({ calcul: "par_nuit", saisie: "deux mille" }), XAF)).toEqual({ erreur: "écrivez le montant en chiffres, par exemple 2000." });
    // Dans une liste, le message dit de quelle taxe il s'agit
    expect(lireTaxes([ligne({}), ligne({ libelle: "" })], XAF)).toEqual({
      erreur: "Taxe 2 : écrivez le nom de la taxe (par exemple : TVA, taxe de séjour).",
    });
    expect(lireTaxes([ligne({ libelle: "TVA", saisie: "" })], XAF)).toEqual({
      erreur: "Taxe 1 (TVA) : écrivez le pourcentage, par exemple 18 ou 19,25.",
    });
    expect(lireTaxes([], XAF)).toEqual({ taxes: [] });
  });

  it("repasse en saisie sans perdre la valeur", () => {
    expect(versSaisie({ libelle: "TVA", calcul: "pourcentage", valeur: 1925, comprise: true }, XAF)).toMatchObject({ saisie: "19,25", comprise: true });
    expect(versSaisie({ libelle: "T", calcul: "par_nuit", valeur: 2000, comprise: false }, XAF).saisie).toBe("2000");
    expect(versSaisie({ libelle: "T", calcul: "par_nuit", valeur: 150, comprise: false }, EUR).saisie).toBe("1,50");
    expect(versSaisie({ libelle: "T", calcul: "par_nuit", valeur: 200, comprise: false }, EUR).saisie).toBe("2");
  });

  it("affiche la taxe en mots", () => {
    expect(formatPourcentage(1925)).toBe("19,25 %");
    expect(formatPourcentage(2000)).toBe("20 %");
    expect(libelleTaxe({ libelle: "TVA", calcul: "pourcentage", valeur: 2000, comprise: true }, XAF)).toBe("TVA (20 %)");
    expect(espaces(libelleTaxe({ libelle: "Taxe de séjour", calcul: "par_nuit", valeur: 2000, comprise: false }, { code: "MGA", decimales: 0 }))).toBe(
      "Taxe de séjour (2 000 MGA par nuit)",
    );
    expect(libelleTaxe({ libelle: "Frais", calcul: "par_sejour", valeur: 500, comprise: false }, XAF)).toBe("Frais");
  });
});
