//! Comptes, mots de passe, PIN, sessions, rôles et permissions.

use std::collections::HashSet;

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::audit::{self, json as en_json};
use crate::domaine::{texte_requis, valider_mot_de_passe, valider_pin};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{
    ConfigurationInitiale, NouvelUtilisateur, PermissionPerso, Role, SessionActive, SessionOuverte, StatutUtilisateur,
    Utilisateur,
};
use crate::permissions::Permission;
use crate::{Acteur, ExigenceMdp, Tanana, horodater, nouvel_id};

pub const ROLE_ADMIN: &str = "role-admin";

// ─────────────────────────── Hachage ───────────────────────────

pub(crate) fn hacher(secret: &str) -> Resultat<String> {
    let sel = SaltString::encode_b64(&aleatoire::<16>()?).map_err(|e| Erreur::Base(format!("sel : {e}")))?;
    Argon2::default()
        .hash_password(secret.as_bytes(), &sel)
        .map(|h| h.to_string())
        .map_err(|e| Erreur::Base(format!("hachage : {e}")))
}

/// Comparaison de jetons en temps constant.
pub(crate) fn egaux_temps_constant(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub(crate) fn verifier_hash(secret: &str, hash: &str) -> bool {
    PasswordHash::new(hash).map(|h| Argon2::default().verify_password(secret.as_bytes(), &h).is_ok()).unwrap_or(false)
}

/// Hash factice : on fait le même travail quand l'identifiant n'existe pas,
/// pour ne pas révéler par le temps de réponse quels comptes existent.
fn hash_factice() -> &'static str {
    use std::sync::OnceLock;
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| hacher("tanana-factice-0").unwrap_or_default())
}

fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

fn aleatoire<const N: usize>() -> Resultat<[u8; N]> {
    let mut o = [0u8; N];
    getrandom::fill(&mut o).map_err(|e| Erreur::Base(format!("aléa : {e}")))?;
    Ok(o)
}

/// Code de récupération lisible : 4 groupes de 5 caractères, sans 0/O/1/I/L.
fn generer_code_recuperation() -> Resultat<String> {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let octets = aleatoire::<20>()?;
    let car: Vec<char> = octets.iter().map(|o| ALPHABET[(*o as usize) % ALPHABET.len()] as char).collect();
    Ok(car.chunks(5).map(|c| c.iter().collect::<String>()).collect::<Vec<_>>().join("-"))
}

fn normaliser_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_uppercase()).collect()
}

fn valider_identifiant(id: &str) -> Resultat<String> {
    let id = id.trim();
    let ok =
        (3..=32).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        return Err(Erreur::Validation("identifiant : 3 à 32 caractères (lettres, chiffres, . _ -)".into()));
    }
    Ok(id.to_string())
}

// ─────────────────────────── Paramètres ───────────────────────────

pub(crate) fn parametre(conn: &Connection, cle: &str) -> Resultat<Option<String>> {
    Ok(conn.query_row("SELECT valeur FROM parametres WHERE cle = ?1", [cle], |r| r.get(0)).optional()?)
}

pub(crate) fn parametre_nombre(conn: &Connection, cle: &str, defaut: i64) -> Resultat<i64> {
    Ok(parametre(conn, cle)?.and_then(|v| v.parse().ok()).unwrap_or(defaut))
}

pub(crate) fn definir_parametre(conn: &Connection, cle: &str, valeur: &str) -> Resultat<()> {
    conn.execute(
        "INSERT INTO parametres (cle, valeur) VALUES (?1, ?2)
         ON CONFLICT(cle) DO UPDATE SET valeur = excluded.valeur",
        params![cle, valeur],
    )?;
    Ok(())
}

// ─────────────────────────── Lecture ───────────────────────────

const SELECT_UTILISATEUR: &str = "SELECT u.id, u.identifiant, u.nom, u.role_id, r.nom, u.statut,
        u.doit_changer_mdp, u.pin_hash IS NOT NULL, u.cree_le, u.derniere_connexion
     FROM utilisateurs u JOIN roles r ON r.id = u.role_id";

fn ligne_utilisateur(r: &rusqlite::Row) -> rusqlite::Result<Utilisateur> {
    Ok(Utilisateur {
        id: r.get(0)?,
        identifiant: r.get(1)?,
        nom: r.get(2)?,
        role_id: r.get(3)?,
        role_nom: r.get(4)?,
        statut: r.get(5)?,
        doit_changer_mdp: r.get(6)?,
        a_un_pin: r.get(7)?,
        cree_le: r.get(8)?,
        derniere_connexion: r.get(9)?,
    })
}

