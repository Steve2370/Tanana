//! Comptes, connexion, sessions, rôles et permissions.

mod commun;

use chrono::Duration;
use commun::*;
use tanana_core::modeles::*;
use tanana_core::{Erreur, Permission, Tanana};

// ─────────────────────────── Premier lancement ───────────────────────────

#[test]
fn premier_lancement() {
    let mut t = Tanana::en_memoire().unwrap();
    assert!(!t.est_initialise().unwrap());
    assert_eq!(t.nom_hotel().unwrap(), None);

    let code = t.initialiser(config()).unwrap();
    assert_eq!(code.len(), 23);
    assert!(t.est_initialise().unwrap());
    assert_eq!(t.nom_hotel().unwrap().as_deref(), Some("Hôtel Tanana"));

    // Une seule fois
    assert_eq!(t.initialiser(config()), Err(Erreur::DejaInitialise));

    let s = t.connexion("admin", MDP_ADMIN).unwrap();
    assert_eq!(s.utilisateur.role_id, "role-admin");
    assert_eq!(s.permissions.len(), Permission::TOUTES.len());
    let p = t.parametres(&s.token).unwrap();
    assert_eq!(p.devise, "XAF", "la devise est normalisée en majuscules");
}

#[test]
fn premier_lancement_refuse_les_donnees_invalides() {
    let mut t = Tanana::en_memoire().unwrap();
    let mut c = config();
    c.admin_mot_de_passe = "court".into();
    assert!(matches!(t.initialiser(c), Err(Erreur::Validation(_))));
    let mut c = config();
    c.devise = "FRANCS".into();
    assert!(matches!(t.initialiser(c), Err(Erreur::Validation(_))));
    let mut c = config();
    c.nom_hotel = "  ".into();
    assert!(matches!(t.initialiser(c), Err(Erreur::Validation(_))));
    assert!(!t.est_initialise().unwrap(), "rien n'a été écrit");
}

// ─────────────────────────────── Connexion ───────────────────────────────

#[test]
fn connexion_reussie_et_echouee() {
    let mut h = hotel();
    assert!(matches!(h.t.connexion("admin", "mauvais123"), Err(Erreur::IdentifiantsInvalides)));
    assert!(matches!(h.t.connexion("inconnu", "x"), Err(Erreur::IdentifiantsInvalides)));
    // identifiant insensible à la casse
    assert!(h.t.connexion("ADMIN", MDP_ADMIN).is_ok());
    let actions = h.actions();
    assert_eq!(actions.iter().filter(|a| *a == "auth.echec").count(), 2);
}

#[test]
fn blocage_apres_cinq_echecs_puis_deblocage() {
    let mut h = hotel();
    for _ in 0..5 {
        assert!(matches!(h.t.connexion("admin", "mauvais123"), Err(Erreur::IdentifiantsInvalides)));
    }
    // Même le bon mot de passe est refusé pendant le blocage
    assert!(matches!(h.t.connexion("admin", MDP_ADMIN), Err(Erreur::CompteBloque { .. })));
    assert!(h.actions().contains(&"auth.blocage".to_string()));

    h.temps.avancer(Duration::minutes(14));
    assert!(matches!(h.t.connexion("admin", MDP_ADMIN), Err(Erreur::CompteBloque { .. })));
    h.temps.avancer(Duration::minutes(2));
    assert!(h.t.connexion("admin", MDP_ADMIN).is_ok());
}

#[test]
fn une_connexion_reussie_remet_le_compteur_a_zero() {
    let mut h = hotel();
    for _ in 0..4 {
        let _ = h.t.connexion("admin", "mauvais123");
    }
    h.t.connexion("admin", MDP_ADMIN).unwrap();
    for _ in 0..4 {
        let _ = h.t.connexion("admin", "mauvais123");
    }
    assert!(h.t.connexion("admin", MDP_ADMIN).is_ok(), "pas de blocage : le compteur repartait de zéro");
}

