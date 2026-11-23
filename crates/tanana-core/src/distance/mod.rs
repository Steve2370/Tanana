//! Consultation à distance.
//!
//! **À l'hôtel** : quand la consultation est activée, chaque changement est
//! noté dans la boîte d'envoi ([`capture`]). Dès qu'Internet est là, les
//! changements partent par lots compressés et chiffrés ([`chiffrement`]) vers
//! le relais ([`relais`]). L'hôtel fonctionne exactement pareil sans Internet.
//!
//! **Chez l'administrateur** : Tanana télécharge les lots, les déchiffre avec
//! la clé secrète et reconstruit une **copie en lecture seule** de l'hôtel,
//! consultée avec les écrans habituels.
//!
//! Les appels réseau ne se font jamais pendant une transaction : chaque
//! opération est découpée en « préparer » (base), « échanger » (réseau),
//! « terminer » (base), pour ne jamais bloquer l'écran pendant un envoi lent.

pub mod acces;
pub mod capture;
pub mod chiffrement;
pub mod commandes;
pub mod principal;
pub mod relais;

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::types::Value as Sql;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tanana_protocole::{DemandeInscription, Inscription, LotChiffre, PageLots, Temoin};
use ts_rs::TS;

use crate::audit;
use crate::db::{self, CleBase};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{SessionOuverte, StatutUtilisateur, Utilisateur};
use crate::permissions::Permission;
use crate::{Acteur, Tanana, horodater};
use chiffrement::{Secret, TEXTE_TEMOIN, associe_lot, associe_temoin};
use relais::Relais;

/// Limites d'un lot : assez petit pour une connexion mobile instable.
const LOT_OCTETS_MAX: usize = 256 * 1024;
const LOT_CHANGEMENTS_MAX: i64 = 2_000;
/// Inactivité tolérée en consultation à distance (lecture seule, ordinateur du propriétaire).
pub const DELAI_COPIE_MINUTES: i64 = 240;

// ───────────────────────────── Types ─────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Ecrire,
    Effacer,
}

/// Un changement d'une ligne d'une table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Changement {
    #[serde(rename = "t")]
    pub table: String,
    #[serde(rename = "o")]
    pub operation: Operation,
    /// Valeurs de la clé primaire
    #[serde(rename = "c")]
    pub cle: Vec<Value>,
    /// Ligne complète (absente pour un effacement)
    #[serde(rename = "d", default, skip_serializing_if = "Option::is_none")]
    pub donnees: Option<serde_json::Map<String, Value>>,
}

/// Contenu (en clair) d'un lot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lot {
    /// Heure de l'hôtel à la préparation (RFC 3339, UTC)
    pub genere_le: String,
    pub changements: Vec<Changement>,
}

/// État de la consultation à distance, vu depuis l'hôtel.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct EtatDistance {
    pub active: bool,
    pub relais: Option<String>,
    pub hotel_id: Option<String>,
    /// Changements pas encore partis
    pub en_attente: i64,
    pub active_le: Option<String>,
    pub dernier_envoi_le: Option<String>,
    pub derniere_tentative_le: Option<String>,
    /// Dernière erreur d'envoi (effacée au premier envoi réussi)
    pub derniere_erreur: Option<String>,
}

/// Ce que l'administrateur doit noter pour consulter l'hôtel à distance.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct CodesDistance {
    pub relais: String,
    pub hotel_id: String,
    /// « K7QF-29XM-… »
    pub cle_secrete: String,
}

/// Préparation d'une activation : à envoyer au relais, puis à terminer.
#[derive(Debug, Clone)]
pub struct ActivationPreparee {
    pub secret: Secret,
    pub demande: DemandeInscription,
}

/// Un lot prêt à partir.
#[derive(Debug, Clone)]
pub struct EnvoiPrepare {
    pub relais: String,
    pub hotel_id: String,
    pub jeton_envoi: String,
    pub lot: LotChiffre,
    pub changements: usize,
}

/// Bilan d'une synchronisation.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct BilanEnvoi {
    pub lots: i64,
    pub changements: i64,
}

/// État d'une copie consultée à distance.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct EtatCopie {
    pub relais: String,
    pub hotel_id: String,
    pub nom_hotel: Option<String>,
    /// Heure de l'hôtel au moment où les dernières données reçues ont été préparées
    pub donnees_du: Option<String>,
    /// Dernier signe de vie de l'hôtel sur le relais
    pub dernier_contact: Option<String>,
    /// Dernière mise à jour réussie de la copie
    pub mise_a_jour_le: Option<String>,
}

/// Ce qu'il faut pour lire la suite des lots d'une copie.
#[derive(Debug, Clone)]
pub struct LecturePreparee {
    pub relais: String,
    pub hotel_id: String,
    pub jeton_lecture: String,
    pub apres: i64,
}

/// Session de consultation d'une copie, gardée en mémoire.
#[derive(Debug, Default)]
pub struct SessionCopie {
    jeton: Option<String>,
    derniere_activite: Option<DateTime<Utc>>,
    echecs: u32,
    bloque_jusqu_a: Option<DateTime<Utc>>,
}

