//! Relais de consultation à distance.
//!
//! Il reçoit des hôtels des paquets **chiffrés de bout en bout** et les
//! redonne aux administrateurs autorisés. Il ne possède aucune clé : il ne
//! peut ni lire, ni modifier, ni fabriquer de données d'hôtel.
//!
//! Ce qu'il garde, par hôtel : deux empreintes de jetons (envoi, lecture),
//! le témoin chiffré, les lots chiffrés dans l'ordre d'arrivée, l'heure du
//! dernier contact. Rien d'autre.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, SecondsFormat, Utc};
use data_encoding::{BASE32_NOPAD, BASE64};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tanana_protocole::{
    ATTENTE_REPONSE_SECONDES, AccesOuvert, Accuse, CoffreAcces, CommandeChiffree, CommandesEnAttente,
    DemandeInscription, ERREUR_HOTEL_INJOIGNABLE, ERREUR_PAS_DE_REPONSE, ERREUR_POSTE_REMPLACE, ErreurRelais,
    Inscription, Instantane, InstantaneRecu, LOTS_PAR_PAGE, LotChiffre, LotRecu, NouveauPrincipal,
    PRINCIPAL_JOIGNABLE_SECONDES, PageLots, PreuveAcces, ReponseChiffree, SelAcces, TAILLE_INSTANTANE_MAX,
    TAILLE_LOT_MAX, Temoin,
};
use tokio::sync::{Notify, oneshot};

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

/// Politique de sécurité de la page : rien d'extérieur, aucun script en ligne,
/// les requêtes ne vont qu'au relais lui-même.
const CSP_PAGE: &str = "default-src 'self'; script-src 'self'; connect-src 'self'; img-src 'self' data:; \
     style-src 'self' 'unsafe-inline'; font-src 'self' data:; object-src 'none'; base-uri 'none'; \
     form-action 'none'; frame-ancestors 'none'";

#[derive(Clone)]
pub struct Relais {
    base: Arc<Mutex<Connection>>,
    reglages: Arc<Reglages>,
    /// Modifications à distance en transit (en mémoire : elles ne durent que quelques secondes).
    canal: Arc<Mutex<Canal>>,
    /// Essais de mot de passe ratés, par compte et par adresse.
    essais: Arc<Mutex<Essais>>,
}

/// Une modification en route vers l'ordinateur principal.
struct EnRoute {
    commande: CommandeChiffree,
    reponse: oneshot::Sender<ReponseChiffree>,
}

#[derive(Default)]
struct Canal {
    /// Pas encore remises à l'hôtel, par hôtel
    attente: HashMap<String, Vec<EnRoute>>,
    /// Remises à l'hôtel, réponse attendue (par identifiant de commande)
    remises: HashMap<String, (String, oneshot::Sender<ReponseChiffree>)>,
    /// Réveil de l'hôtel qui attend des commandes
    reveils: HashMap<String, Arc<Notify>>,
}

impl Canal {
    fn reveil(&mut self, hotel_id: &str) -> Arc<Notify> {
        self.reveils.entry(hotel_id.to_string()).or_insert_with(|| Arc::new(Notify::new())).clone()
    }
}

/// Essais de mot de passe ratés autorisés en 15 minutes, par compte et par adresse.
pub const ECHECS_ACCES_PAR_COMPTE: usize = 10;
pub const ECHECS_ACCES_PAR_ADRESSE: usize = 30;
const FENETRE_ECHECS: Duration = Duration::from_secs(15 * 60);

#[derive(Default)]
struct Essais {
    par_compte: HashMap<String, Vec<Instant>>,
    par_adresse: HashMap<Option<IpAddr>, Vec<Instant>>,
}

impl Essais {
    fn nettoyer(&mut self, maintenant: Instant) {
        let garder = |v: &mut Vec<Instant>| {
            v.retain(|t| maintenant.duration_since(*t) < FENETRE_ECHECS);
            !v.is_empty()
        };
        self.par_compte.retain(|_, v| garder(v));
        self.par_adresse.retain(|_, v| garder(v));
    }