#[test]
fn connexion_par_pin() {
    let mut h = hotel();
    let (_, emp) = h.employe("marie", "role-reception");
    assert!(matches!(h.t.connexion_pin("marie", "1234"), Err(Erreur::IdentifiantsInvalides)), "pas encore de PIN");

    assert!(matches!(h.t.definir_mon_pin(&emp, "mauvais123", "1234"), Err(Erreur::IdentifiantsInvalides)));
    assert!(matches!(h.t.definir_mon_pin(&emp, MDP_DEFINITIF, "12"), Err(Erreur::Validation(_))));
    h.t.definir_mon_pin(&emp, MDP_DEFINITIF, "4821").unwrap();

    let s = h.t.connexion_pin("marie", "4821").unwrap();
    assert!(s.utilisateur.a_un_pin);
    assert!(matches!(h.t.connexion_pin("marie", "0000"), Err(Erreur::IdentifiantsInvalides)));
}

// ──────────────────────────────── Sessions ────────────────────────────────

#[test]
fn session_expire_apres_inactivite() {
    let mut h = hotel();
    h.temps.avancer(Duration::minutes(10));
    assert!(h.t.ma_session(&h.admin).is_ok(), "activité rafraîchie à 10 min");
    h.temps.avancer(Duration::minutes(14));
    assert!(h.t.ma_session(&h.admin).is_ok(), "14 min après la dernière activité : encore valide");
    h.temps.avancer(Duration::minutes(16));
    assert_eq!(h.t.ma_session(&h.admin).map(|_| ()), Err(Erreur::SessionExpiree));
    // Une session expirée est fermée pour de bon
    assert_eq!(h.t.ma_session(&h.admin).map(|_| ()), Err(Erreur::NonAuthentifie));
}

#[test]
fn deconnexion_et_jeton_invalide() {
    let mut h = hotel();
    assert_eq!(h.t.ma_session("jeton-bidon").map(|_| ()), Err(Erreur::NonAuthentifie));
    let token = h.admin.clone();
    h.t.deconnexion(&token).unwrap();
    assert_eq!(h.t.ma_session(&token).map(|_| ()), Err(Erreur::NonAuthentifie));
}

#[test]
fn changement_de_mot_de_passe_obligatoire() {
    let mut h = hotel();
    h.t.creer_utilisateur(
        &h.admin,
        NouvelUtilisateur {
            identifiant: "paul".into(),
            nom: "Paul".into(),
            mot_de_passe_temporaire: MDP_TEMP.into(),
            role_id: "role-gerant".into(),
        },
    )
    .unwrap();
    let s = h.t.connexion("paul", MDP_TEMP).unwrap();
    assert!(s.utilisateur.doit_changer_mdp);

    // Toute action métier est bloquée…
    assert!(matches!(h.t.lister_chambres(&s.token), Err(Erreur::ChangementMdpRequis)));
    // …sauf consulter sa session et changer son mot de passe.
    assert!(h.t.ma_session(&s.token).is_ok());
    assert!(matches!(
        h.t.changer_mon_mot_de_passe(&s.token, "faux12345", MDP_DEFINITIF),
        Err(Erreur::IdentifiantsInvalides)
    ));
    assert!(matches!(h.t.changer_mon_mot_de_passe(&s.token, MDP_TEMP, MDP_TEMP), Err(Erreur::Validation(_))));
    h.t.changer_mon_mot_de_passe(&s.token, MDP_TEMP, MDP_DEFINITIF).unwrap();
    assert!(h.t.lister_chambres(&s.token).is_ok());
    assert!(h.t.connexion("paul", MDP_DEFINITIF).is_ok());
}

// ─────────────────────────── Récupération admin ───────────────────────────

#[test]
fn recuperation_par_code() {
    let mut h = hotel();
    let code = h.code_recuperation.clone();
    assert!(matches!(
        h.t.recuperer_acces_admin("AAAAA-BBBBB-CCCCC-DDDDD", "admin", "nouveau123"),
        Err(Erreur::IdentifiantsInvalides)
    ));

    // Saisie tolérante : minuscules et espaces acceptés
    let saisie = code.to_lowercase().replace('-', " ");
    let nouveau_code = h.t.recuperer_acces_admin(&saisie, "admin", "nouveau123").unwrap();
    assert_ne!(nouveau_code, code);
    assert!(h.t.connexion("admin", "nouveau123").is_ok());
    assert!(matches!(h.t.connexion("admin", MDP_ADMIN), Err(Erreur::IdentifiantsInvalides)));
    // L'ancienne session admin est fermée, l'ancien code ne marche plus
    assert_eq!(h.t.ma_session(&h.admin).map(|_| ()), Err(Erreur::NonAuthentifie));
    assert!(h.t.recuperer_acces_admin(&code, "admin", "encore123").is_err());
    assert!(h.t.recuperer_acces_admin(&nouveau_code, "admin", "encore123").is_ok());
}

