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
    /// Jeton (dérivé de la clé secrète) des actions d'administrateur à distance :
    /// envoyer une modification, récupérer l'instantané, devenir l'ordinateur principal.
    #[serde(default)]
    pub jeton_admin: Option<String>,
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
    /// Jeton réservé à l'ordinateur principal pour envoyer ses lots
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

// ───────────────────── Accès par identifiant et mot de passe ─────────────────────
//
// L'ordinateur principal dépose, pour chaque administrateur, un « coffre » :
// l'identifiant de l'hôtel et la clé secrète, chiffrés avec une clé tirée du
// mot de passe (Argon2). Le relais ne garde que l'empreinte d'une « preuve »,
// elle aussi tirée du mot de passe : il peut vérifier un mot de passe, jamais
// le connaître ni ouvrir le coffre. La clé de recherche est l'empreinte de
// l'identifiant : le relais ne voit pas les identifiants non plus.

/// PUT /v1/hotels/{id}/acces/{recherche} (jeton d'envoi)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoffreAcces {
    /// Sel d'Argon2, base64
    pub sel: String,
    /// SHA-256 (hex) de la preuve
    pub verif: String,
    pub nonce: String,
    pub chiffre: String,
}

/// GET /v1/acces/{recherche}/sel
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelAcces {
    pub sel: String,
}

/// POST /v1/acces/{recherche}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreuveAcces {
    /// Hex
    pub preuve: String,
}

/// Réponse : le coffre chiffré (seul le mot de passe l'ouvre).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccesOuvert {
    pub hotel_id: String,
    pub nonce: String,
    pub chiffre: String,
}

// ───────────────────── Instantané et ordinateur principal ─────────────────────

/// Taille maximale d'un instantané chiffré (base64).
pub const TAILLE_INSTANTANE_MAX: usize = 48 * 1024 * 1024;

/// PUT /v1/hotels/{id}/instantane (jeton d'envoi) : copie complète et chiffrée
/// de la base, prise quand tous les lots jusqu'à `apres_numero` étaient partis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Instantane {
    pub apres_numero: i64,
    pub nonce: String,
    pub chiffre: String,
}

/// GET /v1/hotels/{id}/instantane (jeton d'administrateur)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstantaneRecu {
    pub apres_numero: i64,
    /// RFC 3339
    pub recu_le: String,
    pub nonce: String,
    pub chiffre: String,
}

/// POST /v1/hotels/{id}/principal (jeton d'administrateur) : cet ordinateur
/// devient l'ordinateur principal ; l'ancien ne peut plus rien envoyer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NouveauPrincipal {
    pub jeton_envoi: String,
}

// ───────────────────── Modifications à distance ─────────────────────

/// Délai sans nouvelles de l'ordinateur principal au-delà duquel l'hôtel est
/// considéré comme injoignable (secondes).
pub const PRINCIPAL_JOIGNABLE_SECONDES: i64 = 75;

/// Attente maximale d'une réponse de l'hôtel à une modification (secondes).
pub const ATTENTE_REPONSE_SECONDES: u64 = 45;

/// Une demande chiffrée de l'administrateur à distance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandeChiffree {
    /// Identifiant unique (UUID), lié au chiffrement
    pub id: String,
    pub nonce: String,
    pub chiffre: String,
}

/// GET /v1/hotels/{id}/commandes?attente=N (jeton d'envoi)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CommandesEnAttente {
    pub commandes: Vec<CommandeChiffree>,
}

/// Réponse chiffrée de l'hôtel (POST /v1/hotels/{id}/reponses/{commande}).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReponseChiffree {
    pub nonce: String,
    pub chiffre: String,
}

/// Messages d'erreur du relais que l'app reconnaît.
pub const ERREUR_POSTE_REMPLACE: &str = "poste remplacé";
pub const ERREUR_HOTEL_INJOIGNABLE: &str = "hôtel injoignable";
pub const ERREUR_PAS_DE_REPONSE: &str = "pas de réponse de l'hôtel";

/// Corps des réponses d'erreur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErreurRelais {
    pub erreur: String,
}
