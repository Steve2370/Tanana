//! Accès par identifiant et mot de passe, passage de l'hôtel d'un ordinateur à
//! un autre (« Retrouver mon hôtel »), et modifications faites à distance.

mod commun;

use commun::*;
use serde_json::{Map, Value, json};
use tanana_core::Erreur;
use tanana_core::Tanana;
use tanana_core::distance::acces::ouvrir_acces_distant;
use tanana_core::distance::principal::{reprendre_hotel, trouver_hotel};
use tanana_core::distance::relais::{Relais, RelaisMemoire};
use tanana_core::modeles::*;

const ADRESSE: &str = "https://relais.tanana.test";

fn args(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

/// Lie l'hôtel au relais et envoie tout (lots + coffres).
fn relier(h: &mut Hotel, relais: &RelaisMemoire) {
    h.t.activer_consultation_distance(&h.admin, ADRESSE, None, relais).unwrap();
    h.t.synchroniser(relais).unwrap();
    h.t.synchroniser_acces(relais).unwrap();
}

#[test]
fn l_administrateur_retrouve_son_hotel_avec_son_identifiant_et_son_mot_de_passe() {
    let mut h = hotel();
    let relais = RelaisMemoire::default();
    relier(&mut h, &relais);
    let (hotel_id, secret) = ouvrir_acces_distant(&relais, " Admin ", MDP_ADMIN).unwrap();
    assert!(!hotel_id.is_empty());
    // Le coffre redonne bien la clé de l'hôtel : elle ouvre les lots
    let page = relais.lire(&hotel_id, &secret.jeton_lecture(), 0).unwrap();
    assert!(!page.lots.is_empty());
    // Mauvais mot de passe, compte inconnu
    assert_eq!(ouvrir_acces_distant(&relais, "admin", "mauvais2026").unwrap_err(), Erreur::IdentifiantsInvalides);
    assert_eq!(ouvrir_acces_distant(&relais, "personne", MDP_ADMIN).unwrap_err(), Erreur::IdentifiantsInvalides);
    // Le relais ne voit ni l'identifiant, ni le mot de passe, ni le nom de l'hôtel
    let brut = relais.contenu_brut();
    for mot in ["admin", MDP_ADMIN, "Tanana", "Beltran"] {
        assert!(!brut.contains(mot), "le relais voit « {mot} »");
    }
}

#[test]
fn seuls_les_administrateurs_actifs_ont_un_acces_a_distance() {
    let mut h = hotel();
    let relais = RelaisMemoire::default();
    relier(&mut h, &relais);
    let (_, _) = h.employe("recep", "role-reception");
    h.t.synchroniser_acces(&relais).unwrap();
    assert_eq!(ouvrir_acces_distant(&relais, "recep", MDP_DEFINITIF).unwrap_err(), Erreur::IdentifiantsInvalides);

    // Un deuxième administrateur : accès dès sa première connexion
    let (id2, _) = h.employe("admin2", "role-admin");
    h.t.synchroniser_acces(&relais).unwrap();
    ouvrir_acces_distant(&relais, "admin2", MDP_DEFINITIF).unwrap();

    // Mot de passe réinitialisé par quelqu'un d'autre : l'ancien ne doit plus ouvrir le coffre
    h.t.reinitialiser_mot_de_passe(&h.admin, &id2, "provisoire1").unwrap();
    h.t.synchroniser_acces(&relais).unwrap();
    assert_eq!(ouvrir_acces_distant(&relais, "admin2", MDP_DEFINITIF).unwrap_err(), Erreur::IdentifiantsInvalides);

    // Compte suspendu : plus d'accès à distance
    let jeton2 = h.t.connexion("admin2", "provisoire1").unwrap().token;
    h.t.changer_mon_mot_de_passe(&jeton2, "provisoire1", "nouveau2026").unwrap();
    h.t.synchroniser_acces(&relais).unwrap();
    ouvrir_acces_distant(&relais, "admin2", "nouveau2026").unwrap();
    h.t.changer_statut_utilisateur(&h.admin, &id2, StatutUtilisateur::Suspendu).unwrap();
    h.t.synchroniser_acces(&relais).unwrap();
    assert_eq!(ouvrir_acces_distant(&relais, "admin2", "nouveau2026").unwrap_err(), Erreur::IdentifiantsInvalides);
}

#[test]
fn changer_son_mot_de_passe_change_l_acces_a_distance() {
    let mut h = hotel();
    let relais = RelaisMemoire::default();
    relier(&mut h, &relais);
    h.t.changer_mon_mot_de_passe(&h.admin, MDP_ADMIN, "Antananarivo26").unwrap();
    assert!(!h.t.acces_distant_pret("u").unwrap());
    h.t.synchroniser_acces(&relais).unwrap();
    assert!(ouvrir_acces_distant(&relais, "admin", MDP_ADMIN).is_err());
    ouvrir_acces_distant(&relais, "admin", "Antananarivo26").unwrap();
}

#[test]
fn activation_automatique_des_que_le_relais_repond() {
    let mut h = hotel();
    let relais = RelaisMemoire::default();
    h.t.demander_activation_auto(&h.admin, ADRESSE, Some("CODE-2026")).unwrap();
    // Pas d'Internet : on réessaiera
    relais.hors_ligne.store(true, std::sync::atomic::Ordering::SeqCst);
    let (adresse, prep) = h.t.preparer_activation_auto().unwrap().unwrap();
    assert_eq!(adresse, ADRESSE);
    assert_eq!(prep.demande.code_invitation.as_deref(), Some("CODE-2026"));
    assert!(relais.inscrire(&prep.demande).is_err());
    // Internet revient
    relais.hors_ligne.store(false, std::sync::atomic::Ordering::SeqCst);
    let (adresse, prep) = h.t.preparer_activation_auto().unwrap().unwrap();
    let inscription = relais.inscrire(&prep.demande).unwrap();
    h.t.terminer_activation_auto(&adresse, &prep, &inscription).unwrap();
    assert!(h.t.preparer_activation_auto().unwrap().is_none(), "plus rien à activer");
    h.t.synchroniser(&relais).unwrap();
    // Le coffre préparé à la création de l'hôtel part maintenant
    h.t.synchroniser_acces(&relais).unwrap();
    ouvrir_acces_distant(&relais, "admin", MDP_ADMIN).unwrap();
}

/// Portable de l'administratrice au Canada, base dans un fichier.
fn portable(dossier: &std::path::Path) -> Hotel {
    let temps = Temps::nouveau();
    let mut t = Tanana::ouvrir(&dossier.join("portable.db"), None).unwrap().avec_horloge(temps.horloge());
    let code = t.initialiser(config()).unwrap();
    let admin = t.connexion("admin", MDP_ADMIN).unwrap().token;
    Hotel { t, temps, admin, code_recuperation: code }
}

#[test]
fn preparer_au_canada_puis_retrouver_l_hotel_a_madagascar() {
    let dossier = tempfile::tempdir().unwrap();
    let relais = RelaisMemoire::default();
    let mut h = portable(dossier.path());
    relier(&mut h, &relais);
    let (ch, cl) = h.base();
    h.resa(&ch.id, &cl.id, "2026-04-01", "2026-04-03").unwrap();
    h.t.synchroniser(&relais).unwrap();

    // Pas encore de copie complète : impossible de reprendre l'hôtel
    assert_eq!(trouver_hotel(&relais, "admin", MDP_ADMIN).unwrap_err(), Erreur::InstantaneAbsent);
    assert!(h.t.synchroniser_instantane(&relais, dossier.path()).unwrap());
    assert!(!h.t.synchroniser_instantane(&relais, dossier.path()).unwrap(), "rien de neuf : pas de nouvel instantané");

    // Encore du travail après la copie complète (parti seulement en lots)
    let cl2 = h.client("Rasoa");
    let (id_recep, _) = h.employe("recep", "role-reception");
    h.t.synchroniser(&relais).unwrap();

    // À Madagascar, sur l'ordinateur de l'hôtel
    assert_eq!(trouver_hotel(&relais, "admin", "mauvais2026").unwrap_err(), Erreur::IdentifiantsInvalides);
    let trouve = trouver_hotel(&relais, "admin", MDP_ADMIN).unwrap();
    let chemin = dossier.path().join("hotel").join("tanana.db");
    let mut repris = reprendre_hotel(&relais, ADRESSE, &trouve, &chemin).unwrap().tanana;
    assert!(chemin.exists());

    // Tout est là, y compris ce qui a suivi la copie complète
    let jeton = repris.connexion("admin", MDP_ADMIN).unwrap().token;
    assert_eq!(repris.rechercher_clients(&jeton, "Rasoa").unwrap(), vec![cl2]);
    assert_eq!(repris.planning(&jeton, d("2026-04-01"), d("2026-04-05")).unwrap().len(), 1);
    assert!(repris.lister_utilisateurs(&jeton).unwrap().iter().any(|u| u.id == id_recep));
    // Son mot de passe n'a jamais quitté le portable : à réinitialiser par l'admin
    assert_eq!(repris.connexion("recep", MDP_DEFINITIF).unwrap_err(), Erreur::IdentifiantsInvalides);
    repris.reinitialiser_mot_de_passe(&jeton, &id_recep, "provisoire1").unwrap();
    repris.connexion("recep", "provisoire1").unwrap();
    // Le journal reste intact et garde la trace de la reprise
    assert!(repris.verifier_integrite_journal(&jeton).unwrap().intacte);

    // Le portable apprend qu'il n'est plus l'ordinateur principal
    h.client("Saisie de trop");
    assert_eq!(h.t.synchroniser(&relais).unwrap_err(), Erreur::PosteRemplace);
    assert!(h.t.poste_remplace().unwrap());
    assert!(h.t.preparer_envoi().unwrap().is_none(), "plus rien ne part du portable");

    // L'ordinateur de l'hôtel envoie à son tour, et une copie suit tout
    repris.creer_client(&jeton, DonneesClient { nom: "Randria".into(), ..Default::default() }).unwrap();
    repris.synchroniser(&relais).unwrap();
    let (hotel_id, secret) = ouvrir_acces_distant(&relais, "admin", MDP_ADMIN).unwrap();
    let mut copie = Tanana::copie_en_memoire(ADRESSE, &hotel_id, &secret).unwrap();
    copie.mettre_a_jour_copie(&relais).unwrap();
    copie.definir_acces_copie(MDP_ADMIN).unwrap();
    let jc = copie.connexion_copie(MDP_ADMIN).unwrap().token;
    assert_eq!(copie.rechercher_clients(&jc, "Randria").unwrap().len(), 1);
    assert_eq!(copie.rechercher_clients(&jc, "Rasoa").unwrap().len(), 1);
    assert!(copie.rechercher_clients(&jc, "Saisie de trop").unwrap().is_empty(), "jamais partie du portable");

    // Déjà un hôtel ici : on ne l'écrase pas
    assert!(matches!(reprendre_hotel(&relais, ADRESSE, &trouve, &chemin), Err(Erreur::Validation(_))));
}

#[test]
fn une_reprise_interrompue_ne_laisse_rien_et_peut_recommencer() {
    let dossier = tempfile::tempdir().unwrap();
    let relais = RelaisMemoire::default();
    let mut h = portable(dossier.path());
    relier(&mut h, &relais);
    h.t.synchroniser_instantane(&relais, dossier.path()).unwrap();
    let trouve = trouver_hotel(&relais, "admin", MDP_ADMIN).unwrap();
    let chemin = dossier.path().join("hotel").join("tanana.db");
    relais.hors_ligne.store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(reprendre_hotel(&relais, ADRESSE, &trouve, &chemin).is_err());
    assert!(!chemin.exists());
    relais.hors_ligne.store(false, std::sync::atomic::Ordering::SeqCst);
    reprendre_hotel(&relais, ADRESSE, &trouve, &chemin).unwrap();
    let restes: Vec<_> = std::fs::read_dir(chemin.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("reprise-"))
        .collect();
    assert!(restes.is_empty(), "fichiers de travail oubliés : {restes:?}");
}

/// Poste à distance relié à l'hôtel, à la même heure que lui.
fn poste_distant(h: &mut Hotel, relais: &RelaisMemoire) -> Tanana {
    let (hotel_id, secret) = ouvrir_acces_distant(relais, "admin", MDP_ADMIN).unwrap();
    let mut copie = Tanana::copie_en_memoire(ADRESSE, &hotel_id, &secret).unwrap().avec_horloge(h.temps.horloge());
    copie.mettre_a_jour_copie(relais).unwrap();
    copie
}

/// Ce que fait le relais : remettre la demande à l'hôtel, et la réponse au poste.
fn a_distance(h: &mut Hotel, poste: &Tanana, commande: &str, a: Map<String, Value>) -> Result<Value, Erreur> {
    let (_, _, c) = poste.preparer_commande_distante(commande, a).unwrap();
    let reponse = h.t.traiter_commande_distante(&c).unwrap();
    poste.lire_reponse_distante(&c.id, &reponse)
}

#[test]
fn modifier_a_distance_avec_ses_propres_droits() {
    let mut h = hotel();
    let relais = RelaisMemoire::default();
    relier(&mut h, &relais);
    let (ch, cl) = h.base();
    let mut poste = poste_distant(&mut h, &relais);

    // Connexion à distance : mêmes identifiants qu'à l'hôtel
    assert_eq!(
        a_distance(&mut h, &poste, "connexion", args(json!({ "identifiant": "admin", "mot_de_passe": "faux" })))
            .unwrap_err(),
        Erreur::IdentifiantsInvalides
    );
    let session: SessionOuverte = serde_json::from_value(
        a_distance(&mut h, &poste, "connexion", args(json!({ "identifiant": "admin", "mot_de_passe": MDP_ADMIN })))
            .unwrap(),
    )
    .unwrap();
    assert!(session.permissions.contains(&tanana_core::Permission::ParametresGerer));

    // Une réservation faite depuis le Canada
    let r: Reservation = serde_json::from_value(
        a_distance(
            &mut h,
            &poste,
            "creer_reservation",
            args(json!({ "token": session.token, "donnees": {
                "chambre_id": ch.id, "client_id": cl.id, "date_arrivee": "2026-05-01", "date_depart": "2026-05-04",
                "prix_total": null, "notes": null
            }})),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(h.t.obtenir_reservation(&h.admin, &r.id).unwrap().id, r.id, "faite à l'hôtel");
    // Les règles de l'hôtel s'appliquent : pas de double réservation
    let doublon = a_distance(
        &mut h,
        &poste,
        "creer_reservation",
        args(json!({ "token": session.token, "donnees": {
            "chambre_id": ch.id, "client_id": cl.id, "date_arrivee": "2026-05-02", "date_depart": "2026-05-03",
            "prix_total": null, "notes": null
        }})),
    );
    assert!(matches!(doublon, Err(Erreur::ChambreDejaReservee { .. })), "{doublon:?}");

    // La copie suit dès le prochain envoi de l'hôtel
    h.t.synchroniser(&relais).unwrap();
    poste.mettre_a_jour_copie(&relais).unwrap();
    poste.definir_acces_copie(MDP_ADMIN).unwrap();
    let jc = poste.connexion_copie(MDP_ADMIN).unwrap().token;
    assert_eq!(poste.obtenir_reservation(&jc, &r.id).unwrap().id, r.id);

    // Le journal dit que la connexion venait d'ailleurs
    let journal = h.t.journal(&h.admin, FiltreJournal { limite: Some(50), ..Default::default() }).unwrap();
    assert!(
        journal
            .iter()
            .any(|e| e.action == "auth.connexion"
                && e.apres.as_ref().is_some_and(|a| a.to_string().contains("distance"))),
        "connexion à distance absente du journal"
    );
}

#[test]
fn une_demande_rejouee_ou_trop_vieille_est_refusee() {
    let mut h = hotel();
    let relais = RelaisMemoire::default();
    relier(&mut h, &relais);
    let poste = poste_distant(&mut h, &relais);
    let (_, _, c) = poste
        .preparer_commande_distante("connexion", args(json!({ "identifiant": "admin", "mot_de_passe": MDP_ADMIN })))
        .unwrap();
    let r1 = h.t.traiter_commande_distante(&c).unwrap();
    poste.lire_reponse_distante(&c.id, &r1).unwrap();
    // Le relais rejoue la même demande
    let r2 = h.t.traiter_commande_distante(&c).unwrap();
    assert!(matches!(poste.lire_reponse_distante(&c.id, &r2), Err(Erreur::Validation(m)) if m.contains("déjà faite")));
    // Demande modifiée en route : illisible, ignorée
    let mut abimee = c.clone();
    abimee.id = "autre-id".into();
    assert!(h.t.traiter_commande_distante(&abimee).is_err());
    // Demande préparée il y a longtemps
    let (_, _, vieille) = poste.preparer_commande_distante("ma_session", args(json!({ "token": "x" }))).unwrap();
    h.temps.avancer(chrono::Duration::minutes(30));
    let r3 = h.t.traiter_commande_distante(&vieille).unwrap();
    assert!(
        matches!(poste.lire_reponse_distante(&vieille.id, &r3), Err(Erreur::Validation(m)) if m.contains("trop ancienne"))
    );
}

#[test]
fn le_portable_remplace_ne_traite_plus_les_demandes() {
    let dossier = tempfile::tempdir().unwrap();
    let relais = RelaisMemoire::default();
    let mut h = portable(dossier.path());
    relier(&mut h, &relais);
    h.t.synchroniser_instantane(&relais, dossier.path()).unwrap();
    let trouve = trouver_hotel(&relais, "admin", MDP_ADMIN).unwrap();
    reprendre_hotel(&relais, ADRESSE, &trouve, &dossier.path().join("hotel.db")).unwrap();
    h.client("x");
    let _ = h.t.synchroniser(&relais);
    let poste = poste_distant(&mut h, &relais);
    let (_, _, c) = poste.preparer_commande_distante("ma_session", args(json!({ "token": "x" }))).unwrap();
    assert!(h.t.traiter_commande_distante(&c).is_err());
}
