/** Consultation à distance : petites aides partagées par l'hôtel et l'administrateur. */

/** Serveur de l'hôtel, déjà rempli partout. Peut être changé au moment de
 *  construire l'application (variable VITE_TANANA_RELAIS). */
export const ADRESSE_RELAIS_DEFAUT: string =
  (import.meta.env.VITE_TANANA_RELAIS ?? "").trim() || "https://relais.monkadran.com";

/** Code d'invitation du serveur, donné à la construction (variable VITE_TANANA_CODE_INVITATION). */
export const CODE_INVITATION_DEFAUT: string | null = (import.meta.env.VITE_TANANA_CODE_INVITATION ?? "").trim() || null;

/** Adresse sans « https:// », pour l'afficher simplement. */
export function adresseCourte(adresse: string): string {
  return adresse.replace(/^https?:\/\//i, "");
}

/** Au-delà, on prévient que l'hôtel ne donne plus de nouvelles. */
export const SILENCE_INQUIETANT_MS = 3 * 3600_000;

/** L'écran du poste à distance regarde s'il y a du nouveau (la copie, elle, se met à jour toute seule). */
export const MISE_A_JOUR_AUTO_MS = 15_000;

/** Nettoie une adresse tapée à la main : espaces, « / » final, https:// ajouté si oublié
 *  (http:// pour un serveur d'essai sur cet ordinateur, le seul cas où Rust l'accepte). */
export function nettoyerAdresse(saisie: string): string {
  let a = saisie.trim().replace(/\/+$/, "");
  if (a && !/^https?:\/\//i.test(a)) {
    const local = /^(localhost|127\.0\.0\.1)(:\d+)?(\/|$)/i.test(a);
    a = `${local ? "http" : "https"}://${a}`;
  }
  return a;
}

/** Identifiant d'hôtel tapé avec ou sans tiret, en minuscules… → « HK7Q-F29X ».
 *  0, 1 et 8 sont compris comme O, I et B (le serveur n'utilise pas ces chiffres). Même règle en Rust. */
export function nettoyerIdentifiantHotel(saisie: string): string {
  const brut = saisie
    .toUpperCase()
    .replace(/[^A-Z0-9]/g, "")
    .replace(/0/g, "O")
    .replace(/1/g, "I")
    .replace(/8/g, "B");
  return brut.length === 8 ? `${brut.slice(0, 4)}-${brut.slice(4)}` : brut;
}

/** La clé secrète affichée à l'hôtel : 6 groupes de 4 caractères puis un groupe de 2. */
export const LONGUEUR_CLE = 26;

/** Caractères utiles d'une clé recopiée depuis le papier (tirets et espaces ignorés). Rust tolère O/0, I/1… */
export function caracteresCle(saisie: string): number {
  return saisie.replace(/[^A-Za-z0-9]/g, "").length;
}

/** Durée écoulée en mots simples : « il y a 5 minutes », « il y a 2 heures », « il y a 3 jours ». */
export function depuis(iso: string, maintenant: number = Date.now()): string {
  const ms = maintenant - new Date(iso).getTime();
  if (!Number.isFinite(ms)) return "";
  const minutes = Math.max(0, Math.round(ms / 60_000));
  if (minutes < 1) return "à l'instant";
  if (minutes < 60) return `il y a ${minutes} minute${minutes > 1 ? "s" : ""}`;
  const heures = Math.round(minutes / 60);
  if (heures < 48) return `il y a ${heures} heure${heures > 1 ? "s" : ""}`;
  const jours = Math.round(heures / 24);
  return `il y a ${jours} jours`;
}
