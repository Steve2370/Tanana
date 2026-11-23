//! Modifications à distance.
//!
//! L'administratrice, sur son portable, lit sa copie de l'hôtel ; quand elle
//! modifie quelque chose, la demande part **chiffrée** par le relais vers
//! l'ordinateur principal. Celui-ci l'exécute exactement comme si elle était
//! devant lui (sa session, ses droits, le journal), puis renvoie le résultat.
//!
//! Le relais ne peut ni lire, ni modifier, ni fabriquer une demande. Il
//! pourrait en rejouer une ancienne : l'hôtel refuse une demande déjà vue ou
//! trop vieille.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{OptionalExtension, params};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tanana_protocole::{CommandeChiffree, ReponseChiffree};

use super::chiffrement::{Secret, associe_commande, associe_reponse};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::*;
use crate::permissions::Permission;
use crate::services::documents::DocumentImprime;
use crate::{Tanana, horodater, nouvel_id};

/// Écart toléré entre l'heure de la demande et celle de l'hôtel.
const FRAICHEUR_MINUTES: i64 = 10;

/// Une demande, en clair (une fois déchiffrée par l'hôtel).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DemandeDistante {
    pub commande: String,
    pub args: Map<String, Value>,
    /// Heure d'envoi (RFC 3339)
    pub emise_le: String,
}

/// La réponse, en clair.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Retour {
    Ok(Value),
    Erreur(Erreur),
}

/// Reçu ou facture fait à distance : le PDF revient avec la réponse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTransmis {
    pub document: DocumentEmis,
    pub duplicata: bool,
    pub nom_fichier: String,
    /// Base64
    pub pdf: String,
}

impl From<DocumentImprime> for DocumentTransmis {
    fn from(d: DocumentImprime) -> Self {
        Self {
            nom_fichier: d.nom_fichier(),
            pdf: data_encoding::BASE64.encode(&d.pdf),
            document: d.document,
            duplicata: d.duplicata,
        }
    }
}

fn arg<T: DeserializeOwned>(args: &mut Map<String, Value>, nom: &str) -> Resultat<T> {
    let v = args.remove(nom).unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| Erreur::Validation(format!("argument « {nom} » invalide : {e}")))
}

fn json<T: Serialize>(v: T) -> Resultat<Value> {
    Ok(serde_json::to_value(v)?)
}

/// Déclare les commandes qu'on peut faire à distance (toutes avec session :
/// `token` est lu dans les arguments). Mêmes noms et mêmes arguments que les
/// commandes de l'app : l'interface n'a rien de spécial à faire.
macro_rules! commandes_distantes {
    ($( $nom:ident ( $($a:ident : $t:ty),* ) => |$tt:ident, $tok:ident| $corps:expr ; )*) => {
        /// Toutes les commandes acceptées à distance (en plus de `connexion`).
        pub const COMMANDES: &[&str] = &[$(stringify!($nom)),*];

        fn executer_avec_session(t: &mut Tanana, commande: &str, token: &str, args: &mut Map<String, Value>) -> Option<Resultat<Value>> {
            match commande {
                $( stringify!($nom) => Some((|| -> Resultat<Value> {
                    $( let $a: $t = arg(args, stringify!($a))?; )*
                    let $tt = &mut *t;
                    let $tok = token;
                    json($corps?)
                })()), )*
                _ => None,
            }
        }
    };
}

