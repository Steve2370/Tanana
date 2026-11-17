//! Remettre une copie de sécurité : vérifications avant de toucher à quoi que ce soit.

mod commun;

use std::path::Path;

use commun::*;
use tanana_core::db::CleBase;
use tanana_core::distance::chiffrement::Secret;
use tanana_core::distance::relais::RelaisMemoire;
use tanana_core::modeles::*;
use tanana_core::restauration::preparer;
use tanana_core::{Erreur, Tanana};

/// Un hôtel sur disque, chiffré, avec un client, et sa copie de sécurité.
fn hotel_et_copie(dossier: &Path) -> (CleBase, std::path::PathBuf) {
    let cle = CleBase::generer().unwrap();
    let mut t = Tanana::ouvrir(&dossier.join("hotel.db"), Some(cle.clone())).unwrap();
    t.initialiser(config()).unwrap();
    let tok = t.connexion("admin", MDP_ADMIN).unwrap().token;
    t.creer_client(&tok, DonneesClient { nom: "Rakoto".into(), ..Default::default() }).unwrap();
    let copie = dossier.join("copie.db");
    t.sauvegarder_vers(&tok, &copie).unwrap();
    (cle, copie)
}

fn message(e: Erreur) -> String {
    match e {
        Erreur::Validation(m) => m,
        autre => panic!("erreur inattendue : {autre:?}"),
    }
}

#[test]
fn une_bonne_copie_est_verifiee_puis_prete_a_prendre_la_place() {
    let dir = tempfile::tempdir().unwrap();
    let (cle, copie) = hotel_et_copie(dir.path());
    let travail = dir.path().join("travail.db");

    let prep = preparer(&copie, &travail, Some(&cle)).unwrap();
    assert_eq!(prep.apercu.nom_hotel, "Hôtel Tanana");
    assert_eq!(prep.apercu.devise, "XAF");
    assert_eq!((prep.apercu.clients, prep.apercu.reservations, prep.apercu.comptes), (1, 0, 1));
    assert!(prep.apercu.journal_intact);
    assert!(prep.apercu.derniere_activite.is_some());
    assert!(prep.liaison_distance.is_none());
    assert!(copie.exists(), "le fichier choisi n'est jamais modifié");
    assert!(!Path::new(&format!("{}-wal", travail.display())).exists(), "un seul fichier à déplacer");

    // La copie de travail s'ouvre comme une base normale ; plus personne n'y est connecté
    let mut t = Tanana::ouvrir(&travail, Some(cle)).unwrap();
    let n: i64 = t.conn_brute().query_row("SELECT count(*) FROM sessions", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0);
    t.noter_restauration(Some("inconnu"), "copie.db").unwrap();
    let tok = t.connexion("admin", MDP_ADMIN).unwrap().token;
    let derniere = t.journal(&tok, FiltreJournal { limite: Some(5), ..Default::default() }).unwrap();
    assert!(derniere.iter().any(|e| e.action == "sauvegarde.restauration" && e.sensible));
    assert!(t.verifier_integrite_journal(&tok).unwrap().intacte, "l'historique reste chaîné");
}

#[test]
fn mauvaise_cle_ou_fichier_quelconque_refuses_sans_rien_laisser() {
    let dir = tempfile::tempdir().unwrap();
    let (_, copie) = hotel_et_copie(dir.path());
    let travail = dir.path().join("travail.db");

    let autre = CleBase::generer().unwrap();
    assert!(message(preparer(&copie, &travail, Some(&autre)).unwrap_err()).contains("ne s'ouvre pas avec cette clé"));
    assert!(!travail.exists());

    let texte = dir.path().join("photo.db");
    std::fs::write(&texte, "ceci n'est pas une base").unwrap();
    assert!(message(preparer(&texte, &travail, Some(&autre)).unwrap_err()).contains("ne s'ouvre pas"));
    assert!(!travail.exists());

    let absent = dir.path().join("absent.db");
    assert!(message(preparer(&absent, &travail, None).unwrap_err()).contains("introuvable"));
}

#[test]
fn copie_d_une_version_plus_recente_refusee() {
    let dir = tempfile::tempdir().unwrap();
    let (cle, copie) = hotel_et_copie(dir.path());
    {
        let t = Tanana::ouvrir(&copie, Some(cle.clone())).unwrap();
        t.conn_brute().execute_batch("PRAGMA user_version = 99;").unwrap();
    }
    let e = preparer(&copie, &dir.path().join("travail.db"), Some(&cle)).unwrap_err();
    assert!(message(e).contains("version plus récente"));
}

#[test]
fn base_vide_ou_poste_de_consultation_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let cle = CleBase::generer().unwrap();
    let vide = dir.path().join("vide.db");
    drop(Tanana::ouvrir(&vide, Some(cle.clone())).unwrap());
    let travail = dir.path().join("travail.db");
    assert!(message(preparer(&vide, &travail, Some(&cle)).unwrap_err()).contains("vide"));

    let poste = dir.path().join("poste.db");
    let secret = Secret::generer().unwrap();
    drop(Tanana::creer_copie(&poste, Some(cle.clone()), "https://r.test", "HK7Q-F29X", &secret).unwrap());
    assert!(message(preparer(&poste, &travail, Some(&cle)).unwrap_err()).contains("consultation à distance"));
}

#[test]
fn la_consultation_a_distance_est_arretee_dans_la_copie_remise() {
    let dir = tempfile::tempdir().unwrap();
    let cle = CleBase::generer().unwrap();
    let mut t = Tanana::ouvrir(&dir.path().join("hotel.db"), Some(cle.clone())).unwrap();
    t.initialiser(config()).unwrap();
    let tok = t.connexion("admin", MDP_ADMIN).unwrap().token;
    let relais = RelaisMemoire::default();
    let codes = t.activer_consultation_distance(&tok, "https://r.test", None, &relais).unwrap();
    let copie = dir.path().join("copie.db");
    t.sauvegarder_vers(&tok, &copie).unwrap();

    let travail = dir.path().join("travail.db");
    let prep = preparer(&copie, &travail, Some(&cle)).unwrap();
    let (adresse, hotel_id, _) = prep.liaison_distance.expect("liaison renvoyée pour effacer le relais");
    assert_eq!((adresse.as_str(), hotel_id.as_str()), ("https://r.test", codes.hotel_id.as_str()));

    let mut remise = Tanana::ouvrir(&travail, Some(cle)).unwrap();
    let tok = remise.connexion("admin", MDP_ADMIN).unwrap().token;
    assert!(!remise.etat_consultation_distance(&tok).unwrap().active);
    remise.creer_client(&tok, DonneesClient { nom: "Rasoa".into(), ..Default::default() }).unwrap();
    assert!(remise.preparer_envoi().unwrap().is_none(), "plus rien n'est capturé");
}

#[test]
fn seul_un_administrateur_avec_son_mot_de_passe_peut_remettre_une_copie() {
    let mut h = hotel();
    assert_eq!(h.t.autoriser_restauration(&h.admin, "mauvais99"), Err(Erreur::IdentifiantsInvalides));
    assert!(h.t.autoriser_restauration(&h.admin, MDP_ADMIN).is_ok());
    let (_, reception) = h.employe("marie", "role-reception");
    assert!(matches!(h.t.autoriser_restauration(&reception, MDP_DEFINITIF), Err(Erreur::PermissionRefusee { .. })));
}