    fn bloque(&mut self, compte: &str, ip: Option<IpAddr>, maintenant: Instant) -> bool {
        self.nettoyer(maintenant);
        self.par_compte.get(compte).is_some_and(|v| v.len() >= ECHECS_ACCES_PAR_COMPTE)
            || self.par_adresse.get(&ip).is_some_and(|v| v.len() >= ECHECS_ACCES_PAR_ADRESSE)
    }

    fn echec(&mut self, compte: &str, ip: Option<IpAddr>, maintenant: Instant) {
        self.par_compte.entry(compte.to_string()).or_default().push(maintenant);
        self.par_adresse.entry(ip).or_default().push(maintenant);
    }
}

/// Adresse du client telle que Caddy la transmet (Caddy remplace l'en-tête
/// envoyé par le client : la première adresse est fiable derrière lui).
fn adresse_client(entetes: &HeaderMap) -> Option<IpAddr> {
    entetes
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .and_then(|v| v.trim().parse().ok())
}

const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
CREATE TABLE IF NOT EXISTS hotels (
    id                       TEXT PRIMARY KEY,
    jeton_envoi_hash         TEXT NOT NULL,
    jeton_lecture_hash       TEXT NOT NULL,
    jeton_admin_hash         TEXT,
    temoin_nonce             TEXT NOT NULL,
    temoin_chiffre           TEXT NOT NULL,
    cree_le                  TEXT NOT NULL,
    dernier_contact          TEXT,
    -- Change à chaque nouvel ordinateur principal : les numéros d'envoi repartent
    epoque                   INTEGER NOT NULL DEFAULT 1,
    -- Jeton de l'ancien ordinateur principal : il apprend qu'il a été remplacé
    ancien_jeton_envoi_hash  TEXT
) STRICT;
CREATE TABLE IF NOT EXISTS lots (
    hotel_id  TEXT NOT NULL REFERENCES hotels(id) ON DELETE CASCADE,
    numero    INTEGER NOT NULL,
    epoque    INTEGER NOT NULL DEFAULT 1,
    seq_debut INTEGER NOT NULL,
    seq_fin   INTEGER NOT NULL,
    nonce     TEXT NOT NULL,
    chiffre   TEXT NOT NULL,
    recu_le   TEXT NOT NULL,
    PRIMARY KEY (hotel_id, numero),
    UNIQUE (hotel_id, epoque, seq_debut)
) STRICT;
CREATE TABLE IF NOT EXISTS acces (
    recherche TEXT PRIMARY KEY,
    hotel_id  TEXT NOT NULL REFERENCES hotels(id) ON DELETE CASCADE,
    sel       TEXT NOT NULL,
    verif     TEXT NOT NULL,
    nonce     TEXT NOT NULL,
    chiffre   TEXT NOT NULL,
    maj_le    TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS instantanes (
    hotel_id     TEXT PRIMARY KEY REFERENCES hotels(id) ON DELETE CASCADE,
    apres_numero INTEGER NOT NULL,
    nonce        TEXT NOT NULL,
    chiffre      TEXT NOT NULL,
    recu_le      TEXT NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS reglages (
    cle    TEXT PRIMARY KEY,
    valeur TEXT NOT NULL
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
        archiver_ancien_format(&conn)?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        // Graine des sels factices (comptes inconnus), créée une fois
        conn.execute(
            "INSERT OR IGNORE INTO reglages (cle, valeur) VALUES ('graine', ?1)",
            [hex::encode(aleatoire(32))],
        )?;
        Ok(Self {
            base: Arc::new(Mutex::new(conn)),
            reglages: Arc::new(reglages),
            canal: Default::default(),
            essais: Default::default(),
        })
    }

    /// Routes du relais, plus la page web de consultation (fichiers statiques
    /// du dossier `pages`, construits par `npm run build:web`). La page
    /// déchiffre tout dans le navigateur : le serveur ne voit jamais la clé.
    pub fn routes_avec_pages(self, pages: &std::path::Path) -> Router {
        use axum::http::{HeaderName, HeaderValue, header};
        use tower_http::services::{ServeDir, ServeFile};
        use tower_http::set_header::SetResponseHeaderLayer;

        let entete = |nom: HeaderName, valeur: &'static str| {
            SetResponseHeaderLayer::if_not_present(nom, HeaderValue::from_static(valeur))
        };
        let fichiers = ServeDir::new(pages).not_found_service(ServeFile::new(pages.join("index.html")));
        let page = Router::new()
            .fallback_service(fichiers)
            .layer(entete(header::CONTENT_SECURITY_POLICY, CSP_PAGE))
            .layer(entete(header::REFERRER_POLICY, "no-referrer"))
            .layer(entete(header::X_CONTENT_TYPE_OPTIONS, "nosniff"))
            .layer(entete(header::X_FRAME_OPTIONS, "DENY"))
            .layer(entete(header::CACHE_CONTROL, "no-cache"))
            .layer(entete(HeaderName::from_static("permissions-policy"), "camera=(), microphone=(), geolocation=()"));
        self.routes().merge(page)
    }

    pub fn routes(self) -> Router {
        let instantane = Router::new()
            .route("/v1/hotels/{id}/instantane", get(lire_instantane).put(deposer_instantane))
            .layer(DefaultBodyLimit::max(TAILLE_INSTANTANE_MAX + 16 * 1024))
            .with_state(self.clone());
        Router::new()
            .route("/sante", get(|| async { "ok" }))
            .route("/v1/hotels", post(inscrire))
            .route("/v1/hotels/{id}", delete(desinscrire))
            .route("/v1/hotels/{id}/temoin", get(temoin))
            .route("/v1/hotels/{id}/presence", post(presence))
            .route("/v1/hotels/{id}/lots", post(recevoir).get(lire))
            .route("/v1/hotels/{id}/acces/{recherche}", axum::routing::put(deposer_acces).delete(retirer_acces))
            .route("/v1/acces/{recherche}/sel", get(sel_acces))
            .route("/v1/acces/{recherche}", post(ouvrir_acces))
            .route("/v1/hotels/{id}/principal", post(devenir_principal))
            .route("/v1/hotels/{id}/commandes", post(poster_commande).get(lire_commandes))
            .route("/v1/hotels/{id}/reponses/{commande}", post(repondre))
            .layer(DefaultBodyLimit::max(TAILLE_LOT_MAX + 16 * 1024))
            .with_state(self)
            .merge(instantane)
    }

    /// Vérifie un jeton d'hôtel.
    fn autoriser(&self, conn: &Connection, hotel_id: &str, jeton: &str, sorte: Jeton) -> Result<(), Refus> {
        let colonne = match sorte {
            Jeton::Envoi => "jeton_envoi_hash",
            Jeton::Lecture => "jeton_lecture_hash",
            Jeton::Admin => "jeton_admin_hash",
        };
        let ligne: Option<(Option<String>, Option<String>)> = conn
            .query_row(
                &format!("SELECT {colonne}, ancien_jeton_envoi_hash FROM hotels WHERE id = ?1"),
                [hotel_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let e = empreinte(jeton);
        match ligne {
            None => Err(Refus(StatusCode::NOT_FOUND, "hôtel inconnu")),
            Some((Some(h), _)) if egaux(&h, &e) => Ok(()),
            // L'ancien ordinateur principal : un autre a pris sa place
            Some((_, Some(ancien))) if sorte == Jeton::Envoi && egaux(&ancien, &e) => {
                Err(Refus(StatusCode::CONFLICT, ERREUR_POSTE_REMPLACE))
            }
            Some(_) => Err(Refus(StatusCode::FORBIDDEN, "")),
        }
    }

    /// Note que l'ordinateur principal vient de se manifester.
    fn contact(conn: &Connection, hotel_id: &str) -> Result<(), Refus> {
        conn.execute("UPDATE hotels SET dernier_contact = ?1 WHERE id = ?2", params![maintenant(), hotel_id])?;
        Ok(())
    }
}

/// Base d'avant le travail à distance (v0.1) : ses tables sont mises de côté
/// sous le nom `ancien_*` (rien n'est effacé) et le relais repart à zéro.
fn archiver_ancien_format(conn: &Connection) -> rusqlite::Result<()> {
    let existe = |table: &str| -> rusqlite::Result<bool> {
        conn.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| {
            r.get::<_, i64>(0)
        })
        .map(|n| n > 0)
    };
    if !existe("hotels")? {
        return Ok(());
    }
    let nouveau: i64 =
        conn.query_row("SELECT count(*) FROM pragma_table_info('hotels') WHERE name = 'epoque'", [], |r| r.get(0))?;
    if nouveau > 0 {
        return Ok(());
    }
    let suffixe = Utc::now().format("%Y%m%d%H%M%S");
    conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
    for table in ["lots", "hotels"] {
        if existe(table)? {
            conn.execute_batch(&format!("ALTER TABLE {table} RENAME TO ancien_{table}_{suffixe};"))?;
        }
    }
    tracing::warn!("ancien format de base : tables mises de côté (ancien_*_{suffixe}), le relais repart à zéro");
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Jeton {
    /// L'ordinateur principal de l'hôtel
    Envoi,
    /// Consultation (lecture des lots)
    Lecture,
    /// Administrateur à distance (modifications, instantané, changement de principal)
    Admin,
}

async fn inscrire(State(r): State<Relais>, Json(d): Json<DemandeInscription>) -> Result<impl IntoResponse, Refus> {
    let code_ok = r.reglages.inscriptions_libres
        || d.code_invitation.as_deref().is_some_and(|c| r.reglages.codes_invitation.iter().any(|x| egaux(x, c.trim())));
    if !code_ok {
        return Err(Refus(StatusCode::FORBIDDEN, "code d'invitation absent ou invalide"));
    }
    if d.jeton_lecture.len() != 64
        || d.jeton_admin.as_ref().is_some_and(|j| j.len() != 64)
        || d.temoin.chiffre.len() > 4096
        || d.temoin.nonce.len() > 64
    {
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
        "INSERT INTO hotels (id, jeton_envoi_hash, jeton_lecture_hash, jeton_admin_hash, temoin_nonce, temoin_chiffre, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            hotel_id,
            empreinte(&jeton_envoi),
            empreinte(&d.jeton_lecture),
            d.jeton_admin.as_deref().map(empreinte),
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
    r.autoriser(&conn, &id, &j, Jeton::Envoi)?;
    for table in ["lots", "acces", "instantanes"] {
        conn.execute(&format!("DELETE FROM {table} WHERE hotel_id = ?1"), [&id])?;
    }
    conn.execute("DELETE FROM hotels WHERE id = ?1", [&id])?;
    tracing::info!(hotel = %id, "hôtel désinscrit, données effacées");
    Ok(Json(serde_json::json!({})))
}

async fn temoin(State(r): State<Relais>, Path(id): Path<String>, entetes: HeaderMap) -> Result<Json<Temoin>, Refus> {
    let j = jeton(&entetes)?;
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, Jeton::Lecture)?;
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
    r.autoriser(&conn, &id, &j, Jeton::Envoi)?;
    Relais::contact(&conn, &id)?;
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
    r.autoriser(&conn, &id, &j, Jeton::Envoi)?;
    let quand = maintenant();
    Relais::contact(&conn, &id)?;
    let epoque: i64 = conn.query_row("SELECT epoque FROM hotels WHERE id = ?1", [&id], |x| x.get(0))?;
    // Lot déjà reçu (l'accusé s'était perdu) : même numéro, rien d'ajouté.
    if let Some(numero) = conn
        .query_row(
            "SELECT numero FROM lots WHERE hotel_id = ?1 AND epoque = ?2 AND seq_debut = ?3",
            params![id, epoque, lot.seq_debut],
            |x| x.get(0),
        )
        .optional()?
    {
        return Ok(Json(Accuse { numero }));
    }
    let numero: i64 =
        conn.query_row("SELECT coalesce(max(numero), 0) + 1 FROM lots WHERE hotel_id = ?1", [&id], |x| x.get(0))?;
    conn.execute(
        "INSERT INTO lots (hotel_id, numero, epoque, seq_debut, seq_fin, nonce, chiffre, recu_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, numero, epoque, lot.seq_debut, lot.seq_fin, lot.nonce, lot.chiffre, quand],
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
    r.autoriser(&conn, &id, &j, Jeton::Lecture)?;
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

// ───────────────────── Accès par identifiant et mot de passe ─────────────────────

/// Clé de recherche d'un compte : empreinte SHA-256 en hex (64 caractères).
fn recherche_valide(recherche: &str) -> Result<(), Refus> {
    if recherche.len() == 64 && recherche.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(Refus(StatusCode::BAD_REQUEST, "compte invalide"))
    }
}

async fn deposer_acces(
    State(r): State<Relais>,
    Path((id, recherche)): Path<(String, String)>,
    entetes: HeaderMap,
    Json(c): Json<CoffreAcces>,
) -> Result<impl IntoResponse, Refus> {
    let j = jeton(&entetes)?;
    recherche_valide(&recherche)?;
    if c.sel.len() > 64 || c.verif.len() != 64 || c.nonce.len() > 64 || c.chiffre.len() > 4096 {
        return Err(Refus(StatusCode::BAD_REQUEST, "coffre invalide"));
    }
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, Jeton::Envoi)?;
    // Un compte d'un autre hôtel ne peut pas être remplacé
    let autre: Option<String> =
        conn.query_row("SELECT hotel_id FROM acces WHERE recherche = ?1", [&recherche], |x| x.get(0)).optional()?;
    if autre.is_some_and(|h| h != id) {
        return Err(Refus(StatusCode::CONFLICT, "identifiant déjà utilisé par un autre hôtel"));
    }
    conn.execute(
        "INSERT INTO acces (recherche, hotel_id, sel, verif, nonce, chiffre, maj_le) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (recherche) DO UPDATE SET sel = excluded.sel, verif = excluded.verif, nonce = excluded.nonce,
             chiffre = excluded.chiffre, maj_le = excluded.maj_le",
        params![recherche, id, c.sel, c.verif, c.nonce, c.chiffre, maintenant()],
    )?;
    Ok(Json(serde_json::json!({})))
}

async fn retirer_acces(
    State(r): State<Relais>,
    Path((id, recherche)): Path<(String, String)>,
    entetes: HeaderMap,
) -> Result<impl IntoResponse, Refus> {
    let j = jeton(&entetes)?;
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, Jeton::Envoi)?;
    conn.execute("DELETE FROM acces WHERE recherche = ?1 AND hotel_id = ?2", params![recherche, id])?;
    Ok(Json(serde_json::json!({})))
}

/// Sel d'un compte. Pour un compte inconnu, un sel factice mais stable : on ne
/// peut pas savoir de l'extérieur quels identifiants existent.
async fn sel_acces(State(r): State<Relais>, Path(recherche): Path<String>) -> Result<Json<SelAcces>, Refus> {
    recherche_valide(&recherche)?;
    let conn = r.base.lock().unwrap();
    let sel: Option<String> =
        conn.query_row("SELECT sel FROM acces WHERE recherche = ?1", [&recherche], |x| x.get(0)).optional()?;
    let sel = match sel {
        Some(s) => s,
        None => {
            let graine: String =
                conn.query_row("SELECT valeur FROM reglages WHERE cle = 'graine'", [], |x| x.get(0))?;
            BASE64.encode(&Sha256::digest(format!("{graine}|{recherche}").as_bytes())[..16])
        }
    };
    Ok(Json(SelAcces { sel }))
}

async fn ouvrir_acces(
    State(r): State<Relais>,
    Path(recherche): Path<String>,
    entetes: HeaderMap,
    Json(p): Json<PreuveAcces>,
) -> Result<Json<AccesOuvert>, Refus> {
    recherche_valide(&recherche)?;
    let ip = adresse_client(&entetes);
    let instant = Instant::now();
    if r.essais.lock().unwrap().bloque(&recherche, ip, instant) {
        tracing::warn!("accès : trop d'essais ({ip:?})");
        return Err(Refus(StatusCode::TOO_MANY_REQUESTS, "trop d'essais"));
    }
    let ligne: Option<(String, String, String, String)> = {
        let conn = r.base.lock().unwrap();
        conn.query_row("SELECT hotel_id, verif, nonce, chiffre FROM acces WHERE recherche = ?1", [&recherche], |x| {
            Ok((x.get(0)?, x.get(1)?, x.get(2)?, x.get(3)?))
        })
        .optional()?
    };
    match ligne {
        Some((hotel_id, verif, nonce, chiffre)) if p.preuve.len() <= 128 && egaux(&verif, &empreinte(&p.preuve)) => {
            Ok(Json(AccesOuvert { hotel_id, nonce, chiffre }))
        }
        _ => {
            r.essais.lock().unwrap().echec(&recherche, ip, instant);
            Err(Refus(StatusCode::FORBIDDEN, "identifiant ou mot de passe incorrect"))
        }
    }
}

// ───────────────────── Instantané et ordinateur principal ─────────────────────

async fn deposer_instantane(
    State(r): State<Relais>,
    Path(id): Path<String>,
    entetes: HeaderMap,
    Json(i): Json<Instantane>,
) -> Result<impl IntoResponse, Refus> {
    let j = jeton(&entetes)?;
    if i.apres_numero < 0 || i.nonce.len() > 64 || i.chiffre.len() > TAILLE_INSTANTANE_MAX {
        return Err(Refus(StatusCode::BAD_REQUEST, "instantané invalide"));
    }
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, Jeton::Envoi)?;
    Relais::contact(&conn, &id)?;
    conn.execute(
        "INSERT INTO instantanes (hotel_id, apres_numero, nonce, chiffre, recu_le) VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (hotel_id) DO UPDATE SET apres_numero = excluded.apres_numero, nonce = excluded.nonce,
             chiffre = excluded.chiffre, recu_le = excluded.recu_le",
        params![id, i.apres_numero, i.nonce, i.chiffre, maintenant()],
    )?;
    Ok(Json(serde_json::json!({})))
}

