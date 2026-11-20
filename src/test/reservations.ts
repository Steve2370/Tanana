/** Données d'exemple pour les tests des réservations. Les dates sont relatives à aujourd'hui. */
import type { Chambre } from "../bindings/Chambre";
import type { Client } from "../bindings/Client";
import type { DetailTaxes } from "../bindings/DetailTaxes";
import type { DocumentEmis } from "../bindings/DocumentEmis";
import type { DocumentEnregistre } from "../bindings/DocumentEnregistre";
import type { Paiement } from "../bindings/Paiement";
import type { Reservation } from "../bindings/Reservation";
import type { Taxe } from "../bindings/Taxe";
import { ajouterJours, joursEntre } from "../lib/dates";
import { aujourdhui } from "../lib/format";

export const J = aujourdhui();
/** Date à `n` jours d'aujourd'hui. */
export const j = (n: number) => ajouterJours(J, n);

export function chambre(numero: string, p: Partial<Chambre> = {}): Chambre {
  return {
    id: `c${numero}`,
    numero,
    type_id: "t1",
    type_nom: "Standard",
    prix_nuit_defaut: 25_000,
    etage: 1,
    statut: "disponible",
    notes: null,
    ...p,
  };
}

export const CHAMBRES = [chambre("101"), chambre("102"), chambre("103", { statut: "maintenance" })];

export const MBARGA: Client = {
  id: "cl1",
  nom: "Mbarga",
  prenom: "Aline",
  telephone: "690 00 00 00",
  email: null,
  piece_identite: null,
  notes: null,
  cree_le: "2026-03-10T14:00:00.000Z",
};

/** Réservation de 3 nuits à partir de demain, chambre 101, rien de payé. */
export function resa(p: Partial<Reservation> = {}): Reservation {
  const base: Reservation = {
    id: "r1",
    chambre_id: "c101",
    chambre_numero: "101",
    client_id: "cl1",
    client_nom: "Mbarga Aline",
    date_arrivee: j(1),
    date_depart: j(4),
    nuits: 3,
    prix_total: 75_000,
    taxes_ajoutees: 0,
    total_a_payer: 75_000,
    total_encaisse: 0,
    solde: 75_000,
    statut: "confirmee",
    groupe_id: null,
    solde_groupe: null,
    notes: null,
    cree_le: "2026-03-10T14:00:00.000Z",
    ...p,
  };
  const total = p.total_a_payer ?? base.prix_total + base.taxes_ajoutees;
  return {
    ...base,
    nuits: joursEntre(base.date_arrivee, base.date_depart),
    total_a_payer: total,
    solde: p.solde ?? total - base.total_encaisse,
  };
}

export function paiement(p: Partial<Paiement> = {}): Paiement {
  return {
    id: "p1",
    reservation_id: "r1",
    montant: 30_000,
    type_paiement: "acompte",
    mode: "mobile",
    date: new Date().toISOString(),
    utilisateur_nom: "Marie",
    note: null,
    ...p,
  };
}

export function documentEmis(p: Partial<DocumentEmis> = {}): DocumentEmis {
  return {
    id: "d1",
    type_document: "recu",
    numero: "R-2026-0001",
    reservation_id: "r1",
    paiement_id: "p1",
    emis_le: new Date().toISOString(),
    emis_par_nom: "Marie",
    remplace_numero: null,
    ...p,
  };
}

/** Réponse de Rust après l'impression d'un reçu ou d'une facture. */
export function documentEnregistre(document: Partial<DocumentEmis> = {}, p: Partial<DocumentEnregistre> = {}): DocumentEnregistre {
  const d = documentEmis(document);
  const dossier = d.type_document === "recu" ? "Reçus" : "Factures";
  return {
    document: d,
    duplicata: false,
    chemin: `/Users/marie/Documents/Tanana/${dossier}/${d.numero} Mbarga Aline.pdf`,
    emplacement: `Documents › Tanana › ${dossier}`,
    ouvert: true,
    ...p,
  };
}

/** Calcul simplifié pour les tests (le vrai est en Rust, testé là-bas). */
export function detailSimule(prix: number, nuits: number, taxes: Taxe[]): DetailTaxes {
  const lignes = taxes.map((taxe) => ({
    taxe,
    montant: taxe.calcul === "par_nuit" ? taxe.valeur * nuits : taxe.calcul === "par_sejour" ? taxe.valeur : Math.round((prix * taxe.valeur) / 10_000),
  }));
  const ajoutees = lignes.filter((l) => !l.taxe.comprise).reduce((a, l) => a + l.montant, 0);
  const comprises = lignes.filter((l) => l.taxe.comprise).reduce((a, l) => a + l.montant, 0);
  return { prix, nuits, lignes, total_comprises: comprises, total_ajoutees: ajoutees, hors_taxes: prix - comprises, total_a_payer: prix + ajoutees };
}