/// Essais ratés avant blocage, et durée du blocage.
const ECHECS_MAX_COPIE: u32 = 5;
const BLOCAGE_COPIE_MINUTES: i64 = 15;

fn utilisateur_distant() -> Utilisateur {
    Utilisateur {
        id: "distant".into(),
        identifiant: "distant".into(),
        nom: "Consultation à distance".into(),
        role_id: "distant".into(),
        role_nom: "Lecture seule".into(),
        statut: StatutUtilisateur::Actif,
        doit_changer_mdp: false,
        a_un_pin: false,
        cree_le: String::new(),
        derniere_connexion: None,
    }
}

// ───────────────────────────── Aides ─────────────────────────────

fn active(conn: &Connection) -> Resultat<bool> {
    Ok(conn.query_row("SELECT count(*) FROM sync_config", [], |r| r.get::<_, i64>(0))? > 0)
}

/// À chaque ouverture de la base de l'hôtel : les triggers suivent les
/// colonnes ajoutées par une mise à jour de Tanana.
pub(crate) fn apres_ouverture(conn: &Connection) -> Resultat<()> {
    if active(conn)? {
        capture::installer(conn)?;
    }
    Ok(())
}

/// L'« utilisateur » d'une copie : il voit tout, ne modifie rien.
pub(crate) fn acteur_distant(maintenant: DateTime<Utc>) -> Acteur {
    Acteur {
        utilisateur_id: "distant".into(),
        nom: "Consultation à distance".into(),
        role_id: "distant".into(),
        session_id: "distant".into(),
        permissions: PERMISSIONS_DISTANTES.into_iter().collect::<HashSet<_>>(),
        doit_changer_mdp: false,
        maintenant,
    }
}

/// Ce que l'administrateur à distance peut ouvrir.
pub const PERMISSIONS_DISTANTES: [Permission; 7] = [
    Permission::ChambreVoir,
    Permission::ClientVoir,
    Permission::ReservationVoir,
    Permission::DepenseVoir,
    Permission::RapportFinancierVoir,
    Permission::JournalVoir,
    Permission::UtilisateurGerer, // pour la liste des employés ; la base refuse toute modification
];

/// Nouvelle clé secrète et demande d'inscription au relais.
pub(crate) fn preparer_demande(code_invitation: Option<&str>) -> Resultat<ActivationPreparee> {
    let secret = Secret::generer()?;
    // Le témoin permettra à l'administrateur de vérifier sa clé avant tout téléchargement.
    let (nonce, chiffre) = secret.chiffrer(TEXTE_TEMOIN, &associe_temoin("inscription"))?;
    Ok(ActivationPreparee {
        demande: DemandeInscription {
            jeton_lecture: secret.jeton_lecture(),
            jeton_admin: Some(secret.jeton_admin()),
            temoin: Temoin { nonce, chiffre },
            code_invitation: code_invitation.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()),
        },
        secret,
    })
}

/// Enregistre la liaison avec le relais, installe la capture et met tout
/// l'hôtel dans la boîte d'envoi. `utilisateur_id` absent : action du système.
pub(crate) fn enregistrer_liaison(
    tx: &Connection,
    adresse: &str,
    preparation: &ActivationPreparee,
    inscription: &Inscription,
    horodatage: &str,
    utilisateur_id: Option<&str>,
) -> Resultat<()> {
    if active(tx)? {
        return Err(Erreur::Validation("la consultation à distance est déjà activée".into()));
    }
    let adresse = adresse.trim().trim_end_matches('/');
    tx.execute(
        "INSERT INTO sync_config (id, relais, hotel_id, jeton_envoi, secret, active_le) VALUES (1, ?1, ?2, ?3, ?4, ?5)",
        params![adresse, inscription.hotel_id, inscription.jeton_envoi, preparation.secret.afficher(), horodatage],
    )?;
    tx.execute("DELETE FROM sync_boite", [])?;
    capture::installer(tx)?;
    audit::ecrire(
        tx,
        horodatage,
        utilisateur_id,
        "distance.activation",
        Some("parametres"),
        None,
        None,
        Some(&json!({ "relais": adresse, "hotel_id": inscription.hotel_id })),
    )?;
    // Le journal ci-dessus est déjà capturé ; le reste de l'hôtel suit.
    tx.execute("DELETE FROM sync_boite", [])?;
    capture::tout_mettre_en_boite(tx)?;
    // Les coffres préparés avant l'activation seront fermés et déposés
    tx.execute("UPDATE sync_acces SET a_envoyer = 1", [])?;
    Ok(())
}

fn vers_sql(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(*b as i64),
        Value::Number(n) => n.as_i64().map(Sql::Integer).unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => Sql::Text(s.clone()),
        autre => Sql::Text(autre.to_string()),
    }
}

// ───────────────────────────── À l'hôtel ─────────────────────────────

