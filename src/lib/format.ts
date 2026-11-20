/** Formatage pour l'affichage. Les montants sont des entiers dans la plus
 *  petite unité de la devise (ex. centimes pour CAD, francs pour XAF). */

export interface Devise {
  code: string;
  decimales: number;
}

export function formatMontant(montant: number, devise: Devise): string {
  const valeur = montant / 10 ** devise.decimales;
  try {
    return new Intl.NumberFormat("fr-FR", {
      style: "currency",
      currency: devise.code,
      minimumFractionDigits: devise.decimales,
      maximumFractionDigits: devise.decimales,
    }).format(valeur);
  } catch {
    // Code de devise inconnu du navigateur : on affiche quand même
    return `${valeur.toFixed(devise.decimales)} ${devise.code}`;
  }
}

/** « 1 250,50 » → 125050 (pour 2 décimales). `null` si la saisie est invalide. */
export function lireMontant(saisie: string, decimales: number): number | null {
  const propre = saisie.replace(/[\s  ]/g, "").replace(",", ".");
  if (!/^\d+(\.\d+)?$/.test(propre)) return null;
  const [entier = "0", fraction = ""] = propre.split(".");
  if (fraction.length > decimales) return null;
  const montant = Number(entier) * 10 ** decimales + Number(fraction.padEnd(decimales, "0") || "0");
  return Number.isSafeInteger(montant) ? montant : null;
}

/** Montant → texte à mettre dans un champ de saisie : 125050 → « 1250,50 ». */
export function saisieMontant(montant: number, decimales: number): string {
  return (montant / 10 ** decimales).toFixed(decimales).replace(".", ",");
}

const FMT_DATE = new Intl.DateTimeFormat("fr-FR", { day: "numeric", month: "long", year: "numeric" });
const FMT_JOUR_SEMAINE = new Intl.DateTimeFormat("fr-FR", { weekday: "long", day: "numeric", month: "long" });

/** « 2026-03-10 » → « 10 mars 2026 » (date de séjour, sans fuseau). */
export function formatJour(iso: string): string {
  const [a, m, j] = iso.split("-").map(Number);
  if (!a || !m || !j) return iso;
  return FMT_DATE.format(new Date(a, m - 1, j));
}

function lireDate(iso: string | Date): Date | null {
  const d = iso instanceof Date ? iso : new Date(iso);
  return Number.isNaN(d.getTime()) ? null : d;
}

/** Heure à la française, en heure locale : « 14 h 05 ». */
export function formatHeure(iso: string | Date): string {
  const d = lireDate(iso);
  if (!d) return String(iso);
  return `${d.getHours()} h ${String(d.getMinutes()).padStart(2, "0")}`;
}

const debutDuJour = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/**
 * Horodatage en langage courant, en heure locale :
 * « aujourd'hui à 14 h 05 », « hier à 9 h 30 », « lundi 9 mars à 8 h 00 »,
 * puis « 10 mars 2026 à 14 h 05 » au-delà d'une semaine.
 */
export function formatHorodatage(iso: string, maintenant: Date = new Date()): string {
  const d = lireDate(iso);
  if (!d) return iso;
  const ecart = Math.round((debutDuJour(maintenant) - debutDuJour(d)) / 86_400_000);
  const heure = formatHeure(d);
  if (ecart === 0) return `aujourd'hui à ${heure}`;
  if (ecart === 1) return `hier à ${heure}`;
  if (ecart > 1 && ecart < 7) return `${FMT_JOUR_SEMAINE.format(d)} à ${heure}`;
  return `${FMT_DATE.format(d)} à ${heure}`;
}

/** Date locale du jour au format ISO (pour les champs date). */
export function aujourdhui(maintenant: Date = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${maintenant.getFullYear()}-${p(maintenant.getMonth() + 1)}-${p(maintenant.getDate())}`;
}

/** Décalage du fuseau local en minutes (ex. −300 à Montréal en hiver). */
export function decalageLocal(maintenant: Date = new Date()): number {
  return -maintenant.getTimezoneOffset();
}

/** Décimales usuelles pour les devises proposées au premier lancement. */
export const DEVISES: { code: string; nom: string; decimales: number }[] = [
  { code: "XAF", nom: "Franc CFA (CEMAC)", decimales: 0 },
  { code: "XOF", nom: "Franc CFA (UEMOA)", decimales: 0 },
  { code: "MGA", nom: "Ariary malgache", decimales: 0 },
  { code: "CAD", nom: "Dollar canadien", decimales: 2 },
  { code: "EUR", nom: "Euro", decimales: 2 },
  { code: "USD", nom: "Dollar américain", decimales: 2 },
];

/** Ce qui reste dû sur une chambre : pour une chambre de groupe, c'est le reste du groupe entier. */
export function resteDe(r: { solde: number; solde_groupe: number | null }): number {
  return r.solde_groupe ?? r.solde;
}

/**
 * Reste dû sur plusieurs séjours (non annulés) : chaque groupe ne compte qu'une fois,
 * avec son propre reste (les paiements d'un groupe peuvent être sur n'importe quelle chambre).
 */
export function resteDesSejours(liste: { solde: number; solde_groupe: number | null; groupe_id: string | null }[]): number {
  const groupes = new Map<string, number>();
  let reste = 0;
  for (const r of liste) {
    if (r.groupe_id !== null && r.solde_groupe !== null) groupes.set(r.groupe_id, Math.max(0, r.solde_groupe));
    else reste += Math.max(0, r.solde);
  }
  return reste + [...groupes.values()].reduce((a, b) => a + b, 0);
}
