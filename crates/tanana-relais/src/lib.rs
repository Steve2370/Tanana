//! Relais de consultation à distance.
//!
//! Il reçoit des hôtels des paquets **chiffrés de bout en bout** et les
//! redonne aux administrateurs autorisés. Il ne possède aucune clé : il ne
//! peut ni lire, ni modifier, ni fabriquer de données d'hôtel.
//!
//! Ce qu'il garde, par hôtel : deux empreintes de jetons (envoi, lecture),
//! le témoin chiffré, les lots chiffrés dans l'ordre d'arrivée, l'heure du
//! dernier contact. Rien d'autre.

use std::sync::{Arc, Mutex};

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{SecondsFormat, Utc};
use data_encoding::BASE32_NOPAD;
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tanana_protocole::{
    Accuse, DemandeInscription, ErreurRelais, Inscription, LOTS_PAR_PAGE, LotChiffre, LotRecu, PageLots,
    TAILLE_LOT_MAX, Temoin,
};

/// Réglages du relais.
#[derive(Debug, Clone, Default)]
pub struct Reglages {
    /// Codes d'invitation acceptés à l'inscription. Vide : inscriptions fermées
    /// sauf si `inscriptions_libres`.
    pub codes_invitation: Vec<String>,
    /// Pour un essai local seulement : n'importe qui peut inscrire un hôtel.
    pub inscriptions_libres: bool,
    /// Nombre maximal d'hôtels.
    pub hotels_max: i64,
}

#[derive(Clone)]
pub struct Relais {
    base: Arc<Mutex<Connection>>,
    reglages: Arc<Reglages>,
}

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS hotels (
    id                 TEXT PRIMARY KEY,
    jeton_envoi_hash   TEXT NOT NULL,
    jeton_lecture_hash TEXT NOT NULL,
    temoin_nonce       TEXT NOT NULL,
    temoin_chiffre     TEXT NOT NULL,
    cree_le            TEXT NOT NULL,
    dernier_contact    TEXT
) STRICT;
CREATE TABLE IF NOT EXISTS lots (
    hotel_id  TEXT NOT NULL REFERENCES hotels(id) ON DELETE CASCADE,
    numero    INTEGER NOT NULL,
    seq_debut INTEGER NOT NULL,
    seq_fin   INTEGER NOT NULL,
    nonce     TEXT NOT NULL,
    chiffre   TEXT NOT NULL,
    recu_le   TEXT NOT NULL,
    PRIMARY KEY (hotel_id, numero),
    UNIQUE (hotel_id, seq_debut)
) STRICT;
";

fn maintenant() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn empreinte(jeton: &str) -> String {
    hex::encode(Sha256::digest(jeton.as_bytes()))
}

