import type { ModePaiement } from "../bindings/ModePaiement";
import type { StatutReservation } from "../bindings/StatutReservation";
import type { TypePaiement } from "../bindings/TypePaiement";

/** Mots affichés à l'écran. `Record<…>` oblige à couvrir chaque valeur définie en Rust. */

export const LIBELLE_STATUT_RESERVATION: Record<StatutReservation, string> = {
  confirmee: "Réservée",
  arrivee: "Client présent",
  partie: "Séjour terminé",
  annulee: "Annulée",
};

export const LIBELLE_MODE: Record<ModePaiement, string> = {
  especes: "Espèces",
  carte: "Carte bancaire",
  mobile: "Mobile money",
  virement: "Virement",
  autre: "Autre",
};

export const MODES_PAIEMENT = Object.keys(LIBELLE_MODE) as ModePaiement[];

export const LIBELLE_TYPE_PAIEMENT: Record<TypePaiement, string> = {
  acompte: "Acompte",
  solde: "Solde",
  remboursement: "Remboursement",
};
