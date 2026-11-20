/**
 * Ce que montre la page, calculé à partir de la copie en mémoire.
 * Les calculs reprennent ceux de Tanana (crates/tanana-core) : mêmes soldes,
 * mêmes taxes au franc près, même caisse. Les tests le vérifient sur un
 * hôtel envoyé par le vrai Tanana (src/web/__fixtures__/hotel-demo.json).
 */
import type { Devise } from "../lib/format";
import type { Copie, Ligne } from "./relais";

const texte = (v: unknown): string => (typeof v === "string" ? v : v == null ? "" : String(v));
const nombre = (v: unknown): number => (typeof v === "number" ? v : Number(v) || 0);
const texteOuNul = (v: unknown): string | null => (typeof v === "string" && v.trim() ? v : null);

export interface Hotel {
  nom: string;
  devise: Devise;
}

export function hotel(copie: Copie): Hotel {
  const p = new Map(copie.lignes("parametres").map((l) => [texte(l.cle), texte(l.valeur)]));
  return {
    nom: p.get("nom_hotel") || "Votre hôtel",
    devise: { code: p.get("devise") || "XAF", decimales: Math.min(3, Math.max(0, Number(p.get("decimales_devise")) || 0)) },
  };
}

// ─────────────────────────────── Taxes ───────────────────────────────

interface Taxe {
  calcul: string;
  valeur: number;
  comprise: boolean;
}
const CENT_POURCENT = 10_000n;
const arrondi = (n: bigint, d: bigint) => (2n * n + d) / (2n * d);

/** Taxes qui s'ajoutent au prix (domaine.rs, calculer_taxes → total_ajoutees). */
export function taxesAjoutees(prix: number, nuits: number, taxes: Taxe[]): number {
  const fixe = (t: Taxe) =>
    t.calcul === "par_nuit" ? BigInt(t.valeur) * BigInt(nuits) : t.calcul === "par_sejour" ? BigInt(t.valeur) : 0n;
  const prixL = BigInt(Math.max(0, prix));
  const comprisesFixes = taxes.filter((t) => t.comprise).reduce((s, t) => s + fixe(t), 0n);
  const basePct = prixL - comprisesFixes > 0n ? prixL - comprisesFixes : 0n;
  const sommePct = taxes
    .filter((t) => t.comprise && t.calcul === "pourcentage")
    .reduce((s, t) => s + BigInt(t.valeur), 0n);
  const horsTaxes = arrondi(basePct * CENT_POURCENT, CENT_POURCENT + sommePct);
  let total = 0n;
  for (const t of taxes) {
    if (t.comprise) continue;
    total += t.calcul === "pourcentage" ? arrondi(horsTaxes * BigInt(t.valeur), CENT_POURCENT) : fixe(t);
  }
  return Number(total);
}

// ──────────────────────────── Réservations ────────────────────────────

export type Statut = "confirmee" | "arrivee" | "partie" | "annulee";

export interface ReservationVue {
  id: string;
  chambre_id: string;
  chambre_numero: string;
  client_id: string;
  client_nom: string;
  client_telephone: string | null;
  date_arrivee: string;
  date_depart: string;
  nuits: number;
  prix_total: number;
  taxes_ajoutees: number;
  total_a_payer: number;
  total_encaisse: number;
  /** Reste à payer (négatif : trop perçu) */
  solde: number;
  statut: Statut;
  notes: string | null;
  cree_le: string;
  /** Chambre d'une réservation de groupe : le compte commun du groupe. */
  groupe: CompteGroupe | null;
}

/** Un groupe : plusieurs chambres, un seul compte (comme Tanana : groupe() dans reservations.rs). */
export interface CompteGroupe {
  id: string;
  nom: string | null;
  /** Chambres non annulées */
  chambres: number;
  total_a_payer: number;
  total_encaisse: number;
  solde: number;
}

const jours = (debut: string, fin: string) => Math.round((Date.parse(`${fin}T00:00:00Z`) - Date.parse(`${debut}T00:00:00Z`)) / 86_400_000);

