//! L'ordinateur principal : celui qui garde la base de l'hôtel.
//!
//! Il n'y en a qu'un à la fois. Au début, c'est le portable de l'administratrice
//! (elle prépare l'hôtel au Canada) ; à Madagascar, l'ordinateur de l'hôtel le
//! remplace avec « Retrouver mon hôtel » :
//!
//! 1. le coffre (identifiant + mot de passe) donne l'hôtel et la clé secrète ;
//! 2. cet ordinateur devient le principal auprès du relais : l'ancien ne peut
//!    plus rien envoyer (il l'apprend à son prochain envoi) ;
//! 3. l'**instantané** (copie complète et chiffrée de la base, déposée de temps
//!    en temps par l'ancien) devient la base de cet ordinateur ;
//! 4. les changements arrivés après l'instantané sont rejoués par-dessus.
//!
//! Rien n'est écrit à l'emplacement de la base tant que tout n'a pas réussi.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use rusqlite::{OptionalExtension, params};
use serde_json::json;
use tanana_protocole::{Instantane, PageLots};

use super::acces::ouvrir_acces_distant;
use super::chiffrement::{Secret, associe_instantane, associe_lot};
use super::relais::Relais;
use super::{ActivationPreparee, Lot, appliquer_changements, capture};
use crate::db::CleBase;
use crate::erreur::{Erreur, Resultat};
use crate::{Tanana, audit, horodater};

/// Écart minimal entre deux instantanés (s'il y a eu des changements).
pub const INTERVALLE_INSTANTANE_HEURES: i64 = 3;

const ENTETE_INSTANTANE: &str = "TANANA-INSTANTANE-1";

/// (relais, hôtel, jeton d'envoi, clé secrète, dernier numéro, numéro et date du dernier instantané)
type LigneInstantane = (String, String, String, String, i64, Option<i64>, Option<String>);

/// Un instantané prêt à partir.
#[derive(Debug, Clone)]
pub struct EnvoiInstantane {
    pub relais: String,
    pub hotel_id: String,
    pub jeton_envoi: String,
    pub instantane: Instantane,
}

fn nom_temporaire(dossier: &Path, prefixe: &str) -> Resultat<PathBuf> {
    let mut o = [0u8; 6];
    getrandom::fill(&mut o).map_err(|e| Erreur::Base(format!("hasard indisponible : {e}")))?;
    Ok(dossier.join(format!("{prefixe}-{}.db", hex::encode(o))))
}

fn effacer_base(chemin: &Path) {
    for suffixe in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffixe}", chemin.display()));
    }
}

impl Tanana {
    // ───────────────────────────── Instantané ─────────────────────────────

    /// Prépare un instantané si c'est le moment : tout est parti vers le relais,
    /// il y a du nouveau depuis le dernier, et le dernier date d'au moins
    /// [`INTERVALLE_INSTANTANE_HEURES`] (ou il n'y en a pas encore).
    /// `dossier` : où écrire la copie le temps de la chiffrer.
    pub fn preparer_instantane(&mut self, dossier: &Path) -> Resultat<Option<EnvoiInstantane>> {
        let maintenant = self.maintenant();
        let ligne: Option<LigneInstantane> = self
            .conn_brute()
            .query_row(
                "SELECT relais, hotel_id, jeton_envoi, secret, dernier_numero, instantane_numero, instantane_le
                 FROM sync_config WHERE id = 1 AND remplace_le IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
            )
            .optional()?;
        let Some((relais, hotel_id, jeton_envoi, secret, dernier_numero, inst_numero, inst_le)) = ligne else {
            return Ok(None);
        };
        let en_attente: i64 = self.conn_brute().query_row("SELECT count(*) FROM sync_boite", [], |r| r.get(0))?;
        if en_attente > 0 || dernier_numero == 0 || inst_numero == Some(dernier_numero) {
            return Ok(None);
        }
        if inst_numero.is_some()
            && let Some(le) = inst_le.as_deref().and_then(|l| DateTime::parse_from_rfc3339(l).ok())
            && maintenant - le.with_timezone(&Utc) < Duration::hours(INTERVALLE_INSTANTANE_HEURES)
        {
            return Ok(None);
        }
        std::fs::create_dir_all(dossier).map_err(|e| Erreur::EnregistrementImpossible(e.to_string()))?;
        let temporaire = nom_temporaire(dossier, "instantane")?;
        let resultat = (|| -> Resultat<Instantane> {
            self.mettre_de_cote(&temporaire)?;
            let octets = std::fs::read(&temporaire).map_err(|e| Erreur::Base(e.to_string()))?;
            let cle = self.cle.as_ref().map(|c| c.en_hex().to_string()).unwrap_or_else(|| "-".into());
            let mut clair = format!("{ENTETE_INSTANTANE}\n{cle}\n").into_bytes();
            clair.extend_from_slice(&octets);
            let (nonce, chiffre) =
                Secret::lire(&secret)?.chiffrer(&clair, &associe_instantane(&hotel_id, dernier_numero))?;
            Ok(Instantane { apres_numero: dernier_numero, nonce, chiffre })
        })();
        effacer_base(&temporaire);
        Ok(Some(EnvoiInstantane { relais, hotel_id, jeton_envoi, instantane: resultat? }))
    }

