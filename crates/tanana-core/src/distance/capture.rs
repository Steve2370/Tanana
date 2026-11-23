//! Capture des changements dans la « boîte d'envoi » (`sync_boite`).
//!
//! Des triggers, créés à partir de la structure réelle des tables, notent
//! chaque écriture et chaque effacement. Une liste explicite dit quelles
//! tables partent, et quelles colonnes ou lignes ne partent jamais : un
//! oubli dans cette liste empêche l'envoi, il ne provoque pas de fuite.

use rusqlite::Connection;

use crate::erreur::Resultat;

/// Tables envoyées, dans un ordre qui respecte les clés étrangères.
pub const TABLES_ENVOYEES: [&str; 18] = [
    "parametres",
    "roles",
    "permissions",
    "role_permissions",
    "utilisateurs",
    "permissions_perso",
    "types_chambre",
    "chambres",
    "clients",
    "groupes",
    "reservations",
    "paiements",
    "depenses",
    "journal_audit",
    "logos",
    "documents",
    "taxes_hotel",
    "taxes_reservation",
];

/// Tables qui ne quittent jamais l'hôtel.
pub const TABLES_LOCALES: [&str; 7] = [
    "sessions",
    "sync_boite",
    "sync_config",
    "sync_acces",
    "sync_acces_retraits",
    "sync_commandes_vues",
    "sync_activation_auto",
];

/// Colonnes secrètes : leur valeur n'est jamais envoyée, seulement « * » si
/// elle existe (l'administrateur voit qu'un employé a un PIN, pas le PIN).
pub const COLONNES_SECRETES: [(&str, &str); 2] = [("utilisateurs", "mot_de_passe_hash"), ("utilisateurs", "pin_hash")];

/// Paramètres secrets : jamais envoyés.
pub const PARAMETRES_SECRETS: [&str; 1] = ["code_recuperation_hash"];

/// Colonnes d'une table : (toutes celles envoyées, celles de la clé primaire).
pub(crate) fn colonnes(conn: &Connection, table: &str) -> Resultat<(Vec<String>, Vec<String>)> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let infos: Vec<(String, i64)> =
        stmt.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, i64>(5)?)))?.collect::<Result<_, _>>()?;
    let mut cles: Vec<(String, i64)> = infos.iter().filter(|(_, pk)| *pk > 0).cloned().collect();
    cles.sort_by_key(|(_, pk)| *pk);
    Ok((infos.into_iter().map(|(n, _)| n).collect(), cles.into_iter().map(|(n, _)| n).collect()))
}

fn json_objet(table: &str, prefixe: &str, colonnes: &[String]) -> String {
    let paires: Vec<String> = colonnes
        .iter()
        .map(|c| {
            if COLONNES_SECRETES.contains(&(table, c.as_str())) {
                format!("'{c}', CASE WHEN {prefixe}{c} IS NULL THEN NULL ELSE '*' END")
            } else {
                format!("'{c}', {prefixe}{c}")
            }
        })
        .collect();
    format!("json_object({})", paires.join(", "))
}

fn json_cle(prefixe: &str, cles: &[String]) -> String {
    let v: Vec<String> = cles.iter().map(|c| format!("{prefixe}{c}")).collect();
    format!("json_array({})", v.join(", "))
}

fn condition_secrets(table: &str, prefixe: &str) -> Option<String> {
    (table == "parametres").then(|| {
        let liste: Vec<String> = PARAMETRES_SECRETS.iter().map(|p| format!("'{p}'")).collect();
        format!("{prefixe}cle NOT IN ({})", liste.join(", "))
    })
}

/// (Re)crée les triggers de capture. Sans effet sur les données ; à refaire
/// après chaque migration, pour suivre les colonnes ajoutées.
pub fn installer(conn: &Connection) -> Resultat<()> {
    retirer(conn)?;
    for table in TABLES_ENVOYEES {
        let (cols, cles) = colonnes(conn, table)?;
        let quand_new = condition_secrets(table, "NEW.").map(|c| format!("WHEN {c}")).unwrap_or_default();
        let quand_old = condition_secrets(table, "OLD.").map(|c| format!("WHEN {c}")).unwrap_or_default();
        let ecrire = format!(
            "INSERT INTO sync_boite (nom_table, operation, cle, donnees) VALUES ('{table}', 'ecrire', {}, {});",
            json_cle("NEW.", &cles),
            json_objet(table, "NEW.", &cols)
        );
        // Si la clé primaire change (rare), l'ancienne ligne doit disparaître de la copie.
        let effacer_ancienne = format!(
            "INSERT INTO sync_boite (nom_table, operation, cle) SELECT '{table}', 'effacer', {ancienne}
             WHERE {ancienne} <> {nouvelle};",
            ancienne = json_cle("OLD.", &cles),
            nouvelle = json_cle("NEW.", &cles),
        );
        conn.execute_batch(&format!(
            "CREATE TRIGGER sync_{table}_ins AFTER INSERT ON {table} {quand_new} BEGIN {ecrire} END;
             CREATE TRIGGER sync_{table}_maj AFTER UPDATE ON {table} {quand_new} BEGIN {effacer_ancienne} {ecrire} END;
             CREATE TRIGGER sync_{table}_eff AFTER DELETE ON {table} {quand_old} BEGIN
                INSERT INTO sync_boite (nom_table, operation, cle) VALUES ('{table}', 'effacer', {});
             END;",
            json_cle("OLD.", &cles)
        ))?;
    }
    Ok(())
}

pub fn retirer(conn: &Connection) -> Resultat<()> {
    for table in TABLES_ENVOYEES {
        for suffixe in ["ins", "maj", "eff"] {
            conn.execute_batch(&format!("DROP TRIGGER IF EXISTS sync_{table}_{suffixe};"))?;
        }
    }
    Ok(())
}

/// Met tout le contenu actuel dans la boîte d'envoi (premier envoi complet).
pub fn tout_mettre_en_boite(conn: &Connection) -> Resultat<usize> {
    let mut total = 0;
    for table in TABLES_ENVOYEES {
        let (cols, cles) = colonnes(conn, table)?;
        let filtre = condition_secrets(table, "").map(|c| format!("WHERE {c}")).unwrap_or_default();
        total += conn.execute(
            &format!(
                "INSERT INTO sync_boite (nom_table, operation, cle, donnees)
                 SELECT '{table}', 'ecrire', {}, {} FROM {table} {filtre} ORDER BY rowid",
                json_cle("", &cles),
                json_objet(table, "", &cols)
            ),
            [],
        )?;
    }
    Ok(total)
}
