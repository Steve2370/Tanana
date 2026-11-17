/**
 * Lecture sur le relais (même adresse que la page) et reconstruction de
 * l'hôtel dans la mémoire du navigateur. Rien n'est enregistré sur l'appareil.
 */
import { ASSOCIE_TEMOIN, CleIncorrecte, TEXTE_TEMOIN, associeLot, dechiffrer, type Cles } from "./crypto";

export interface LotChiffre {
  seq_debut: number;
  seq_fin: number;
  nonce: string;
  chiffre: string;
}
export interface PageLots {
  lots: { numero: number; recu_le: string; lot: LotChiffre }[];
  suite: boolean;
  dernier_contact: string | null;
}
export interface Changement {
  t: string;
  o: "ecrire" | "effacer";
  c: unknown[];
  d?: Record<string, unknown>;
}
interface Lot {
  genere_le: string;
  changements: Changement[];
}

/** Erreur à montrer telle quelle à la personne. */
export class ErreurConsultation extends Error {}

/** Tables reçues de l'hôtel (crates/tanana-core/src/distance/capture.rs, TABLES_ENVOYEES). */
export const TABLES_ENVOYEES = [
  "parametres",
  "roles",
  "permissions",
  "role_permissions",
  "utilisateurs",
  "permissions_perso",
  "types_chambre",
  "chambres",
  "clients",
  "reservations",
  "paiements",
  "depenses",
  "journal_audit",
  "documents",
  "taxes_hotel",
  "taxes_reservation",
] as const;

export type Ligne = Record<string, unknown>;

/** Copie de l'hôtel en mémoire : table → (clé primaire → ligne). */
export class Copie {
  readonly tables = new Map<string, Map<string, Ligne>>();
  dernierNumero = 0;
  /** Heure de l'hôtel quand il a préparé les dernières données reçues */
  donneesDu: string | null = null;
  /** Dernier signe de vie de l'hôtel (même sans nouveauté) */
  dernierContact: string | null = null;

  appliquer(changements: Changement[]) {
    for (const ch of changements) {
      if (!(TABLES_ENVOYEES as readonly string[]).includes(ch.t)) continue;
      let table = this.tables.get(ch.t);
      if (!table) this.tables.set(ch.t, (table = new Map()));
      const cle = JSON.stringify(ch.c);
      if (ch.o === "ecrire" && ch.d) table.set(cle, ch.d);
      else if (ch.o === "effacer") table.delete(cle);
    }
  }

  lignes(table: string): Ligne[] {
    return [...(this.tables.get(table)?.values() ?? [])];
  }
}

type Lecteur = typeof fetch;

async function appel<T>(lecteur: Lecteur, adresse: string, jeton: string): Promise<T> {
  let reponse: Response;
  try {
    reponse = await lecteur(adresse, { headers: { Authorization: `Bearer ${jeton}` }, cache: "no-store" });
  } catch {
    throw new ErreurConsultation("Le serveur de consultation ne répond pas. Vérifiez votre connexion Internet.");
  }
  if (reponse.status === 404)
    throw new ErreurConsultation("Cet identifiant d'hôtel est inconnu. Vérifiez-le (2 groupes de 4 caractères).");
  if (reponse.status === 401 || reponse.status === 403) throw new CleIncorrecte();
  if (!reponse.ok) throw new ErreurConsultation("Le serveur de consultation a un problème. Réessayez dans un moment.");
  return (await reponse.json()) as T;
}

/** Vérifie la clé avec le témoin chiffré, avant de télécharger quoi que ce soit. */
export async function verifierCle(lecteur: Lecteur, hotelId: string, cles: Cles): Promise<void> {
  const t = await appel<{ nonce: string; chiffre: string }>(
    lecteur,
    `/v1/hotels/${encodeURIComponent(hotelId)}/temoin`,
    cles.jetonLecture,
  );
  const clair = await dechiffrer(cles, t.nonce, t.chiffre, ASSOCIE_TEMOIN);
  if (new TextDecoder().decode(clair) !== TEXTE_TEMOIN) throw new CleIncorrecte();
}

/**
 * Télécharge et applique les lots manquants, page par page. Une page n'est
 * appliquée que si tous ses lots se déchiffrent : la copie reste cohérente.
 * Renvoie le nombre de lots appliqués.
 */
export async function mettreAJour(lecteur: Lecteur, hotelId: string, cles: Cles, copie: Copie): Promise<number> {
  let appliques = 0;
  for (;;) {
    const page = await appel<PageLots>(
      lecteur,
      `/v1/hotels/${encodeURIComponent(hotelId)}/lots?apres=${copie.dernierNumero}`,
      cles.jetonLecture,
    );
    const nouveaux = page.lots.filter((l) => l.numero > copie.dernierNumero);
    const lots: { numero: number; lot: Lot }[] = [];
    for (const recu of nouveaux) {
      const l = recu.lot;
      const clair = await dechiffrer(cles, l.nonce, l.chiffre, associeLot(hotelId, l.seq_debut, l.seq_fin));
      lots.push({ numero: recu.numero, lot: JSON.parse(new TextDecoder().decode(clair)) as Lot });
    }
    for (const { numero, lot } of lots) {
      copie.appliquer(lot.changements);
      copie.dernierNumero = numero;
      copie.donneesDu = lot.genere_le;
      appliques++;
    }
    copie.dernierContact = page.dernier_contact ?? copie.dernierContact;
    if (!page.suite || nouveaux.length === 0) return appliques;
  }
}