impl Tanana {
    /// 1/2 - Vérifie le droit, prépare la clé secrète et la demande d'inscription.
    pub fn preparer_activation_distance(
        &mut self,
        token: &str,
        code_invitation: Option<&str>,
    ) -> Resultat<ActivationPreparee> {
        self.executer(token, Some(Permission::ParametresGerer), |tx, _| {
            if active(tx)? {
                return Err(Erreur::Validation("la consultation à distance est déjà activée".into()));
            }
            Ok(())
        })?;
        preparer_demande(code_invitation)
    }

    /// 2/2 - Enregistre la liaison, installe la capture et met tout l'hôtel en boîte d'envoi.
    pub fn terminer_activation_distance(
        &mut self,
        token: &str,
        adresse_relais: &str,
        preparation: &ActivationPreparee,
        inscription: &Inscription,
    ) -> Resultat<CodesDistance> {
        let adresse = adresse_relais.trim().trim_end_matches('/').to_string();
        self.executer(token, Some(Permission::ParametresGerer), |tx, a| {
            enregistrer_liaison(tx, &adresse, preparation, inscription, &a.horodatage(), Some(&a.utilisateur_id))?;
            tx.execute("DELETE FROM sync_activation_auto", [])?;
            Ok(CodesDistance {
                relais: adresse.clone(),
                hotel_id: inscription.hotel_id.clone(),
                cle_secrete: preparation.secret.afficher(),
            })
        })
    }

    /// Activation complète (préparer, inscrire, terminer) : pratique en test ;
    /// l'application, elle, libère la base pendant l'appel réseau.
    pub fn activer_consultation_distance(
        &mut self,
        token: &str,
        adresse_relais: &str,
        code_invitation: Option<&str>,
        relais: &dyn Relais,
    ) -> Resultat<CodesDistance> {
        let p = self.preparer_activation_distance(token, code_invitation)?;
        let inscription = relais.inscrire(&p.demande)?;
        self.terminer_activation_distance(token, adresse_relais, &p, &inscription)
    }

    pub fn etat_consultation_distance(&mut self, token: &str) -> Resultat<EtatDistance> {
        self.executer(token, Some(Permission::ParametresGerer), |tx, _| etat_distance(tx))
    }

