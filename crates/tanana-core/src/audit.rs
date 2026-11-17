//! Journal d'audit chaîné : qui a fait quoi, quand, avec l'avant/après.
//!
//! Chaque entrée stocke `hash = SHA-256(contenu + hash_precedent)`. Modifier ou
//! supprimer une ligne au milieu casse la chaîne ; [`verifier_chaine`] le détecte.
//! Limite connue : supprimer les toutes dernières entrées ne casse pas la chaîne
//! (rien ne les suit). La base chiffrée et les triggers rendent cela difficile.

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::erreur::{Erreur, Resultat};
use crate::modeles::{EntreeJournal, FiltreJournal, RapportIntegrite};
use crate::permissions::Permission;
use crate::{Tanana, nouvel_id};

pub const HASH_ORIGINE: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Actions mises en évidence sur le tableau de bord admin.
pub fn est_sensible(action: &str) -> bool {
    const SENSIBLES: [&str; 14] = [
        "reservation.annulation",
        "reservation.taxes",
        "taxes.",
        "depense.annulation",
        "reservation.modification",
        "paiement.remboursement",
        "utilisateur.",
        "role.",
        "auth.echec",
        "auth.blocage",
        "auth.recuperation",
        "acces.refuse",
        "parametres.",
        "sauvegarde.restauration",
    ];
    SENSIBLES.iter().any(|s| action.starts_with(s))
}

#[allow(clippy::too_many_arguments)]
fn calculer_hash(
    id: &str,
    date: &str,
    utilisateur_id: Option<&str>,
    action: &str,
    entite: Option<&str>,
    entite_id: Option<&str>,
    avant: Option<&str>,
    apres: Option<&str>,
    hash_precedent: &str,
) -> String {
    // \u{1f} sépare les champs, \u{0} marque une valeur absente : aucune
    // ambiguïté possible entre deux contenus différents.
    let champ = |v: Option<&str>| v.map(str::to_string).unwrap_or_else(|| "\u{0}".into());
    let contenu = [
        id.to_string(),
        date.to_string(),
        champ(utilisateur_id),
        action.to_string(),
        champ(entite),
        champ(entite_id),
        champ(avant),
        champ(apres),
        hash_precedent.to_string(),
    ]
    .join("\u{1f}");
    hex::encode(Sha256::digest(contenu.as_bytes()))
}