#[test]
fn recuperation_debloque_un_compte_bloque() {
    let mut h = hotel();
    for _ in 0..5 {
        let _ = h.t.connexion("admin", "mauvais123");
    }
    let code = h.code_recuperation.clone();
    h.t.recuperer_acces_admin(&code, "admin", "nouveau123").unwrap();
    assert!(h.t.connexion("admin", "nouveau123").is_ok());
}

#[test]
fn le_code_ne_sert_pas_pour_un_employe() {
    let mut h = hotel();
    h.employe("marie", "role-reception");
    let code = h.code_recuperation.clone();
    assert!(matches!(h.t.recuperer_acces_admin(&code, "marie", "nouveau123"), Err(Erreur::IdentifiantsInvalides)));
}

// ─────────────────────────── Gestion des comptes ───────────────────────────

#[test]
fn creation_de_compte_et_doublon() {
    let mut h = hotel();
    h.employe("marie", "role-reception");
    let r = h.t.creer_utilisateur(
        &h.admin,
        NouvelUtilisateur {
            identifiant: "MARIE".into(),
            nom: "Autre".into(),
            mot_de_passe_temporaire: MDP_TEMP.into(),
            role_id: "role-reception".into(),
        },
    );
    assert!(matches!(r, Err(Erreur::Doublon(_))), "identifiant unique, insensible à la casse");
    let r = h.t.creer_utilisateur(
        &h.admin,
        NouvelUtilisateur {
            identifiant: "jean".into(),
            nom: "Jean".into(),
            mot_de_passe_temporaire: MDP_TEMP.into(),
            role_id: "role-inexistant".into(),
        },
    );
    assert!(matches!(r, Err(Erreur::Introuvable(_))));
    let noms: Vec<_> = h.t.lister_utilisateurs(&h.admin).unwrap().into_iter().map(|u| u.identifiant).collect();
    assert_eq!(noms.len(), 2);
}

#[test]
fn suspension_ferme_les_sessions_et_bloque_la_connexion() {
    let mut h = hotel();
    let (id, emp) = h.employe("marie", "role-reception");
    assert!(h.t.lister_chambres(&emp).is_ok());

    h.t.changer_statut_utilisateur(&h.admin, &id, StatutUtilisateur::Suspendu).unwrap();
    assert_eq!(h.t.lister_chambres(&emp).map(|_| ()), Err(Erreur::NonAuthentifie), "session fermée immédiatement");
    assert!(matches!(h.t.connexion("marie", MDP_DEFINITIF), Err(Erreur::CompteInactif)));
    // Un mauvais mot de passe ne révèle pas que le compte est suspendu
    assert!(matches!(h.t.connexion("marie", "mauvais123"), Err(Erreur::IdentifiantsInvalides)));

    h.t.changer_statut_utilisateur(&h.admin, &id, StatutUtilisateur::Actif).unwrap();
    assert!(h.t.connexion("marie", MDP_DEFINITIF).is_ok());
    let a = h.actions();
    assert!(a.contains(&"utilisateur.suspension".into()) && a.contains(&"utilisateur.reactivation".into()));
}

#[test]
fn reinitialisation_du_mot_de_passe_par_l_admin() {
    let mut h = hotel();
    let (id, emp) = h.employe("marie", "role-reception");
    h.t.definir_mon_pin(&emp, MDP_DEFINITIF, "1234").unwrap();
    h.t.reinitialiser_mot_de_passe(&h.admin, &id, "provisoire9").unwrap();
    assert_eq!(h.t.ma_session(&emp).map(|_| ()), Err(Erreur::NonAuthentifie));
    assert!(h.t.connexion_pin("marie", "1234").is_err(), "le PIN est effacé aussi");
    let s = h.t.connexion("marie", "provisoire9").unwrap();
    assert!(s.utilisateur.doit_changer_mdp);
}