async fn lire_instantane(
    State(r): State<Relais>,
    Path(id): Path<String>,
    entetes: HeaderMap,
) -> Result<Json<InstantaneRecu>, Refus> {
    let j = jeton(&entetes)?;
    let conn = r.base.lock().unwrap();
    r.autoriser(&conn, &id, &j, Jeton::Admin)?;
    conn.query_row("SELECT apres_numero, recu_le, nonce, chiffre FROM instantanes WHERE hotel_id = ?1", [&id], |x| {
        Ok(InstantaneRecu { apres_numero: x.get(0)?, recu_le: x.get(1)?, nonce: x.get(2)?, chiffre: x.get(3)? })
    })
    .optional()?
    .map(Json)
    .ok_or(Refus(StatusCode::NOT_FOUND, "aucun instantané"))
}

/// Cet ordinateur devient l'ordinateur principal : nouveau jeton d'envoi,
/// nouvelle époque. L'ancien apprend qu'il a été remplacé à son prochain envoi.
async fn devenir_principal(
    State(r): State<Relais>,
    Path(id): Path<String>,
    entetes: HeaderMap,
) -> Result<Json<NouveauPrincipal>, Refus> {
    let j = jeton(&entetes)?;
    let jeton_envoi = hex::encode(aleatoire(32));
    {
        let conn = r.base.lock().unwrap();
        r.autoriser(&conn, &id, &j, Jeton::Admin)?;
        conn.execute(
            "UPDATE hotels SET ancien_jeton_envoi_hash = jeton_envoi_hash, jeton_envoi_hash = ?1,
                    epoque = epoque + 1, dernier_contact = ?2 WHERE id = ?3",
            params![empreinte(&jeton_envoi), maintenant(), id],
        )?;
    }
    // Les modifications en attente étaient destinées à l'ancien ordinateur
    let mut canal = r.canal.lock().unwrap();
    canal.attente.remove(&id);
    canal.remises.retain(|_, (h, _)| h != &id);
    tracing::info!(hotel = %id, "nouvel ordinateur principal");
    Ok(Json(NouveauPrincipal { jeton_envoi }))
}