    /// Réaffiche la clé secrète (mot de passe de l'administrateur exigé, action journalisée).
    pub fn afficher_cle_distance(&mut self, token: &str, mot_de_passe: &str) -> Resultat<CodesDistance> {
        self.executer(token, Some(Permission::ParametresGerer), |tx, a| {
            let hash: String =
                tx.query_row("SELECT mot_de_passe_hash FROM utilisateurs WHERE id = ?1", [&a.utilisateur_id], |r| {
                    r.get(0)
                })?;
            if !crate::auth::verifier_hash(mot_de_passe, &hash) {
                return Err(Erreur::IdentifiantsInvalides);
            }
            let codes = tx
                .query_row("SELECT relais, hotel_id, secret FROM sync_config WHERE id = 1", [], |r| {
                    Ok(CodesDistance { relais: r.get(0)?, hotel_id: r.get(1)?, cle_secrete: r.get(2)? })
                })
                .optional()?
                .ok_or_else(|| Erreur::Validation("la consultation à distance n'est pas activée".into()))?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "distance.cle_affichee",
                Some("parametres"),
                None,
                None,
                None,
            )?;
            Ok(codes)
        })
    }

    /// Remet tout l'hôtel dans la boîte d'envoi (si le relais a perdu ses
    /// données, ou pour une nouvelle copie complète). Renvoie le nombre de lignes.
    pub fn renvoyer_tout_distance(&mut self, token: &str) -> Resultat<i64> {
        self.executer(token, Some(Permission::ParametresGerer), |tx, a| {
            if !active(tx)? {
                return Err(Erreur::Validation("la consultation à distance n'est pas activée".into()));
            }
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "distance.renvoi_complet",
                Some("parametres"),
                None,
                None,
                None,
            )?;
            Ok(capture::tout_mettre_en_boite(tx)? as i64)
        })
    }

    /// Arrête les envois. Renvoie (relais, hotel_id, jeton_envoi) pour
    /// prévenir le relais, si possible, qu'il peut tout effacer.
    pub fn desactiver_consultation_distance(&mut self, token: &str) -> Resultat<Option<(String, String, String)>> {
        self.executer(token, Some(Permission::ParametresGerer), |tx, a| {
            let config = tx
                .query_row("SELECT relais, hotel_id, jeton_envoi FROM sync_config WHERE id = 1", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })
                .optional()?;
            capture::retirer(tx)?;
            tx.execute("DELETE FROM sync_boite", [])?;
            tx.execute("DELETE FROM sync_config", [])?;
            if config.is_some() {
                audit::ecrire(
                    tx,
                    &a.horodatage(),
                    Some(&a.utilisateur_id),
                    "distance.desactivation",
                    Some("parametres"),
                    None,
                    None,
                    None,
                )?;
            }
            Ok(config)
        })
    }

    /// Prépare le prochain lot à envoyer (aucun si rien n'attend ou si la
    /// consultation n'est pas activée). Action du système : pas de session.
    pub fn preparer_envoi(&mut self) -> Resultat<Option<EnvoiPrepare>> {
        let maintenant = horodater(self.maintenant());
        let conn = &self.conn;
        let Some((relais, hotel_id, jeton_envoi, secret)) = conn
            .query_row(
                "SELECT relais, hotel_id, jeton_envoi, secret FROM sync_config WHERE id = 1 AND remplace_le IS NULL",
                [],
                |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
                },
            )
            .optional()?
        else {
            return Ok(None);
        };
        let mut stmt =
            conn.prepare("SELECT seq, nom_table, operation, cle, donnees FROM sync_boite ORDER BY seq LIMIT ?1")?;
        let mut lignes = stmt.query([LOT_CHANGEMENTS_MAX])?;
        let (mut changements, mut octets, mut debut, mut fin) = (Vec::new(), 0usize, None, 0i64);
        while let Some(l) = lignes.next()? {
            let seq: i64 = l.get(0)?;
            let cle: String = l.get(3)?;
            let donnees: Option<String> = l.get(4)?;
            let taille = cle.len() + donnees.as_ref().map_or(0, String::len) + 32;
            if !changements.is_empty() && octets + taille > LOT_OCTETS_MAX {
                break;
            }
            octets += taille;
            debut.get_or_insert(seq);
            fin = seq;
            let operation: String = l.get(2)?;
            changements.push(Changement {
                table: l.get(1)?,
                operation: if operation == "effacer" { Operation::Effacer } else { Operation::Ecrire },
                cle: serde_json::from_str(&cle)?,
                donnees: donnees.map(|d| serde_json::from_str(&d)).transpose()?,
            });
        }
        let Some(debut) = debut else { return Ok(None) };
        let lot = Lot { genere_le: maintenant, changements };
        let clair = serde_json::to_vec(&lot)?;
        let (nonce, chiffre) = Secret::lire(&secret)?.chiffrer(&clair, &associe_lot(&hotel_id, debut, fin))?;
        Ok(Some(EnvoiPrepare {
            relais,
            hotel_id,
            jeton_envoi,
            changements: lot.changements.len(),
            lot: LotChiffre { seq_debut: debut, seq_fin: fin, nonce, chiffre },
        }))
    }

    /// Le relais a bien reçu le lot (numéro `numero`) : on vide la boîte jusque-là.
    /// `numero` absent : simple signe de vie.
    pub fn envoi_reussi(&mut self, seq_fin: i64, numero: Option<i64>) -> Resultat<()> {
        let maintenant = horodater(self.maintenant());
        self.conn.execute("DELETE FROM sync_boite WHERE seq <= ?1", [seq_fin])?;
        self.conn.execute(
            "UPDATE sync_config SET dernier_envoi_le = ?1, derniere_tentative_le = ?1, derniere_erreur = NULL,
                    dernier_numero = max(dernier_numero, coalesce(?2, 0))",
            params![maintenant, numero],
        )?;
        Ok(())
    }

    /// L'envoi a échoué. Si un autre ordinateur est devenu l'ordinateur
    /// principal, on le note : plus rien ne partira d'ici.
    pub fn envoi_refuse(&mut self, e: &Erreur) -> Resultat<()> {
        if *e == Erreur::PosteRemplace {
            let maintenant = horodater(self.maintenant());
            self.conn.execute("UPDATE sync_config SET remplace_le = coalesce(remplace_le, ?1)", [&maintenant])?;
        }
        self.envoi_echoue(&e.to_string())
    }

    /// Vrai si un autre ordinateur a pris la place de celui-ci.
    pub fn poste_remplace(&self) -> Resultat<bool> {
        Ok(self
            .conn
            .query_row("SELECT remplace_le IS NOT NULL FROM sync_config WHERE id = 1", [], |r| r.get::<_, bool>(0))
            .optional()?
            .unwrap_or(false))
    }

    /// L'envoi a échoué (pas d'Internet…) : on le note, on réessaiera.
    pub fn envoi_echoue(&mut self, message: &str) -> Resultat<()> {
        let maintenant = horodater(self.maintenant());
        self.conn.execute(
            "UPDATE sync_config SET derniere_tentative_le = ?1, derniere_erreur = ?2",
            params![maintenant, message.chars().take(300).collect::<String>()],
        )?;
        Ok(())
    }

    /// Envoie tout ce qui attend (boucle préparer → envoyer → confirmer).
    /// L'application fait la même boucle en libérant la base pendant l'envoi.
    pub fn synchroniser(&mut self, relais: &dyn Relais) -> Resultat<BilanEnvoi> {
        let mut bilan = BilanEnvoi::default();
        let liaison = self.liaison_distance()?;
        while let Some(envoi) = self.preparer_envoi()? {
            match relais.envoyer(&envoi.hotel_id, &envoi.jeton_envoi, &envoi.lot) {
                Ok(accuse) => {
                    self.envoi_reussi(envoi.lot.seq_fin, Some(accuse.numero))?;
                    bilan.lots += 1;
                    bilan.changements += envoi.changements as i64;
                }
                Err(e) => {
                    self.envoi_refuse(&e)?;
                    return Err(e);
                }
            }
        }
        // Rien de neuf : on signale tout de même que l'hôtel est en ligne.
        if bilan.lots == 0
            && let Some((_, hotel_id, jeton)) = liaison
        {
            match relais.signaler_presence(&hotel_id, &jeton) {
                Ok(()) => self.envoi_reussi(0, None)?,
                Err(e) => {
                    self.envoi_refuse(&e)?;
                    return Err(e);
                }
            }
        }
        Ok(bilan)
    }

    /// (adresse du relais, hotel_id, jeton d'envoi) si la consultation à distance est
    /// activée et que cet ordinateur est toujours l'ordinateur principal.
    pub fn liaison_distance(&self) -> Resultat<Option<(String, String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT relais, hotel_id, jeton_envoi FROM sync_config WHERE id = 1 AND remplace_le IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }
}