    /// Le relais a reçu l'instantané.
    pub fn instantane_depose(&mut self, apres_numero: i64) -> Resultat<()> {
        let maintenant = horodater(self.maintenant());
        self.conn_brute().execute(
            "UPDATE sync_config SET instantane_numero = ?1, instantane_le = ?2",
            params![apres_numero, maintenant],
        )?;
        Ok(())
    }

    /// Instantané complet (préparer, déposer, confirmer) : pratique en test.
    pub fn synchroniser_instantane(&mut self, relais: &dyn Relais, dossier: &Path) -> Resultat<bool> {
        let Some(e) = self.preparer_instantane(dossier)? else { return Ok(false) };
        match relais.deposer_instantane(&e.hotel_id, &e.jeton_envoi, &e.instantane) {
            Ok(()) => {
                self.instantane_depose(e.instantane.apres_numero)?;
                Ok(true)
            }
            Err(err) => {
                self.envoi_refuse(&err)?;
                Err(err)
            }
        }
    }

    // ───────────────────────────── Activation automatique ─────────────────────────────

    /// À la création de l'hôtel : le lien avec le relais se fera tout seul dès
    /// qu'Internet sera là (adresse et code d'invitation fournis par l'app).
    pub fn demander_activation_auto(
        &mut self,
        token: &str,
        relais: &str,
        code_invitation: Option<&str>,
    ) -> Resultat<()> {
        self.executer(token, Some(crate::Permission::ParametresGerer), |_, _| Ok(()))?;
        self.programmer_activation_auto(relais, code_invitation)
    }

    /// Même chose, juste après la création de l'hôtel (personne n'est encore connecté).
    /// Sans effet si le lien avec le relais existe déjà.
    pub fn programmer_activation_auto(&mut self, relais: &str, code_invitation: Option<&str>) -> Resultat<()> {
        let relais = relais.trim().trim_end_matches('/').to_string();
        let deja: i64 = self.conn_brute().query_row("SELECT count(*) FROM sync_config", [], |r| r.get(0))?;
        if relais.is_empty() || deja > 0 || self.est_une_copie() {
            return Ok(());
        }
        self.conn_brute().execute(
            "INSERT INTO sync_activation_auto (id, relais, code_invitation) VALUES (1, ?1, ?2)
             ON CONFLICT (id) DO UPDATE SET relais = excluded.relais, code_invitation = excluded.code_invitation",
            params![relais, code_invitation.map(str::trim).filter(|c| !c.is_empty())],
        )?;
        Ok(())
    }