commandes_distantes! {
    deconnexion() => |t, tok| t.deconnexion(tok);
    ma_session() => |t, tok| t.ma_session(tok);
    changer_mon_mot_de_passe(actuel: String, nouveau: String) => |t, tok| t.changer_mon_mot_de_passe(tok, &actuel, &nouveau);
    definir_mon_pin(mot_de_passe: String, pin: String) => |t, tok| t.definir_mon_pin(tok, &mot_de_passe, &pin);

    creer_utilisateur(donnees: NouvelUtilisateur) => |t, tok| t.creer_utilisateur(tok, donnees);
    changer_role(utilisateur_id: String, role_id: String) => |t, tok| t.changer_role(tok, &utilisateur_id, &role_id);
    changer_statut_utilisateur(utilisateur_id: String, statut: StatutUtilisateur) =>
        |t, tok| t.changer_statut_utilisateur(tok, &utilisateur_id, statut);
    reinitialiser_mot_de_passe(utilisateur_id: String, mot_de_passe_temporaire: String) =>
        |t, tok| t.reinitialiser_mot_de_passe(tok, &utilisateur_id, &mot_de_passe_temporaire);
    definir_permission_perso(utilisateur_id: String, permission: Permission, accorde: Option<bool>) =>
        |t, tok| t.definir_permission_perso(tok, &utilisateur_id, permission, accorde);
    creer_role(nom: String, permissions: Vec<Permission>) => |t, tok| t.creer_role(tok, &nom, permissions);
    modifier_permissions_role(role_id: String, permissions: Vec<Permission>) =>
        |t, tok| t.modifier_permissions_role(tok, &role_id, permissions);
    sessions_actives() => |t, tok| t.sessions_actives(tok);
    fermer_session(session_id: String) => |t, tok| t.fermer_session(tok, &session_id);

    creer_type_chambre(donnees: NouveauTypeChambre) => |t, tok| t.creer_type_chambre(tok, donnees);
    modifier_type_chambre(id: String, donnees: NouveauTypeChambre, actif: bool) =>
        |t, tok| t.modifier_type_chambre(tok, &id, donnees, actif);
    creer_chambre(donnees: DonneesChambre) => |t, tok| t.creer_chambre(tok, donnees);
    modifier_chambre(id: String, donnees: DonneesChambre) => |t, tok| t.modifier_chambre(tok, &id, donnees);

    creer_client(donnees: DonneesClient) => |t, tok| t.creer_client(tok, donnees);
    modifier_client(id: String, donnees: DonneesClient) => |t, tok| t.modifier_client(tok, &id, donnees);

    creer_reservation(donnees: NouvelleReservation) => |t, tok| t.creer_reservation(tok, donnees);
    creer_groupe(donnees: NouveauGroupe) => |t, tok| t.creer_groupe(tok, donnees);
    ajouter_au_groupe(groupe_id: String, chambre_id: String, prix_total: Option<i64>) =>
        |t, tok| t.ajouter_au_groupe(tok, &groupe_id, &chambre_id, prix_total);
    modifier_reservation(id: String, donnees: ModificationReservation) => |t, tok| t.modifier_reservation(tok, &id, donnees);
    annuler_reservation(id: String, motif: String) => |t, tok| t.annuler_reservation(tok, &id, &motif);
    enregistrer_arrivee(id: String) => |t, tok| t.enregistrer_arrivee(tok, &id);
    enregistrer_depart(id: String) => |t, tok| t.enregistrer_depart(tok, &id);

    encaisser(donnees: Encaissement) => |t, tok| t.encaisser(tok, donnees);
    rembourser(reservation_id: String, montant: i64, mode: ModePaiement, motif: String) =>
        |t, tok| t.rembourser(tok, &reservation_id, montant, mode, &motif);
    saisir_depense(donnees: NouvelleDepense) => |t, tok| t.saisir_depense(tok, donnees);
    annuler_depense(id: String, motif: String) => |t, tok| t.annuler_depense(tok, &id, &motif);

    definir_taxes_hotel(taxes: Vec<Taxe>) => |t, tok| t.definir_taxes_hotel(tok, taxes);
    modifier_taxes_reservation(reservation_id: String, taxes: Vec<Taxe>) =>
        |t, tok| t.modifier_taxes_reservation(tok, &reservation_id, taxes);

    imprimer_recu(paiement_id: String, decalage_utc_minutes: i32) =>
        |t, tok| t.imprimer_recu(tok, &paiement_id, decalage_utc_minutes).map(DocumentTransmis::from);
    imprimer_facture(reservation_id: String, decalage_utc_minutes: i32) =>
        |t, tok| t.imprimer_facture(tok, &reservation_id, decalage_utc_minutes).map(DocumentTransmis::from);

    modifier_parametres(donnees: Parametres) => |t, tok| t.modifier_parametres(tok, donnees);
    definir_logo(image: Option<NouveauLogo>) => |t, tok| t.definir_logo(tok, image);

    // Lectures qui ne peuvent pas se faire sur la copie (elle n'a pas les sessions)
    lister_utilisateurs() => |t, tok| t.lister_utilisateurs(tok);
    chambres_disponibles(arrivee: NaiveDate, depart: NaiveDate) => |t, tok| t.chambres_disponibles(tok, arrivee, depart);
}

