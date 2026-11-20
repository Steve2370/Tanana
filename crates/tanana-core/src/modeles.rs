//! Types échangés avec l'interface. `#[ts(export)]` génère automatiquement
//! les types TypeScript correspondants dans `src/bindings/` (via `cargo test`).

use chrono::NaiveDate;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::permissions::Permission;

/// Énumération stockée en TEXT dans la base, avec conversion dans les deux sens.
macro_rules! enum_texte {
    ($nom:ident { $($variante:ident => $texte:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[ts(export)]
        pub enum $nom {
            $(#[serde(rename = $texte)] $variante),+
        }

        impl $nom {
            pub const TOUTES: &'static [$nom] = &[$($nom::$variante),+];

            pub fn texte(self) -> &'static str {
                match self { $($nom::$variante => $texte),+ }
            }

            pub fn depuis_texte(t: &str) -> Option<Self> {
                match t { $($texte => Some($nom::$variante),)+ _ => None }
            }
        }

        impl ToSql for $nom {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.texte()))
            }
        }

        impl FromSql for $nom {
            fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> {
                let t = v.as_str()?;
                $nom::depuis_texte(t).ok_or_else(|| FromSqlError::Other(
                    format!("valeur inconnue pour {} : {t}", stringify!($nom)).into(),
                ))
            }
        }
    };
}

enum_texte!(StatutUtilisateur { Actif => "actif", Suspendu => "suspendu", Desactive => "desactive" });
enum_texte!(StatutChambre { Disponible => "disponible", Maintenance => "maintenance", HorsService => "hors_service" });
enum_texte!(StatutReservation { Confirmee => "confirmee", Arrivee => "arrivee", Partie => "partie", Annulee => "annulee" });
enum_texte!(TypePaiement { Acompte => "acompte", Solde => "solde", Remboursement => "remboursement" });
enum_texte!(ModePaiement { Especes => "especes", Carte => "carte", Mobile => "mobile", Virement => "virement", Autre => "autre" });
enum_texte!(CalculTaxe { Pourcentage => "pourcentage", ParNuit => "par_nuit", ParSejour => "par_sejour" });