/// Comparaison en temps constant (n'indique pas où deux jetons diffèrent).
fn egaux(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn aleatoire(octets: usize) -> Vec<u8> {
    let mut v = vec![0u8; octets];
    getrandom::fill(&mut v).expect("hasard du système");
    v
}

/// Identifiant court, facile à recopier : « HK7Q-F29X » (sans 0, 1, 8, 9).
fn nouvel_identifiant() -> String {
    let brut = BASE32_NOPAD.encode(&aleatoire(5)); // 8 caractères
    format!("{}-{}", &brut[..4], &brut[4..])
}

struct Refus(StatusCode, &'static str);

impl IntoResponse for Refus {
    fn into_response(self) -> Response {
        (self.0, Json(ErreurRelais { erreur: self.1.into() })).into_response()
    }
}

impl From<rusqlite::Error> for Refus {
    fn from(e: rusqlite::Error) -> Self {
        tracing::error!("base du relais : {e}");
        Refus(StatusCode::INTERNAL_SERVER_ERROR, "erreur interne")
    }
}

fn jeton(entetes: &HeaderMap) -> Result<String, Refus> {
    entetes
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty() && v.len() <= 200)
        .ok_or(Refus(StatusCode::UNAUTHORIZED, "jeton manquant"))
}

impl Relais {
    pub fn ouvrir(chemin: &std::path::Path, reglages: Reglages) -> rusqlite::Result<Self> {
        let conn = Connection::open(chemin)?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(Self { base: Arc::new(Mutex::new(conn)), reglages: Arc::new(reglages) })
    }

    pub fn routes(self) -> Router {
        Router::new()
            .route("/sante", get(|| async { "ok" }))
            .route("/v1/hotels", post(inscrire))
            .route("/v1/hotels/{id}", delete(desinscrire))
            .route("/v1/hotels/{id}/temoin", get(temoin))
            .route("/v1/hotels/{id}/presence", post(presence))
            .route("/v1/hotels/{id}/lots", post(recevoir).get(lire))
            .layer(DefaultBodyLimit::max(TAILLE_LOT_MAX + 16 * 1024))
            .with_state(self)
    }

    /// Vérifie un jeton d'hôtel. `envoi` : jeton d'envoi (hôtel), sinon de lecture (administrateur).
    fn autoriser(&self, conn: &Connection, hotel_id: &str, jeton: &str, envoi: bool) -> Result<(), Refus> {
        let colonne = if envoi { "jeton_envoi_hash" } else { "jeton_lecture_hash" };
        let attendu: Option<String> = conn
            .query_row(&format!("SELECT {colonne} FROM hotels WHERE id = ?1"), [hotel_id], |r| r.get(0))
            .optional()?;
        match attendu {
            None => Err(Refus(StatusCode::NOT_FOUND, "hôtel inconnu")),
            Some(h) if egaux(&h, &empreinte(jeton)) => Ok(()),
            Some(_) => Err(Refus(StatusCode::FORBIDDEN, "")),
        }
    }
}

async fn inscrire(State(r): State<Relais>, Json(d): Json<DemandeInscription>) -> Result<impl IntoResponse, Refus> {
    let code_ok = r.reglages.inscriptions_libres
        || d.code_invitation.as_deref().is_some_and(|c| r.reglages.codes_invitation.iter().any(|x| egaux(x, c.trim())));
    if !code_ok {
        return Err(Refus(StatusCode::FORBIDDEN, "code d'invitation absent ou invalide"));
    }
    if d.jeton_lecture.len() != 64 || d.temoin.chiffre.len() > 4096 || d.temoin.nonce.len() > 64 {
        return Err(Refus(StatusCode::BAD_REQUEST, "demande invalide"));
    }
    let conn = r.base.lock().unwrap();
    let nombre: i64 = conn.query_row("SELECT count(*) FROM hotels", [], |x| x.get(0))?;
    if r.reglages.hotels_max > 0 && nombre >= r.reglages.hotels_max {
        return Err(Refus(StatusCode::SERVICE_UNAVAILABLE, "relais complet"));
    }
    let jeton_envoi = hex::encode(aleatoire(32));
    let mut hotel_id = nouvel_identifiant();
    while conn.query_row("SELECT count(*) FROM hotels WHERE id = ?1", [&hotel_id], |x| x.get::<_, i64>(0))? > 0 {
        hotel_id = nouvel_identifiant();
    }
    conn.execute(
        "INSERT INTO hotels (id, jeton_envoi_hash, jeton_lecture_hash, temoin_nonce, temoin_chiffre, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            hotel_id,
            empreinte(&jeton_envoi),
            empreinte(&d.jeton_lecture),
            d.temoin.nonce,
            d.temoin.chiffre,
            maintenant()
        ],
    )?;
    tracing::info!(hotel = %hotel_id, "nouvel hôtel inscrit");
    Ok((StatusCode::CREATED, Json(Inscription { hotel_id, jeton_envoi })))
}

async fn desinscrire(
    State(r): State<Relais>,
    Path(id): Path<String>,
    entetes: HeaderMap,
) -> Result<impl IntoResponse, Refus> {
    let j = jeton(&entetes)?;
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, true)?;
    conn.execute("DELETE FROM lots WHERE hotel_id = ?1", [&id])?;
    conn.execute("DELETE FROM hotels WHERE id = ?1", [&id])?;
    tracing::info!(hotel = %id, "hôtel désinscrit, données effacées");
    Ok(Json(serde_json::json!({})))
}

