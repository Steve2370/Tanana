//! Commandes appelées par l'interface. Volontairement minces : toute la logique,
//! la sécurité et le journal sont dans `tanana-core`.
//!
//! Les arguments gardent le snake_case côté JavaScript (`rename_all`), comme les
//! types générés dans `src/bindings/`.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use tanana_core::Permission;
use tanana_core::modeles::*;
use tanana_core::{Erreur, Resultat};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::documents::{self, DocumentEnregistre};
use crate::etat::{Demandeur, RestaurationFaite, SauvegardeDisponible};
use crate::etat::{Etat, EtatApplication};
use tanana_core::distance::chiffrement::Secret;
use tanana_core::distance::relais::{Relais as _, RelaisHttp};
use tanana_core::distance::{
    BilanEnvoi, CodesDistance, EtatCopie, EtatDistance, normaliser_identifiant_hotel, verifier_cle,
};
use tanana_core::restauration::ApercuSauvegarde;

/// Déclare une commande qui exige une session : `token` est toujours le premier argument.
macro_rules! commande {
    ($nom:ident ( $($arg:ident : $t:ty),* ) -> $ret:ty => |$tanana:ident, $token:ident| $corps:expr) => {
        #[tauri::command(async, rename_all = "snake_case")]
        pub fn $nom(etat: State<'_, Etat>, token: String, $($arg: $t),*) -> Resultat<$ret> {
            etat.avec(|$tanana| {
                let $token = token.as_str();
                $corps
            })
        }
    };
}

// ───────────────────────────── Démarrage ─────────────────────────────