fn etat_distance(conn: &Connection) -> Resultat<EtatDistance> {
    let en_attente: i64 = conn.query_row("SELECT count(*) FROM sync_boite", [], |r| r.get(0))?;
    let config = conn
        .query_row(
            "SELECT relais, hotel_id, active_le, dernier_envoi_le, derniere_tentative_le, derniere_erreur
             FROM sync_config WHERE id = 1",
            [],
            |r| {
                Ok(EtatDistance {
                    active: true,
                    relais: r.get(0)?,
                    hotel_id: r.get(1)?,
                    en_attente,
                    active_le: r.get(2)?,
                    dernier_envoi_le: r.get(3)?,
                    derniere_tentative_le: r.get(4)?,
                    derniere_erreur: r.get(5)?,
                })
            },
        )
        .optional()?;
    Ok(config.unwrap_or(EtatDistance {
        active: false,
        relais: None,
        hotel_id: None,
        en_attente: 0,
        active_le: None,
        dernier_envoi_le: None,
        derniere_tentative_le: None,
        derniere_erreur: None,
    }))
}

// ───────────────────────────── Chez l'administrateur ─────────────────────────────

/// Identifiant d'hôtel recopié à la main : « hk7q f29x », « HK7Q-F29X »…
/// Comme pour la clé, 0, 1 et 8 tapés par erreur sont compris comme O, I et B
/// (le relais n'utilise jamais ces chiffres).
pub fn normaliser_identifiant_hotel(saisie: &str) -> String {
    let brut: String = saisie
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c.to_ascii_uppercase() {
            '0' => 'O',
            '1' => 'I',
            '8' => 'B',
            c => c,
        })
        .collect();
    if brut.len() == 8 { format!("{}-{}", &brut[..4], &brut[4..]) } else { brut }
}

/// Vérifie, avant de créer la copie, que la clé secrète correspond à cet hôtel.
pub fn verifier_cle(relais: &dyn Relais, hotel_id: &str, secret: &Secret) -> Resultat<()> {
    let temoin = relais.temoin(hotel_id, &secret.jeton_lecture()).map_err(|e| match e {
        // Jeton de lecture refusé = clé secrète qui ne correspond pas
        Erreur::RelaisRefuse(m) if m.starts_with("accès refusé") => Erreur::CleDistanceIncorrecte,
        e => e,
    })?;
    let clair = secret.dechiffrer(&temoin.nonce, &temoin.chiffre, &associe_temoin("inscription"))?;
    if clair != TEXTE_TEMOIN {
        return Err(Erreur::CleDistanceIncorrecte);
    }
    Ok(())
}

const SCHEMA_COPIE: &str = "CREATE TABLE IF NOT EXISTS copie_distance (
    id              INTEGER PRIMARY KEY CHECK (id = 1),
    relais          TEXT NOT NULL,
    hotel_id        TEXT NOT NULL,
    secret          TEXT NOT NULL,
    acces_hash      TEXT,
    identifiant     TEXT,
    dernier_numero  INTEGER NOT NULL DEFAULT 0,
    donnees_du      TEXT,
    dernier_contact TEXT,
    mise_a_jour_le  TEXT
) STRICT;";

/// Prépare une base fraîchement migrée pour servir de copie : plus aucun
/// trigger (la cohérence vient de l'hôtel), clés étrangères non imposées
/// (les lignes arrivent dans l'ordre de l'hôtel), puis lecture seule.
fn preparer_base_copie(conn: &Connection) -> Resultat<()> {
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    conn.execute_batch(SCHEMA_COPIE)?;
    let triggers: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'trigger'")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for t in triggers {
        conn.execute_batch(&format!("DROP TRIGGER IF EXISTS \"{t}\";"))?;
    }
    // Les données de démarrage (rôles, permissions) viendront de l'hôtel.
    conn.execute_batch(
        "DELETE FROM role_permissions; DELETE FROM permissions; DELETE FROM roles; DELETE FROM parametres;",
    )?;
    Ok(())
}

impl Tanana {
    /// Crée la copie locale d'un hôtel (clé secrète déjà vérifiée par [`verifier_cle`]).
    pub fn creer_copie(
        chemin: &Path,
        cle_base: Option<CleBase>,
        adresse_relais: &str,
        hotel_id: &str,
        secret: &Secret,
    ) -> Resultat<Self> {
        let conn = db::ouvrir(chemin, cle_base.as_ref())?;
        Self::initialiser_copie(&conn, adresse_relais, hotel_id, secret)?;
        Self::ouvrir_copie(chemin, cle_base)
    }