pub(crate) fn utilisateur(conn: &Connection, id: &str) -> Resultat<Utilisateur> {
    conn.query_row(&format!("{SELECT_UTILISATEUR} WHERE u.id = ?1"), [id], ligne_utilisateur)
        .optional()?
        .ok_or_else(|| Erreur::Introuvable("utilisateur".into()))
}

/// Permissions effectives : celles du rôle, plus les ajouts, moins les retraits.
/// Un administrateur a toujours tout.
pub(crate) fn permissions_effectives(conn: &Connection, utilisateur_id: &str) -> Resultat<HashSet<Permission>> {
    let role_id: String =
        conn.query_row("SELECT role_id FROM utilisateurs WHERE id = ?1", [utilisateur_id], |r| r.get(0))?;
    if role_id == ROLE_ADMIN {
        return Ok(Permission::TOUTES.into_iter().collect());
    }
    let mut stmt = conn.prepare(
        "SELECT permission_code FROM role_permissions WHERE role_id = ?1
         UNION
         SELECT permission_code FROM permissions_perso WHERE utilisateur_id = ?2 AND accorde = 1
         EXCEPT
         SELECT permission_code FROM permissions_perso WHERE utilisateur_id = ?2 AND accorde = 0",
    )?;
    let codes =
        stmt.query_map(params![role_id, utilisateur_id], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
    Ok(codes.iter().filter_map(|c| Permission::depuis_code(c)).collect())
}

fn liste_triee(perms: &HashSet<Permission>) -> Vec<Permission> {
    Permission::TOUTES.into_iter().filter(|p| perms.contains(p)).collect()
}

fn admins_actifs_autres_que(conn: &Connection, exclu: &str) -> Resultat<i64> {
    Ok(conn.query_row(
        "SELECT count(*) FROM utilisateurs WHERE role_id = ?1 AND statut = 'actif' AND id <> ?2",
        params![ROLE_ADMIN, exclu],
        |r| r.get(0),
    )?)
}

fn fermer_sessions(conn: &Connection, utilisateur_id: &str, quand: &str) -> Resultat<()> {
    conn.execute(
        "UPDATE sessions SET fin = ?2 WHERE utilisateur_id = ?1 AND fin IS NULL",
        params![utilisateur_id, quand],
    )?;
    Ok(())
}

// ─────────────────────────── Sessions ───────────────────────────

/// Vérifie le jeton et rafraîchit l'activité. Appelée par `Tanana::executer`.
pub(crate) fn verifier_session(tx: &Transaction, token: &str, maintenant: DateTime<Utc>) -> Resultat<Acteur> {
    let ligne: Option<(String, String, String, String, StatutUtilisateur, bool, String)> = tx
        .query_row(
            "SELECT s.id, s.derniere_activite, u.id, u.nom, u.statut, u.doit_changer_mdp, u.role_id
             FROM sessions s JOIN utilisateurs u ON u.id = s.utilisateur_id
             WHERE s.token_hash = ?1 AND s.fin IS NULL",
            [sha256_hex(token)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?;
    let (session_id, derniere, uid, nom, statut, doit_changer, role_id) = ligne.ok_or(Erreur::NonAuthentifie)?;

    let maintenant_txt = horodater(maintenant);
    if statut != StatutUtilisateur::Actif {
        tx.execute("UPDATE sessions SET fin = ?2 WHERE id = ?1", params![session_id, maintenant_txt])?;
        return Err(Erreur::CompteInactif);
    }

    let delai = parametre_nombre(tx, "delai_inactivite_minutes", 15)?;
    let derniere =
        DateTime::parse_from_rfc3339(&derniere).map_err(|e| Erreur::Base(e.to_string()))?.with_timezone(&Utc);
    if maintenant - derniere > Duration::minutes(delai) {
        tx.execute("UPDATE sessions SET fin = ?2 WHERE id = ?1", params![session_id, maintenant_txt])?;
        return Err(Erreur::SessionExpiree);
    }

    tx.execute("UPDATE sessions SET derniere_activite = ?2 WHERE id = ?1", params![session_id, maintenant_txt])?;
    Ok(Acteur {
        permissions: permissions_effectives(tx, &uid)?,
        utilisateur_id: uid,
        nom,
        role_id,
        session_id,
        doit_changer_mdp: doit_changer,
        maintenant,
    })
}

/// id, hash mot de passe, hash PIN, statut, échecs, bloqué jusqu'à
type LigneConnexion = (String, String, Option<String>, StatutUtilisateur, i64, Option<String>);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Methode {
    MotDePasse,
    Pin,
    /// Mot de passe, depuis un autre ordinateur, par le relais (inscrit « à distance » au journal)
    Distance,
}

impl Tanana {
    // ───────────────────────── Premier lancement ─────────────────────────

    pub fn est_initialise(&self) -> Resultat<bool> {
        let n: i64 = self.conn_brute().query_row("SELECT count(*) FROM utilisateurs", [], |r| r.get(0))?;
        Ok(n > 0)
    }

    /// Crée le compte administrateur et retourne le **code de récupération**,
    /// affiché une seule fois. Sans internet, c'est le seul moyen de récupérer
    /// l'accès si le mot de passe admin est perdu.
    pub fn initialiser(&mut self, cfg: ConfigurationInitiale) -> Resultat<String> {
        let maintenant = horodater(self.maintenant());
        let tx = self.transaction()?;
        let n: i64 = tx.query_row("SELECT count(*) FROM utilisateurs", [], |r| r.get(0))?;
        if n > 0 {
            return Err(Erreur::DejaInitialise);
        }
        let nom_hotel = texte_requis(&cfg.nom_hotel, "le nom de l'hôtel")?;
        let devise = texte_requis(&cfg.devise, "la devise")?.to_uppercase();
        if devise.len() != 3 || !devise.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(Erreur::Validation("devise : code ISO à 3 lettres (ex. XAF, CAD, EUR)".into()));
        }
        if cfg.decimales_devise > 3 {
            return Err(Erreur::Validation("décimales de la devise : 0 à 3".into()));
        }
        let identifiant = valider_identifiant(&cfg.admin_identifiant)?;
        let nom = texte_requis(&cfg.admin_nom, "le nom")?;
        valider_mot_de_passe(&cfg.admin_mot_de_passe)?;

        let admin_id = nouvel_id();
        tx.execute(
            "INSERT INTO utilisateurs (id, identifiant, nom, mot_de_passe_hash, role_id, cree_le)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![admin_id, identifiant, nom, hacher(&cfg.admin_mot_de_passe)?, ROLE_ADMIN, maintenant],
        )?;
        crate::distance::acces::noter_acces(&tx, &admin_id, &cfg.admin_mot_de_passe)?;
        definir_parametre(&tx, "nom_hotel", &nom_hotel)?;
        definir_parametre(&tx, "devise", &devise)?;
        definir_parametre(&tx, "decimales_devise", &cfg.decimales_devise.to_string())?;

        let code = generer_code_recuperation()?;
        definir_parametre(&tx, "code_recuperation_hash", &hacher(&normaliser_code(&code))?)?;

        audit::ecrire(
            &tx,
            &maintenant,
            Some(&admin_id),
            "systeme.initialisation",
            Some("parametres"),
            None,
            None,
            Some(&json!({ "nom_hotel": nom_hotel, "devise": devise, "admin": identifiant })),
        )?;
        tx.commit()?;
        Ok(code)
    }

    /// Réinitialise le mot de passe d'un administrateur avec le code de
    /// récupération. Retourne un **nouveau** code (l'ancien ne marche plus).
    pub fn recuperer_acces_admin(
        &mut self,
        code: &str,
        identifiant_admin: &str,
        nouveau_mdp: &str,
    ) -> Resultat<String> {
        let maintenant = horodater(self.maintenant());
        let tx = self.transaction()?;
        let hash = parametre(&tx, "code_recuperation_hash")?.ok_or(Erreur::NonInitialise)?;
        if !verifier_hash(&normaliser_code(code), &hash) {
            audit::ecrire(&tx, &maintenant, None, "auth.recuperation_echec", None, None, None, None)?;
            tx.commit()?;
            return Err(Erreur::IdentifiantsInvalides);
        }
        valider_mot_de_passe(nouveau_mdp)?;
        let id: String = tx
            .query_row(
                "SELECT id FROM utilisateurs WHERE identifiant = ?1 AND role_id = ?2",
                params![identifiant_admin.trim(), ROLE_ADMIN],
                |r| r.get(0),
            )
            .optional()?
            .ok_or(Erreur::IdentifiantsInvalides)?;
        tx.execute(
            "UPDATE utilisateurs SET mot_de_passe_hash = ?2, statut = 'actif', doit_changer_mdp = 0,
                    echecs_connexion = 0, bloque_jusqu_a = NULL
             WHERE id = ?1",
            params![id, hacher(nouveau_mdp)?],
        )?;
        fermer_sessions(&tx, &id, &maintenant)?;
        crate::distance::acces::noter_acces(&tx, &id, nouveau_mdp)?;
        let nouveau_code = generer_code_recuperation()?;
        definir_parametre(&tx, "code_recuperation_hash", &hacher(&normaliser_code(&nouveau_code))?)?;
        audit::ecrire(&tx, &maintenant, Some(&id), "auth.recuperation", Some("utilisateur"), Some(&id), None, None)?;
        tx.commit()?;
        Ok(nouveau_code)
    }

    // ───────────────────────────── Connexion ─────────────────────────────

    pub fn connexion(&mut self, identifiant: &str, mot_de_passe: &str) -> Resultat<SessionOuverte> {
        self.tenter_connexion(identifiant, mot_de_passe, Methode::MotDePasse)
    }

    /// Changement rapide d'utilisateur au comptoir.
    pub fn connexion_pin(&mut self, identifiant: &str, pin: &str) -> Resultat<SessionOuverte> {
        self.tenter_connexion(identifiant, pin, Methode::Pin)
    }

    /// Connexion demandée depuis un autre ordinateur, par le relais : mêmes
    /// règles (blocage après plusieurs échecs), inscrite « à distance ».
    pub(crate) fn connexion_distante(&mut self, identifiant: &str, mot_de_passe: &str) -> Resultat<SessionOuverte> {
        self.tenter_connexion(identifiant, mot_de_passe, Methode::Distance)
    }

    fn tenter_connexion(&mut self, identifiant: &str, secret: &str, methode: Methode) -> Resultat<SessionOuverte> {
        let maintenant = self.maintenant();
        let horodatage = horodater(maintenant);
        let tx = self.transaction()?;
        let resultat = Self::connexion_dans(&tx, identifiant.trim(), secret, methode, maintenant, &horodatage);
        // On valide même en cas d'échec : compteur d'échecs et journal doivent rester.
        tx.commit()?;
        resultat
    }

    fn connexion_dans(
        tx: &Transaction,
        identifiant: &str,
        secret: &str,
        methode: Methode,
        maintenant: DateTime<Utc>,
        horodatage: &str,
    ) -> Resultat<SessionOuverte> {
        let nom_methode = match methode {
            Methode::MotDePasse => "mot_de_passe",
            Methode::Pin => "pin",
            Methode::Distance => "distance",
        };
        let ligne: Option<LigneConnexion> = tx
            .query_row(
                "SELECT id, mot_de_passe_hash, pin_hash, statut, echecs_connexion, bloque_jusqu_a
                 FROM utilisateurs WHERE identifiant = ?1",
                [identifiant],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .optional()?;

        let Some((uid, mdp_hash, pin_hash, statut, echecs, bloque)) = ligne else {
            let _ = verifier_hash(secret, hash_factice());
            audit::ecrire(
                tx,
                horodatage,
                None,
                "auth.echec",
                None,
                None,
                None,
                Some(&json!({ "identifiant": identifiant, "raison": "inconnu", "methode": nom_methode })),
            )?;
            return Err(Erreur::IdentifiantsInvalides);
        };

        if let Some(b) = bloque.as_deref() {
            let fin = DateTime::parse_from_rfc3339(b).map_err(|e| Erreur::Base(e.to_string()))?;
            if fin > maintenant {
                audit::ecrire(
                    tx,
                    horodatage,
                    Some(&uid),
                    "auth.echec",
                    Some("utilisateur"),
                    Some(&uid),
                    None,
                    Some(&json!({ "raison": "bloque", "methode": nom_methode })),
                )?;
                return Err(Erreur::CompteBloque { jusqu_a: b.to_string() });
            }
        }

        let hash = match methode {
            Methode::MotDePasse | Methode::Distance => Some(mdp_hash),
            Methode::Pin => pin_hash,
        };
        let valide = match &hash {
            Some(h) => verifier_hash(secret, h),
            None => {
                let _ = verifier_hash(secret, hash_factice());
                false
            }
        };

        if !valide {
            let max = parametre_nombre(tx, "max_echecs_connexion", 5)?;
            let echecs = echecs + 1;
            if echecs >= max {
                let duree = parametre_nombre(tx, "duree_blocage_minutes", 15)?;
                let jusqu_a = horodater(maintenant + Duration::minutes(duree));
                tx.execute(
                    "UPDATE utilisateurs SET echecs_connexion = 0, bloque_jusqu_a = ?2 WHERE id = ?1",
                    params![uid, jusqu_a],
                )?;
                audit::ecrire(
                    tx,
                    horodatage,
                    Some(&uid),
                    "auth.blocage",
                    Some("utilisateur"),
                    Some(&uid),
                    None,
                    Some(&json!({ "jusqu_a": jusqu_a, "methode": nom_methode })),
                )?;
            } else {
                tx.execute("UPDATE utilisateurs SET echecs_connexion = ?2 WHERE id = ?1", params![uid, echecs])?;
            }
            audit::ecrire(
                tx,
                horodatage,
                Some(&uid),
                "auth.echec",
                Some("utilisateur"),
                Some(&uid),
                None,
                Some(&json!({ "raison": "secret_invalide", "methode": nom_methode })),
            )?;
            return Err(Erreur::IdentifiantsInvalides);
        }

        // Le statut est vérifié APRÈS le secret : un inconnu ne peut pas savoir
        // si un compte existe et est suspendu.
        if statut != StatutUtilisateur::Actif {
            audit::ecrire(
                tx,
                horodatage,
                Some(&uid),
                "auth.echec",
                Some("utilisateur"),
                Some(&uid),
                None,
                Some(&json!({ "raison": "compte_inactif", "methode": nom_methode })),
            )?;
            return Err(Erreur::CompteInactif);
        }

        tx.execute(
            "UPDATE utilisateurs SET echecs_connexion = 0, bloque_jusqu_a = NULL, derniere_connexion = ?2
             WHERE id = ?1",
            params![uid, horodatage],
        )?;
        let token = hex::encode(aleatoire::<32>()?);
        tx.execute(
            "INSERT INTO sessions (id, utilisateur_id, token_hash, debut, derniere_activite)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            params![nouvel_id(), uid, sha256_hex(&token), horodatage],
        )?;
        audit::ecrire(
            tx,
            horodatage,
            Some(&uid),
            "auth.connexion",
            Some("utilisateur"),
            Some(&uid),
            None,
            Some(&json!({ "methode": nom_methode })),
        )?;

        if methode != Methode::Pin {
            crate::distance::acces::noter_acces(tx, &uid, secret)?;
        }
        let perms = permissions_effectives(tx, &uid)?;
        Ok(SessionOuverte { token, utilisateur: utilisateur(tx, &uid)?, permissions: liste_triee(&perms) })
    }

    pub fn deconnexion(&mut self, token: &str) -> Resultat<()> {
        if self.est_une_copie() {
            self.deconnexion_copie();
            return Ok(());
        }
        self.executer_avec(token, None, ExigenceMdp::Ignorer, |tx, a| {
            tx.execute("UPDATE sessions SET fin = ?2 WHERE id = ?1", params![a.session_id, a.horodatage()])?;
            audit::ecrire(tx, &a.horodatage(), Some(&a.utilisateur_id), "auth.deconnexion", None, None, None, None)
        })
    }

    /// Profil et permissions de la session courante (rafraîchit l'activité).
    pub fn ma_session(&mut self, token: &str) -> Resultat<SessionOuverte> {
        if self.est_une_copie() {
            return self.ma_session_copie(token);
        }
        self.executer_avec(token, None, ExigenceMdp::Ignorer, |tx, a| {
            Ok(SessionOuverte {
                token: token.to_string(),
                utilisateur: utilisateur(tx, &a.utilisateur_id)?,
                permissions: liste_triee(&a.permissions),
            })
        })
    }

    pub fn changer_mon_mot_de_passe(&mut self, token: &str, actuel: &str, nouveau: &str) -> Resultat<()> {
        self.executer_avec(token, None, ExigenceMdp::Ignorer, |tx, a| {
            let hash: String =
                tx.query_row("SELECT mot_de_passe_hash FROM utilisateurs WHERE id = ?1", [&a.utilisateur_id], |r| {
                    r.get(0)
                })?;
            if !verifier_hash(actuel, &hash) {
                return Err(Erreur::IdentifiantsInvalides);
            }
            valider_mot_de_passe(nouveau)?;
            if actuel == nouveau {
                return Err(Erreur::Validation("le nouveau mot de passe doit être différent".into()));
            }
            tx.execute(
                "UPDATE utilisateurs SET mot_de_passe_hash = ?2, doit_changer_mdp = 0 WHERE id = ?1",
                params![a.utilisateur_id, hacher(nouveau)?],
            )?;
            crate::distance::acces::noter_acces(tx, &a.utilisateur_id, nouveau)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "auth.changement_mdp",
                Some("utilisateur"),
                Some(&a.utilisateur_id),
                None,
                None,
            )
        })
    }

    pub fn definir_mon_pin(&mut self, token: &str, mot_de_passe: &str, pin: &str) -> Resultat<()> {
        self.executer(token, None, |tx, a| {
            let hash: String =
                tx.query_row("SELECT mot_de_passe_hash FROM utilisateurs WHERE id = ?1", [&a.utilisateur_id], |r| {
                    r.get(0)
                })?;
            if !verifier_hash(mot_de_passe, &hash) {
                return Err(Erreur::IdentifiantsInvalides);
            }
            valider_pin(pin)?;
            tx.execute("UPDATE utilisateurs SET pin_hash = ?2 WHERE id = ?1", params![a.utilisateur_id, hacher(pin)?])?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "auth.definition_pin",
                Some("utilisateur"),
                Some(&a.utilisateur_id),
                None,
                None,
            )
        })
    }

    // ─────────────────────── Administration des comptes ───────────────────────

    pub fn lister_utilisateurs(&mut self, token: &str) -> Resultat<Vec<Utilisateur>> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, _| {
            let mut stmt = tx.prepare(&format!("{SELECT_UTILISATEUR} ORDER BY u.nom COLLATE NOCASE"))?;
            let v = stmt.query_map([], ligne_utilisateur)?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    pub fn creer_utilisateur(&mut self, token: &str, donnees: NouvelUtilisateur) -> Resultat<Utilisateur> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            let identifiant = valider_identifiant(&donnees.identifiant)?;
            let nom = texte_requis(&donnees.nom, "le nom")?;
            valider_mot_de_passe(&donnees.mot_de_passe_temporaire)?;
            role_existe(tx, &donnees.role_id)?;
            let id = nouvel_id();
            tx.execute(
                "INSERT INTO utilisateurs (id, identifiant, nom, mot_de_passe_hash, role_id, doit_changer_mdp, cree_le)
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)",
                params![
                    id,
                    identifiant,
                    nom,
                    hacher(&donnees.mot_de_passe_temporaire)?,
                    donnees.role_id,
                    a.horodatage()
                ],
            )
            .map_err(|e| match Erreur::from(e) {
                Erreur::Doublon(_) => Erreur::Doublon(format!("identifiant « {identifiant} »")),
                autre => autre,
            })?;
            let u = utilisateur(tx, &id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "utilisateur.creation",
                Some("utilisateur"),
                Some(&id),
                None,
                Some(&en_json(&u)?),
            )?;
            Ok(u)
        })
    }

    pub fn changer_role(&mut self, token: &str, utilisateur_id: &str, role_id: &str) -> Resultat<Utilisateur> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            if utilisateur_id == a.utilisateur_id {
                return Err(Erreur::ActionSurSoiMeme);
            }
            role_existe(tx, role_id)?;
            let avant = utilisateur(tx, utilisateur_id)?;
            if avant.role_id == ROLE_ADMIN
                && role_id != ROLE_ADMIN
                && admins_actifs_autres_que(tx, utilisateur_id)? == 0
            {
                return Err(Erreur::DernierAdministrateur);
            }
            tx.execute("UPDATE utilisateurs SET role_id = ?2 WHERE id = ?1", params![utilisateur_id, role_id])?;
            let apres = utilisateur(tx, utilisateur_id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "utilisateur.changement_role",
                Some("utilisateur"),
                Some(utilisateur_id),
                Some(&json!({ "role": avant.role_nom })),
                Some(&json!({ "role": apres.role_nom })),
            )?;
            Ok(apres)
        })
    }

    /// Suspendre, désactiver ou réactiver un compte. Toute suspension ferme
    /// immédiatement les sessions ouvertes du compte.
    pub fn changer_statut_utilisateur(
        &mut self,
        token: &str,
        utilisateur_id: &str,
        statut: StatutUtilisateur,
    ) -> Resultat<Utilisateur> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            if utilisateur_id == a.utilisateur_id {
                return Err(Erreur::ActionSurSoiMeme);
            }
            let avant = utilisateur(tx, utilisateur_id)?;
            if avant.role_id == ROLE_ADMIN
                && statut != StatutUtilisateur::Actif
                && admins_actifs_autres_que(tx, utilisateur_id)? == 0
            {
                return Err(Erreur::DernierAdministrateur);
            }
            tx.execute(
                "UPDATE utilisateurs SET statut = ?2, echecs_connexion = 0, bloque_jusqu_a = NULL WHERE id = ?1",
                params![utilisateur_id, statut],
            )?;
            if statut != StatutUtilisateur::Actif {
                fermer_sessions(tx, utilisateur_id, &a.horodatage())?;
            }
            let action = match statut {
                StatutUtilisateur::Actif => "utilisateur.reactivation",
                StatutUtilisateur::Suspendu => "utilisateur.suspension",
                StatutUtilisateur::Desactive => "utilisateur.desactivation",
            };
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                action,
                Some("utilisateur"),
                Some(utilisateur_id),
                Some(&json!({ "statut": avant.statut })),
                Some(&json!({ "statut": statut })),
            )?;
            utilisateur(tx, utilisateur_id)
        })
    }

    pub fn reinitialiser_mot_de_passe(
        &mut self,
        token: &str,
        utilisateur_id: &str,
        mot_de_passe_temporaire: &str,
    ) -> Resultat<()> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            if utilisateur_id == a.utilisateur_id {
                return Err(Erreur::ActionSurSoiMeme);
            }
            utilisateur(tx, utilisateur_id)?;
            valider_mot_de_passe(mot_de_passe_temporaire)?;
            tx.execute(
                "UPDATE utilisateurs SET mot_de_passe_hash = ?2, doit_changer_mdp = 1, pin_hash = NULL,
                        echecs_connexion = 0, bloque_jusqu_a = NULL
                 WHERE id = ?1",
                params![utilisateur_id, hacher(mot_de_passe_temporaire)?],
            )?;
            fermer_sessions(tx, utilisateur_id, &a.horodatage())?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "utilisateur.reinitialisation_mdp",
                Some("utilisateur"),
                Some(utilisateur_id),
                None,
                None,
            )
        })
    }

    /// Ajoute (`Some(true)`), retire (`Some(false)`) une permission à une
    /// personne précise, ou revient au rôle (`None`).
    pub fn definir_permission_perso(
        &mut self,
        token: &str,
        utilisateur_id: &str,
        permission: Permission,
        accorde: Option<bool>,
    ) -> Resultat<Vec<PermissionPerso>> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            if utilisateur_id == a.utilisateur_id {
                return Err(Erreur::ActionSurSoiMeme);
            }
            let cible = utilisateur(tx, utilisateur_id)?;
            if cible.role_id == ROLE_ADMIN {
                return Err(Erreur::Validation("un administrateur a toujours toutes les permissions".into()));
            }
            let avant = lire_permissions_perso(tx, utilisateur_id)?;
            match accorde {
                Some(v) => tx.execute(
                    "INSERT INTO permissions_perso (utilisateur_id, permission_code, accorde) VALUES (?1, ?2, ?3)
                     ON CONFLICT(utilisateur_id, permission_code) DO UPDATE SET accorde = excluded.accorde",
                    params![utilisateur_id, permission.code(), v],
                )?,
                None => tx.execute(
                    "DELETE FROM permissions_perso WHERE utilisateur_id = ?1 AND permission_code = ?2",
                    params![utilisateur_id, permission.code()],
                )?,
            };
            let apres = lire_permissions_perso(tx, utilisateur_id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "utilisateur.permission_perso",
                Some("utilisateur"),
                Some(utilisateur_id),
                Some(&en_json(&avant)?),
                Some(&en_json(&apres)?),
            )?;
            Ok(apres)
        })
    }

    pub fn permissions_perso(&mut self, token: &str, utilisateur_id: &str) -> Resultat<Vec<PermissionPerso>> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, _| lire_permissions_perso(tx, utilisateur_id))
    }

    pub fn permissions_utilisateur(&mut self, token: &str, utilisateur_id: &str) -> Resultat<Vec<Permission>> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, _| {
            utilisateur(tx, utilisateur_id)?;
            Ok(liste_triee(&permissions_effectives(tx, utilisateur_id)?))
        })
    }

    // ───────────────────────────── Rôles ─────────────────────────────

    pub fn lister_roles(&mut self, token: &str) -> Resultat<Vec<Role>> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, _| lire_roles(tx))
    }

    pub fn creer_role(&mut self, token: &str, nom: &str, permissions: Vec<Permission>) -> Resultat<Role> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            let nom = texte_requis(nom, "le nom du rôle")?;
            let id = nouvel_id();
            tx.execute("INSERT INTO roles (id, nom) VALUES (?1, ?2)", params![id, nom]).map_err(
                |e| match Erreur::from(e) {
                    Erreur::Doublon(_) => Erreur::Doublon(format!("rôle « {nom} »")),
                    autre => autre,
                },
            )?;
            ecrire_permissions_role(tx, &id, &permissions)?;
            let role = lire_roles(tx)?.into_iter().find(|r| r.id == id).ok_or(Erreur::Introuvable("rôle".into()))?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "role.creation",
                Some("role"),
                Some(&id),
                None,
                Some(&en_json(&role)?),
            )?;
            Ok(role)
        })
    }

    pub fn modifier_permissions_role(
        &mut self,
        token: &str,
        role_id: &str,
        permissions: Vec<Permission>,
    ) -> Resultat<Role> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            let roles = lire_roles(tx)?;
            let avant = roles.into_iter().find(|r| r.id == role_id).ok_or(Erreur::Introuvable("rôle".into()))?;
            if avant.systeme {
                return Err(Erreur::Validation("le rôle Administrateur ne peut pas être modifié".into()));
            }
            tx.execute("DELETE FROM role_permissions WHERE role_id = ?1", [role_id])?;
            ecrire_permissions_role(tx, role_id, &permissions)?;
            let apres =
                lire_roles(tx)?.into_iter().find(|r| r.id == role_id).ok_or(Erreur::Introuvable("rôle".into()))?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "role.modification",
                Some("role"),
                Some(role_id),
                Some(&en_json(&avant.permissions)?),
                Some(&en_json(&apres.permissions)?),
            )?;
            Ok(apres)
        })
    }

    // ───────────────────────────── Sessions ─────────────────────────────

    /// Qui est connecté en ce moment (sessions ouvertes et non expirées).
    pub fn sessions_actives(&mut self, token: &str) -> Resultat<Vec<SessionActive>> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            let delai = parametre_nombre(tx, "delai_inactivite_minutes", 15)?;
            let seuil = horodater(a.maintenant - Duration::minutes(delai));
            let mut stmt = tx.prepare(
                "SELECT s.id, u.id, u.nom, s.debut, s.derniere_activite
                 FROM sessions s JOIN utilisateurs u ON u.id = s.utilisateur_id
                 WHERE s.fin IS NULL AND s.derniere_activite >= ?1
                 ORDER BY s.derniere_activite DESC",
            )?;
            let v = stmt
                .query_map([seuil], |r| {
                    Ok(SessionActive {
                        session_id: r.get(0)?,
                        utilisateur_id: r.get(1)?,
                        utilisateur_nom: r.get(2)?,
                        debut: r.get(3)?,
                        derniere_activite: r.get(4)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    /// Déconnecte de force une session (ex. un poste resté ouvert).
    pub fn fermer_session(&mut self, token: &str, session_id: &str) -> Resultat<()> {
        self.executer(token, Some(Permission::UtilisateurGerer), |tx, a| {
            let n = tx.execute(
                "UPDATE sessions SET fin = ?2 WHERE id = ?1 AND fin IS NULL",
                params![session_id, a.horodatage()],
            )?;
            if n == 0 {
                return Err(Erreur::Introuvable("session ouverte".into()));
            }
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "utilisateur.fermeture_session",
                Some("session"),
                Some(session_id),
                None,
                None,
            )
        })
    }
}

fn role_existe(conn: &Connection, role_id: &str) -> Resultat<()> {
    let n: i64 = conn.query_row("SELECT count(*) FROM roles WHERE id = ?1", [role_id], |r| r.get(0))?;
    if n == 0 {
        return Err(Erreur::Introuvable("rôle".into()));
    }
    Ok(())
}

fn ecrire_permissions_role(conn: &Connection, role_id: &str, permissions: &[Permission]) -> Resultat<()> {
    for p in permissions.iter().collect::<HashSet<_>>() {
        conn.execute(
            "INSERT INTO role_permissions (role_id, permission_code) VALUES (?1, ?2)",
            params![role_id, p.code()],
        )?;
    }
    Ok(())
}

fn lire_roles(conn: &Connection) -> Resultat<Vec<Role>> {
    let mut stmt = conn.prepare("SELECT id, nom, systeme FROM roles ORDER BY systeme DESC, nom")?;
    let roles = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, bool>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stmt = conn.prepare("SELECT permission_code FROM role_permissions WHERE role_id = ?1")?;
    roles
        .into_iter()
        .map(|(id, nom, systeme)| {
            let codes: HashSet<Permission> = stmt
                .query_map([&id], |r| r.get::<_, String>(0))?
                .filter_map(|c| c.ok().and_then(|c| Permission::depuis_code(&c)))
                .collect();
            Ok(Role { id, nom, systeme, permissions: liste_triee(&codes) })
        })
        .collect()
}

fn lire_permissions_perso(conn: &Connection, utilisateur_id: &str) -> Resultat<Vec<PermissionPerso>> {
    let mut stmt = conn.prepare(
        "SELECT permission_code, accorde FROM permissions_perso WHERE utilisateur_id = ?1 ORDER BY permission_code",
    )?;
    let v = stmt
        .query_map([utilisateur_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?
        .filter_map(|r| r.ok())
        .filter_map(|(c, accorde)| {
            Permission::depuis_code(&c).map(|permission| PermissionPerso { permission, accorde })
        })
        .collect();
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hachage_argon2() {
        let h = hacher("secret123").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verifier_hash("secret123", &h));
        assert!(!verifier_hash("secret124", &h));
        assert!(!verifier_hash("secret123", "pas-un-hash"));
        // Deux hachages du même secret diffèrent (sel aléatoire).
        assert_ne!(h, hacher("secret123").unwrap());
    }

    #[test]
    fn code_de_recuperation() {
        let c = generer_code_recuperation().unwrap();
        assert_eq!(c.len(), 23);
        assert_eq!(c.split('-').count(), 4);
        assert!(!c.contains(['0', 'O', '1', 'I', 'L']));
        assert_eq!(normaliser_code(&c.to_lowercase().replace('-', " ")), c.replace('-', ""));
        assert_ne!(c, generer_code_recuperation().unwrap());
    }

    #[test]
    fn identifiants() {
        assert_eq!(valider_identifiant(" marie.k ").unwrap(), "marie.k");
        assert!(valider_identifiant("ab").is_err());
        assert!(valider_identifiant("avec espace").is_err());
        assert!(valider_identifiant("élodie").is_err());
        assert!(valider_identifiant(&"a".repeat(33)).is_err());
    }
}
