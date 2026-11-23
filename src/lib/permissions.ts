import type { Permission } from "../bindings/Permission";

/** Libellés en français. `Record<Permission, …>` oblige à couvrir chaque
 *  permission définie en Rust : un oubli ne compile pas. */
export const LIBELLES_PERMISSIONS: Record<Permission, string> = {
  "chambre.voir": "Voir les chambres et le planning",
  "chambre.gerer": "Créer et modifier les chambres",
  "client.voir": "Voir les fiches clients",
  "client.gerer": "Créer et modifier les clients",
  "reservation.voir": "Voir les réservations",
  "reservation.creer": "Créer une réservation",
  "reservation.modifier": "Modifier dates, chambre ou prix",
  "reservation.annuler": "Annuler une réservation",
  "sejour.gerer": "Arrivées et départs",
  "paiement.encaisser": "Encaisser acomptes et soldes",
  "paiement.rembourser": "Faire un remboursement",
  "depense.voir": "Voir les dépenses",
  "depense.saisir": "Enregistrer une dépense",
  "rapport.financier.voir": "Voir les rapports financiers",
  "utilisateur.gerer": "Gérer les comptes et les accès",
  "journal.voir": "Voir le journal des actions",
  "parametres.gerer": "Modifier les paramètres de l'hôtel",
  "sauvegarde.faire": "Faire une sauvegarde",
};

export const TOUTES_PERMISSIONS = Object.keys(LIBELLES_PERMISSIONS) as Permission[];

export const GROUPES_PERMISSIONS: { titre: string; permissions: Permission[] }[] = [
  { titre: "Hébergement", permissions: ["chambre.voir", "chambre.gerer", "client.voir", "client.gerer"] },
  {
    titre: "Réservations",
    permissions: [
      "reservation.voir",
      "reservation.creer",
      "reservation.modifier",
      "reservation.annuler",
      "sejour.gerer",
    ],
  },
  {
    titre: "Argent",
    permissions: [
      "paiement.encaisser",
      "paiement.rembourser",
      "depense.voir",
      "depense.saisir",
      "rapport.financier.voir",
    ],
  },
  {
    titre: "Administration",
    permissions: ["utilisateur.gerer", "journal.voir", "parametres.gerer", "sauvegarde.faire"],
  },
];

/** Libellés lisibles des actions du journal. */
const LIBELLES_ACTIONS: Record<string, string> = {
  "systeme.initialisation": "Configuration initiale",
  "auth.connexion": "Connexion",
  "auth.deconnexion": "Déconnexion",
  "auth.echec": "Échec de connexion",
  "auth.blocage": "Compte bloqué (trop d’essais)",
  "auth.changement_mdp": "Changement de mot de passe",
  "auth.definition_pin": "Définition du PIN",
  "auth.recuperation": "Récupération d'accès admin",
  "auth.recuperation_echec": "Code de récupération invalide",
  "acces.refuse": "Accès refusé",
  "utilisateur.creation": "Création de compte",
  "utilisateur.changement_role": "Changement de rôle",
  "utilisateur.suspension": "Compte suspendu",
  "utilisateur.desactivation": "Compte désactivé",
  "utilisateur.reactivation": "Compte réactivé",
  "utilisateur.reinitialisation_mdp": "Mot de passe réinitialisé",
  "utilisateur.permission_perso": "Permission individuelle modifiée",
  "utilisateur.fermeture_session": "Session fermée de force",
  "role.creation": "Création de rôle",
  "role.modification": "Rôle modifié",
  "type_chambre.creation": "Type de chambre créé",
  "type_chambre.modification": "Type de chambre modifié",
  "chambre.creation": "Chambre créée",
  "chambre.modification": "Chambre modifiée",
  "client.creation": "Client créé",
  "client.modification": "Client modifié",
  "reservation.creation": "Réservation créée",
  "reservation.modification": "Réservation modifiée",
  "reservation.annulation": "Réservation annulée",
  "groupe.creation": "Réservation de groupe (plusieurs chambres)",
  "groupe.ajout": "Chambre ajoutée à un groupe",
  "sejour.arrivee": "Arrivée (check-in)",
  "sejour.depart": "Départ (check-out)",
  "paiement.encaissement": "Encaissement",
  "paiement.remboursement": "Remboursement",
  "depense.saisie": "Dépense enregistrée",
  "depense.annulation": "Dépense annulée",
  "document.emission": "Reçu ou facture imprimé",
  "taxes.modification": "Taxes de l'hôtel modifiées",
  "reservation.taxes": "Taxes d'un séjour modifiées",
  "document.reimpression": "Reçu ou facture réimprimé (duplicata)",
  "parametres.modification": "Paramètres modifiés",
  "parametres.logo": "Logo de l'hôtel changé",
  "parametres.export_cle": "Clé de protection affichée",
  "sauvegarde.manuelle": "Copie de sécurité (manuelle)",
  "sauvegarde.automatique": "Copie de sécurité (automatique)",
  "sauvegarde.restauration": "Copie de sécurité remise (retour en arrière)",
  "distance.activation": "Travail à distance activé",
  "distance.reprise": "Hôtel repris sur cet ordinateur (ordinateur de l'hôtel)",
  "distance.cle_affichee": "Clé de consultation à distance affichée",
  "distance.renvoi_complet": "Consultation à distance : tout renvoyé",
  "distance.desactivation": "Consultation à distance arrêtée",
};

/** `apres` : détails de l'entrée ; une connexion faite depuis un autre ordinateur est dite « à distance ». */
export function libelleAction(action: string, apres?: unknown): string {
  const libelle = LIBELLES_ACTIONS[action] ?? action;
  const methode = (apres as { methode?: unknown } | null | undefined)?.methode;
  return action === "auth.connexion" && methode === "distance" ? `${libelle} à distance` : libelle;
}