    fn initialiser_copie(conn: &Connection, adresse: &str, hotel_id: &str, secret: &Secret) -> Resultat<()> {
        preparer_base_copie(conn)?;
        conn.execute(
            "INSERT INTO copie_distance (id, relais, hotel_id, secret) VALUES (1, ?1, ?2, ?3)",
            params![adresse.trim().trim_end_matches('/'), hotel_id.trim(), secret.afficher()],
        )?;
        Ok(())
    }

    /// Rouvre une copie existante, en lecture seule.
    pub fn ouvrir_copie(chemin: &Path, cle_base: Option<CleBase>) -> Resultat<Self> {
        let conn = db::ouvrir(chemin, cle_base.as_ref())?;
        // Une mise à jour de Tanana a pu recréer des triggers : on les retire.
        let existe: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'copie_distance'",
            [],
            |r| r.get(0),
        )?;
        if existe == 0 {
            return Err(Erreur::Validation("ce fichier n'est pas une copie consultée à distance".into()));
        }
        // Copie créée par une version d'avant la connexion par identifiant
        let a_identifiant: i64 = conn.query_row(
            "SELECT count(*) FROM pragma_table_info('copie_distance') WHERE name = 'identifiant'",
            [],
            |r| r.get(0),
        )?;
        if a_identifiant == 0 {
            conn.execute_batch("ALTER TABLE copie_distance ADD COLUMN identifiant TEXT;")?;
        }
        let triggers: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'trigger'")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        for t in triggers {
            conn.execute_batch(&format!("DROP TRIGGER IF EXISTS \"{t}\";"))?;
        }
        conn.execute_batch("PRAGMA foreign_keys = OFF; PRAGMA query_only = ON;")?;
        Ok(Self { conn, horloge: Arc::new(Utc::now), cle: cle_base, copie: true, session_copie: Default::default() })
    }

    /// Copie en mémoire (tests).
    #[doc(hidden)]
    pub fn copie_en_memoire(adresse: &str, hotel_id: &str, secret: &Secret) -> Resultat<Self> {
        let conn = db::ouvrir_en_memoire()?;
        Self::initialiser_copie(&conn, adresse, hotel_id, secret)?;
        conn.execute_batch("PRAGMA query_only = ON;")?;
        Ok(Self { conn, horloge: Arc::new(Utc::now), cle: None, copie: true, session_copie: Default::default() })
    }

    /// Choisit le mot de passe qui protège la copie sur cet ordinateur.
    pub fn definir_acces_copie(&mut self, mot_de_passe: &str) -> Resultat<()> {
        self.exiger_copie()?;
        crate::domaine::valider_mot_de_passe(mot_de_passe)?;
        let hash = crate::auth::hacher(mot_de_passe)?;
        self.conn.execute_batch("PRAGMA query_only = OFF;")?;
        let r = self.conn.execute("UPDATE copie_distance SET acces_hash = ?1 WHERE id = 1", [&hash]);
        self.conn.execute_batch("PRAGMA query_only = ON;")?;
        r?;
        Ok(())
    }

    /// Ouvre la session de consultation avec le mot de passe de la copie.
    /// Cinq essais ratés bloquent pendant 15 minutes.
    pub fn connexion_copie(&mut self, mot_de_passe: &str) -> Resultat<SessionOuverte> {
        self.exiger_copie()?;
        let maintenant = self.maintenant();
        if let Some(jusqu_a) = self.session_copie.bloque_jusqu_a
            && jusqu_a > maintenant
        {
            return Err(Erreur::CompteBloque { jusqu_a: horodater(jusqu_a) });
        }
        let hash: Option<String> =
            self.conn.query_row("SELECT acces_hash FROM copie_distance WHERE id = 1", [], |r| r.get(0))?;
        let ok = hash.as_deref().is_some_and(|h| crate::auth::verifier_hash(mot_de_passe, h));
        if !ok {
            self.session_copie.echecs += 1;
            if self.session_copie.echecs >= ECHECS_MAX_COPIE {
                self.session_copie.echecs = 0;
                let jusqu_a = maintenant + chrono::Duration::minutes(BLOCAGE_COPIE_MINUTES);
                self.session_copie.bloque_jusqu_a = Some(jusqu_a);
                return Err(Erreur::CompteBloque { jusqu_a: horodater(jusqu_a) });
            }
            return Err(Erreur::IdentifiantsInvalides);
        }
        let mut octets = [0u8; 32];
        getrandom::fill(&mut octets).map_err(|e| Erreur::Base(format!("hasard indisponible : {e}")))?;
        let jeton = hex::encode(octets);
        self.session_copie = SessionCopie {
            jeton: Some(jeton.clone()),
            derniere_activite: Some(maintenant),
            echecs: 0,
            bloque_jusqu_a: None,
        };
        Ok(self.session_ouverte_copie(jeton))
    }

    fn session_ouverte_copie(&self, token: String) -> SessionOuverte {
        let mut permissions = PERMISSIONS_DISTANTES.to_vec();
        permissions.sort_by_key(|p| p.code());
        SessionOuverte { token, utilisateur: utilisateur_distant(), permissions }
    }

    /// Délai d'inactivité de la consultation à distance. Celui de la réception
    /// (souvent 15 minutes, poste partagé) ne convient pas au propriétaire, seul
    /// devant son propre ordinateur et en lecture seule.
    fn delai_copie(&self) -> chrono::Duration {
        chrono::Duration::minutes(DELAI_COPIE_MINUTES)
    }

    pub(crate) fn verifier_session_copie(&mut self, token: &str, maintenant: DateTime<Utc>) -> Resultat<()> {
        let delai = self.delai_copie();
        let s = &mut self.session_copie;
        match (&s.jeton, s.derniere_activite) {
            (Some(j), Some(derniere)) if crate::auth::egaux_temps_constant(j, token) => {
                if maintenant - derniere > delai {
                    s.jeton = None;
                    return Err(Erreur::SessionExpiree);
                }
                s.derniere_activite = Some(maintenant);
                Ok(())
            }
            _ => Err(Erreur::NonAuthentifie),
        }
    }

    /// Session de consultation en cours (rafraîchit l'activité).
    pub(crate) fn ma_session_copie(&mut self, token: &str) -> Resultat<SessionOuverte> {
        let maintenant = self.maintenant();
        self.verifier_session_copie(token, maintenant)?;
        Ok(self.session_ouverte_copie(token.to_string()))
    }

    pub(crate) fn deconnexion_copie(&mut self) {
        self.session_copie.jeton = None;
    }

    /// Identifiant de l'administratrice qui consulte (pour se connecter à l'hôtel).
    pub fn definir_identifiant_copie(&mut self, identifiant: &str) -> Resultat<()> {
        self.exiger_copie()?;
        self.conn.execute_batch("PRAGMA query_only = OFF;")?;
        let r = self.conn.execute("UPDATE copie_distance SET identifiant = ?1 WHERE id = 1", [identifiant.trim()]);
        self.conn.execute_batch("PRAGMA query_only = ON;")?;
        r?;
        Ok(())
    }

    pub fn identifiant_copie(&self) -> Resultat<Option<String>> {
        self.exiger_copie()?;
        Ok(self.conn.query_row("SELECT identifiant FROM copie_distance WHERE id = 1", [], |r| r.get(0))?)
    }

    /// Vrai si le mot de passe de consultation a déjà été choisi.
    pub fn acces_copie_defini(&self) -> Resultat<bool> {
        self.exiger_copie()?;
        Ok(self.conn.query_row("SELECT acces_hash IS NOT NULL FROM copie_distance WHERE id = 1", [], |r| r.get(0))?)
    }

    fn exiger_copie(&self) -> Resultat<()> {
        if !self.copie {
            return Err(Erreur::Validation("réservé à une copie consultée à distance".into()));
        }
        Ok(())
    }

    /// 1/2 - Ce qu'il faut demander au relais.
    pub fn preparer_lecture_copie(&self) -> Resultat<LecturePreparee> {
        self.exiger_copie()?;
        let (relais, hotel_id, secret, apres) = self.conn.query_row(
            "SELECT relais, hotel_id, secret, dernier_numero FROM copie_distance WHERE id = 1",
            [],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)),
        )?;
        Ok(LecturePreparee { relais, hotel_id, jeton_lecture: Secret::lire(&secret)?.jeton_lecture(), apres })
    }

    /// 2/2 - Déchiffre et applique une page de lots, dans l'ordre. Tout ou rien :
    /// un lot illisible annule la page entière, la copie reste cohérente.
    pub fn appliquer_page_copie(&mut self, page: &PageLots) -> Resultat<usize> {
        self.exiger_copie()?;
        let maintenant = horodater(self.maintenant());
        self.conn.execute_batch("PRAGMA query_only = OFF;")?;
        let resultat = (|| -> Resultat<usize> {
            let tx = self.conn.transaction()?;
            let (hotel_id, secret, mut dernier): (String, String, i64) =
                tx.query_row("SELECT hotel_id, secret, dernier_numero FROM copie_distance WHERE id = 1", [], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?))
                })?;
            let secret = Secret::lire(&secret)?;
            let mut appliques = 0;
            let mut genere_le = None;
            for recu in &page.lots {
                if recu.numero <= dernier {
                    continue; // déjà appliqué
                }
                let l = &recu.lot;
                let clair = secret.dechiffrer(&l.nonce, &l.chiffre, &associe_lot(&hotel_id, l.seq_debut, l.seq_fin))?;
                let lot: Lot = serde_json::from_slice(&clair)?;
                appliquer_changements(&tx, &lot.changements, false)?;
                dernier = recu.numero;
                genere_le = Some(lot.genere_le);
                appliques += 1;
            }
            tx.execute(
                "UPDATE copie_distance SET dernier_numero = ?1, donnees_du = coalesce(?2, donnees_du),
                        dernier_contact = coalesce(?3, dernier_contact), mise_a_jour_le = ?4",
                params![dernier, genere_le, page.dernier_contact, maintenant],
            )?;
            tx.commit()?;
            Ok(appliques)
        })();
        self.conn.execute_batch("PRAGMA query_only = ON;")?;
        resultat
    }

    /// Met la copie à jour (boucle lire → appliquer) : pratique en test ;
    /// l'application libère la base pendant le téléchargement.
    pub fn mettre_a_jour_copie(&mut self, relais: &dyn Relais) -> Resultat<usize> {
        let mut total = 0;
        loop {
            let p = self.preparer_lecture_copie()?;
            let page = relais.lire(&p.hotel_id, &p.jeton_lecture, p.apres)?;
            total += self.appliquer_page_copie(&page)?;
            if !page.suite {
                return Ok(total);
            }
        }
    }

    pub fn etat_copie(&self) -> Resultat<EtatCopie> {
        self.exiger_copie()?;
        let nom_hotel: Option<String> = self
            .conn
            .query_row("SELECT valeur FROM parametres WHERE cle = 'nom_hotel'", [], |r| r.get(0))
            .optional()?;
        Ok(self.conn.query_row(
            "SELECT relais, hotel_id, donnees_du, dernier_contact, mise_a_jour_le FROM copie_distance WHERE id = 1",
            [],
            |r| {
                Ok(EtatCopie {
                    relais: r.get(0)?,
                    hotel_id: r.get(1)?,
                    nom_hotel,
                    donnees_du: r.get(2)?,
                    dernier_contact: r.get(3)?,
                    mise_a_jour_le: r.get(4)?,
                })
            },
        )?)
    }
}