    /// 1/2 - Activation automatique en attente ? (adresse du relais, demande à envoyer).
    pub fn preparer_activation_auto(&mut self) -> Resultat<Option<(String, ActivationPreparee)>> {
        let conn = self.conn_brute();
        let deja: i64 = conn.query_row("SELECT count(*) FROM sync_config", [], |r| r.get(0))?;
        let attente: Option<(String, Option<String>)> = conn
            .query_row("SELECT relais, code_invitation FROM sync_activation_auto WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        let Some((relais, code)) = attente else { return Ok(None) };
        if deja > 0 || self.est_une_copie() {
            conn.execute("DELETE FROM sync_activation_auto", [])?;
            return Ok(None);
        }
        Ok(Some((relais, super::preparer_demande(code.as_deref())?)))
    }

    /// 2/2 - Le relais a inscrit l'hôtel : on enregistre la liaison (action du système).
    pub fn terminer_activation_auto(
        &mut self,
        adresse_relais: &str,
        preparation: &ActivationPreparee,
        inscription: &tanana_protocole::Inscription,
    ) -> Resultat<()> {
        let maintenant = horodater(self.maintenant());
        let tx = self.transaction()?;
        super::enregistrer_liaison(&tx, adresse_relais, preparation, inscription, &maintenant, None)?;
        tx.execute("DELETE FROM sync_activation_auto", [])?;
        tx.commit()?;
        Ok(())
    }

    // ───────────────────────────── Reprise ─────────────────────────────

    /// Rejoue des lots (arrivés après l'instantané) sur la base reprise.
    fn rattraper(&mut self, secret: &Secret, hotel_id: &str, page: &PageLots) -> Resultat<i64> {
        let tx = self.transaction()?;
        let mut dernier = 0;
        for recu in &page.lots {
            let l = &recu.lot;
            let clair = secret.dechiffrer(&l.nonce, &l.chiffre, &associe_lot(hotel_id, l.seq_debut, l.seq_fin))?;
            let lot: Lot = serde_json::from_slice(&clair)?;
            appliquer_changements(&tx, &lot.changements, true)?;
            dernier = recu.numero;
        }
        tx.commit()?;
        Ok(dernier)
    }

    /// Cet ordinateur est désormais l'ordinateur principal : nouvelle liaison,
    /// boîte d'envoi vide, plus personne de connecté, et une trace au journal.
    fn terminer_reprise(
        &mut self,
        adresse: &str,
        hotel_id: &str,
        secret: &Secret,
        jeton_envoi: &str,
        dernier_numero: i64,
    ) -> Resultat<()> {
        let maintenant = horodater(self.maintenant());
        let tx = self.transaction()?;
        tx.execute("DELETE FROM sync_config", [])?;
        tx.execute(
            "INSERT INTO sync_config (id, relais, hotel_id, jeton_envoi, secret, active_le, dernier_numero)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                adresse.trim().trim_end_matches('/'),
                hotel_id,
                jeton_envoi,
                secret.afficher(),
                maintenant,
                dernier_numero
            ],
        )?;
        tx.execute("UPDATE sessions SET fin = ?1 WHERE fin IS NULL", [&maintenant])?;
        tx.execute("DELETE FROM sync_commandes_vues", [])?;
        tx.execute("DELETE FROM sync_activation_auto", [])?;
        capture::installer(&tx)?;
        tx.execute("DELETE FROM sync_boite", [])?;
        audit::ecrire(
            &tx,
            &maintenant,
            None,
            "distance.reprise",
            Some("parametres"),
            None,
            None,
            Some(&json!({ "hotel_id": hotel_id })),
        )?;
        tx.commit()?;
        Ok(())
    }
}

/// Ce que l'app montre avant de reprendre l'hôtel.
#[derive(Debug, Clone)]
pub struct HotelRetrouve {
    pub hotel_id: String,
    pub secret: Secret,
    /// Heure de la dernière copie complète reçue par le relais (RFC 3339)
    pub instantane_du: String,
}