#[test]
fn protections_de_l_administrateur() {
    let mut h = hotel();
    let admin_id = h.t.ma_session(&h.admin).unwrap().utilisateur.id;

    // Pas d'action sur soi-même
    assert_eq!(
        h.t.changer_statut_utilisateur(&h.admin, &admin_id, StatutUtilisateur::Suspendu).map(|_| ()),
        Err(Erreur::ActionSurSoiMeme)
    );
    assert_eq!(h.t.changer_role(&h.admin, &admin_id, "role-reception").map(|_| ()), Err(Erreur::ActionSurSoiMeme));

    // Avec deux admins, l'un peut suspendre l'autre, pas lui-même
    let (id2, admin2) = h.employe("admin2", "role-admin");
    h.t.changer_statut_utilisateur(&admin2, &admin_id, StatutUtilisateur::Suspendu).unwrap();
    assert_eq!(h.t.ma_session(&h.admin).map(|_| ()), Err(Erreur::NonAuthentifie));
    assert_eq!(
        h.t.changer_statut_utilisateur(&admin2, &id2, StatutUtilisateur::Suspendu).map(|_| ()),
        Err(Erreur::ActionSurSoiMeme)
    );
    h.t.changer_statut_utilisateur(&admin2, &admin_id, StatutUtilisateur::Actif).unwrap();
    let admin = h.t.connexion("admin", MDP_ADMIN).unwrap().token;
    h.t.changer_role(&admin, &id2, "role-gerant").unwrap();
    assert!(
        matches!(h.t.lister_utilisateurs(&admin2), Err(Erreur::PermissionRefusee { .. })),
        "rétrogradé : perd les droits admin sur sa session ouverte"
    );
}

#[test]
fn dernier_administrateur_protege() {
    let mut h = hotel();
    let admin_id = h.t.ma_session(&h.admin).unwrap().utilisateur.id;
    // Un gérant avec la permission de gérer les comptes ne peut pas retirer le seul admin
    let (gid, _) = h.employe("gerant", "role-gerant");
    h.t.definir_permission_perso(&h.admin, &gid, Permission::UtilisateurGerer, Some(true)).unwrap();
    let g = h.t.connexion("gerant", MDP_DEFINITIF).unwrap().token;
    assert_eq!(
        h.t.changer_statut_utilisateur(&g, &admin_id, StatutUtilisateur::Suspendu).map(|_| ()),
        Err(Erreur::DernierAdministrateur)
    );
    assert_eq!(h.t.changer_role(&g, &admin_id, "role-gerant").map(|_| ()), Err(Erreur::DernierAdministrateur));
}

// ─────────────────────────────── Permissions ───────────────────────────────

#[test]
fn permissions_par_role() {
    let mut h = hotel();
    let (_, rec) = h.employe("marie", "role-reception");
    let (_, ger) = h.employe("paul", "role-gerant");

    assert!(h.t.lister_chambres(&rec).is_ok());
    assert!(matches!(h.t.lister_utilisateurs(&rec), Err(Erreur::PermissionRefusee { .. })));
    assert!(matches!(h.t.journal(&rec, FiltreJournal::default()), Err(Erreur::PermissionRefusee { .. })));
    assert!(matches!(
        h.t.rapport_caisse(&rec, d("2026-03-01"), d("2026-04-01"), 0),
        Err(Erreur::PermissionRefusee { .. })
    ));

    assert!(h.t.rapport_caisse(&ger, d("2026-03-01"), d("2026-04-01"), 0).is_ok());
    assert!(matches!(h.t.lister_utilisateurs(&ger), Err(Erreur::PermissionRefusee { .. })));
}

#[test]
fn acces_refuse_est_journalise() {
    let mut h = hotel();
    let (id, rec) = h.employe("marie", "role-reception");
    let _ = h.t.lister_utilisateurs(&rec);
    let e =
        h.t.journal(&h.admin, FiltreJournal { action_prefixe: Some("acces.".into()), ..Default::default() }).unwrap();
    assert_eq!(e.len(), 1);
    assert_eq!(e[0].utilisateur_id.as_deref(), Some(id.as_str()));
    assert!(e[0].sensible);
    assert_eq!(e[0].apres.as_ref().unwrap()["permission"], "utilisateur.gerer");
}

