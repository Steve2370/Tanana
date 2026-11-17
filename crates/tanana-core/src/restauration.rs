//! Remettre une copie de sécurité.
//!
//! Deux cas : l'administrateur revient en arrière sur le même ordinateur, ou
//! un ordinateur neuf remplace celui qui est tombé en panne. Dans les deux cas
//! le fichier choisi n'est **jamais** ouvert directement : on travaille sur une
//! copie, vérifiée de fond en comble (clé, intégrité, version, journal), et
//! l'application ne bascule dessus qu'une fois tout validé. L'échange des
//! fichiers lui-même est fait par l'application (qui tient la base ouverte).

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::db::{self, CleBase};
use crate::erreur::{Erreur, Resultat};
use crate::{Tanana, audit, distance};

/// Ce que contient une copie de sécurité, montré avant de la remettre.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export)]
pub struct ApercuSauvegarde {
    pub nom_hotel: String,
    pub devise: String,
    /// Dernière action notée dans l'historique (RFC 3339) : « l'état de l'hôtel à ce moment-là »
    pub derniere_activite: Option<String>,
    pub reservations: i64,
    pub clients: i64,
    pub comptes: i64,
    /// Faux si l'historique a été modifié en dehors de Tanana (la copie reste utilisable).
    pub journal_intact: bool,
}

/// Résultat de la préparation : la copie de travail est prête à prendre la place de la base.
#[derive(Debug)]
pub struct RestaurationPreparee {
    pub apercu: ApercuSauvegarde,
    pub fichier: PathBuf,
    /// La copie avait la consultation à distance active : elle a été arrêtée
    /// dans la copie (revenir en arrière désynchroniserait le propriétaire).
    /// (relais, hotel_id, jeton_envoi) pour demander au relais d'effacer.
    pub liaison_distance: Option<(String, String, String)>,
}

fn effacer(chemin: &Path) {
    for suffixe in ["", "-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(format!("{}{suffixe}", chemin.display()));
    }
}

fn illisible() -> Erreur {
    Erreur::Validation(
        "cette copie ne s'ouvre pas avec cette clé, ou ce n'est pas une copie de sécurité de Tanana".into(),
    )
}

/// Copie `source` vers `travail`, l'ouvre avec `cle`, la vérifie et la prépare
/// (sessions fermées, consultation à distance arrêtée). Si quoi que ce soit
/// cloche, `travail` est effacé et rien d'autre n'a été touché.
pub fn preparer(source: &Path, travail: &Path, cle: Option<&CleBase>) -> Resultat<RestaurationPreparee> {
    if !source.is_file() {
        return Err(Erreur::Validation("le fichier choisi est introuvable".into()));
    }
    effacer(travail);
    let resultat = preparer_sans_nettoyage(source, travail, cle);
    if resultat.is_err() {
        effacer(travail);
    }
    resultat
}

