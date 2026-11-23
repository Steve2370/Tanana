use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Toutes les erreurs que l'interface peut recevoir.
///
/// Sérialisée en `{ "code": "...", "details": ... }` pour que le frontend
/// affiche un message adapté sans analyser du texte.
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize, TS, PartialEq)]
#[serde(tag = "code", content = "details", rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(export)]
pub enum Erreur {
    #[error("Application pas encore configurée")]
    NonInitialise,
    #[error("Application déjà configurée")]
    DejaInitialise,
    #[error("Session absente ou fermée")]
    NonAuthentifie,
    #[error("Session expirée après inactivité")]
    SessionExpiree,
    #[error("Identifiant ou mot de passe invalide")]
    IdentifiantsInvalides,
    #[error("Compte suspendu ou désactivé")]
    CompteInactif,
    #[error("Compte bloqué temporairement jusqu'à {jusqu_a}")]
    CompteBloque { jusqu_a: String },
    #[error("Changement de mot de passe obligatoire avant de continuer")]
    ChangementMdpRequis,
    #[error("Permission refusée : {permission}")]
    PermissionRefusee { permission: String },
    #[error("Action interdite sur son propre compte")]
    ActionSurSoiMeme,
    #[error("Impossible : il doit rester au moins un administrateur actif")]
    DernierAdministrateur,
    #[error("Donnée invalide : {0}")]
    Validation(String),
    #[error("Introuvable : {0}")]
    Introuvable(String),
    #[error("Déjà utilisé : {0}")]
    Doublon(String),
    #[error("La chambre est déjà réservée sur cette période")]
    ChambreDejaReservee { reservation_en_conflit: Option<String> },
    #[error("La chambre n'est pas disponible (maintenance ou hors service)")]
    ChambreIndisponible,
    #[error("Transition de statut impossible : {de} → {vers}")]
    StatutInvalide { de: String, vers: String },
    #[error("Solde impayé : {montant}")]
    SoldeImpaye { montant: i64 },
    #[error("Montant invalide : {0}")]
    MontantInvalide(String),
    #[error("Relais de consultation à distance injoignable : {0}")]
    RelaisInjoignable(String),
    #[error("Le relais refuse la demande : {0}")]
    RelaisRefuse(String),
    /// L'hôtel n'existe pas (ou plus) sur le relais : identifiant mal recopié,
    /// ou consultation à distance arrêtée à l'hôtel.
    #[error("Hôtel inconnu du serveur de consultation à distance")]
    HotelInconnuDuRelais,
    #[error("Clé secrète incorrecte, ou données abîmées")]
    CleDistanceIncorrecte,
    #[error("Consultation à distance : modification impossible")]
    LectureSeule,
    /// Un autre ordinateur est devenu l'ordinateur principal de l'hôtel.
    #[error("Cet ordinateur n'est plus l'ordinateur principal de l'hôtel")]
    PosteRemplace,
    /// Modification à distance : l'ordinateur de l'hôtel n'est pas en ligne.
    #[error("L'ordinateur de l'hôtel n'est pas joignable")]
    HotelInjoignable,
    /// Modification à distance envoyée, mais aucune réponse n'est revenue.
    #[error("L'ordinateur de l'hôtel n'a pas répondu")]
    PasDeReponseHotel,
    /// Trop de mots de passe incorrects pour ce compte (relais).
    #[error("Trop d'essais : réessayez dans 15 minutes")]
    TropDEssais,
    /// « Retrouver mon hôtel » : l'ancien ordinateur n'a encore envoyé aucune copie complète.
    #[error("Aucune copie complète de l'hôtel sur le serveur")]
    InstantaneAbsent,
    #[error("Impossible d'enregistrer le fichier : {0}")]
    EnregistrementImpossible(String),
    #[error("Erreur de base de données : {0}")]
    Base(String),
}

pub type Resultat<T> = Result<T, Erreur>;

impl From<rusqlite::Error> for Erreur {
    fn from(e: rusqlite::Error) -> Self {
        // Les triggers de la base remontent des codes lisibles.
        let texte = e.to_string();
        if texte.contains("CHAMBRE_DEJA_RESERVEE") {
            return Erreur::ChambreDejaReservee { reservation_en_conflit: None };
        }
        if let rusqlite::Error::SqliteFailure(f, Some(msg)) = &e
            && f.code == rusqlite::ErrorCode::ConstraintViolation
            && msg.contains("UNIQUE")
        {
            return Erreur::Doublon(msg.clone());
        }
        Erreur::Base(texte)
    }
}

impl From<rusqlite_migration::Error> for Erreur {
    fn from(e: rusqlite_migration::Error) -> Self {
        Erreur::Base(format!("migration : {e}"))
    }
}

impl From<serde_json::Error> for Erreur {
    fn from(e: serde_json::Error) -> Self {
        Erreur::Base(format!("json : {e}"))
    }
}
