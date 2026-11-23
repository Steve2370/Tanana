//! Accès à distance par identifiant et mot de passe.
//!
//! Pour chaque administrateur, l'ordinateur principal dépose sur le relais un
//! **coffre** : l'identifiant de l'hôtel et la clé secrète, chiffrés avec une
//! clé tirée de son mot de passe. Sur un autre ordinateur, l'identifiant et le
//! mot de passe suffisent pour rouvrir ce coffre : plus de clé à noter.
//!
//! Le relais ne voit ni l'identifiant (seulement son empreinte), ni le mot de
//! passe, ni le contenu du coffre. Il garde l'empreinte d'une « preuve » tirée
//! du mot de passe, pour refuser les mauvais mots de passe et freiner les essais.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tanana_protocole::{CoffreAcces, PreuveAcces};

use super::chiffrement::{ClesMotDePasse, Secret, associe_coffre, recherche_compte};
use super::relais::Relais;
use crate::Tanana;
use crate::auth::permissions_effectives;
use crate::erreur::{Erreur, Resultat};
use crate::permissions::Permission;

/// Ce que contient un coffre (une fois ouvert).
#[derive(Debug, Serialize, Deserialize)]
struct ContenuCoffre {
    hotel_id: String,
    secret: String,
}

fn active(conn: &Connection) -> Resultat<bool> {
    Ok(conn.query_row("SELECT count(*) FROM sync_config", [], |r| r.get::<_, i64>(0))? > 0)
}

fn aleatoire_16() -> Resultat<[u8; 16]> {
    let mut o = [0u8; 16];
    getrandom::fill(&mut o).map_err(|e| Erreur::Base(format!("hasard indisponible : {e}")))?;
    Ok(o)
}