/// Ajoute une entrée. Doit être appelée dans la même transaction que l'action.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ecrire(
    conn: &Connection,
    date: &str,
    utilisateur_id: Option<&str>,
    action: &str,
    entite: Option<&str>,
    entite_id: Option<&str>,
    avant: Option<&Value>,
    apres: Option<&Value>,
) -> Resultat<()> {
    let precedent: String = conn
        .query_row("SELECT hash FROM journal_audit ORDER BY seq DESC LIMIT 1", [], |r| r.get(0))
        .optional()?
        .unwrap_or_else(|| HASH_ORIGINE.to_string());
    let id = nouvel_id();
    let avant = avant.map(serde_json::to_string).transpose()?;
    let apres = apres.map(serde_json::to_string).transpose()?;
    let hash = calculer_hash(
        &id,
        date,
        utilisateur_id,
        action,
        entite,
        entite_id,
        avant.as_deref(),
        apres.as_deref(),
        &precedent,
    );
    conn.execute(
        "INSERT INTO journal_audit
           (id, date, utilisateur_id, action, entite, entite_id, avant_json, apres_json, hash_precedent, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![id, date, utilisateur_id, action, entite, entite_id, avant, apres, precedent, hash],
    )?;
    Ok(())
}

/// Recalcule toute la chaîne depuis le début.
pub fn verifier_chaine(conn: &Connection) -> Resultat<RapportIntegrite> {
    let mut stmt = conn.prepare(
        "SELECT seq, id, date, utilisateur_id, action, entite, entite_id,
                avant_json, apres_json, hash_precedent, hash
         FROM journal_audit ORDER BY seq",
    )?;
    let mut lignes = stmt.query([])?;
    let mut attendu = HASH_ORIGINE.to_string();
    let mut n = 0i64;
    while let Some(r) = lignes.next()? {
        let seq: i64 = r.get(0)?;
        let precedent: String = r.get(9)?;
        let hash: String = r.get(10)?;
        let recalcule = calculer_hash(
            &r.get::<_, String>(1)?,
            &r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?.as_deref(),
            &r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?.as_deref(),
            r.get::<_, Option<String>>(6)?.as_deref(),
            r.get::<_, Option<String>>(7)?.as_deref(),
            r.get::<_, Option<String>>(8)?.as_deref(),
            &precedent,
        );
        if precedent != attendu || recalcule != hash {
            return Ok(RapportIntegrite { intacte: false, entrees_verifiees: n, premiere_rupture: Some(seq) });
        }
        attendu = hash;
        n += 1;
    }
    Ok(RapportIntegrite { intacte: true, entrees_verifiees: n, premiere_rupture: None })
}

fn parse_json(v: Option<String>) -> Option<Value> {
    v.and_then(|s| serde_json::from_str(&s).ok())
}

impl Tanana {
    /// Dernières actions, de la plus récente à la plus ancienne.
    pub fn journal(&mut self, token: &str, filtre: FiltreJournal) -> Resultat<Vec<EntreeJournal>> {
        self.executer(token, Some(Permission::JournalVoir), |tx, _| {
            let limite = filtre.limite.unwrap_or(100).clamp(1, 1000) as i64;
            // Les actions sensibles sont filtrées en Rust (liste de préfixes),
            // donc on lit plus large puis on tronque.
            let lecture = if filtre.sensibles_seulement { limite * 20 } else { limite };
            let prefixe = filtre.action_prefixe.as_ref().map(|p| format!("{}%", p.replace('%', "")));
            let mut stmt = tx.prepare(
                "SELECT j.seq, j.date, j.utilisateur_id, u.nom, j.action, j.entite, j.entite_id,
                        j.avant_json, j.apres_json
                 FROM journal_audit j LEFT JOIN utilisateurs u ON u.id = j.utilisateur_id
                 WHERE (?1 IS NULL OR j.utilisateur_id = ?1)
                   AND (?2 IS NULL OR j.action LIKE ?2)
                   AND (?3 IS NULL OR j.seq < ?3)
                 ORDER BY j.seq DESC LIMIT ?4",
            )?;
            let entrees = stmt
                .query_map(params![filtre.utilisateur_id, prefixe, filtre.avant_seq, lecture], |r| {
                    let action: String = r.get(4)?;
                    Ok(EntreeJournal {
                        seq: r.get(0)?,
                        date: r.get(1)?,
                        utilisateur_id: r.get(2)?,
                        utilisateur_nom: r.get(3)?,
                        sensible: est_sensible(&action),
                        action,
                        entite: r.get(5)?,
                        entite_id: r.get(6)?,
                        avant: parse_json(r.get(7)?),
                        apres: parse_json(r.get(8)?),
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(entrees
                .into_iter()
                .filter(|e| !filtre.sensibles_seulement || e.sensible)
                .take(limite as usize)
                .collect())
        })
    }

    pub fn verifier_integrite_journal(&mut self, token: &str) -> Resultat<RapportIntegrite> {
        self.executer(token, Some(Permission::JournalVoir), |tx, _| verifier_chaine(tx))
    }
}

/// Convertit une valeur en JSON pour l'avant/après du journal.
pub(crate) fn json<T: serde::Serialize>(v: &T) -> Resultat<Value> {
    serde_json::to_value(v).map_err(Erreur::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_deterministe_et_sensible_au_contenu() {
        let h1 = calculer_hash("i", "d", Some("u"), "a", None, None, None, Some("{}"), HASH_ORIGINE);
        let h2 = calculer_hash("i", "d", Some("u"), "a", None, None, None, Some("{}"), HASH_ORIGINE);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        // None et chaîne vide ne donnent pas le même hash
        let h3 = calculer_hash("i", "d", Some("u"), "a", Some(""), None, None, Some("{}"), HASH_ORIGINE);
        assert_ne!(h1, h3);
        // un champ décalé ne donne pas le même hash
        let h4 = calculer_hash("i", "d", Some("u"), "a", None, Some(""), None, Some("{}"), HASH_ORIGINE);
        assert_ne!(h3, h4);
    }

    #[test]
    fn actions_sensibles() {
        assert!(est_sensible("paiement.remboursement"));
        assert!(est_sensible("depense.annulation"));
        assert!(!est_sensible("depense.saisie"));
        assert!(est_sensible("utilisateur.suspension"));
        assert!(est_sensible("auth.echec"));
        assert!(!est_sensible("auth.connexion"));
        assert!(!est_sensible("paiement.encaissement"));
        assert!(!est_sensible("reservation.creation"));
    }

    #[test]
    fn chaine_vide_est_intacte() {
        let conn = crate::db::ouvrir_en_memoire().unwrap();
        let r = verifier_chaine(&conn).unwrap();
        assert!(r.intacte);
        assert_eq!(r.entrees_verifiees, 0);
    }

    #[test]
    fn detecte_modification_et_suppression() {
        let conn = crate::db::ouvrir_en_memoire().unwrap();
        for i in 0..5 {
            ecrire(&conn, "2026-01-01T00:00:00.000Z", None, "test", None, Some(&i.to_string()), None, None).unwrap();
        }
        assert!(verifier_chaine(&conn).unwrap().intacte);

        // Les triggers bloquent la modification…
        assert!(conn.execute("UPDATE journal_audit SET action = 'x' WHERE seq = 3", []).is_err());
        assert!(conn.execute("DELETE FROM journal_audit WHERE seq = 3", []).is_err());

        // …mais un attaquant qui les supprime est quand même détecté.
        conn.execute_batch("DROP TRIGGER journal_pas_de_modification; DROP TRIGGER journal_pas_de_suppression;")
            .unwrap();
        conn.execute("UPDATE journal_audit SET action = 'falsifie' WHERE seq = 3", []).unwrap();
        let r = verifier_chaine(&conn).unwrap();
        assert!(!r.intacte);
        assert_eq!(r.premiere_rupture, Some(3));
        assert_eq!(r.entrees_verifiees, 2);
    }

    #[test]
    fn detecte_suppression_au_milieu() {
        let conn = crate::db::ouvrir_en_memoire().unwrap();
        for _ in 0..4 {
            ecrire(&conn, "2026-01-01T00:00:00.000Z", None, "test", None, None, None, None).unwrap();
        }
        conn.execute_batch("DROP TRIGGER journal_pas_de_suppression;").unwrap();
        conn.execute("DELETE FROM journal_audit WHERE seq = 2", []).unwrap();
        let r = verifier_chaine(&conn).unwrap();
        assert!(!r.intacte);
        assert_eq!(r.premiere_rupture, Some(3));
    }
}
