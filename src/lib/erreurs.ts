import type { Erreur } from "../bindings/Erreur";
import type { StatutReservation } from "../bindings/StatutReservation";
import { formatHeure } from "./format";
import { LIBELLE_STATUT_RESERVATION } from "./libelles";

/** Erreur levée par l'API : garde le code structuré venu de Rust. */
export class ErreurTanana extends Error {
  readonly erreur: Erreur;
  constructor(erreur: Erreur) {
    super(messageErreur(erreur));
    this.name = "ErreurTanana";
    this.erreur = erreur;
  }
  get code(): Erreur["code"] {
    return this.erreur.code;
  }
}

/** Convertit n'importe quelle valeur rejetée par `invoke` en ErreurTanana. */
export function versErreur(e: unknown): ErreurTanana {
  if (e instanceof ErreurTanana) return e;
  if (typeof e === "object" && e !== null && typeof (e as { code?: unknown }).code === "string") {
    return new ErreurTanana(e as Erreur);
  }
  return new ErreurTanana({ code: "BASE", details: e instanceof Error ? e.message : String(e) });
}

/**
 * Message affiché à l'utilisateur. Règles d'écriture :
 *  - dire ce qui se passe, en mots simples, sans jargon technique ;
 *  - dire QUOI FAIRE ensuite ;
 *  - ne jamais culpabiliser.
 * Le `switch` est exhaustif : une nouvelle erreur Rust non traduite ne compile pas.
 */
export function messageErreur(e: Erreur): string {
  switch (e.code) {
    case "NON_INITIALISE":
      return "Tanana n'est pas encore prêt. Fermez l'application, puis rouvrez-la.";
    case "DEJA_INITIALISE":
      return "L'hôtel est déjà configuré. Vous pouvez vous connecter.";
    case "NON_AUTHENTIFIE":
      return "Vous avez été déconnecté. Reconnectez-vous pour continuer.";
    case "SESSION_EXPIREE":
      return "Par sécurité, vous avez été déconnecté après un moment sans activité. Reconnectez-vous pour continuer.";
    case "IDENTIFIANTS_INVALIDES":
      return "Identifiant ou mot de passe incorrect. Vérifiez l'orthographe et la touche Majuscule, puis réessayez.";
    case "COMPTE_INACTIF":
      return "Ce compte est suspendu. Demandez à l'administrateur de le réactiver.";
    case "COMPTE_BLOQUE":
      return `Trop d'essais incorrects. Par sécurité, ce compte est bloqué jusqu'à ${formatHeure(e.details.jusqu_a)}. Réessayez ensuite, ou demandez à l'administrateur.`;
    case "CHANGEMENT_MDP_REQUIS":
      return "Avant de continuer, choisissez votre mot de passe personnel.";
    case "PERMISSION_REFUSEE":
      return "Votre compte n'a pas le droit de faire cette action. Demandez à l'administrateur si vous en avez besoin.";
    case "ACTION_SUR_SOI_MEME":
      return "Vous ne pouvez pas faire cette action sur votre propre compte. Demandez à un autre administrateur.";
    case "DERNIER_ADMINISTRATEUR":
      return "Impossible : l'hôtel doit toujours garder au moins un administrateur actif.";
    case "VALIDATION":
      return majuscule(e.details) + ".";
    case "INTROUVABLE":
      return `Élément introuvable (${e.details}). Il a peut-être été modifié entre-temps : rechargez l'écran.`;
    case "DOUBLON":
      return `Ce nom est déjà utilisé : ${e.details}. Choisissez-en un autre.`;
    case "CHAMBRE_DEJA_RESERVEE":
      return "Cette chambre est déjà réservée sur une partie de ces dates. Choisissez d'autres dates ou une autre chambre.";
    case "CHAMBRE_INDISPONIBLE":
      return "Cette chambre est en maintenance ou hors service. Choisissez une autre chambre.";
    case "STATUT_INVALIDE":
      if (e.details.vers === "facture") {
        return "Une réservation annulée n'a pas de facture. Vous pouvez imprimer le reçu de chaque paiement.";
      }
      return `Cette opération n'est pas possible pour une réservation « ${LIBELLE_STATUT_RESERVATION[e.details.de as StatutReservation] ?? e.details.de} ».`;
    case "SOLDE_IMPAYE":
      return `Le client doit encore payer ${e.details.montant.toLocaleString("fr-FR")}. Encaissez le solde avant d'enregistrer le départ.`;
    case "MONTANT_INVALIDE":
      return `Montant incorrect : ${e.details}.`;
    case "RELAIS_INJOIGNABLE":
      return "Impossible de joindre le serveur de Tanana. Vérifiez la connexion Internet de cet ordinateur, puis réessayez.";
    case "RELAIS_REFUSE":
      // À l'activation, l'hôtel n'a pas encore d'identifiant : seul le code d'invitation peut être en cause.
      if (e.details.includes("invitation")) {
        return "Le code d'invitation n'est pas accepté par le serveur. Vérifiez-le (il vous a été donné avec Tanana), puis réessayez.";
      }
      return `Le serveur de consultation à distance refuse la demande (${e.details}). Vérifiez l'adresse du serveur, l'identifiant de l'hôtel et la clé.`;
    case "HOTEL_INCONNU_DU_RELAIS":
      return "Le serveur ne connaît pas cet hôtel. Vérifiez l'identifiant de l'hôtel. Si la consultation à distance a été arrêtée à l'hôtel, il faut la réactiver là-bas et noter la nouvelle clé.";
    case "CLE_DISTANCE_INCORRECTE":
      return "La clé secrète ne correspond pas à cet hôtel. Vérifiez chaque groupe de 4 caractères sur le papier où elle a été notée.";
    case "LECTURE_SEULE":
      return "L'hôtel n'est pas joignable pour l'instant : vous pouvez consulter, mais pas modifier. Réessayez plus tard.";
    case "POSTE_REMPLACE":
      return "Cet ordinateur n'est plus celui de l'hôtel : l'hôtel a été installé sur un autre ordinateur.";
    case "HOTEL_INJOIGNABLE":
      return "L'ordinateur de l'hôtel n'est pas joignable (éteint, ou Internet coupé là-bas). Rien n'a été modifié. Réessayez plus tard.";
    case "PAS_DE_REPONSE_HOTEL":
      return "L'ordinateur de l'hôtel n'a pas répondu à temps. Le changement a peut-être été fait : attendez une minute, regardez s'il apparaît, puis réessayez si besoin.";
    case "TROP_D_ESSAIS":
      return "Trop de mots de passe incorrects. Par sécurité, attendez 15 minutes avant de réessayer.";
    case "INSTANTANE_ABSENT":
      return "Le serveur n'a pas encore reçu de copie complète de votre hôtel. Ouvrez Tanana quelques minutes avec Internet sur l'ordinateur où vous avez créé l'hôtel, puis réessayez.";
    case "ENREGISTREMENT_IMPOSSIBLE":
      return `Le fichier n'a pas pu être enregistré sur cet ordinateur. Vérifiez qu'il reste de la place, puis réessayez. Détail pour le support : ${e.details}`;
    case "BASE":
      return `Un problème technique est survenu. Réessayez. Si cela recommence, notez ce message pour le support : ${e.details}`;
    default: {
      const jamais: never = e;
      return String(jamais);
    }
  }
}

function majuscule(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Erreurs qui signifient « la session n'est plus valable ». */
export function estSessionPerdue(e: unknown): boolean {
  return e instanceof ErreurTanana && (e.code === "NON_AUTHENTIFIE" || e.code === "SESSION_EXPIREE");
}