/// Commandes envoyées à l'hôtel quand on travaille à distance ; toutes les
/// autres se lisent sur la copie. (L'interface garde la même liste : un test
/// vérifie qu'elles restent identiques.)
pub const ENVOYEES_A_L_HOTEL: &[&str] = &[
    "changer_mon_mot_de_passe",
    "definir_mon_pin",
    "creer_utilisateur",
    "changer_role",
    "changer_statut_utilisateur",
    "reinitialiser_mot_de_passe",
    "definir_permission_perso",
    "creer_role",
    "modifier_permissions_role",
    "sessions_actives",
    "fermer_session",
    "creer_type_chambre",
    "modifier_type_chambre",
    "creer_chambre",
    "modifier_chambre",
    "creer_client",
    "modifier_client",
    "creer_reservation",
    "creer_groupe",
    "ajouter_au_groupe",
    "modifier_reservation",
    "annuler_reservation",
    "enregistrer_arrivee",
    "enregistrer_depart",
    "encaisser",
    "rembourser",
    "saisir_depense",
    "annuler_depense",
    "definir_taxes_hotel",
    "modifier_taxes_reservation",
    "imprimer_recu",
    "imprimer_facture",
    "modifier_parametres",
    "definir_logo",
];

/// Exécute une demande venue d'ailleurs, sur l'ordinateur principal.
pub fn executer(t: &mut Tanana, commande: &str, mut args: Map<String, Value>) -> Resultat<Value> {
    let a = &mut args;
    if commande == "connexion" {
        let (identifiant, mdp): (String, String) = (arg(a, "identifiant")?, arg(a, "mot_de_passe")?);
        return json(t.connexion_distante(&identifiant, &mdp)?);
    }
    let token: String = arg::<Option<String>>(a, "token")?.ok_or(Erreur::NonAuthentifie)?;
    executer_avec_session(t, commande, &token, a)
        .unwrap_or_else(|| Err(Erreur::Validation(format!("« {commande} » ne peut pas se faire à distance"))))
}

