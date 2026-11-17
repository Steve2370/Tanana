/**
 * Faux moteur Rust complet, avec un petit état : ce qu'on crée apparaît
 * ensuite dans les listes. Sert au fuzzing des formulaires, au singe et au
 * banc d'essai dans un vrai navigateur. Toute commande inconnue est notée
 * (`inconnues`) : l'interface a appelé quelque chose que ce faux ne connaît pas.
 */
import type { Client } from "../../bindings/Client";
import type { Depense } from "../../bindings/Depense";
import type { Reservation } from "../../bindings/Reservation";
import type { Taxe } from "../../bindings/Taxe";
import type { Utilisateur } from "../../bindings/Utilisateur";
import { aujourdhui } from "../../lib/format";
import { TOUTES_PERMISSIONS } from "../../lib/permissions";
import { PARAMETRES, session, utilisateur } from "../../test/outils";
import { CHAMBRES, chambre, detailSimule, documentEmis, documentEnregistre, MBARGA, paiement, resa } from "../../test/reservations";

export type Gestionnaires = Record<string, (args: Record<string, unknown>) => unknown>;

export function fauxRust(options: { ecrituresEnAttente?: boolean } = {}) {
  const J = aujourdhui();
  const inconnues = new Set<string>();
  const ecritures: string[] = [];
  const etat = {
    chambres: [...CHAMBRES],
    clients: [MBARGA] as Client[],
    reservations: [resa(), resa({ id: "r2", chambre_id: "c102", chambre_numero: "102", statut: "arrivee", date_arrivee: J })] as Reservation[],
    depenses: [] as Depense[],
    utilisateurs: [utilisateur(), utilisateur({ id: "u-marie", identifiant: "marie", nom: "Marie", role_id: "role-reception", role_nom: "Réception" })] as Utilisateur[],
    taxes: [{ libelle: "TVA", calcul: "pourcentage", valeur: 1925, comprise: true }] as Taxe[],
    parametres: { ...PARAMETRES },
  };
  const ROLES = [
    { id: "role-admin", nom: "Administrateur", systeme: true, permissions: TOUTES_PERMISSIONS },
    { id: "role-reception", nom: "Réception", systeme: true, permissions: ["reservation.voir", "reservation.creer"] },
  ];
  const n = { v: 0 };
  const id = (p: string) => `${p}-${++n.v}`;
  /** Écriture : notée, puis répondue tout de suite, ou jamais (pour tester le double envoi). */
  const ecrire = (nom: string, reponse: () => unknown) => () => {
    ecritures.push(nom);
    return options.ecrituresEnAttente ? new Promise(() => {}) : reponse();
  };
  const g: Gestionnaires = {
    // Démarrage et session
    etat_application: () => ({ statut: "prete", initialise: true, consultation_distance: false, nom_hotel: "Hôtel du Lac", message: null, version: "0.1.0" }),
    ma_session: () => session(),
    parametres: () => etat.parametres,
    infos_mise_a_jour: () => ({ version: "0.1.0", configuree: false }),
    deconnexion: () => null,
    sessions_actives: () => [{ session_id: "s1", utilisateur_id: "u-admin", utilisateur_nom: "Beltran", debut: `${J}T08:00:00.000Z`, derniere_activite: `${J}T09:00:00.000Z` }],
    fermer_session: ecrire("fermer_session", () => null),
    verifier_integrite_journal: () => ({ intacte: true, entrees_verifiees: 3, premiere_rupture: null }),
    journal: () => [
      { seq: 2, date: `${J}T09:00:00.000Z`, utilisateur_id: "u-admin", utilisateur_nom: "Beltran", action: "reservation.creation", entite: "reservation", entite_id: "r1", avant: null, apres: null, sensible: false },
      { seq: 1, date: `${J}T08:00:00.000Z`, utilisateur_id: "u-admin", utilisateur_nom: "Beltran", action: "auth.connexion", entite: null, entite_id: null, avant: null, apres: null, sensible: false },
    ],
    // Chambres
    lister_chambres: () => etat.chambres,
    lister_types_chambre: () => [{ id: "t1", nom: "Standard", prix_nuit_defaut: 25_000, capacite: 2, actif: true }],
    chambres_disponibles: () => etat.chambres.filter((c) => c.statut === "disponible"),
    creer_type_chambre: ecrire("creer_type_chambre", () => ({ id: id("t"), nom: "Nouveau", prix_nuit_defaut: 1, capacite: 1, actif: true })),
    modifier_type_chambre: ecrire("modifier_type_chambre", () => ({ id: "t1", nom: "Standard", prix_nuit_defaut: 25_000, capacite: 2, actif: true })),
    creer_chambre: ecrire("creer_chambre", () => {
      const c = chambre(String(200 + ++n.v));
      etat.chambres.push(c);
      return c;
    }),
    modifier_chambre: ecrire("modifier_chambre", () => etat.chambres[0]),
    // Clients
    rechercher_clients: () => etat.clients,
    obtenir_client: () => etat.clients[0],
    reservations_du_client: () => etat.reservations,
    creer_client: ecrire("creer_client", () => {
      const c = { ...MBARGA, id: id("cl"), nom: "Nouveau" };
      etat.clients.push(c);
      return c;
    }),
    modifier_client: ecrire("modifier_client", () => etat.clients[0]),
    // Réservations
    planning: () => etat.reservations,
    obtenir_reservation: ({ id: rid }) => etat.reservations.find((r) => r.id === rid) ?? etat.reservations[0],
    paiements_reservation: () => [paiement()],
    documents_reservation: () => [documentEmis()],
    taxes_reservation: () => detailSimule(75_000, 3, etat.taxes),
    calculer_taxes: ({ prix_total, nuits, taxes }) => detailSimule(Number(prix_total) || 0, Number(nuits) || 1, (taxes as Taxe[]) ?? etat.taxes),
    creer_reservation: ecrire("creer_reservation", () => etat.reservations[0]),
    modifier_reservation: ecrire("modifier_reservation", () => etat.reservations[0]),
    modifier_taxes_reservation: ecrire("modifier_taxes_reservation", () => etat.reservations[0]),
    annuler_reservation: ecrire("annuler_reservation", () => ({ ...etat.reservations[0], statut: "annulee" })),
    enregistrer_arrivee: ecrire("enregistrer_arrivee", () => ({ ...etat.reservations[0], statut: "arrivee" })),
    enregistrer_depart: ecrire("enregistrer_depart", () => ({ ...etat.reservations[1], statut: "partie" })),
    encaisser: ecrire("encaisser", () => paiement()),
    rembourser: ecrire("rembourser", () => paiement({ type_paiement: "remboursement" })),
    imprimer_recu: ecrire("imprimer_recu", () => documentEnregistre()),
    imprimer_facture: ecrire("imprimer_facture", () => documentEnregistre({ type_document: "facture" })),
    ouvrir_dossier_documents: () => null,
    // Caisse
    rapport_caisse: () => ({ debut: J, fin: J, encaissements: 50_000, encaissements_par_mode: [{ mode: "especes", montant: 50_000 }], remboursements: 0, depenses: 0, net: 50_000 }),
    lister_paiements: () => [{ id: "p1", reservation_id: "r1", client_nom: "Mbarga Aline", chambre_numero: "101", montant: 50_000, type_paiement: "acompte", mode: "especes", date: `${J}T10:00:00.000Z`, utilisateur_nom: "Beltran", note: null }],
    lister_depenses: () => etat.depenses,
    saisir_depense: ecrire("saisir_depense", () => {
      const d = { id: id("d"), categorie: "Eau", montant: 1, description: null, date: J, utilisateur_nom: "Beltran", annulee: false, motif_annulation: null };
      etat.depenses.unshift(d);
      return d;
    }),
    annuler_depense: ecrire("annuler_depense", () => etat.depenses[0]),
    // Employés
    lister_utilisateurs: () => etat.utilisateurs,
    lister_roles: () => ROLES,
    permissions_perso: () => [],
    permissions_utilisateur: () => ["reservation.voir"],
    creer_utilisateur: ecrire("creer_utilisateur", () => {
      const u = utilisateur({ id: id("u"), identifiant: "nouveau", nom: "Nouveau" });
      etat.utilisateurs.push(u);
      return u;
    }),
    changer_role: ecrire("changer_role", () => etat.utilisateurs[1]),
    changer_statut_utilisateur: ecrire("changer_statut_utilisateur", () => etat.utilisateurs[1]),
    reinitialiser_mot_de_passe: ecrire("reinitialiser_mot_de_passe", () => null),
    definir_permission_perso: ecrire("definir_permission_perso", () => []),
    creer_role: ecrire("creer_role", () => ROLES[1]),
    modifier_permissions_role: ecrire("modifier_permissions_role", () => ROLES[1]),
    // Mon compte
    changer_mon_mot_de_passe: ecrire("changer_mon_mot_de_passe", () => null),
    definir_mon_pin: ecrire("definir_mon_pin", () => null),
    // Réglages
    modifier_parametres: ecrire("modifier_parametres", () => etat.parametres),
    taxes_hotel: () => etat.taxes,
    definir_taxes_hotel: ecrire("definir_taxes_hotel", () => etat.taxes),
    sauvegarder_vers: ecrire("sauvegarder_vers", () => null),
    exporter_cle_base: () => "3ad442fb0269ec164acf7f89fce12e84bd2df4482866cdc6c0fef3115fcd3c4f",
    lister_sauvegardes: () => [],
    examiner_sauvegarde: () => ({ nom_hotel: "Hôtel du Lac", devise: "XAF", derniere_activite: null, reservations: 2, clients: 1, comptes: 2, journal_intact: true }),
    restaurer_sauvegarde: ecrire("restaurer_sauvegarde", () => ({ copie_avant: null, distance_arretee: false })),
    etat_distance: () => ({ active: false, relais: null, hotel_id: null, en_attente: 0, active_le: null, dernier_envoi_le: null, derniere_tentative_le: null, derniere_erreur: null }),
    activer_distance: ecrire("activer_distance", () => ({ relais: "https://r.test", hotel_id: "HK7Q-F29X", cle_secrete: "IVNG-I2WV-JNBA-4R3C-LQU7-ZI2E-SI" })),
    // Dialogues natifs
    "plugin:dialog|save": () => null,
    "plugin:dialog|open": () => null,
    "plugin:opener|open_path": () => null,
  };
  const gestionnaires = new Proxy(g, {
    get(cible, nom: string) {
      if (nom in cible) return cible[nom];
      inconnues.add(nom);
      return () => null;
    },
    has: () => true,
  });
  return { gestionnaires, inconnues, ecritures, etat };
}