fn preparer_sans_nettoyage(source: &Path, travail: &Path, cle: Option<&CleBase>) -> Resultat<RestaurationPreparee> {
    std::fs::copy(source, travail).map_err(|e| Erreur::Base(format!("copie du fichier choisi : {e}")))?;
    // Une base copiée « à chaud » peut avoir son journal à côté
    let wal = PathBuf::from(format!("{}-wal", source.display()));
    if wal.is_file() {
        std::fs::copy(&wal, format!("{}-wal", travail.display()))
            .map_err(|e| Erreur::Base(format!("copie du fichier choisi : {e}")))?;
    }

    {
        // 1. La clé ouvre-t-elle le fichier ? Vient-il d'une version plus récente ?
        let conn = Connection::open(travail)?;
        db::appliquer_cle(&conn, cle)?;
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).map_err(|_| illisible())?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > db::VERSION_SCHEMA {
            return Err(Erreur::Validation(
                "cette copie vient d'une version plus récente de Tanana : mettez d'abord Tanana à jour sur cet ordinateur"
                    .into(),
            ));
        }
        if version < 1 {
            return Err(illisible());
        }
        let integrite: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrite != "ok" {
            return Err(Erreur::Validation("cette copie est abîmée : choisissez-en une autre (la précédente)".into()));
        }
        let est_copie_distance: bool = conn
            .query_row("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'copie_distance'", [], |_| Ok(true))
            .optional()?
            .unwrap_or(false);
        if est_copie_distance {
            return Err(Erreur::Validation(
                "ce fichier vient d'un poste de consultation à distance, pas de l'ordinateur de l'hôtel".into(),
            ));
        }
    }

    // 2. Mise à niveau (une copie plus ancienne reçoit les nouveautés), puis contenu
    let conn = db::ouvrir(travail, cle).map_err(|_| illisible())?;
    let nom_hotel: Option<String> =
        conn.query_row("SELECT valeur FROM parametres WHERE cle = 'nom_hotel'", [], |r| r.get(0)).optional()?;
    let comptes: i64 = conn.query_row("SELECT count(*) FROM utilisateurs", [], |r| r.get(0))?;
    let Some(nom_hotel) = nom_hotel.filter(|_| comptes > 0) else {
        return Err(Erreur::Validation("cette copie est vide : l'hôtel n'y avait pas encore été configuré".into()));
    };
    let devise: String = conn
        .query_row("SELECT valeur FROM parametres WHERE cle = 'devise'", [], |r| r.get(0))
        .optional()?
        .unwrap_or_default();
    let apercu = ApercuSauvegarde {
        nom_hotel,
        devise,
        derniere_activite: conn.query_row("SELECT max(date) FROM journal_audit", [], |r| r.get(0))?,
        reservations: conn.query_row("SELECT count(*) FROM reservations", [], |r| r.get(0))?,
        clients: conn.query_row("SELECT count(*) FROM clients", [], |r| r.get(0))?,
        comptes,
        journal_intact: audit::verifier_chaine(&conn)?.intacte,
    };

    // 3. Plus personne n'est connecté ; la consultation à distance repartira de zéro
    let liaison_distance = conn
        .query_row("SELECT relais, hotel_id, jeton_envoi FROM sync_config WHERE id = 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?;
    conn.execute_batch("BEGIN")?;
    conn.execute("DELETE FROM sessions", [])?;
    distance::capture::retirer(&conn)?;
    conn.execute("DELETE FROM sync_boite", [])?;
    conn.execute("DELETE FROM sync_config", [])?;
    conn.execute_batch("COMMIT")?;
    // Tout dans le fichier principal, pour qu'il puisse être déplacé seul
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
    drop(conn);
    for suffixe in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffixe}", travail.display()));
    }

    Ok(RestaurationPreparee { apercu, fichier: travail.to_path_buf(), liaison_distance })
}

impl Tanana {
    /// Avant de remettre une copie : l'administrateur retape son mot de passe.
    /// Renvoie son identifiant (pour l'historique de la base remise).
    pub fn autoriser_restauration(&mut self, token: &str, mot_de_passe: &str) -> Resultat<String> {
        self.executer(token, Some(crate::permissions::Permission::ParametresGerer), |tx, a| {
            let hash: String =
                tx.query_row("SELECT mot_de_passe_hash FROM utilisateurs WHERE id = ?1", [&a.utilisateur_id], |r| {
                    r.get(0)
                })?;
            if !crate::auth::verifier_hash(mot_de_passe, &hash) {
                return Err(Erreur::IdentifiantsInvalides);
            }
            Ok(a.utilisateur_id.clone())
        })
    }

    /// Met l'état actuel de côté avant de le remplacer. Action du système.
    pub fn mettre_de_cote(&self, destination: &Path) -> Resultat<()> {
        db::sauvegarder(self.conn_brute(), destination, self.cle.as_ref())
    }

    /// Consultation à distance active sur la base ouverte : (relais, hotel_id, jeton_envoi).
    pub fn liaison_distance_brute(&self) -> Resultat<Option<(String, String, String)>> {
        Ok(self
            .conn_brute()
            .query_row("SELECT relais, hotel_id, jeton_envoi FROM sync_config WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?)
    }

    /// Note la restauration dans l'historique de la base remise.
    /// `auteur` : l'administrateur, s'il existe aussi dans la copie remise.
    pub fn noter_restauration(&mut self, auteur: Option<&str>, fichier: &str) -> Resultat<()> {
        let date = crate::horodater(self.maintenant());
        let tx = self.transaction()?;
        let auteur = match auteur {
            Some(id) => {
                tx.query_row("SELECT id FROM utilisateurs WHERE id = ?1", [id], |r| r.get::<_, String>(0)).optional()?
            }
            None => None,
        };
        audit::ecrire(
            &tx,
            &date,
            auteur.as_deref(),
            "sauvegarde.restauration",
            None,
            None,
            None,
            Some(&serde_json::json!({ "fichier": fichier })),
        )?;
        tx.commit()?;
        Ok(())
    }
}