async fn temoin(State(r): State<Relais>, Path(id): Path<String>, entetes: HeaderMap) -> Result<Json<Temoin>, Refus> {
    let j = jeton(&entetes)?;
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, false)?;
    let t = conn.query_row("SELECT temoin_nonce, temoin_chiffre FROM hotels WHERE id = ?1", [&id], |x| {
        Ok(Temoin { nonce: x.get(0)?, chiffre: x.get(1)? })
    })?;
    Ok(Json(t))
}

async fn presence(
    State(r): State<Relais>,
    Path(id): Path<String>,
    entetes: HeaderMap,
) -> Result<impl IntoResponse, Refus> {
    let j = jeton(&entetes)?;
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, true)?;
    conn.execute("UPDATE hotels SET dernier_contact = ?1 WHERE id = ?2", params![maintenant(), id])?;
    Ok(Json(serde_json::json!({})))
}

async fn recevoir(
    State(r): State<Relais>,
    Path(id): Path<String>,
    entetes: HeaderMap,
    Json(lot): Json<LotChiffre>,
) -> Result<Json<Accuse>, Refus> {
    let j = jeton(&entetes)?;
    if lot.seq_debut < 1 || lot.seq_fin < lot.seq_debut || lot.nonce.len() > 64 || lot.chiffre.len() > TAILLE_LOT_MAX {
        return Err(Refus(StatusCode::BAD_REQUEST, "lot invalide"));
    }
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, true)?;
    let quand = maintenant();
    conn.execute("UPDATE hotels SET dernier_contact = ?1 WHERE id = ?2", params![quand, id])?;
    // Lot déjà reçu (l'accusé s'était perdu) : même numéro, rien d'ajouté.
    if let Some(numero) = conn
        .query_row("SELECT numero FROM lots WHERE hotel_id = ?1 AND seq_debut = ?2", params![id, lot.seq_debut], |x| {
            x.get(0)
        })
        .optional()?
    {
        return Ok(Json(Accuse { numero }));
    }
    let numero: i64 =
        conn.query_row("SELECT coalesce(max(numero), 0) + 1 FROM lots WHERE hotel_id = ?1", [&id], |x| x.get(0))?;
    conn.execute(
        "INSERT INTO lots (hotel_id, numero, seq_debut, seq_fin, nonce, chiffre, recu_le) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, numero, lot.seq_debut, lot.seq_fin, lot.nonce, lot.chiffre, quand],
    )?;
    Ok(Json(Accuse { numero }))
}

#[derive(Deserialize)]
struct Apres {
    #[serde(default)]
    apres: i64,
}

async fn lire(
    State(r): State<Relais>,
    Path(id): Path<String>,
    Query(q): Query<Apres>,
    entetes: HeaderMap,
) -> Result<Json<PageLots>, Refus> {
    let j = jeton(&entetes)?;
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, false)?;
    let mut stmt = conn.prepare(
        "SELECT numero, recu_le, seq_debut, seq_fin, nonce, chiffre FROM lots
         WHERE hotel_id = ?1 AND numero > ?2 ORDER BY numero LIMIT ?3",
    )?;
    let mut lots: Vec<LotRecu> = stmt
        .query_map(params![id, q.apres, LOTS_PAR_PAGE as i64 + 1], |x| {
            Ok(LotRecu {
                numero: x.get(0)?,
                recu_le: x.get(1)?,
                lot: LotChiffre { seq_debut: x.get(2)?, seq_fin: x.get(3)?, nonce: x.get(4)?, chiffre: x.get(5)? },
            })
        })?
        .collect::<Result<_, _>>()?;
    let suite = lots.len() > LOTS_PAR_PAGE;
    lots.truncate(LOTS_PAR_PAGE);
    let dernier_contact: Option<String> =
        conn.query_row("SELECT dernier_contact FROM hotels WHERE id = ?1", [&id], |x| x.get(0))?;
    Ok(Json(PageLots { lots, suite, dernier_contact }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiants_courts_et_lisibles() {
        let id = nouvel_identifiant();
        assert_eq!(id.len(), 9);
        assert!(!id.contains(['0', '1', '8', '9']));
        assert_ne!(nouvel_identifiant(), id);
    }

    #[test]
    fn comparaison_des_jetons() {
        assert!(egaux("abc", "abc"));
        assert!(!egaux("abc", "abd"));
        assert!(!egaux("abc", "abcd"));
    }
}