export function reservations(copie: Copie): ReservationVue[] {
  const chambres = new Map(copie.lignes("chambres").map((c) => [texte(c.id), c]));
  const clients = new Map(copie.lignes("clients").map((c) => [texte(c.id), c]));
  const encaisse = new Map<string, number>();
  for (const p of copie.lignes("paiements")) {
    const id = texte(p.reservation_id);
    const signe = p.type === "remboursement" ? -1 : 1;
    encaisse.set(id, (encaisse.get(id) ?? 0) + signe * nombre(p.montant));
  }
  const taxes = new Map<string, (Taxe & { ordre: number })[]>();
  for (const t of copie.lignes("taxes_reservation")) {
    const id = texte(t.reservation_id);
    const liste = taxes.get(id) ?? [];
    liste.push({ ordre: nombre(t.ordre), calcul: texte(t.calcul), valeur: nombre(t.valeur), comprise: nombre(t.comprise) === 1 });
    taxes.set(id, liste);
  }
  return copie
    .lignes("reservations")
    .map((r): ReservationVue => {
      const id = texte(r.id);
      const client: Ligne = clients.get(texte(r.client_id)) ?? {};
      const nuits = jours(texte(r.date_arrivee), texte(r.date_depart));
      const prix = nombre(r.prix_total);
      const ajoutees = taxesAjoutees(prix, nuits, (taxes.get(id) ?? []).sort((a, b) => a.ordre - b.ordre));
      const total = prix + ajoutees;
      const recu = encaisse.get(id) ?? 0;
      return {
        id,
        chambre_id: texte(r.chambre_id),
        chambre_numero: texte(chambres.get(texte(r.chambre_id))?.numero) || "?",
        client_id: texte(r.client_id),
        client_nom: `${texte(client.nom)} ${texte(client.prenom)}`.trim() || "Client inconnu",
        client_telephone: texteOuNul(client.telephone),
        date_arrivee: texte(r.date_arrivee),
        date_depart: texte(r.date_depart),
        nuits,
        prix_total: prix,
        taxes_ajoutees: ajoutees,
        total_a_payer: total,
        total_encaisse: recu,
        solde: total - recu,
        statut: texte(r.statut) as Statut,
        notes: texteOuNul(r.notes),
        cree_le: texte(r.cree_le),
        groupe: texteOuNul(r.groupe_id) ? ({ id: texte(r.groupe_id) } as CompteGroupe) : null,
      };
    })
    .map((r, _, toutes) => (r.groupe ? { ...r, groupe: compteGroupe(copie, r.groupe.id, toutes) } : r))
    .sort(
      (a, b) =>
        a.chambre_numero.toLowerCase().localeCompare(b.chambre_numero.toLowerCase(), "fr", { numeric: true }) ||
        a.date_arrivee.localeCompare(b.date_arrivee),
    );
}

function compteGroupe(copie: Copie, id: string, toutes: ReservationVue[]): CompteGroupe {
  const chambres = toutes.filter((r) => r.groupe?.id === id);
  const actives = chambres.filter((r) => r.statut !== "annulee");
  const total = actives.reduce((t, r) => t + r.total_a_payer, 0);
  const paye = chambres.reduce((t, r) => t + r.total_encaisse, 0);
  const ligne = copie.lignes("groupes").find((g) => texte(g.id) === id);
  return { id, nom: ligne ? texteOuNul(ligne.nom) : null, chambres: actives.length, total_a_payer: total, total_encaisse: paye, solde: total - paye };
}

/** Reste à payer de séjours : chaque groupe n'est compté qu'une fois. */
export function resteDe(liste: ReservationVue[]): number {
  const groupes = new Map<string, number>();
  let reste = 0;
  for (const r of liste) {
    if (r.groupe) groupes.set(r.groupe.id, Math.max(0, r.groupe.solde));
    else reste += Math.max(0, r.solde);
  }
  return reste + [...groupes.values()].reduce((a, b) => a + b, 0);
}

/** Séjours non annulés qui touchent la période [debut, fin[ (comme le planning de Tanana). */
export function planning(toutes: ReservationVue[], debut: string, fin: string): ReservationVue[] {
  return toutes.filter((r) => r.statut !== "annulee" && r.date_arrivee < fin && r.date_depart > debut);
}

// ─────────────────────────────── Journée ───────────────────────────────

export type EtatChambre = "occupee" | "depart" | "attendue" | "libre" | "maintenance" | "hors_service";

export interface ChambreDuJour {
  id: string;
  numero: string;
  type: string;
  etat: EtatChambre;
  reservation: ReservationVue | null;
}

export interface Journee {
  chambres: ChambreDuJour[];
  arrivees: ReservationVue[];
  departs: ReservationVue[];
  /** Clients dans l'hôtel (arrivée enregistrée) */
  presents: ReservationVue[];
  /** Ce que les clients présents doivent encore */
  resteAEncaisser: number;
}

export function journee(copie: Copie, toutes: ReservationVue[], jour: string): Journee {
  const types = new Map(copie.lignes("types_chambre").map((t) => [texte(t.id), texte(t.nom)]));
  const actives = toutes.filter((r) => r.statut === "confirmee" || r.statut === "arrivee");
  const arrivees = actives.filter((r) => r.date_arrivee === jour);
  const departs = toutes.filter((r) => r.statut === "arrivee" && r.date_depart === jour);
  const presents = toutes.filter((r) => r.statut === "arrivee" && r.date_arrivee <= jour && jour <= r.date_depart);
  const chambres = copie
    .lignes("chambres")
    .map((c): ChambreDuJour => {
      const id = texte(c.id);
      const statut = texte(c.statut);
      const present = presents.find((r) => r.chambre_id === id);
      const attendue = arrivees.find((r) => r.chambre_id === id && r.statut === "confirmee");
      let etat: EtatChambre = "libre";
      if (statut === "maintenance" || statut === "hors_service") etat = statut;
      else if (present) etat = present.date_depart === jour ? "depart" : "occupee";
      else if (attendue) etat = "attendue";
      return { id, numero: texte(c.numero), type: types.get(texte(c.type_id)) ?? "", etat, reservation: present ?? attendue ?? null };
    })
    .sort((a, b) => a.numero.localeCompare(b.numero, "fr", { numeric: true }));
  return {
    chambres,
    arrivees,
    departs,
    presents,
    resteAEncaisser: resteDe(presents),
  };
}

