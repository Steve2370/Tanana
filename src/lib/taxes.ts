/** Taxes : mots affichés, et passage entre la saisie (texte) et les valeurs envoyées à Rust. */
import type { CalculTaxe } from "../bindings/CalculTaxe";
import type { Taxe } from "../bindings/Taxe";
import { formatMontant, lireMontant, saisieMontant, type Devise } from "./format";

export const LIBELLE_CALCUL: Record<CalculTaxe, string> = {
  pourcentage: "En pourcentage (%)",
  par_nuit: "Montant par nuit",
  par_sejour: "Montant pour le séjour",
};

export const CALCULS = Object.keys(LIBELLE_CALCUL) as CalculTaxe[];

/** 1925 → « 19,25 % » ; 2000 → « 20 % ». */
export function formatPourcentage(centiemes: number): string {
  return `${String(centiemes / 100).replace(".", ",")} %`;
}

/** « TVA (20 %) », « Taxe de séjour (2 000 Ar par nuit) », « Frais de dossier ». */
export function libelleTaxe(t: Taxe, devise: Devise): string {
  switch (t.calcul) {
    case "pourcentage":
      return `${t.libelle} (${formatPourcentage(t.valeur)})`;
    case "par_nuit":
      return `${t.libelle} (${formatMontant(t.valeur, devise)} par nuit)`;
    case "par_sejour":
      return t.libelle;
  }
}

/** Une ligne de l'éditeur : la valeur reste du texte tant qu'elle est en cours de saisie. */
export interface TaxeSaisie {
  cle: number;
  libelle: string;
  calcul: CalculTaxe;
  saisie: string;
  comprise: boolean;
}

let compteur = 0;
export const nouvelleCle = () => ++compteur;

export function versSaisie(t: Taxe, devise: Devise): TaxeSaisie {
  return {
    cle: nouvelleCle(),
    libelle: t.libelle,
    calcul: t.calcul,
    saisie: t.calcul === "pourcentage" ? String(t.valeur / 100).replace(".", ",") : saisieMontant(t.valeur, devise.decimales).replace(/,0+$/, ""),
    comprise: t.comprise,
  };
}

/** Lit une ligne ; renvoie un message clair si elle est incomplète. */
export function lireTaxe(s: TaxeSaisie, devise: Devise): { taxe: Taxe } | { erreur: string } {
  const libelle = s.libelle.trim();
  if (!libelle) return { erreur: "écrivez le nom de la taxe (par exemple : TVA, taxe de séjour)." };
  if (s.calcul === "pourcentage") {
    const valeur = lireMontant(s.saisie.replace("%", ""), 2);
    if (valeur === null || valeur <= 0) return { erreur: "écrivez le pourcentage, par exemple 18 ou 19,25." };
    if (valeur > 10_000) return { erreur: "le pourcentage ne peut pas dépasser 100 %." };
    return { taxe: { libelle, calcul: s.calcul, valeur, comprise: s.comprise } };
  }
  const valeur = lireMontant(s.saisie, devise.decimales);
  if (valeur === null || valeur <= 0) return { erreur: "écrivez le montant en chiffres, par exemple 2000." };
  return { taxe: { libelle, calcul: s.calcul, valeur, comprise: s.comprise } };
}

/** Toutes les lignes, ou la première erreur, qui dit de quelle case il s'agit : « Taxe 2 (TVA) : … ». */
export function lireTaxes(lignes: TaxeSaisie[], devise: Devise): { taxes: Taxe[] } | { erreur: string } {
  const taxes: Taxe[] = [];
  for (const [i, l] of lignes.entries()) {
    const r = lireTaxe(l, devise);
    if ("erreur" in r) {
      const nom = l.libelle.trim();
      return { erreur: `Taxe ${i + 1}${nom ? ` (${nom})` : ""} : ${r.erreur}` };
    }
    taxes.push(r.taxe);
  }
  return { taxes };
}