/// 1/2 - Vérifie l'identifiant et le mot de passe, et qu'une copie complète existe.
/// Rien ne change encore, ni ici ni sur le relais.
pub fn trouver_hotel(relais: &dyn Relais, identifiant: &str, mot_de_passe: &str) -> Resultat<HotelRetrouve> {
    let (hotel_id, secret) = ouvrir_acces_distant(relais, identifiant, mot_de_passe)?;
    let inst = relais.lire_instantane(&hotel_id, &secret.jeton_admin())?;
    Ok(HotelRetrouve { hotel_id, secret, instantane_du: inst.recu_le })
}

/// Base reprise, ouverte, avec la clé à ranger dans le trousseau.
pub struct HotelRepris {
    pub tanana: Tanana,
    pub cle: Option<CleBase>,
}

/// 2/2 - Fait de cet ordinateur l'ordinateur principal et y installe la base.
/// `chemin` : emplacement final de la base (ne doit pas exister).
/// En cas d'échec, rien n'est écrit à `chemin` ; on peut recommencer.
pub fn reprendre_hotel(
    relais: &dyn Relais,
    adresse: &str,
    trouve: &HotelRetrouve,
    chemin: &Path,
) -> Resultat<HotelRepris> {
    if chemin.exists() {
        return Err(Erreur::Validation("un hôtel est déjà installé sur cet ordinateur".into()));
    }
    let jeton_admin = trouve.secret.jeton_admin();
    // 1. L'ancien ordinateur principal ne peut plus rien envoyer : tout ce qui
    //    est sur le relais à partir de maintenant est complet.
    let principal = relais.devenir_principal(&trouve.hotel_id, &jeton_admin)?;
    // 2. La copie complète
    let inst = relais.lire_instantane(&trouve.hotel_id, &jeton_admin)?;
    let clair = trouve.secret.dechiffrer(
        &inst.nonce,
        &inst.chiffre,
        &associe_instantane(&trouve.hotel_id, inst.apres_numero),
    )?;
    let illisible = || Erreur::Base("copie complète illisible".into());
    let mut parties = clair.splitn(3, |o| *o == b'\n');
    let (entete, cle, octets) = (parties.next(), parties.next(), parties.next());
    if entete != Some(ENTETE_INSTANTANE.as_bytes()) {
        return Err(illisible());
    }
    let cle = match std::str::from_utf8(cle.ok_or_else(illisible)?).map_err(|_| illisible())? {
        "-" => None,
        hex => Some(CleBase::depuis_hex(hex)?),
    };
    let octets = octets.ok_or_else(illisible)?;
    let dossier = chemin.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dossier).map_err(|e| Erreur::EnregistrementImpossible(e.to_string()))?;
    let travail = nom_temporaire(dossier, "reprise")?;
    let resultat = (|| -> Resultat<()> {
        std::fs::write(&travail, octets).map_err(|e| Erreur::EnregistrementImpossible(e.to_string()))?;
        let mut t = Tanana::ouvrir(&travail, cle.clone())?;
        // 3. Les changements arrivés après la copie complète
        let jeton_lecture = trouve.secret.jeton_lecture();
        let mut apres = inst.apres_numero;
        loop {
            let page = relais.lire(&trouve.hotel_id, &jeton_lecture, apres)?;
            let dernier = t.rattraper(&trouve.secret, &trouve.hotel_id, &page)?;
            apres = apres.max(dernier);
            if !page.suite {
                break;
            }
        }
        t.terminer_reprise(adresse, &trouve.hotel_id, &trouve.secret, &principal.jeton_envoi, apres)?;
        Ok(())
    })();
    if let Err(e) = resultat {
        effacer_base(&travail);
        return Err(e);
    }
    std::fs::rename(&travail, chemin).map_err(|e| {
        effacer_base(&travail);
        Erreur::EnregistrementImpossible(e.to_string())
    })?;
    for suffixe in ["-wal", "-shm"] {
        let _ = std::fs::rename(format!("{}{suffixe}", travail.display()), format!("{}{suffixe}", chemin.display()));
    }
    let tanana = Tanana::ouvrir(chemin, cle.clone())?;
    Ok(HotelRepris { tanana, cle })
}