#[tauri::command(async)]
pub fn etat_application(etat: State<'_, Etat>) -> EtatApplication {
    etat.etat_application()
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn fournir_cle(etat: State<'_, Etat>, cle: String) -> Resultat<()> {
    etat.fournir_cle(&cle)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn initialiser(etat: State<'_, Etat>, configuration: ConfigurationInitiale) -> Resultat<String> {
    etat.avec(|t| t.initialiser(configuration))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn recuperer_acces_admin(
    etat: State<'_, Etat>,
    code: String,
    identifiant: String,
    nouveau_mot_de_passe: String,
) -> Resultat<String> {
    etat.avec(|t| t.recuperer_acces_admin(&code, &identifiant, &nouveau_mot_de_passe))
}

// ───────────────────────────── Connexion ─────────────────────────────

#[tauri::command(async, rename_all = "snake_case")]
pub fn connexion(etat: State<'_, Etat>, identifiant: String, mot_de_passe: String) -> Resultat<SessionOuverte> {
    etat.avec(|t| t.connexion(&identifiant, &mot_de_passe))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn connexion_pin(etat: State<'_, Etat>, identifiant: String, pin: String) -> Resultat<SessionOuverte> {
    etat.avec(|t| t.connexion_pin(&identifiant, &pin))
}

commande!(deconnexion() -> () => |t, tok| t.deconnexion(tok));
commande!(ma_session() -> SessionOuverte => |t, tok| t.ma_session(tok));
commande!(changer_mon_mot_de_passe(actuel: String, nouveau: String) -> () =>
    |t, tok| t.changer_mon_mot_de_passe(tok, &actuel, &nouveau));
commande!(definir_mon_pin(mot_de_passe: String, pin: String) -> () =>
    |t, tok| t.definir_mon_pin(tok, &mot_de_passe, &pin));

// ─────────────────────────── Comptes et rôles ───────────────────────────

commande!(lister_utilisateurs() -> Vec<Utilisateur> => |t, tok| t.lister_utilisateurs(tok));
commande!(creer_utilisateur(donnees: NouvelUtilisateur) -> Utilisateur => |t, tok| t.creer_utilisateur(tok, donnees));
commande!(changer_role(utilisateur_id: String, role_id: String) -> Utilisateur =>
    |t, tok| t.changer_role(tok, &utilisateur_id, &role_id));
commande!(changer_statut_utilisateur(utilisateur_id: String, statut: StatutUtilisateur) -> Utilisateur =>
    |t, tok| t.changer_statut_utilisateur(tok, &utilisateur_id, statut));
commande!(reinitialiser_mot_de_passe(utilisateur_id: String, mot_de_passe_temporaire: String) -> () =>
    |t, tok| t.reinitialiser_mot_de_passe(tok, &utilisateur_id, &mot_de_passe_temporaire));
commande!(definir_permission_perso(utilisateur_id: String, permission: Permission, accorde: Option<bool>)
    -> Vec<PermissionPerso> => |t, tok| t.definir_permission_perso(tok, &utilisateur_id, permission, accorde));
commande!(permissions_perso(utilisateur_id: String) -> Vec<PermissionPerso> =>
    |t, tok| t.permissions_perso(tok, &utilisateur_id));
commande!(permissions_utilisateur(utilisateur_id: String) -> Vec<Permission> =>
    |t, tok| t.permissions_utilisateur(tok, &utilisateur_id));
commande!(lister_roles() -> Vec<Role> => |t, tok| t.lister_roles(tok));
commande!(creer_role(nom: String, permissions: Vec<Permission>) -> Role => |t, tok| t.creer_role(tok, &nom, permissions));
commande!(modifier_permissions_role(role_id: String, permissions: Vec<Permission>) -> Role =>
    |t, tok| t.modifier_permissions_role(tok, &role_id, permissions));
commande!(sessions_actives() -> Vec<SessionActive> => |t, tok| t.sessions_actives(tok));
commande!(fermer_session(session_id: String) -> () => |t, tok| t.fermer_session(tok, &session_id));

// ─────────────────────────────── Journal ───────────────────────────────

commande!(journal(filtre: FiltreJournal) -> Vec<EntreeJournal> => |t, tok| t.journal(tok, filtre));
commande!(verifier_integrite_journal() -> RapportIntegrite => |t, tok| t.verifier_integrite_journal(tok));

// ───────────────────────────── Chambres ─────────────────────────────

commande!(lister_types_chambre() -> Vec<TypeChambre> => |t, tok| t.lister_types_chambre(tok));
commande!(creer_type_chambre(donnees: NouveauTypeChambre) -> TypeChambre => |t, tok| t.creer_type_chambre(tok, donnees));
commande!(modifier_type_chambre(id: String, donnees: NouveauTypeChambre, actif: bool) -> TypeChambre =>
    |t, tok| t.modifier_type_chambre(tok, &id, donnees, actif));
commande!(lister_chambres() -> Vec<Chambre> => |t, tok| t.lister_chambres(tok));
commande!(creer_chambre(donnees: DonneesChambre) -> Chambre => |t, tok| t.creer_chambre(tok, donnees));
commande!(modifier_chambre(id: String, donnees: DonneesChambre) -> Chambre => |t, tok| t.modifier_chambre(tok, &id, donnees));

// ────────────────────────────── Clients ──────────────────────────────

commande!(rechercher_clients(recherche: String) -> Vec<Client> => |t, tok| t.rechercher_clients(tok, &recherche));
commande!(obtenir_client(id: String) -> Client => |t, tok| t.obtenir_client(tok, &id));
commande!(creer_client(donnees: DonneesClient) -> Client => |t, tok| t.creer_client(tok, donnees));
commande!(modifier_client(id: String, donnees: DonneesClient) -> Client => |t, tok| t.modifier_client(tok, &id, donnees));

// ──────────────────────────── Réservations ────────────────────────────

commande!(planning(debut: NaiveDate, fin: NaiveDate) -> Vec<Reservation> => |t, tok| t.planning(tok, debut, fin));
commande!(obtenir_reservation(id: String) -> Reservation => |t, tok| t.obtenir_reservation(tok, &id));
commande!(reservations_du_client(client_id: String) -> Vec<Reservation> =>
    |t, tok| t.reservations_du_client(tok, &client_id));
commande!(chambres_disponibles(arrivee: NaiveDate, depart: NaiveDate) -> Vec<Chambre> =>
    |t, tok| t.chambres_disponibles(tok, arrivee, depart));
commande!(creer_reservation(donnees: NouvelleReservation) -> Reservation => |t, tok| t.creer_reservation(tok, donnees));
commande!(modifier_reservation(id: String, donnees: ModificationReservation) -> Reservation =>
    |t, tok| t.modifier_reservation(tok, &id, donnees));
commande!(annuler_reservation(id: String, motif: String) -> Reservation => |t, tok| t.annuler_reservation(tok, &id, &motif));
commande!(enregistrer_arrivee(id: String) -> Reservation => |t, tok| t.enregistrer_arrivee(tok, &id));
commande!(enregistrer_depart(id: String) -> Reservation => |t, tok| t.enregistrer_depart(tok, &id));

// ─────────────────────────────── Argent ───────────────────────────────

commande!(paiements_reservation(reservation_id: String) -> Vec<Paiement> =>
    |t, tok| t.paiements_reservation(tok, &reservation_id));
commande!(encaisser(donnees: Encaissement) -> Paiement => |t, tok| t.encaisser(tok, donnees));
commande!(rembourser(reservation_id: String, montant: i64, mode: ModePaiement, motif: String) -> Paiement =>
    |t, tok| t.rembourser(tok, &reservation_id, montant, mode, &motif));
commande!(saisir_depense(donnees: NouvelleDepense) -> Depense => |t, tok| t.saisir_depense(tok, donnees));
commande!(lister_depenses(debut: NaiveDate, fin: NaiveDate) -> Vec<Depense> => |t, tok| t.lister_depenses(tok, debut, fin));
commande!(annuler_depense(id: String, motif: String) -> Depense => |t, tok| t.annuler_depense(tok, &id, &motif));
commande!(lister_paiements(debut: NaiveDate, fin: NaiveDate, decalage_utc_minutes: i32) -> Vec<PaiementDetaille> =>
    |t, tok| t.lister_paiements(tok, debut, fin, decalage_utc_minutes));
commande!(rapport_caisse(debut: NaiveDate, fin: NaiveDate, decalage_utc_minutes: i32) -> RapportCaisse =>
    |t, tok| t.rapport_caisse(tok, debut, fin, decalage_utc_minutes));

// ───────────────────────────── Taxes ─────────────────────────────

commande!(taxes_hotel() -> Vec<Taxe> => |t, tok| t.taxes_hotel(tok));
commande!(definir_taxes_hotel(taxes: Vec<Taxe>) -> Vec<Taxe> => |t, tok| t.definir_taxes_hotel(tok, taxes));
commande!(taxes_reservation(reservation_id: String) -> DetailTaxes =>
    |t, tok| t.taxes_reservation(tok, &reservation_id));
commande!(modifier_taxes_reservation(reservation_id: String, taxes: Vec<Taxe>) -> Reservation =>
    |t, tok| t.modifier_taxes_reservation(tok, &reservation_id, taxes));
commande!(calculer_taxes(prix: i64, nuits: i64, taxes: Vec<Taxe>) -> DetailTaxes =>
    |t, tok| t.calculer_taxes(tok, prix, nuits, taxes));

// ───────────────────────────── Mises à jour ─────────────────────────────

#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export)]
pub struct InfosMiseAJour {
    pub version: String,
    /// Faux pour une version de développement : pas de recherche de mise à jour.
    pub configuree: bool,
}

#[tauri::command]
pub fn infos_mise_a_jour(app: AppHandle) -> InfosMiseAJour {
    InfosMiseAJour {
        version: app.package_info().version.to_string(),
        configuree: crate::mise_a_jour_configuree(app.config()),
    }
}

/// Juste avant d'installer une nouvelle version : une copie de l'hôtel, au cas où.
/// Sans effet sur un poste de consultation à distance (sa copie se retélécharge).
#[tauri::command(async, rename_all = "snake_case")]
pub fn sauvegarder_avant_mise_a_jour(
    etat: State<'_, Etat>,
    token: String,
    version: String,
) -> Resultat<Option<String>> {
    etat.avec(|t| {
        t.ma_session(&token)?;
        if t.est_une_copie() {
            return Ok(None);
        }
        let propre: String = version.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').collect();
        let horodatage = chrono::Local::now().format("%Y-%m-%d-%Hh%M%S");
        std::fs::create_dir_all(&etat.dossier_sauvegardes).map_err(|e| Erreur::Base(e.to_string()))?;
        let chemin = etat.dossier_sauvegardes.join(format!("tanana-avant-mise-a-jour-{propre}-{horodatage}.db"));
        t.mettre_de_cote(&chemin)?;
        Ok(Some(chemin.display().to_string()))
    })
}

// ───────────────────────────── Remettre une copie de sécurité ─────────────────────────────

/// Hôtel installé : seul un administrateur connecté peut voir ou remettre des copies.
fn exiger_administrateur(etat: &Etat, token: Option<&str>) -> Resultat<()> {
    if !etat.hotel_installe() {
        return Ok(());
    }
    let token = token.ok_or(Erreur::NonAuthentifie)?;
    let session = etat.avec(|t| t.ma_session(token))?;
    if session.permissions.contains(&Permission::ParametresGerer) {
        Ok(())
    } else {
        Err(Erreur::PermissionRefusee { permission: Permission::ParametresGerer.code().into() })
    }
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn lister_sauvegardes(etat: State<'_, Etat>, token: Option<String>) -> Resultat<Vec<SauvegardeDisponible>> {
    exiger_administrateur(&etat, token.as_deref())?;
    Ok(etat.sauvegardes_disponibles())
}

/// Vérifie une copie et dit ce qu'elle contient, sans rien changer.
/// `cle` : seulement pour une copie venant d'un autre ordinateur.
#[tauri::command(async, rename_all = "snake_case")]
pub fn examiner_sauvegarde(
    etat: State<'_, Etat>,
    token: Option<String>,
    chemin: String,
    cle: Option<String>,
) -> Resultat<ApercuSauvegarde> {
    exiger_administrateur(&etat, token.as_deref())?;
    etat.examiner_sauvegarde(Path::new(&chemin), cle.as_deref())
}

/// Remet une copie. Hôtel installé : administrateur + mot de passe retapé.
/// Ensuite, tout le monde doit se reconnecter.
#[tauri::command(async, rename_all = "snake_case")]
pub fn restaurer_sauvegarde(
    etat: State<'_, Etat>,
    token: Option<String>,
    mot_de_passe: Option<String>,
    chemin: String,
    cle: Option<String>,
) -> Resultat<RestaurationFaite> {
    let demandeur = if etat.hotel_installe() {
        let token = token.ok_or(Erreur::NonAuthentifie)?;
        let mdp = mot_de_passe.unwrap_or_default();
        Demandeur::Administrateur(etat.avec(|t| t.autoriser_restauration(&token, &mdp))?)
    } else {
        Demandeur::SansSession
    };
    let faite = etat.restaurer(Path::new(&chemin), cle.as_deref(), demandeur)?;
    // Le relais n'a plus à garder les données de l'ancienne liaison (sans bloquer : réseau)
    let liaisons = faite.liaisons.clone();
    if !liaisons.is_empty() {
        std::thread::spawn(move || {
            for (adresse, hotel_id, jeton) in liaisons {
                if let Err(e) = RelaisHttp::new(&adresse).and_then(|r| r.desinscrire(&hotel_id, &jeton)) {
                    log::warn!("après restauration, le relais n'a pas effacé {hotel_id} : {e}");
                }
            }
        });
    }
    Ok(faite)
}

// ───────────────────────────── Consultation à distance : l'hôtel ─────────────────────────────

commande!(etat_distance() -> EtatDistance => |t, tok| t.etat_consultation_distance(tok));
commande!(afficher_cle_distance(mot_de_passe: String) -> CodesDistance =>
    |t, tok| t.afficher_cle_distance(tok, &mot_de_passe));
commande!(renvoyer_tout_distance() -> i64 => |t, tok| t.renvoyer_tout_distance(tok));

/// Inscrit l'hôtel auprès du relais. La base n'est pas verrouillée pendant l'appel réseau.
#[tauri::command(async, rename_all = "snake_case")]
pub fn activer_distance(
    app: AppHandle,
    etat: State<'_, Etat>,
    token: String,
    adresse: String,
    code_invitation: Option<String>,
) -> Resultat<CodesDistance> {
    let relais = RelaisHttp::new(&adresse)?;
    let preparation = etat.avec(|t| t.preparer_activation_distance(&token, code_invitation.as_deref()))?;
    let inscription = relais.inscrire(&preparation.demande)?;
    let codes = etat.avec(|t| t.terminer_activation_distance(&token, &adresse, &preparation, &inscription))?;
    // Premier envoi tout de suite, sans faire attendre l'écran.
    std::thread::spawn(move || {
        if let Err(e) = crate::distance::envoyer_tout(&app.state::<Etat>()) {
            log::warn!("premier envoi à distance : {e}");
        }
    });
    Ok(codes)
}

/// Envoie maintenant ce qui attend (bouton « Envoyer maintenant »).
#[tauri::command(async, rename_all = "snake_case")]
pub fn envoyer_distance(etat: State<'_, Etat>, token: String) -> Resultat<BilanEnvoi> {
    etat.avec(|t| t.etat_consultation_distance(&token))?; // droit d'administrer exigé
    crate::distance::envoyer_tout(&etat)
}

/// Arrête la consultation à distance, puis demande au relais d'effacer les données de l'hôtel.
#[tauri::command(async, rename_all = "snake_case")]
pub fn desactiver_distance(etat: State<'_, Etat>, token: String) -> Resultat<bool> {
    let Some((adresse, hotel_id, jeton)) = etat.avec(|t| t.desactiver_consultation_distance(&token))? else {
        return Ok(true);
    };
    match RelaisHttp::new(&adresse).and_then(|r| r.desinscrire(&hotel_id, &jeton)) {
        Ok(()) => Ok(true),
        Err(e) => {
            log::warn!("le relais n'a pas pu effacer les données de l'hôtel : {e}");
            Ok(false)
        }
    }
}

// ───────────────────────────── Consultation à distance : l'administrateur ─────────────────────────────

/// Fait de cet ordinateur un poste de consultation : vérifie la clé auprès du relais,
/// crée la copie protégée par `mot_de_passe`, puis télécharge les données.
/// Renvoie un avertissement si le premier téléchargement n'a pas pu se faire.
#[tauri::command(async, rename_all = "snake_case")]
pub fn consulter_hotel_distant(
    etat: State<'_, Etat>,
    adresse: String,
    hotel_id: String,
    cle_secrete: String,
    mot_de_passe: String,
) -> Resultat<Option<String>> {
    tanana_core::domaine::valider_mot_de_passe(&mot_de_passe)?;
    let hotel_id = normaliser_identifiant_hotel(&hotel_id);
    let secret = Secret::lire(&cle_secrete)?;
    let relais = RelaisHttp::new(&adresse)?;
    verifier_cle(&relais, &hotel_id, &secret)?;
    etat.creer_copie(&adresse, &hotel_id, &secret, &mot_de_passe)?;
    Ok(crate::distance::mettre_a_jour_copie(&etat).err().map(|e| e.to_string()))
}

/// Oublie l'hôtel consulté (mot de passe oublié, clé perdue) pour le rattacher à nouveau.
/// Sans session : c'est justement quand on ne peut plus se connecter qu'on en a besoin,
/// et cela ne donne accès à rien (il faudra la clé secrète notée sur papier).
#[tauri::command(async, rename_all = "snake_case")]
pub fn oublier_hotel_distant(etat: State<'_, Etat>) -> Resultat<()> {
    etat.oublier_copie()
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn connexion_copie(etat: State<'_, Etat>, mot_de_passe: String) -> Resultat<SessionOuverte> {
    etat.avec(|t| t.connexion_copie(&mot_de_passe))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn etat_copie(etat: State<'_, Etat>, token: String) -> Resultat<EtatCopie> {
    etat.avec(|t| {
        t.ma_session(&token)?;
        t.etat_copie()
    })
}

/// Télécharge les nouveautés de l'hôtel. Renvoie le nombre de paquets appliqués.
#[tauri::command(async, rename_all = "snake_case")]
pub fn mettre_a_jour_copie(etat: State<'_, Etat>, token: String) -> Resultat<usize> {
    etat.avec(|t| t.ma_session(&token).map(|_| ()))?;
    crate::distance::mettre_a_jour_copie(&etat)
}

// ───────────────────────────── Reçus et factures ─────────────────────────────

commande!(documents_reservation(reservation_id: String) -> Vec<DocumentEmis> =>
    |t, tok| t.documents_reservation(tok, &reservation_id));

/// Dossier où ranger les documents : « Documents », sinon le dossier personnel.
/// Renvoie aussi son nom tel que la personne le voit.
fn racine_documents(app: &AppHandle) -> Resultat<(PathBuf, &'static str)> {
    if let Ok(d) = app.path().document_dir() {
        return Ok((d, "Documents"));
    }
    app.path()
        .home_dir()
        .map(|d| (d, "votre dossier personnel"))
        .map_err(|e| tanana_core::Erreur::EnregistrementImpossible(format!("dossier Documents introuvable : {e}")))
}

/// Range le PDF dans Documents › Tanana, puis l'ouvre avec le lecteur PDF du système.
fn ranger_et_ouvrir(
    app: &AppHandle,
    d: tanana_core::services::documents::DocumentImprime,
) -> Resultat<DocumentEnregistre> {
    let (dossier, racine) = racine_documents(app)?;
    let chemin = documents::enregistrer(&dossier, &d)?;
    let ouvert = match app.opener().open_path(chemin.display().to_string(), None::<&str>) {
        Ok(()) => true,
        Err(e) => {
            log::warn!("ouverture du PDF impossible : {e}");
            false
        }
    };
    Ok(documents::resultat(d, racine, chemin, ouvert))
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn imprimer_recu(
    app: AppHandle,
    etat: State<'_, Etat>,
    token: String,
    paiement_id: String,
    decalage_utc_minutes: i32,
) -> Resultat<DocumentEnregistre> {
    let d = etat.avec(|t| t.imprimer_recu(&token, &paiement_id, decalage_utc_minutes))?;
    ranger_et_ouvrir(&app, d)
}

#[tauri::command(async, rename_all = "snake_case")]
pub fn imprimer_facture(
    app: AppHandle,
    etat: State<'_, Etat>,
    token: String,
    reservation_id: String,
    decalage_utc_minutes: i32,
) -> Resultat<DocumentEnregistre> {
    let d = etat.avec(|t| t.imprimer_facture(&token, &reservation_id, decalage_utc_minutes))?;
    ranger_et_ouvrir(&app, d)
}

/// Ouvre le dossier Documents › Tanana › Reçus (ou Factures) dans le Finder / l'Explorateur.
#[tauri::command(async, rename_all = "snake_case")]
pub fn ouvrir_dossier_documents(app: AppHandle, type_document: TypeDocument) -> Resultat<()> {
    let dossier = racine_documents(&app)?.0.join("Tanana").join(documents::sous_dossier(type_document));
    std::fs::create_dir_all(&dossier).map_err(|e| tanana_core::Erreur::EnregistrementImpossible(e.to_string()))?;
    app.opener()
        .open_path(dossier.display().to_string(), None::<&str>)
        .map_err(|e| tanana_core::Erreur::EnregistrementImpossible(e.to_string()))
}

// ───────────────────────────── Système ─────────────────────────────

commande!(parametres() -> Parametres => |t, tok| t.parametres(tok));
commande!(modifier_parametres(donnees: Parametres) -> Parametres => |t, tok| t.modifier_parametres(tok, donnees));
commande!(sauvegarder_vers(chemin: String) -> () => |t, tok| t.sauvegarder_vers(tok, &PathBuf::from(&chemin)));
commande!(exporter_cle_base(mot_de_passe: String) -> Option<String> => |t, tok| t.exporter_cle_base(tok, &mot_de_passe));