#[test]
fn permissions_individuelles() {
    let mut h = hotel();
    let (id, rec) = h.employe("marie", "role-reception");

    // Restreindre : retirer l'encaissement à cette réceptionniste seulement
    h.t.definir_permission_perso(&h.admin, &id, Permission::PaiementEncaisser, Some(false)).unwrap();
    // Étendre : lui donner l'accès aux dépenses
    let perso = h.t.definir_permission_perso(&h.admin, &id, Permission::DepenseVoir, Some(true)).unwrap();
    assert_eq!(perso.len(), 2);

    let perms = h.t.permissions_utilisateur(&h.admin, &id).unwrap();
    assert!(!perms.contains(&Permission::PaiementEncaisser));
    assert!(perms.contains(&Permission::DepenseVoir));
    assert!(perms.contains(&Permission::ReservationCreer), "le reste du rôle est conservé");
    // Effet immédiat sur la session ouverte
    assert!(h.t.lister_depenses(&rec, d("2026-03-01"), d("2026-04-01")).is_ok());

    // Revenir au rôle
    h.t.definir_permission_perso(&h.admin, &id, Permission::DepenseVoir, None).unwrap();
    assert!(matches!(
        h.t.lister_depenses(&rec, d("2026-03-01"), d("2026-04-01")),
        Err(Erreur::PermissionRefusee { .. })
    ));
    assert_eq!(h.t.permissions_perso(&h.admin, &id).unwrap().len(), 1);
}

#[test]
fn un_admin_ne_peut_pas_etre_restreint() {
    let mut h = hotel();
    let (id2, _) = h.employe("admin2", "role-admin");
    assert!(matches!(
        h.t.definir_permission_perso(&h.admin, &id2, Permission::JournalVoir, Some(false)),
        Err(Erreur::Validation(_))
    ));
}

#[test]
fn roles_personnalises() {
    let mut h = hotel();
    let roles = h.t.lister_roles(&h.admin).unwrap();
    assert_eq!(roles.len(), 3);
    assert!(roles[0].systeme);

    let caissier =
        h.t.creer_role(
            &h.admin,
            "Caissier",
            vec![Permission::ReservationVoir, Permission::PaiementEncaisser, Permission::PaiementEncaisser],
        )
        .unwrap();
    assert_eq!(caissier.permissions.len(), 2, "doublons ignorés");
    assert!(matches!(h.t.creer_role(&h.admin, "Caissier", vec![]), Err(Erreur::Doublon(_))));

    let (_, tok) = h.employe("caisse1", &caissier.id);
    assert!(matches!(h.t.lister_chambres(&tok), Err(Erreur::PermissionRefusee { .. })));
    h.t.modifier_permissions_role(&h.admin, &caissier.id, vec![Permission::ChambreVoir]).unwrap();
    assert!(h.t.lister_chambres(&tok).is_ok(), "la modification du rôle s'applique tout de suite");

    assert!(matches!(h.t.modifier_permissions_role(&h.admin, "role-admin", vec![]), Err(Erreur::Validation(_))));
}

#[test]
fn qui_est_connecte_et_deconnexion_forcee() {
    let mut h = hotel();
    let (_, rec) = h.employe("marie", "role-reception");
    let sessions = h.t.sessions_actives(&h.admin).unwrap();
    assert!(sessions.iter().any(|s| s.utilisateur_nom == "Employé marie"));

    let sid = sessions.iter().find(|s| s.utilisateur_nom == "Employé marie").unwrap().session_id.clone();
    h.t.fermer_session(&h.admin, &sid).unwrap();
    assert_eq!(h.t.lister_chambres(&rec).map(|_| ()), Err(Erreur::NonAuthentifie));
    assert!(matches!(h.t.fermer_session(&h.admin, &sid), Err(Erreur::Introuvable(_))));

    // Les sessions inactives n'apparaissent plus
    h.temps.avancer(Duration::minutes(10));
    h.t.ma_session(&h.admin).unwrap();
    h.temps.avancer(Duration::minutes(10));
    let n = h.t.sessions_actives(&h.admin).unwrap().len();
    assert_eq!(n, 1, "seul l'admin, actif à l'instant");
}