// ───────────────────── Modifications à distance ─────────────────────

/// L'ordinateur principal s'est-il manifesté récemment ?
fn principal_joignable(conn: &Connection, hotel_id: &str) -> Result<bool, Refus> {
    let dernier: Option<String> =
        conn.query_row("SELECT dernier_contact FROM hotels WHERE id = ?1", [hotel_id], |x| x.get(0))?;
    Ok(dernier
        .and_then(|d| DateTime::parse_from_rfc3339(&d).ok())
        .is_some_and(|d| (Utc::now() - d.with_timezone(&Utc)).num_seconds() <= PRINCIPAL_JOIGNABLE_SECONDES))
}

/// L'administrateur à distance envoie une modification, puis attend la réponse de l'hôtel.
async fn poster_commande(
    State(r): State<Relais>,
    Path(id): Path<String>,
    entetes: HeaderMap,
    Json(c): Json<CommandeChiffree>,
) -> Result<Json<ReponseChiffree>, Refus> {
    let j = jeton(&entetes)?;
    if c.id.is_empty() || c.id.len() > 64 || c.nonce.len() > 64 || c.chiffre.len() > TAILLE_LOT_MAX {
        return Err(Refus(StatusCode::BAD_REQUEST, "demande invalide"));
    }
    {
        let conn = r.base.lock().unwrap();
        r.autoriser(&conn, &id, &j, Jeton::Admin)?;
        if !principal_joignable(&conn, &id)? {
            return Err(Refus(StatusCode::CONFLICT, ERREUR_HOTEL_INJOIGNABLE));
        }
    }
    let commande_id = c.id.clone();
    let (envoi, reception) = oneshot::channel();
    {
        let mut canal = r.canal.lock().unwrap();
        let file = canal.attente.entry(id.clone()).or_default();
        if file.len() >= 20 {
            return Err(Refus(StatusCode::TOO_MANY_REQUESTS, "trop de demandes en attente"));
        }
        file.push(EnRoute { commande: c, reponse: envoi });
        canal.reveil(&id).notify_waiters();
    }
    match tokio::time::timeout(Duration::from_secs(ATTENTE_REPONSE_SECONDES), reception).await {
        Ok(Ok(reponse)) => Ok(Json(reponse)),
        _ => {
            let mut canal = r.canal.lock().unwrap();
            canal.remises.remove(&commande_id);
            // Jamais remise à l'hôtel : on la retire, elle ne sera pas faite.
            let jamais_remise = canal.attente.get_mut(&id).is_some_and(|f| {
                let avant = f.len();
                f.retain(|e| e.commande.id != commande_id);
                f.len() < avant
            });
            if jamais_remise {
                Err(Refus(StatusCode::CONFLICT, ERREUR_HOTEL_INJOIGNABLE))
            } else {
                Err(Refus(StatusCode::GATEWAY_TIMEOUT, ERREUR_PAS_DE_REPONSE))
            }
        }
    }
}

