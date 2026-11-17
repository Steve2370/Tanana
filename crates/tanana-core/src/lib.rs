//! # tanana-core
//!
//! Tout le métier de Tanana, sans dépendance à Tauri ni à l'interface :
//! base de données, comptes, permissions, journal d'audit, réservations, argent.
//! L'application Tauri n'est qu'une fine couche qui appelle ces fonctions.
//!
//! Chaque action passe par [`Tanana::executer`] :
//! **vérifier la session → vérifier la permission → agir → écrire le journal → commit**.
//! Si une étape échoue, rien n'est écrit (sauf la trace d'un accès refusé).

pub mod audit;
pub mod auth;
pub mod db;
pub mod distance;
pub mod domaine;
pub mod erreur;
pub mod francais;
pub mod modeles;
pub mod pdf;
pub mod permissions;
pub mod restauration;
pub mod services;

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, Transaction, TransactionBehavior};

pub use db::CleBase;
pub use erreur::{Erreur, Resultat};
pub use permissions::Permission;

/// Source de l'heure courante. Remplaçable dans les tests pour simuler
/// l'inactivité, les blocages de compte, etc.
pub type Horloge = Arc<dyn Fn() -> DateTime<Utc> + Send + Sync>;

/// Qui fait l'action en cours.
#[derive(Debug, Clone)]
pub struct Acteur {
    pub utilisateur_id: String,
    pub nom: String,
    pub role_id: String,
    pub session_id: String,
    pub permissions: HashSet<Permission>,
    pub doit_changer_mdp: bool,
    /// Heure de l'action, identique pour toutes les écritures d'une même transaction.
    pub maintenant: DateTime<Utc>,
}

impl Acteur {
    pub fn horodatage(&self) -> String {
        horodater(self.maintenant)
    }
}

pub fn horodater(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn nouvel_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Point d'entrée unique du cœur de l'application.
pub struct Tanana {
    conn: Connection,
    horloge: Horloge,
    cle: Option<CleBase>,
    /// Copie d'un hôtel consulté à distance : lecture seule.
    copie: bool,
    /// Session de consultation (en mémoire : la copie est en lecture seule).
    session_copie: distance::SessionCopie,
}

/// Exigence de changement de mot de passe : bloque toutes les actions sauf
/// celles qui servent justement à changer le mot de passe.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ExigenceMdp {
    Appliquer,
    Ignorer,
}

impl Tanana {
    pub fn ouvrir(chemin: &Path, cle: Option<CleBase>) -> Resultat<Self> {
        let conn = db::ouvrir(chemin, cle.as_ref())?;
        distance::apres_ouverture(&conn)?;
        Ok(Self { conn, horloge: Arc::new(Utc::now), cle, copie: false, session_copie: Default::default() })
    }

    pub fn en_memoire() -> Resultat<Self> {
        Ok(Self {
            conn: db::ouvrir_en_memoire()?,
            horloge: Arc::new(Utc::now),
            cle: None,
            copie: false,
            session_copie: Default::default(),
        })
    }

    /// Vrai pour la copie d'un hôtel consulté à distance (lecture seule).
    pub fn est_une_copie(&self) -> bool {
        self.copie
    }

    pub fn avec_horloge(mut self, horloge: Horloge) -> Self {
        self.horloge = horloge;
        self
    }

    pub fn maintenant(&self) -> DateTime<Utc> {
        (self.horloge)()
    }

    /// Accès direct à la connexion : réservé aux tests et aux outils internes.
    #[doc(hidden)]
    pub fn conn_brute(&self) -> &Connection {
        &self.conn
    }

    pub(crate) fn transaction(&mut self) -> Resultat<Transaction<'_>> {
        Ok(self.conn.transaction_with_behavior(TransactionBehavior::Immediate)?)
    }

    /// Le motif central : vérifier → agir → logger → commit.
    pub(crate) fn executer<T>(
        &mut self,
        token: &str,
        permission: Option<Permission>,
        f: impl FnOnce(&Transaction, &Acteur) -> Resultat<T>,
    ) -> Resultat<T> {
        self.executer_avec(token, permission, ExigenceMdp::Appliquer, f)
    }

    pub(crate) fn executer_avec<T>(
        &mut self,
        token: &str,
        permission: Option<Permission>,
        exigence: ExigenceMdp,
        f: impl FnOnce(&Transaction, &Acteur) -> Resultat<T>,
    ) -> Resultat<T> {
        let maintenant = self.maintenant();
        if self.copie {
            return self.executer_sur_copie(token, permission, maintenant, f);
        }
        let tx = self.transaction()?;

        let acteur = match auth::verifier_session(&tx, token, maintenant) {
            Ok(a) => a,
            Err(e @ Erreur::SessionExpiree) => {
                tx.commit()?; // garde la fermeture de la session expirée
                return Err(e);
            }
            Err(e) => return Err(e),
        };

        if exigence == ExigenceMdp::Appliquer && acteur.doit_changer_mdp {
            return Err(Erreur::ChangementMdpRequis);
        }

        if let Some(p) = permission
            && !acteur.permissions.contains(&p)
        {
            audit::ecrire(
                &tx,
                &acteur.horodatage(),
                Some(&acteur.utilisateur_id),
                "acces.refuse",
                None,
                None,
                None,
                Some(&serde_json::json!({ "permission": p.code() })),
            )?;
            tx.commit()?;
            return Err(Erreur::PermissionRefusee { permission: p.code().to_string() });
        }

        let resultat = f(&tx, &acteur)?;
        tx.commit()?;
        Ok(resultat)
    }

    /// Sur une copie : session en mémoire, pas de journal ; l'administrateur
    /// à distance peut tout consulter, et la base refuse toute écriture.
    fn executer_sur_copie<T>(
        &mut self,
        token: &str,
        permission: Option<Permission>,
        maintenant: DateTime<Utc>,
        f: impl FnOnce(&Transaction, &Acteur) -> Resultat<T>,
    ) -> Resultat<T> {
        self.verifier_session_copie(token, maintenant)?;
        let acteur = distance::acteur_distant(maintenant);
        if let Some(p) = permission
            && !acteur.permissions.contains(&p)
        {
            return Err(Erreur::LectureSeule);
        }
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let resultat = f(&tx, &acteur).map_err(|e| match e {
            Erreur::Base(m) if m.contains("readonly") || m.contains("read-only") => Erreur::LectureSeule,
            e => e,
        })?;
        tx.commit()?;
        Ok(resultat)
    }
}
