/**
 * Seul point de contact avec Rust. Chaque fonction correspond à une commande
 * de `src-tauri/src/commandes.rs` (un test vérifie que les deux listes sont
 * identiques). Les types viennent de `src/bindings/`, générés depuis Rust.
 */
import { invoke } from "@tauri-apps/api/core";

import type { ApercuSauvegarde } from "../bindings/ApercuSauvegarde";
import type { BilanEnvoi } from "../bindings/BilanEnvoi";
import type { Chambre } from "../bindings/Chambre";
import type { CodesDistance } from "../bindings/CodesDistance";
import type { Client } from "../bindings/Client";
import type { ConfigurationInitiale } from "../bindings/ConfigurationInitiale";
import type { Depense } from "../bindings/Depense";
import type { DetailTaxes } from "../bindings/DetailTaxes";
import type { DocumentEmis } from "../bindings/DocumentEmis";
import type { DocumentEnregistre } from "../bindings/DocumentEnregistre";
import type { DonneesChambre } from "../bindings/DonneesChambre";
import type { DonneesClient } from "../bindings/DonneesClient";
import type { Encaissement } from "../bindings/Encaissement";
import type { EntreeJournal } from "../bindings/EntreeJournal";
import type { EtatApplication } from "../bindings/EtatApplication";
import type { EtatCopie } from "../bindings/EtatCopie";
import type { EtatDistance } from "../bindings/EtatDistance";
import type { InfosMiseAJour } from "../bindings/InfosMiseAJour";
import type { FiltreJournal } from "../bindings/FiltreJournal";
import type { ModePaiement } from "../bindings/ModePaiement";
import type { ModificationReservation } from "../bindings/ModificationReservation";
import type { NouveauTypeChambre } from "../bindings/NouveauTypeChambre";
import type { NouvelleDepense } from "../bindings/NouvelleDepense";
import type { NouvelleReservation } from "../bindings/NouvelleReservation";
import type { NouvelUtilisateur } from "../bindings/NouvelUtilisateur";
import type { Paiement } from "../bindings/Paiement";
import type { PaiementDetaille } from "../bindings/PaiementDetaille";
import type { Parametres } from "../bindings/Parametres";
import type { Permission } from "../bindings/Permission";
import type { PermissionPerso } from "../bindings/PermissionPerso";
import type { RapportCaisse } from "../bindings/RapportCaisse";
import type { RapportIntegrite } from "../bindings/RapportIntegrite";
import type { Reservation } from "../bindings/Reservation";
import type { RestaurationFaite } from "../bindings/RestaurationFaite";
import type { SauvegardeDisponible } from "../bindings/SauvegardeDisponible";
import type { Role } from "../bindings/Role";
import type { SessionActive } from "../bindings/SessionActive";
import type { SessionOuverte } from "../bindings/SessionOuverte";
import type { StatutUtilisateur } from "../bindings/StatutUtilisateur";
import type { Taxe } from "../bindings/Taxe";
import type { TypeChambre } from "../bindings/TypeChambre";
import type { TypeDocument } from "../bindings/TypeDocument";
import type { Utilisateur } from "../bindings/Utilisateur";
import { ErreurTanana, estSessionPerdue, versErreur } from "./erreurs";
import { decalageLocal } from "./format";

// ─────────────── Jeton de session : en mémoire seulement ───────────────
// Jamais dans localStorage : fermer l'app = être déconnecté.

let jeton: string | null = null;
let surSessionPerdue: ((e: ErreurTanana) => void) | null = null;
/** Moment du dernier échange réussi avec Rust (qui prolonge la session côté serveur). */
let dernierContact = 0;

export function dernierContactServeur(): number {
  return dernierContact;
}

export const jetonSession = {
  definir(t: string | null) {
    jeton = t;
  },
  lire() {
    return jeton;
  },
};

/** Appelé quand Rust répond « session fermée/expirée » à n'importe quelle commande. */
export function quandSessionPerdue(f: ((e: ErreurTanana) => void) | null) {
  surSessionPerdue = f;
}

async function appel<T>(commande: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(commande, args);
  } catch (e) {
    throw versErreur(e);
  }
}