#[derive(Deserialize)]
struct Attente {
    #[serde(default)]
    attente: u64,
}

/// L'ordinateur principal attend les modifications (jusqu'à 30 s). Sert aussi de signe de vie.
async fn lire_commandes(
    State(r): State<Relais>,
    Path(id): Path<String>,
    Query(q): Query<Attente>,
    entetes: HeaderMap,
) -> Result<Json<CommandesEnAttente>, Refus> {
    let j = jeton(&entetes)?;
    {
        let conn = r.base.lock().unwrap();
        r.autoriser(&conn, &id, &j, Jeton::Envoi)?;
        Relais::contact(&conn, &id)?;
    }
    let attente = Duration::from_secs(q.attente.min(30));
    let reveil = r.canal.lock().unwrap().reveil(&id);
    let fin = Instant::now() + attente;
    loop {
        let notifie = reveil.notified();
        tokio::pin!(notifie);
        notifie.as_mut().enable();
        {
            let mut canal = r.canal.lock().unwrap();
            let file = canal.attente.remove(&id).unwrap_or_default();
            if !file.is_empty() {
                let mut commandes = Vec::new();
                for e in file {
                    canal.remises.insert(e.commande.id.clone(), (id.clone(), e.reponse));
                    commandes.push(e.commande);
                }
                return Ok(Json(CommandesEnAttente { commandes }));
            }
        }
        let reste = fin.saturating_duration_since(Instant::now());
        if reste.is_zero() || tokio::time::timeout(reste, notifie).await.is_err() {
            // Le temps est écoulé : on renouvelle le signe de vie et on rend la main
            let conn = r.base.lock().unwrap();
            Relais::contact(&conn, &id)?;
            return Ok(Json(CommandesEnAttente::default()));
        }
    }
}