/// Valeur mise à la place d'un secret jamais reçu (mot de passe, PIN) quand
/// la ligne n'existait pas : aucun mot de passe ne peut y correspondre.
const SECRET_INCONNU: &str = "!distance";

/// Applique des changements sur la copie (ou, avec `preserver_secrets`, sur la
/// base d'un ordinateur qui reprend l'hôtel : les secrets reçus comme « * »
/// gardent leur valeur locale). Une table ou une colonne inconnue (hôtel
/// équipé d'une version plus récente de Tanana) est ignorée.
pub(crate) fn appliquer_changements(
    conn: &Connection,
    changements: &[Changement],
    preserver_secrets: bool,
) -> Resultat<()> {
    let mut structure: std::collections::HashMap<String, (Vec<String>, Vec<String>)> = Default::default();
    for c in changements {
        if !capture::TABLES_ENVOYEES.contains(&c.table.as_str()) {
            continue;
        }
        if !structure.contains_key(&c.table) {
            // Colonnes réelles de la copie.
            let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", c.table))?;
            let infos: Vec<(String, i64)> =
                stmt.query_map([], |r| Ok((r.get(1)?, r.get(5)?)))?.collect::<Result<_, _>>()?;
            let mut cles: Vec<(String, i64)> = infos.iter().filter(|(_, pk)| *pk > 0).cloned().collect();
            cles.sort_by_key(|(_, pk)| *pk);
            structure.insert(
                c.table.clone(),
                (infos.into_iter().map(|(n, _)| n).collect(), cles.into_iter().map(|(n, _)| n).collect()),
            );
        }
        let (colonnes, cles) = &structure[&c.table];
        if c.cle.len() != cles.len() {
            continue;
        }
        match (c.operation, &c.donnees) {
            (Operation::Ecrire, Some(d)) => {
                let mut noms = Vec::new();
                let mut valeurs = Vec::new();
                let mut gardes = Vec::new();
                for col in colonnes {
                    if let Some(v) = d.get(col) {
                        let secret = capture::COLONNES_SECRETES.contains(&(c.table.as_str(), col.as_str()));
                        if preserver_secrets && secret && v.as_str() == Some("*") {
                            noms.push(col.clone());
                            valeurs.push(Sql::Text(SECRET_INCONNU.into()));
                            gardes.push(col.clone());
                            continue;
                        }
                        noms.push(col.clone());
                        valeurs.push(vers_sql(v));
                    }
                }
                let places = vec!["?"; noms.len()].join(", ");
                let maj: Vec<String> = noms
                    .iter()
                    .filter(|n| !cles.contains(n) && !gardes.contains(n))
                    .map(|n| format!("{n} = excluded.{n}"))
                    .collect();
                let sur_conflit =
                    if maj.is_empty() { "DO NOTHING".to_string() } else { format!("DO UPDATE SET {}", maj.join(", ")) };
                conn.execute(
                    &format!(
                        "INSERT INTO {} ({}) VALUES ({places}) ON CONFLICT ({}) {sur_conflit}",
                        c.table,
                        noms.join(", "),
                        cles.join(", ")
                    ),
                    params_from_iter(valeurs),
                )?;
            }
            (Operation::Effacer, _) => {
                let condition: Vec<String> = cles.iter().map(|k| format!("{k} = ?")).collect();
                conn.execute(
                    &format!("DELETE FROM {} WHERE {}", c.table, condition.join(" AND ")),
                    params_from_iter(c.cle.iter().map(vers_sql)),
                )?;
            }
            (Operation::Ecrire, None) => {}
        }
    }
    Ok(())
}