async function appelConnecte<T>(commande: string, args: Record<string, unknown> = {}): Promise<T> {
  if (!jeton) {
    const e = new ErreurTanana({ code: "NON_AUTHENTIFIE" });
    surSessionPerdue?.(e);
    throw e;
  }
  try {
    const r = await appel<T>(commande, { token: jeton, ...args });
    dernierContact = Date.now();
    return r;
  } catch (e) {
    if (estSessionPerdue(e)) {
      jeton = null;
      surSessionPerdue?.(e as ErreurTanana);
    }
    throw e;
  }
}

export const FILTRE_JOURNAL_DEFAUT: FiltreJournal = {
  utilisateur_id: null,
  action_prefixe: null,
  sensibles_seulement: false,
  avant_seq: null,
  limite: 50,
};

export const api = {
  // Démarrage
  etatApplication: () => appel<EtatApplication>("etat_application"),
  fournirCle: (cle: string) => appel<null>("fournir_cle", { cle }),
  initialiser: (configuration: ConfigurationInitiale) => appel<string>("initialiser", { configuration }),
  recupererAccesAdmin: (code: string, identifiant: string, nouveau_mot_de_passe: string) =>
    appel<string>("recuperer_acces_admin", { code, identifiant, nouveau_mot_de_passe }),

  // Connexion
  connexion: (identifiant: string, mot_de_passe: string) =>
    appel<SessionOuverte>("connexion", { identifiant, mot_de_passe }),
  connexionPin: (identifiant: string, pin: string) => appel<SessionOuverte>("connexion_pin", { identifiant, pin }),
  deconnexion: () => appelConnecte<null>("deconnexion"),
  maSession: () => appelConnecte<SessionOuverte>("ma_session"),
  changerMonMotDePasse: (actuel: string, nouveau: string) =>
    appelConnecte<null>("changer_mon_mot_de_passe", { actuel, nouveau }),
  definirMonPin: (mot_de_passe: string, pin: string) => appelConnecte<null>("definir_mon_pin", { mot_de_passe, pin }),

  // Comptes et rôles
  listerUtilisateurs: () => appelConnecte<Utilisateur[]>("lister_utilisateurs"),
  creerUtilisateur: (donnees: NouvelUtilisateur) => appelConnecte<Utilisateur>("creer_utilisateur", { donnees }),
  changerRole: (utilisateur_id: string, role_id: string) =>
    appelConnecte<Utilisateur>("changer_role", { utilisateur_id, role_id }),
  changerStatutUtilisateur: (utilisateur_id: string, statut: StatutUtilisateur) =>
    appelConnecte<Utilisateur>("changer_statut_utilisateur", { utilisateur_id, statut }),
  reinitialiserMotDePasse: (utilisateur_id: string, mot_de_passe_temporaire: string) =>
    appelConnecte<null>("reinitialiser_mot_de_passe", { utilisateur_id, mot_de_passe_temporaire }),
  definirPermissionPerso: (utilisateur_id: string, permission: Permission, accorde: boolean | null) =>
    appelConnecte<PermissionPerso[]>("definir_permission_perso", { utilisateur_id, permission, accorde }),
  permissionsPerso: (utilisateur_id: string) => appelConnecte<PermissionPerso[]>("permissions_perso", { utilisateur_id }),
  permissionsUtilisateur: (utilisateur_id: string) =>
    appelConnecte<Permission[]>("permissions_utilisateur", { utilisateur_id }),
  listerRoles: () => appelConnecte<Role[]>("lister_roles"),
  creerRole: (nom: string, permissions: Permission[]) => appelConnecte<Role>("creer_role", { nom, permissions }),
  modifierPermissionsRole: (role_id: string, permissions: Permission[]) =>
    appelConnecte<Role>("modifier_permissions_role", { role_id, permissions }),
  sessionsActives: () => appelConnecte<SessionActive[]>("sessions_actives"),
  fermerSession: (session_id: string) => appelConnecte<null>("fermer_session", { session_id }),

  // Journal
  journal: (filtre: Partial<FiltreJournal> = {}) =>
    appelConnecte<EntreeJournal[]>("journal", { filtre: { ...FILTRE_JOURNAL_DEFAUT, ...filtre } }),
  verifierIntegriteJournal: () => appelConnecte<RapportIntegrite>("verifier_integrite_journal"),

  // Chambres
  listerTypesChambre: () => appelConnecte<TypeChambre[]>("lister_types_chambre"),
  creerTypeChambre: (donnees: NouveauTypeChambre) => appelConnecte<TypeChambre>("creer_type_chambre", { donnees }),
  modifierTypeChambre: (id: string, donnees: NouveauTypeChambre, actif: boolean) =>
    appelConnecte<TypeChambre>("modifier_type_chambre", { id, donnees, actif }),
  listerChambres: () => appelConnecte<Chambre[]>("lister_chambres"),
  creerChambre: (donnees: DonneesChambre) => appelConnecte<Chambre>("creer_chambre", { donnees }),
  modifierChambre: (id: string, donnees: DonneesChambre) => appelConnecte<Chambre>("modifier_chambre", { id, donnees }),

  // Clients
  rechercherClients: (recherche: string) => appelConnecte<Client[]>("rechercher_clients", { recherche }),
  obtenirClient: (id: string) => appelConnecte<Client>("obtenir_client", { id }),
  creerClient: (donnees: DonneesClient) => appelConnecte<Client>("creer_client", { donnees }),
  modifierClient: (id: string, donnees: DonneesClient) => appelConnecte<Client>("modifier_client", { id, donnees }),

  // Réservations
  planning: (debut: string, fin: string) => appelConnecte<Reservation[]>("planning", { debut, fin }),
  obtenirReservation: (id: string) => appelConnecte<Reservation>("obtenir_reservation", { id }),
  reservationsDuClient: (client_id: string) => appelConnecte<Reservation[]>("reservations_du_client", { client_id }),
  chambresDisponibles: (arrivee: string, depart: string) =>
    appelConnecte<Chambre[]>("chambres_disponibles", { arrivee, depart }),
  creerReservation: (donnees: NouvelleReservation) => appelConnecte<Reservation>("creer_reservation", { donnees }),
  modifierReservation: (id: string, donnees: ModificationReservation) =>
    appelConnecte<Reservation>("modifier_reservation", { id, donnees }),
  annulerReservation: (id: string, motif: string) => appelConnecte<Reservation>("annuler_reservation", { id, motif }),
  enregistrerArrivee: (id: string) => appelConnecte<Reservation>("enregistrer_arrivee", { id }),
  enregistrerDepart: (id: string) => appelConnecte<Reservation>("enregistrer_depart", { id }),

  // Argent
  paiementsReservation: (reservation_id: string) =>
    appelConnecte<Paiement[]>("paiements_reservation", { reservation_id }),
  encaisser: (donnees: Encaissement) => appelConnecte<Paiement>("encaisser", { donnees }),
  rembourser: (reservation_id: string, montant: number, mode: ModePaiement, motif: string) =>
    appelConnecte<Paiement>("rembourser", { reservation_id, montant, mode, motif }),
  saisirDepense: (donnees: NouvelleDepense) => appelConnecte<Depense>("saisir_depense", { donnees }),
  listerDepenses: (debut: string, fin: string) => appelConnecte<Depense[]>("lister_depenses", { debut, fin }),
  annulerDepense: (id: string, motif: string) => appelConnecte<Depense>("annuler_depense", { id, motif }),
  listerPaiements: (debut: string, fin: string, decalage_utc_minutes: number) =>
    appelConnecte<PaiementDetaille[]>("lister_paiements", { debut, fin, decalage_utc_minutes }),
  rapportCaisse: (debut: string, fin: string, decalage_utc_minutes: number) =>
    appelConnecte<RapportCaisse>("rapport_caisse", { debut, fin, decalage_utc_minutes }),

  // Taxes
  taxesHotel: () => appelConnecte<Taxe[]>("taxes_hotel"),
  definirTaxesHotel: (taxes: Taxe[]) => appelConnecte<Taxe[]>("definir_taxes_hotel", { taxes }),
  taxesReservation: (reservation_id: string) => appelConnecte<DetailTaxes>("taxes_reservation", { reservation_id }),
  modifierTaxesReservation: (reservation_id: string, taxes: Taxe[]) =>
    appelConnecte<Reservation>("modifier_taxes_reservation", { reservation_id, taxes }),
  calculerTaxes: (prix: number, nuits: number, taxes: Taxe[]) =>
    appelConnecte<DetailTaxes>("calculer_taxes", { prix, nuits, taxes }),

  // Reçus et factures : rangés dans Documents › Tanana, puis ouverts à l'écran
  imprimerRecu: (paiement_id: string) =>
    appelConnecte<DocumentEnregistre>("imprimer_recu", { paiement_id, decalage_utc_minutes: decalageLocal() }),
  imprimerFacture: (reservation_id: string) =>
    appelConnecte<DocumentEnregistre>("imprimer_facture", { reservation_id, decalage_utc_minutes: decalageLocal() }),
  documentsReservation: (reservation_id: string) =>
    appelConnecte<DocumentEmis[]>("documents_reservation", { reservation_id }),
  ouvrirDossierDocuments: (type_document: TypeDocument) => appel<null>("ouvrir_dossier_documents", { type_document }),

  // Mises à jour
  infosMiseAJour: () => appel<InfosMiseAJour>("infos_mise_a_jour"),
  /** Copie de l'hôtel juste avant d'installer `version` (null sur un poste de consultation). */
  sauvegarderAvantMiseAJour: (version: string) =>
    appelConnecte<string | null>("sauvegarder_avant_mise_a_jour", { version }),

  // Remettre une copie de sécurité. Hôtel installé : administrateur connecté ;
  // ordinateur neuf ou base illisible : sans session (le jeton est alors absent).
  listerSauvegardes: () => appel<SauvegardeDisponible[]>("lister_sauvegardes", { token: jeton }),
  /** `cle` : seulement pour une copie venant d'un autre ordinateur. */
  examinerSauvegarde: (chemin: string, cle: string | null) =>
    appel<ApercuSauvegarde>("examiner_sauvegarde", { token: jeton, chemin, cle }),
  restaurerSauvegarde: (chemin: string, cle: string | null, mot_de_passe: string | null) =>
    appel<RestaurationFaite>("restaurer_sauvegarde", { token: jeton, chemin, cle, mot_de_passe }),

  // Consultation à distance : côté hôtel
  etatDistance: () => appelConnecte<EtatDistance>("etat_distance"),
  activerDistance: (adresse: string, code_invitation: string | null) =>
    appelConnecte<CodesDistance>("activer_distance", { adresse, code_invitation }),
  envoyerDistance: () => appelConnecte<BilanEnvoi>("envoyer_distance"),
  afficherCleDistance: (mot_de_passe: string) => appelConnecte<CodesDistance>("afficher_cle_distance", { mot_de_passe }),
  renvoyerToutDistance: () => appelConnecte<number>("renvoyer_tout_distance"),
  /** `false` : arrêté ici, mais le serveur n'a pas pu effacer les données (pas d'Internet). */
  desactiverDistance: () => appelConnecte<boolean>("desactiver_distance"),

  // Consultation à distance : côté administrateur (copie en lecture seule)
  /** Renvoie un avertissement si le premier téléchargement n'a pas pu se faire. */
  consulterHotelDistant: (adresse: string, hotel_id: string, cle_secrete: string, mot_de_passe: string) =>
    appel<string | null>("consulter_hotel_distant", { adresse, hotel_id, cle_secrete, mot_de_passe }),
  connexionCopie: (mot_de_passe: string) => appel<SessionOuverte>("connexion_copie", { mot_de_passe }),
  /** Mot de passe oublié ou clé perdue : on oublie l'hôtel, puis on le rattache avec la clé secrète. */
  oublierHotelDistant: () => appel<null>("oublier_hotel_distant"),
  etatCopie: () => appelConnecte<EtatCopie>("etat_copie"),
  mettreAJourCopie: () => appelConnecte<number>("mettre_a_jour_copie"),

  // Système
  parametres: () => appelConnecte<Parametres>("parametres"),
  modifierParametres: (donnees: Parametres) => appelConnecte<Parametres>("modifier_parametres", { donnees }),
  sauvegarderVers: (chemin: string) => appelConnecte<null>("sauvegarder_vers", { chemin }),
  exporterCleBase: (mot_de_passe: string) => appelConnecte<string | null>("exporter_cle_base", { mot_de_passe }),
};

export type Api = typeof api;