/// L'ordinateur principal renvoie le résultat (chiffré) d'une modification.
async fn repondre(
    State(r): State<Relais>,
    Path((id, commande)): Path<(String, String)>,
    entetes: HeaderMap,
    Json(rep): Json<ReponseChiffree>,
) -> Result<impl IntoResponse, Refus> {
    let j = jeton(&entetes)?;
    if rep.nonce.len() > 64 || rep.chiffre.len() > TAILLE_LOT_MAX {
        return Err(Refus(StatusCode::BAD_REQUEST, "réponse invalide"));
    }
    {
        let conn = r.base.lock().unwrap();
        r.autoriser(&conn, &id, &j, Jeton::Envoi)?;
        Relais::contact(&conn, &id)?;
    }
    let attendue = r.canal.lock().unwrap().remises.remove(&commande);
    match attendue {
        Some((hotel, envoi)) if hotel == id => {
            let _ = envoi.send(rep); // l'administrateur a pu abandonner entre-temps
            Ok(Json(serde_json::json!({})))
        }
        _ => Err(Refus(StatusCode::NOT_FOUND, "demande inconnue ou expirée")),
    }
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
    fn les_essais_rates_sont_freines() {
        let mut e = Essais::default();
        let t0 = Instant::now();
        let ip: Option<IpAddr> = "41.188.1.2".parse().ok();
        for _ in 0..ECHECS_ACCES_PAR_COMPTE {
            assert!(!e.bloque("compte", ip, t0));
            e.echec("compte", ip, t0);
        }
        assert!(e.bloque("compte", ip, t0), "compte freiné");
        assert!(!e.bloque("autre", "41.188.1.3".parse().ok(), t0), "autre compte, autre adresse : libre");
        assert!(!e.bloque("compte", ip, t0 + Duration::from_secs(15 * 60 + 1)), "15 minutes plus tard");
        let ip2: Option<IpAddr> = "41.188.9.9".parse().ok();
        for i in 0..ECHECS_ACCES_PAR_ADRESSE {
            e.echec(&format!("c{i}"), ip2, t0);
        }
        assert!(e.bloque("nouveau", ip2, t0), "adresse freinée, quel que soit le compte");
    }

    #[test]
    fn adresse_venant_de_caddy() {
        let mut h = HeaderMap::new();
        assert_eq!(adresse_client(&h), None);
        h.insert("x-forwarded-for", "41.188.1.2, 10.0.0.1".parse().unwrap());
        assert_eq!(adresse_client(&h), "41.188.1.2".parse().ok());
    }

    #[test]
    fn comparaison_des_jetons() {
        assert!(egaux("abc", "abc"));
        assert!(!egaux("abc", "abd"));
        assert!(!egaux("abc", "abcd"));
    }
}
