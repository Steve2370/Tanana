//! Messages échangés entre Tanana (l'hôtel et l'administrateur à distance)
//! et le relais. Le relais ne voit que des paquets chiffrés : rien ici ne
//! contient de donnée lisible de l'hôtel.

use serde::{Deserialize, Serialize};

/// Version du protocole, dans le chemin des requêtes : `/v1/…`.
pub const VERSION: &str = "v1";

/// Taille maximale acceptée pour un lot chiffré (en caractères base64).
pub const TAILLE_LOT_MAX: usize = 2 * 1024 * 1024;

/// Nombre maximal de lots renvoyés par une lecture.
pub const LOTS_PAR_PAGE: usize = 50;

/// Un paquet chiffré : le relais le garde tel quel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LotChiffre {
    /// Numéros des changements contenus (de la boîte d'envoi de l'hôtel)
    pub seq_debut: i64,
    pub seq_fin: i64,
    /// Base64
    pub nonce: String,
    /// Base64
    pub chiffre: String,
}

/// Texte connu chiffré à l'inscription : permet à l'administrateur de
/// vérifier sa clé secrète avant de télécharger.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Temoin {
    pub nonce: String,
    pub chiffre: String,
}

/// POST /v1/hotels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemandeInscription {
    /// Jeton (dérivé de la clé secrète) qui autorisera la lecture
    pub jeton_lecture: String,
    pub temoin: Temoin,
    /// Code remis par l'exploitant du relais (si le relais en exige un)
    #[serde(default)]
    pub code_invitation: Option<String>,
}

/// Réponse à l'inscription.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Inscription {
    /// Identifiant court de l'hôtel sur ce relais, à donner à l'administrateur
    pub hotel_id: String,
    /// Jeton réservé à l'hôtel pour envoyer ses lots
    pub jeton_envoi: String,
}

/// Réponse à un envoi.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Accuse {
    /// Numéro donné par le relais (croissant, par hôtel)
    pub numero: i64,
}

/// Un lot tel que le relais le redonne.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LotRecu {
    pub numero: i64,
    /// Heure de réception par le relais (RFC 3339, UTC)
    pub recu_le: String,
    pub lot: LotChiffre,
}

/// GET /v1/hotels/{id}/lots?apres=N
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageLots {
    pub lots: Vec<LotRecu>,
    /// Il en reste après cette page
    pub suite: bool,
    /// Dernier envoi reçu de l'hôtel (même vide), RFC 3339
    pub dernier_contact: Option<String>,
}

/// Corps des réponses d'erreur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErreurRelais {
    pub erreur: String,
}