impl Tanana {
    /// Sur l'ordinateur principal : déchiffre, vérifie (fraîcheur, rejeu),
    /// exécute et chiffre la réponse. Une demande qu'on ne peut pas déchiffrer
    /// est ignorée (Err) : elle ne vient pas d'un administrateur de cet hôtel.
    pub fn traiter_commande_distante(&mut self, c: &CommandeChiffree) -> Resultat<ReponseChiffree> {
        let (hotel_id, secret): (String, String) = self
            .conn_brute()
            .query_row("SELECT hotel_id, secret FROM sync_config WHERE id = 1 AND remplace_le IS NULL", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?
            .ok_or_else(|| Erreur::Validation("cet ordinateur n'est pas l'ordinateur principal".into()))?;
        let secret = Secret::lire(&secret)?;
        let clair = secret.dechiffrer(&c.nonce, &c.chiffre, &associe_commande(&hotel_id, &c.id))?;
        let demande: DemandeDistante = serde_json::from_slice(&clair)?;
        let retour = match self.verifier_demande(&c.id, &demande) {
            Err(e) => Retour::Erreur(e),
            Ok(()) => match executer(self, &demande.commande, demande.args) {
                Ok(v) => Retour::Ok(v),
                Err(e) => Retour::Erreur(e),
            },
        };
        let (nonce, chiffre) = secret.chiffrer(&serde_json::to_vec(&retour)?, &associe_reponse(&hotel_id, &c.id))?;
        Ok(ReponseChiffree { nonce, chiffre })
    }

    fn verifier_demande(&mut self, id: &str, d: &DemandeDistante) -> Resultat<()> {
        let maintenant = self.maintenant();
        let emise = DateTime::parse_from_rfc3339(&d.emise_le)
            .map_err(|_| Erreur::Validation("demande sans heure".into()))?
            .with_timezone(&Utc);
        if (maintenant - emise).abs() > Duration::minutes(FRAICHEUR_MINUTES) {
            return Err(Erreur::Validation(
                "demande trop ancienne, ou horloge de l'ordinateur mal réglée : vérifiez la date et l'heure".into(),
            ));
        }
        let conn = self.conn_brute();
        let limite = horodater(maintenant - Duration::days(2));
        conn.execute("DELETE FROM sync_commandes_vues WHERE vue_le < ?1", [&limite])?;
        let nouvelle = conn.execute(
            "INSERT OR IGNORE INTO sync_commandes_vues (id, vue_le) VALUES (?1, ?2)",
            params![id, horodater(maintenant)],
        )?;
        if nouvelle == 0 {
            return Err(Erreur::Validation("demande déjà faite".into()));
        }
        Ok(())
    }

    /// Sur le poste à distance : chiffre une demande pour l'hôtel consulté.
    /// Renvoie (hôtel, jeton d'administrateur, demande chiffrée).
    pub fn preparer_commande_distante(
        &self,
        commande: &str,
        args: Map<String, Value>,
    ) -> Resultat<(String, String, CommandeChiffree)> {
        let (relais_hotel, secret) = self.liaison_copie()?;
        let id = nouvel_id();
        let demande = DemandeDistante { commande: commande.to_string(), args, emise_le: horodater(self.maintenant()) };
        let (nonce, chiffre) =
            secret.chiffrer(&serde_json::to_vec(&demande)?, &associe_commande(&relais_hotel, &id))?;
        Ok((relais_hotel, secret.jeton_admin(), CommandeChiffree { id, nonce, chiffre }))
    }

    /// Sur le poste à distance : lit la réponse de l'hôtel.
    pub fn lire_reponse_distante(&self, commande_id: &str, r: &ReponseChiffree) -> Resultat<Value> {
        let (hotel_id, secret) = self.liaison_copie()?;
        let clair = secret.dechiffrer(&r.nonce, &r.chiffre, &associe_reponse(&hotel_id, commande_id))?;
        match serde_json::from_slice::<Retour>(&clair)? {
            Retour::Ok(v) => Ok(v),
            Retour::Erreur(e) => Err(e),
        }
    }

    fn liaison_copie(&self) -> Resultat<(String, Secret)> {
        if !self.est_une_copie() {
            return Err(Erreur::Validation("réservé au poste à distance".into()));
        }
        let (hotel_id, secret): (String, String) =
            self.conn_brute().query_row("SELECT hotel_id, secret FROM copie_distance WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
        Ok((hotel_id, Secret::lire(&secret)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tout_ce_qui_part_a_l_hotel_y_est_execute() {
        for c in ENVOYEES_A_L_HOTEL {
            assert!(COMMANDES.contains(c), "{c} est envoyée à l'hôtel mais il ne sait pas l'exécuter");
        }
    }

    #[test]
    fn commande_inconnue_refusee() {
        let mut t = Tanana::en_memoire().unwrap();
        let mut args = Map::new();
        args.insert("token".into(), Value::String("x".into()));
        assert!(matches!(executer(&mut t, "sauvegarder_vers", args), Err(Erreur::Validation(_))));
        assert!(matches!(executer(&mut t, "lister_chambres", Map::new()), Err(Erreur::NonAuthentifie)));
    }
}
