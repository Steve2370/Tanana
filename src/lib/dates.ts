/**
 * Dates de séjour : des chaînes « AAAA-MM-JJ », sans heure ni fuseau horaire
 * (une nuit d'hôtel n'a pas de fuseau). Tous les calculs passent par l'UTC
 * pour ne jamais être décalés par un changement d'heure.
 */

const JOUR_MS = 86_400_000;

function versUtc(iso: string): number {
  const [a, m, j] = iso.split("-").map(Number);
  return Date.UTC(a ?? 1970, (m ?? 1) - 1, j ?? 1);
}

function depuisUtc(ms: number): string {
  return new Date(ms).toISOString().slice(0, 10);
}

export function estDateValide(iso: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(iso) && depuisUtc(versUtc(iso)) === iso;
}

export function ajouterJours(iso: string, n: number): string {
  return depuisUtc(versUtc(iso) + n * JOUR_MS);
}

/** Nombre de nuits entre deux dates (négatif si `fin` est avant `debut`). */
export function joursEntre(debut: string, fin: string): number {
  return Math.round((versUtc(fin) - versUtc(debut)) / JOUR_MS);
}

/** Les `n` dates à partir de `debut`. */
export function plage(debut: string, n: number): string[] {
  return Array.from({ length: n }, (_, i) => ajouterJours(debut, i));
}

export function joursDansLeMois(annee: number, mois: number): number {
  return new Date(Date.UTC(annee, mois, 0)).getUTCDate();
}

export function decomposer(iso: string): { annee: number; mois: number; jour: number } {
  const [annee = 1970, mois = 1, jour = 1] = iso.split("-").map(Number);
  return { annee, mois, jour };
}

/** Recompose une date ; un jour trop grand est ramené au dernier jour du mois (31 → 28 février). */
export function composer(annee: number, mois: number, jour: number): string {
  const j = Math.min(jour, joursDansLeMois(annee, mois));
  const p = (n: number) => String(n).padStart(2, "0");
  return `${annee}-${p(mois)}-${p(j)}`;
}

const fmt = (options: Intl.DateTimeFormatOptions) => new Intl.DateTimeFormat("fr-FR", { ...options, timeZone: "UTC" });
const LONG = fmt({ weekday: "long", day: "numeric", month: "long", year: "numeric" });
const MOYEN = fmt({ weekday: "long", day: "numeric", month: "long" });
const SEMAINE = fmt({ weekday: "short" });
const MOIS_COURT = fmt({ month: "short" });
const MOIS_LONG = fmt({ month: "long" });

/** « vendredi 13 novembre 2026 » */
export function formatJourLong(iso: string): string {
  return estDateValide(iso) ? LONG.format(versUtc(iso)) : iso;
}

/** « vendredi 13 novembre » */
export function formatJourMoyen(iso: string): string {
  return estDateValide(iso) ? MOYEN.format(versUtc(iso)) : iso;
}

/** Pour l'en-tête d'une colonne du tableau : « ven. », « 13 », « nov. » */
export function enTeteJour(iso: string): { semaine: string; jour: number; mois: string } {
  const t = versUtc(iso);
  return { semaine: SEMAINE.format(t), jour: decomposer(iso).jour, mois: MOIS_COURT.format(t) };
}

export const NOMS_DES_MOIS: string[] = Array.from({ length: 12 }, (_, i) => MOIS_LONG.format(Date.UTC(2026, i, 1)));

/** « du vendredi 13 au dimanche 15 novembre 2026 » */
export function formatSejour(arrivee: string, depart: string): string {
  return `du ${formatJourMoyen(arrivee)} au ${formatJourLong(depart)}`;
}

export function pluriel(n: number, mot: string): string {
  return `${n} ${mot}${n > 1 ? "s" : ""}`;
}

/** Lundi de la semaine qui contient ce jour. */
export function debutSemaine(iso: string): string {
  const jourSemaine = new Date(versUtc(iso)).getUTCDay(); // 0 = dimanche
  return ajouterJours(iso, -((jourSemaine + 6) % 7));
}

/** Premier jour du mois qui contient ce jour. */
export function debutMois(iso: string): string {
  const { annee, mois } = decomposer(iso);
  return composer(annee, mois, 1);
}

/** Premier jour du mois suivant. */
export function debutMoisSuivant(iso: string): string {
  const { annee, mois } = decomposer(iso);
  return mois === 12 ? composer(annee + 1, 1, 1) : composer(annee, mois + 1, 1);
}