// ───────────────────────────── Comptes ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Utilisateur {
    pub id: String,
    pub identifiant: String,
    pub nom: String,
    pub role_id: String,
    pub role_nom: String,
    pub statut: StatutUtilisateur,
    pub doit_changer_mdp: bool,
    pub a_un_pin: bool,
    pub cree_le: String,
    pub derniere_connexion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionOuverte {
    /// Jeton à renvoyer à chaque commande. Gardé en mémoire seulement côté interface.
    pub token: String,
    pub utilisateur: Utilisateur,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Role {
    pub id: String,
    pub nom: String,
    pub systeme: bool,
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct PermissionPerso {
    pub permission: Permission,
    pub accorde: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ConfigurationInitiale {
    pub nom_hotel: String,
    pub devise: String,
    pub decimales_devise: u8,
    pub admin_identifiant: String,
    pub admin_nom: String,
    pub admin_mot_de_passe: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NouvelUtilisateur {
    pub identifiant: String,
    pub nom: String,
    pub mot_de_passe_temporaire: String,
    pub role_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionActive {
    pub session_id: String,
    pub utilisateur_id: String,
    pub utilisateur_nom: String,
    pub debut: String,
    pub derniere_activite: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Parametres {
    pub nom_hotel: String,
    pub devise: String,
    pub decimales_devise: u8,
    pub delai_inactivite_minutes: u32,
    /// Coordonnées imprimées en tête des reçus et factures (toutes facultatives).
    #[serde(default)]
    pub adresse_hotel: Option<String>,
    #[serde(default)]
    pub telephone_hotel: Option<String>,
    #[serde(default)]
    pub email_hotel: Option<String>,
    /// Mentions en bas des documents : numéro de contribuable, RCCM, conditions…
    #[serde(default)]
    pub mentions_documents: Option<String>,
}

enum_texte!(FormatLogo { Png => "png", Jpeg => "jpeg" });

/// Logo de l'hôtel : une image PNG ou JPEG, en base64.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Logo {
    pub format: FormatLogo,
    /// L'image, en base64.
    pub donnees: String,
    pub largeur: u32,
    pub hauteur: u32,
}

/// Image choisie dans les réglages (déjà réduite par l'interface).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NouveauLogo {
    pub format: FormatLogo,
    /// L'image, en base64.
    pub donnees: String,
}

// ─────────────────────────── Hébergement ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct TypeChambre {
    pub id: String,
    pub nom: String,
    pub prix_nuit_defaut: i64,
    pub capacite: i64,
    pub actif: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NouveauTypeChambre {
    pub nom: String,
    pub prix_nuit_defaut: i64,
    pub capacite: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Chambre {
    pub id: String,
    pub numero: String,
    pub type_id: String,
    pub type_nom: String,
    pub prix_nuit_defaut: i64,
    pub etage: Option<i64>,
    pub statut: StatutChambre,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DonneesChambre {
    pub numero: String,
    pub type_id: String,
    pub etage: Option<i64>,
    pub statut: StatutChambre,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Client {
    pub id: String,
    pub nom: String,
    pub prenom: Option<String>,
    pub telephone: Option<String>,
    pub email: Option<String>,
    pub piece_identite: Option<String>,
    pub notes: Option<String>,
    pub cree_le: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DonneesClient {
    pub nom: String,
    pub prenom: Option<String>,
    pub telephone: Option<String>,
    pub email: Option<String>,
    pub piece_identite: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Reservation {
    pub id: String,
    pub chambre_id: String,
    pub chambre_numero: String,
    pub client_id: String,
    pub client_nom: String,
    pub date_arrivee: NaiveDate,
    pub date_depart: NaiveDate,
    pub nuits: i64,
    /// Prix de l'hébergement convenu (taxes comprises éventuelles incluses).
    pub prix_total: i64,
    /// Taxes qui s'ajoutent au prix (taxe de séjour par nuit, TVA ajoutée…).
    pub taxes_ajoutees: i64,
    /// Ce que le client doit en tout : prix + taxes ajoutées.
    pub total_a_payer: i64,
    pub total_encaisse: i64,
    /// Reste à payer : total à payer − encaissé (négatif s'il a trop payé).
    pub solde: i64,
    pub statut: StatutReservation,
    pub notes: Option<String>,
    pub cree_le: String,
    /// Réservation de plusieurs chambres : le groupe dont cette chambre fait partie.
    #[serde(default)]
    pub groupe_id: Option<String>,
    /// Chambre d'un groupe : reste à payer du groupe entier (c'est lui qui compte,
    /// `solde` ne regarde que les paiements enregistrés sur cette chambre).
    #[serde(default)]
    pub solde_groupe: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NouvelleReservation {
    pub chambre_id: String,
    pub client_id: String,
    pub date_arrivee: NaiveDate,
    pub date_depart: NaiveDate,
    /// Si absent : prix par défaut du type de chambre × nombre de nuits.
    pub prix_total: Option<i64>,
    pub notes: Option<String>,
}

/// Une chambre d'une réservation de groupe.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ChambreDuGroupe {
    pub chambre_id: String,
    /// Si absent : prix par défaut du type de chambre × nombre de nuits.
    pub prix_total: Option<i64>,
}

/// Plusieurs chambres pour un même client, aux mêmes dates.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NouveauGroupe {
    pub client_id: String,
    pub chambres: Vec<ChambreDuGroupe>,
    pub date_arrivee: NaiveDate,
    pub date_depart: NaiveDate,
    /// Nom facultatif du groupe (« Équipe SOGEA »).
    pub nom: Option<String>,
    pub notes: Option<String>,
}

/// Un groupe et ses chambres. L'argent se compte pour tout le groupe.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Groupe {
    pub id: String,
    pub nom: Option<String>,
    pub client_id: String,
    pub client_nom: String,
    /// Toutes les chambres, y compris celles annulées, par numéro.
    pub reservations: Vec<Reservation>,
    /// Ce que le client doit pour les chambres non annulées.
    pub total_a_payer: i64,
    /// Encaissé net (encaissements moins remboursements), toutes chambres confondues.
    pub total_encaisse: i64,
    /// Reste à payer pour le groupe (négatif s'il a trop payé).
    pub solde: i64,
    pub cree_le: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ModificationReservation {
    pub chambre_id: String,
    pub date_arrivee: NaiveDate,
    pub date_depart: NaiveDate,
    pub prix_total: i64,
    pub notes: Option<String>,
}

// ──────────────────────────────── Argent ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Paiement {
    pub id: String,
    pub reservation_id: String,
    pub montant: i64,
    pub type_paiement: TypePaiement,
    pub mode: ModePaiement,
    pub date: String,
    pub utilisateur_nom: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Encaissement {
    pub reservation_id: String,
    pub montant: i64,
    /// `acompte` ou `solde` (un remboursement passe par sa propre commande).
    pub type_paiement: TypePaiement,
    pub mode: ModePaiement,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Depense {
    pub id: String,
    pub categorie: String,
    pub montant: i64,
    pub description: Option<String>,
    pub date: NaiveDate,
    pub utilisateur_nom: String,
    /// Une dépense annulée reste affichée (barrée) mais ne compte plus dans les totaux.
    pub annulee: bool,
    pub motif_annulation: Option<String>,
}

/// Un paiement avec de quoi le reconnaître dans la liste de la caisse.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct PaiementDetaille {
    pub id: String,
    pub reservation_id: String,
    pub client_nom: String,
    pub chambre_numero: String,
    pub montant: i64,
    pub type_paiement: TypePaiement,
    pub mode: ModePaiement,
    pub date: String,
    pub utilisateur_nom: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NouvelleDepense {
    pub categorie: String,
    pub montant: i64,
    pub description: Option<String>,
    pub date: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct LigneMode {
    pub mode: ModePaiement,
    pub montant: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct RapportCaisse {
    pub debut: NaiveDate,
    /// Borne exclue.
    pub fin: NaiveDate,
    pub encaissements: i64,
    pub encaissements_par_mode: Vec<LigneMode>,
    pub remboursements: i64,
    pub depenses: i64,
    /// encaissements − remboursements − dépenses
    pub net: i64,
}

// ──────────────────────────────── Journal ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct EntreeJournal {
    pub seq: i64,
    pub date: String,
    pub utilisateur_id: Option<String>,
    pub utilisateur_nom: Option<String>,
    pub action: String,
    pub entite: Option<String>,
    pub entite_id: Option<String>,
    pub avant: Option<serde_json::Value>,
    pub apres: Option<serde_json::Value>,
    /// Action à surveiller (annulation, remboursement, accès…)
    pub sensible: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct FiltreJournal {
    pub utilisateur_id: Option<String>,
    /// Préfixe d'action, ex. `paiement.` ou `auth.`
    pub action_prefixe: Option<String>,
    pub sensibles_seulement: bool,
    /// Pagination : entrées dont `seq` est strictement inférieur.
    pub avant_seq: Option<i64>,
    pub limite: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct RapportIntegrite {
    pub intacte: bool,
    pub entrees_verifiees: i64,
    /// Numéro (`seq`) de la première entrée où la chaîne est rompue.
    pub premiere_rupture: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_texte_aller_retour() {
        for s in StatutReservation::TOUTES {
            assert_eq!(StatutReservation::depuis_texte(s.texte()), Some(*s));
        }
        for s in TypePaiement::TOUTES {
            assert_eq!(TypePaiement::depuis_texte(s.texte()), Some(*s));
        }
        for s in ModePaiement::TOUTES {
            assert_eq!(ModePaiement::depuis_texte(s.texte()), Some(*s));
            assert_eq!(serde_json::to_string(s).unwrap(), format!("\"{}\"", s.texte()));
        }
        assert_eq!(StatutChambre::depuis_texte("inconnu"), None);
    }

    #[test]
    fn enums_compatibles_avec_les_contraintes_sql() {
        // Chaque valeur Rust doit être acceptée par le CHECK de la base.
        let conn = crate::db::ouvrir_en_memoire().unwrap();
        for s in StatutChambre::TOUTES {
            conn.execute(
                "INSERT INTO types_chambre (id, nom, prix_nuit_defaut, capacite) VALUES (?1, ?1, 1, 1)",
                [s.texte()],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO chambres (id, numero, type_id, statut, cree_le) VALUES (?1, ?1, ?1, ?2, 'x')",
                rusqlite::params![s.texte(), s],
            )
            .unwrap();
        }
    }
}

// ───────────────────────────── Reçus et factures ─────────────────────────────

enum_texte!(TypeDocument { Recu => "recu", Facture => "facture" });

/// Un reçu ou une facture déjà émis, tel que l'interface le liste.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct DocumentEmis {
    pub id: String,
    pub type_document: TypeDocument,
    /// « R-2026-0001 » ou « F-2026-0001 »
    pub numero: String,
    pub reservation_id: String,
    pub paiement_id: Option<String>,
    pub emis_le: String,
    pub emis_par_nom: String,
    /// Pour une facture qui en remplace une autre (séjour modifié après la première).
    pub remplace_numero: Option<String>,
}

// ───────────────────────────── Taxes ─────────────────────────────

/// Une taxe : TVA, taxe de séjour… Chaque hôtel règle les siennes, dans sa devise.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct Taxe {
    /// « TVA », « Taxe de séjour »…
    pub libelle: String,
    pub calcul: CalculTaxe,
    /// Pourcentage en centièmes (1925 = 19,25 %), sinon montant dans la plus
    /// petite unité de la devise (2000 = 2 000 Ar).
    pub valeur: i64,
    /// Vrai : déjà comprise dans le prix de la chambre. Faux : s'ajoute au prix.
    pub comprise: bool,
}

/// Une taxe avec son montant calculé pour un séjour.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct LigneTaxe {
    pub taxe: Taxe,
    pub montant: i64,
}

/// Détail des taxes d'un séjour.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct DetailTaxes {
    /// Prix de l'hébergement tel que convenu
    pub prix: i64,
    pub nuits: i64,
    pub lignes: Vec<LigneTaxe>,
    /// Part des taxes déjà comprises dans le prix
    pub total_comprises: i64,
    /// Taxes qui s'ajoutent au prix
    pub total_ajoutees: i64,
    /// Prix sans aucune taxe
    pub hors_taxes: i64,
    /// Prix + taxes ajoutées : ce que le client paie
    pub total_a_payer: i64,
}
