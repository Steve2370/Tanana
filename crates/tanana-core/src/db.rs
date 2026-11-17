//! Ouverture de la base, chiffrement, migrations et sauvegardes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use rusqlite_migration::{M, Migrations};

use crate::erreur::{Erreur, Resultat};

#[cfg(not(any(feature = "chiffrement", feature = "sans-chiffrement")))]
compile_error!("Activer la feature `chiffrement` (recommandé) ou `sans-chiffrement`.");

/// Les migrations sont embarquées dans le binaire. On n'en modifie JAMAIS une
/// déjà livrée : on en ajoute une nouvelle (0002_..., 0003_...).
const MIGRATIONS: [&str; 5] = [
    include_str!("../migrations/0001_initial.sql"),
    include_str!("../migrations/0002_annulation_depenses.sql"),
    include_str!("../migrations/0003_documents.sql"),
    include_str!("../migrations/0004_taxes.sql"),
    include_str!("../migrations/0005_consultation_distance.sql"),
];

/// Version de la structure de la base que connaît cette version de Tanana
/// (`PRAGMA user_version` après les migrations).
pub const VERSION_SCHEMA: i64 = MIGRATIONS.len() as i64;

pub fn migrations() -> Migrations<'static> {
    Migrations::new(MIGRATIONS.iter().map(|sql| M::up(sql)).collect())
}

/// Clé de chiffrement brute de 32 octets, encodée en hexadécimal (64 caractères).
/// Côté application, elle est générée au premier lancement et rangée dans le
/// trousseau du système (Trousseau macOS, Gestionnaire d'identification Windows).
#[derive(Clone)]
pub struct CleBase(String);

impl CleBase {
    pub fn generer() -> Resultat<Self> {
        let mut octets = [0u8; 32];
        getrandom::fill(&mut octets).map_err(|e| Erreur::Base(format!("aléa : {e}")))?;
        Ok(Self(hex::encode(octets)))
    }

    /// Lit une clé, y compris recopiée à la main depuis le papier : les espaces,
    /// tirets et retours à la ligne entre les groupes sont ignorés.
    pub fn depuis_hex(hex_cle: &str) -> Resultat<Self> {
        let propre: String = hex_cle.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
        let valide = propre.len() == 64 && propre.chars().all(|c| c.is_ascii_hexdigit());
        if !valide {
            return Err(Erreur::Validation("clé de base invalide".into()));
        }
        Ok(Self(propre.to_ascii_lowercase()))
    }

    pub fn en_hex(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for CleBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CleBase(***)") // ne jamais afficher la clé dans un log
    }
}

pub(crate) fn appliquer_cle(conn: &Connection, cle: Option<&CleBase>) -> Resultat<()> {
    match cle {
        #[cfg(feature = "chiffrement")]
        Some(c) => {
            // Clé brute (pas de dérivation PBKDF2) : format x'...'
            conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", c.en_hex()))?;
            Ok(())
        }
        #[cfg(not(feature = "chiffrement"))]
        Some(_) => Err(Erreur::Base("clé fournie mais l'app est compilée sans chiffrement".into())),
        None => Ok(()),
    }
}

fn configurer(conn: &Connection) -> Resultat<()> {
    // Lecture test EN PREMIER : avec une mauvaise clé, toute autre instruction
    // (même un PRAGMA) échouerait avec un message moins clair.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .map_err(|_| Erreur::Base("base illisible : clé incorrecte ou fichier corrompu".into()))?;
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;",
    )?;
    Ok(())
}

/// Ouvre (ou crée) la base et applique les migrations en attente.
pub fn ouvrir(chemin: &Path, cle: Option<&CleBase>) -> Resultat<Connection> {
    if let Some(dossier) = chemin.parent() {
        std::fs::create_dir_all(dossier).map_err(|e| Erreur::Base(e.to_string()))?;
    }
    let mut conn = Connection::open(chemin)?;
    appliquer_cle(&conn, cle)?;
    configurer(&conn)?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

/// Base en mémoire, pour les tests.
pub fn ouvrir_en_memoire() -> Resultat<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    migrations().to_latest(&mut conn)?;
    Ok(conn)
}

/// Copie cohérente de la base via l'API backup de SQLite (sûre même pendant
/// une écriture, contrairement à une copie de fichier). La copie est chiffrée
/// avec la même clé. À appeler HORS transaction sur `conn`.
pub fn sauvegarder(conn: &Connection, destination: &Path, cle: Option<&CleBase>) -> Resultat<()> {
    if destination.exists() {
        return Err(Erreur::Doublon(destination.display().to_string()));
    }
    let mut dest =
        Connection::open_with_flags(destination, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)?;
    appliquer_cle(&dest, cle)?;
    {
        use rusqlite::backup::{Backup, StepResult};
        let sauvegarde = Backup::new(conn, &mut dest)?;
        // Boucle bornée : on ne reste jamais bloqué si la base est occupée.
        let mut attentes = 0;
        loop {
            match sauvegarde.step(512)? {
                StepResult::Done => break,
                StepResult::More => {}
                StepResult::Busy | StepResult::Locked => {
                    attentes += 1;
                    if attentes > 200 {
                        return Err(Erreur::Base("base occupée : sauvegarde impossible pour l'instant".into()));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                _ => return Err(Erreur::Base("sauvegarde : état inattendu".into())),
            }
        }
    }
    // Vérifie que la copie est lisible et intègre.
    let integrite: String = dest.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrite != "ok" {
        return Err(Erreur::Base(format!("sauvegarde corrompue : {integrite}")));
    }
    Ok(())
}

/// Sauvegarde quotidienne automatique dans `dossier`, en gardant les
/// `a_garder` plus récentes. Retourne le chemin créé, ou `None` si une
/// sauvegarde du jour existe déjà.
pub fn sauvegarde_automatique(
    conn: &Connection,
    dossier: &Path,
    cle: Option<&CleBase>,
    jour: chrono::NaiveDate,
    a_garder: usize,
) -> Resultat<Option<PathBuf>> {
    std::fs::create_dir_all(dossier).map_err(|e| Erreur::Base(e.to_string()))?;
    let chemin = dossier.join(format!("tanana-sauvegarde-{}.db", jour.format("%Y-%m-%d")));
    if chemin.exists() {
        return Ok(None);
    }
    sauvegarder(conn, &chemin, cle)?;

    let mut existantes: Vec<PathBuf> = std::fs::read_dir(dossier)
        .map_err(|e| Erreur::Base(e.to_string()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("tanana-sauvegarde-") && n.ends_with(".db"))
        })
        .collect();
    existantes.sort(); // le nom contient la date ISO : l'ordre alphabétique est chronologique
    let a_supprimer = existantes.len().saturating_sub(a_garder.max(1));
    for vieux in existantes.into_iter().take(a_supprimer) {
        let _ = std::fs::remove_file(vieux);
    }
    Ok(Some(chemin))
}