/// Appelé chaque fois que le mot de passe d'un utilisateur est connu en clair
/// (connexion, création de l'hôtel, changement ou récupération du mot de passe).
/// Pour un administrateur, prépare les clés de son coffre. Rien à faire si
/// elles correspondent déjà à ce mot de passe (cas habituel : coût nul).
pub(crate) fn noter_acces(conn: &Connection, utilisateur_id: &str, mot_de_passe: &str) -> Resultat<()> {
    if !permissions_effectives(conn, utilisateur_id)?.contains(&Permission::ParametresGerer) {
        return Ok(());
    }
    let (identifiant, mdp_hash): (String, String) = conn.query_row(
        "SELECT identifiant, mot_de_passe_hash FROM utilisateurs WHERE id = ?1",
        [utilisateur_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let recherche = recherche_compte(&identifiant);
    let existant: Option<(String, String)> = conn
        .query_row("SELECT recherche, mdp_hash FROM sync_acces WHERE utilisateur_id = ?1", [utilisateur_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    if let Some((r, h)) = &existant {
        if *r == recherche && *h == mdp_hash {
            return Ok(()); // même compte, même mot de passe
        }
        if *r != recherche {
            // Identifiant changé : l'ancien coffre doit disparaître du relais
            conn.execute("INSERT OR IGNORE INTO sync_acces_retraits (recherche) VALUES (?1)", [r])?;
        }
    }
    let sel = aleatoire_16()?;
    let cles = ClesMotDePasse::deriver(mot_de_passe, &sel)?;
    conn.execute(
        "INSERT INTO sync_acces (utilisateur_id, recherche, sel, verif, cle_coffre, nonce, chiffre, mdp_hash, a_envoyer)
         VALUES (?1, ?2, ?3, ?4, ?5, '', '', ?6, 1)
         ON CONFLICT (utilisateur_id) DO UPDATE SET recherche = excluded.recherche, sel = excluded.sel,
             verif = excluded.verif, cle_coffre = excluded.cle_coffre, nonce = '', chiffre = '',
             mdp_hash = excluded.mdp_hash, a_envoyer = 1",
        params![
            utilisateur_id,
            recherche,
            data_encoding::BASE64.encode(&sel),
            cles.verif(),
            cles.cle_coffre_hex(),
            mdp_hash
        ],
    )?;
    Ok(())
}

/// (utilisateur, recherche, empreinte du mot de passe au dépôt, empreinte actuelle, statut)
type LigneAcces = (String, String, String, Option<String>, Option<String>);

/// Coffres à déposer ou à retirer du relais.
#[derive(Debug, Clone)]
pub struct EnvoiAcces {
    pub relais: String,
    pub hotel_id: String,
    pub jeton_envoi: String,
    /// (utilisateur, recherche, coffre)
    pub depots: Vec<(String, String, CoffreAcces)>,
    pub retraits: Vec<String>,
}

impl Tanana {
    /// Prépare les coffres à déposer et à retirer. Action du système, sans session.
    /// Retire d'abord les coffres qui ne doivent plus servir : compte inactif,
    /// plus administrateur, ou mot de passe changé par quelqu'un d'autre.
    pub fn preparer_acces(&mut self) -> Resultat<Option<EnvoiAcces>> {
        let tx = self.transaction()?;
        if !active(&tx)? {
            return Ok(None);
        }
        let (relais, hotel_id, jeton_envoi, secret, remplace): (String, String, String, String, Option<String>) = tx
            .query_row(
                "SELECT relais, hotel_id, jeton_envoi, secret, remplace_le FROM sync_config WHERE id = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )?;
        if remplace.is_some() {
            return Ok(None);
        }
        let lignes: Vec<LigneAcces> = tx
            .prepare(
                "SELECT a.utilisateur_id, a.recherche, a.mdp_hash, u.mot_de_passe_hash, u.statut
                 FROM sync_acces a LEFT JOIN utilisateurs u ON u.id = a.utilisateur_id",
            )?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<Result<_, _>>()?;
        for (uid, recherche, mdp_hash, actuel, statut) in lignes {
            let valable = actuel.as_deref() == Some(mdp_hash.as_str())
                && statut.as_deref() == Some("actif")
                && permissions_effectives(&tx, &uid)?.contains(&Permission::ParametresGerer);
            if !valable {
                tx.execute("DELETE FROM sync_acces WHERE utilisateur_id = ?1", [&uid])?;
                tx.execute("INSERT OR IGNORE INTO sync_acces_retraits (recherche) VALUES (?1)", [&recherche])?;
            }
        }
        // Fermer les coffres pas encore fermés (ou à refermer après un changement d'hôtel)
        let contenu = serde_json::to_vec(&ContenuCoffre { hotel_id: hotel_id.clone(), secret: secret.clone() })?;
        let a_fermer: Vec<(String, String, String)> = tx
            .prepare("SELECT utilisateur_id, recherche, cle_coffre FROM sync_acces WHERE a_envoyer = 1")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        for (uid, recherche, cle) in a_fermer {
            let (nonce, chiffre) =
                ClesMotDePasse::depuis_hex("", &cle)?.fermer(&contenu, &associe_coffre(&recherche))?;
            tx.execute(
                "UPDATE sync_acces SET nonce = ?2, chiffre = ?3 WHERE utilisateur_id = ?1",
                params![uid, nonce, chiffre],
            )?;
        }
        let depots: Vec<(String, String, CoffreAcces)> = tx
            .prepare(
                "SELECT utilisateur_id, recherche, sel, verif, nonce, chiffre FROM sync_acces WHERE a_envoyer = 1",
            )?
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    CoffreAcces { sel: r.get(2)?, verif: r.get(3)?, nonce: r.get(4)?, chiffre: r.get(5)? },
                ))
            })?
            .collect::<Result<_, _>>()?;
        let retraits: Vec<String> = tx
            .prepare("SELECT recherche FROM sync_acces_retraits")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        tx.commit()?;
        if depots.is_empty() && retraits.is_empty() {
            return Ok(None);
        }
        Ok(Some(EnvoiAcces { relais, hotel_id, jeton_envoi, depots, retraits }))
    }

    /// Le relais a bien reçu le coffre (et il n'a pas changé entre-temps).
    pub fn acces_depose(&mut self, utilisateur_id: &str, coffre: &CoffreAcces) -> Resultat<()> {
        self.conn_brute().execute(
            "UPDATE sync_acces SET a_envoyer = 0 WHERE utilisateur_id = ?1 AND verif = ?2 AND chiffre = ?3",
            params![utilisateur_id, coffre.verif, coffre.chiffre],
        )?;
        Ok(())
    }

    pub fn acces_retire(&mut self, recherche: &str) -> Resultat<()> {
        self.conn_brute().execute("DELETE FROM sync_acces_retraits WHERE recherche = ?1", [recherche])?;
        Ok(())
    }

    /// Dépose et retire les coffres (boucle préparer → envoyer → confirmer).
    /// L'application fait la même chose en libérant la base pendant l'envoi.
    pub fn synchroniser_acces(&mut self, relais: &dyn Relais) -> Resultat<()> {
        if let Some(e) = self.preparer_acces()? {
            for (uid, recherche, coffre) in &e.depots {
                relais.deposer_acces(&e.hotel_id, &e.jeton_envoi, recherche, coffre)?;
                self.acces_depose(uid, coffre)?;
            }
            for recherche in &e.retraits {
                relais.retirer_acces(&e.hotel_id, &e.jeton_envoi, recherche)?;
                self.acces_retire(recherche)?;
            }
        }
        Ok(())
    }

    /// Vrai si l'utilisateur connecté a déjà un coffre déposé sur le relais
    /// (il peut se connecter à distance avec son identifiant et son mot de passe).
    pub fn acces_distant_pret(&self, utilisateur_id: &str) -> Resultat<bool> {
        Ok(self.conn_brute().query_row(
            "SELECT count(*) FROM sync_acces WHERE utilisateur_id = ?1 AND a_envoyer = 0",
            [utilisateur_id],
            |r| r.get::<_, i64>(0),
        )? > 0)
    }
}

/// Sur un autre ordinateur : rouvre le coffre avec l'identifiant et le mot de
/// passe. Renvoie l'identifiant de l'hôtel sur le relais et la clé secrète.
pub fn ouvrir_acces_distant(relais: &dyn Relais, identifiant: &str, mot_de_passe: &str) -> Resultat<(String, Secret)> {
    let recherche = recherche_compte(identifiant);
    let sel = relais.sel_acces(&recherche)?;
    let sel = data_encoding::BASE64.decode(sel.sel.as_bytes()).map_err(|_| Erreur::IdentifiantsInvalides)?;
    let cles = ClesMotDePasse::deriver(mot_de_passe, &sel)?;
    let ouvert = relais.ouvrir_acces(&recherche, &PreuveAcces { preuve: cles.preuve.clone() })?;
    let clair = cles.ouvrir(&ouvert.nonce, &ouvert.chiffre, &associe_coffre(&recherche))?;
    let contenu: ContenuCoffre = serde_json::from_slice(&clair).map_err(|_| Erreur::IdentifiantsInvalides)?;
    if contenu.hotel_id != ouvert.hotel_id {
        return Err(Erreur::IdentifiantsInvalides);
    }
    Ok((contenu.hotel_id, Secret::lire(&contenu.secret)?))
}