// ─────────────────────────────── Caisse ───────────────────────────────

export const MODES = ["especes", "carte", "mobile", "virement", "autre"] as const;

export interface Caisse {
  encaissements: number;
  encaissements_par_mode: { mode: string; montant: number }[];
  remboursements: number;
  depenses: number;
  net: number;
  paiements: { date: string; montant: number; type: string; mode: string; reservation: ReservationVue | null; par: string }[];
  liste_depenses: { date: string; categorie: string; montant: number; description: string | null }[];
}

/** Minuit local → instant UTC au format de Tanana (argent.rs, bornes_instants). */
function instant(jour: string, decalageMinutes: number): string {
  return new Date(Date.parse(`${jour}T00:00:00Z`) - decalageMinutes * 60_000).toISOString();
}

/** Caisse du [debut, fin[ (jours de l'hôtel). `decalageMinutes` : décalage de l'heure locale sur UTC (Paris en hiver : 60). */
export function caisse(copie: Copie, toutes: ReservationVue[], debut: string, fin: string, decalageMinutes: number): Caisse {
  const d = instant(debut, decalageMinutes);
  const f = instant(fin, decalageMinutes);
  const parId = new Map(toutes.map((r) => [r.id, r]));
  const noms = new Map(copie.lignes("utilisateurs").map((u) => [texte(u.id), texte(u.nom)]));
  const paiements = copie
    .lignes("paiements")
    .filter((p) => texte(p.date) >= d && texte(p.date) < f)
    .map((p) => ({
      date: texte(p.date),
      montant: nombre(p.montant),
      type: texte(p.type),
      mode: texte(p.mode),
      reservation: parId.get(texte(p.reservation_id)) ?? null,
      par: noms.get(texte(p.utilisateur_id)) ?? "",
    }))
    .sort((a, b) => b.date.localeCompare(a.date));
  const liste_depenses = copie
    .lignes("depenses")
    .filter((x) => !x.annulee_le && texte(x.date) >= debut && texte(x.date) < fin)
    .map((x) => ({ date: texte(x.date), categorie: texte(x.categorie), montant: nombre(x.montant), description: texteOuNul(x.description) }))
    .sort((a, b) => b.date.localeCompare(a.date));
  const entrees = paiements.filter((p) => p.type === "acompte" || p.type === "solde");
  const encaissements_par_mode = MODES.map((mode) => ({
    mode,
    montant: entrees.filter((p) => p.mode === mode).reduce((s, p) => s + p.montant, 0),
  })).filter((m) => m.montant > 0);
  const encaissements = encaissements_par_mode.reduce((s, m) => s + m.montant, 0);
  const remboursements = paiements.filter((p) => p.type === "remboursement").reduce((s, p) => s + p.montant, 0);
  const depenses = liste_depenses.reduce((s, x) => s + x.montant, 0);
  return {
    encaissements,
    encaissements_par_mode,
    remboursements,
    depenses,
    net: encaissements - remboursements - depenses,
    paiements,
    liste_depenses,
  };
}

// ─────────────────────────────── Journal ───────────────────────────────

/** Actions à vérifier (crates/tanana-core/src/audit.rs, SENSIBLES). */
export const SENSIBLES = [
  "reservation.annulation",
  "reservation.taxes",
  "taxes.",
  "depense.annulation",
  "reservation.modification",
  "paiement.remboursement",
  "utilisateur.",
  "role.",
  "auth.echec",
  "auth.blocage",
  "auth.recuperation",
  "acces.refuse",
  "parametres.",
  "sauvegarde.restauration",
];

export interface EntreeVue {
  seq: number;
  date: string;
  utilisateur: string;
  action: string;
  sensible: boolean;
}

export function journal(copie: Copie): EntreeVue[] {
  const noms = new Map(copie.lignes("utilisateurs").map((u) => [texte(u.id), texte(u.nom)]));
  return copie
    .lignes("journal_audit")
    .map((e) => {
      const action = texte(e.action);
      return {
        seq: nombre(e.seq),
        date: texte(e.date),
        utilisateur: e.utilisateur_id ? (noms.get(texte(e.utilisateur_id)) ?? "Employé supprimé") : "Système",
        action,
        sensible: SENSIBLES.some((s) => action.startsWith(s)),
      };
    })
    .sort((a, b) => b.seq - a.seq);
}
