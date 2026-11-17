//! Journal d'audit (traçabilité complète), paramètres, sauvegardes, chiffrement.

mod commun;

use commun::*;
use tanana_core::modeles::*;
use tanana_core::{CleBase, Erreur, Permission, Tanana};

#[test]
fn chaque_action_est_tracee_avec_son_auteur() {
    let mut h = hotel();
    let (marie_id, rec) = h.employe("marie", "role-reception");
    let (ch, cl) = h.base();
    let r =
        h.t.creer_reservation(
            &rec,
            NouvelleReservation {
                chambre_id: ch.id,
                client_id: cl.id,
                date_arrivee: d("2026-03-10"),
                date_depart: d("2026-03-11"),
                prix_total: None,
                notes: None,
            },
        )
        .unwrap();
    h.t.encaisser(
        &rec,
        Encaissement {
            reservation_id: r.id.clone(),
            montant: 1_000,
            type_paiement: TypePaiement::Acompte,
            mode: ModePaiement::Especes,
            note: None,
        },
    )
    .unwrap();

    assert_eq!(
        h.actions(),
        [
            "systeme.initialisation",
            "auth.connexion",
            "utilisateur.creation",
            "auth.connexion",
            "auth.changement_mdp",
            "type_chambre.creation",
            "chambre.creation",
            "client.creation",
            "reservation.creation",
            "paiement.encaissement",
        ]
    );

    let de_marie =
        h.t.journal(&h.admin, FiltreJournal { utilisateur_id: Some(marie_id.clone()), ..Default::default() }).unwrap();
    let actions: Vec<_> = de_marie.iter().map(|e| e.action.as_str()).collect();
    assert_eq!(actions, ["paiement.encaissement", "reservation.creation", "auth.changement_mdp", "auth.connexion"]);
    assert!(de_marie.iter().all(|e| e.utilisateur_nom.as_deref() == Some("Employé marie")));
    assert_eq!(de_marie[0].entite_id.as_deref(), Some(r.id.as_str()));
    assert_eq!(de_marie[0].apres.as_ref().unwrap()["solde"], 24_000, "25 000 − acompte de 1 000");
}

#[test]
fn aucun_secret_dans_le_journal() {
    let mut h = hotel();
    let (id, tok) = h.employe("marie", "role-reception");
    h.t.definir_mon_pin(&tok, MDP_DEFINITIF, "4821").unwrap();
    h.t.reinitialiser_mot_de_passe(&h.admin, &id, "provisoire9").unwrap();
    let tout: String =
        h.t.conn_brute()
            .prepare("SELECT coalesce(avant_json, '') || coalesce(apres_json, '') FROM journal_audit")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
    for secret in [MDP_ADMIN, MDP_TEMP, MDP_DEFINITIF, "4821", "provisoire9", "argon2", &h.admin] {
        assert!(!tout.contains(secret), "« {secret} » ne doit pas apparaître dans le journal");
    }
}

#[test]
fn filtres_pagination_et_actions_sensibles() {
    let mut h = hotel();
    for _ in 0..3 {
        let _ = h.t.connexion("admin", "mauvais123");
    }
    let tout = h.t.journal(&h.admin, FiltreJournal::default()).unwrap();
    assert_eq!(tout.len(), 5);
    assert!(tout.windows(2).all(|w| w[0].seq > w[1].seq), "du plus récent au plus ancien");

    let page1 = h.t.journal(&h.admin, FiltreJournal { limite: Some(2), ..Default::default() }).unwrap();
    let page2 =
        h.t.journal(&h.admin, FiltreJournal { limite: Some(2), avant_seq: Some(page1[1].seq), ..Default::default() })
            .unwrap();
    assert_eq!(page2[0].seq, page1[1].seq - 1);

    let sensibles = h.t.journal(&h.admin, FiltreJournal { sensibles_seulement: true, ..Default::default() }).unwrap();
    assert_eq!(sensibles.len(), 3);
    assert!(sensibles.iter().all(|e| e.action == "auth.echec"));

    let auth =
        h.t.journal(&h.admin, FiltreJournal { action_prefixe: Some("auth.".into()), ..Default::default() }).unwrap();
    assert_eq!(auth.len(), 4);
}

#[test]
fn integrite_du_journal_apres_beaucoup_d_actions() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    for i in 0..20 {
        let a = d("2026-03-01") + chrono::Duration::days(i * 2);
        let r =
            h.t.creer_reservation(
                &h.admin,
                NouvelleReservation {
                    chambre_id: ch.id.clone(),
                    client_id: cl.id.clone(),
                    date_arrivee: a,
                    date_depart: a + chrono::Duration::days(1),
                    prix_total: None,
                    notes: None,
                },
            )
            .unwrap();
        h.t.encaisser(
            &h.admin,
            Encaissement {
                reservation_id: r.id,
                montant: 1_000,
                type_paiement: TypePaiement::Acompte,
                mode: ModePaiement::Carte,
                note: None,
            },
        )
        .unwrap();
    }
    let rapport = h.t.verifier_integrite_journal(&h.admin).unwrap();
    assert!(rapport.intacte);
    assert!(rapport.entrees_verifiees > 40);

    // Falsification : un « petit malin » efface un encaissement du journal
    let c = h.t.conn_brute();
    c.execute_batch("DROP TRIGGER journal_pas_de_suppression;").unwrap();
    c.execute("DELETE FROM journal_audit WHERE seq = (SELECT min(seq) FROM journal_audit WHERE action = 'paiement.encaissement')", []).unwrap();
    let rapport = h.t.verifier_integrite_journal(&h.admin).unwrap();
    assert!(!rapport.intacte);
    assert!(rapport.premiere_rupture.is_some());
}

#[test]
fn journal_reserve_aux_autorises() {
    let mut h = hotel();
    let (_, ger) = h.employe("paul", "role-gerant");
    assert!(matches!(h.t.journal(&ger, FiltreJournal::default()), Err(Erreur::PermissionRefusee { .. })));
    assert!(matches!(h.t.verifier_integrite_journal(&ger), Err(Erreur::PermissionRefusee { .. })));
    // Mais l'admin peut le lui ouvrir individuellement
    let id = h.t.ma_session(&ger).unwrap().utilisateur.id;
    h.t.definir_permission_perso(&h.admin, &id, Permission::JournalVoir, Some(true)).unwrap();
    assert!(h.t.journal(&ger, FiltreJournal::default()).is_ok());
}

#[test]
fn parametres() {
    let mut h = hotel();
    let (_, rec) = h.employe("marie", "role-reception");
    let p = h.t.parametres(&rec).unwrap();
    assert_eq!(
        p,
        Parametres {
            nom_hotel: "Hôtel Tanana".into(),
            devise: "XAF".into(),
            decimales_devise: 0,
            delai_inactivite_minutes: 15,
            adresse_hotel: None,
            telephone_hotel: None,
            email_hotel: None,
            mentions_documents: None,
        }
    );

    let nouveau = Parametres {
        nom_hotel: "Hôtel du Lac".into(),
        devise: "cad".into(),
        decimales_devise: 2,
        delai_inactivite_minutes: 30,
        adresse_hotel: Some("  12 rue du Lac, Yaoundé ".into()),
        telephone_hotel: Some("+237 222 00 00 00".into()),
        email_hotel: Some("".into()),
        mentions_documents: Some("NIU : M0123456789".into()),
    };
    assert!(matches!(h.t.modifier_parametres(&rec, nouveau.clone()), Err(Erreur::PermissionRefusee { .. })));
    let p = h.t.modifier_parametres(&h.admin, nouveau.clone()).unwrap();
    assert_eq!(p.devise, "CAD");
    assert_eq!(h.t.nom_hotel().unwrap().as_deref(), Some("Hôtel du Lac"));
    assert_eq!(p.adresse_hotel.as_deref(), Some("12 rue du Lac, Yaoundé"), "espaces retirés");
    assert_eq!(p.email_hotel, None, "un champ vidé redevient absent");
    assert_eq!(h.t.parametres(&rec).unwrap().mentions_documents.as_deref(), Some("NIU : M0123456789"));
    for invalide in [
        Parametres { devise: "CA".into(), ..nouveau.clone() },
        Parametres { decimales_devise: 4, ..nouveau.clone() },
        Parametres { delai_inactivite_minutes: 0, ..nouveau.clone() },
        Parametres { nom_hotel: " ".into(), ..nouveau.clone() },
        Parametres { email_hotel: Some("hotel.cm".into()), ..nouveau.clone() },
        Parametres { mentions_documents: Some("x".repeat(401)), ..nouveau.clone() },
    ] {
        assert!(matches!(h.t.modifier_parametres(&h.admin, invalide), Err(Erreur::Validation(_))));
    }
    // Le nouveau délai d'inactivité s'applique
    h.temps.avancer(chrono::Duration::minutes(20));
    assert!(h.t.ma_session(&h.admin).is_ok());
}

// ─────────────────────────────── Sauvegardes ───────────────────────────────

fn hotel_sur_disque(chemin: &std::path::Path, cle: Option<CleBase>) -> (Tanana, String) {
    let mut t = Tanana::ouvrir(chemin, cle).unwrap();
    t.initialiser(config()).unwrap();
    let tok = t.connexion("admin", MDP_ADMIN).unwrap().token;
    t.creer_client(&tok, DonneesClient { nom: "Client sauvegardé".into(), ..Default::default() }).unwrap();
    (t, tok)
}

#[test]
fn sauvegarde_manuelle_et_restauration() {
    let dir = tempfile::tempdir().unwrap();
    let (mut t, tok) = hotel_sur_disque(&dir.path().join("tanana.db"), None);
    let copie = dir.path().join("cle-usb").join("copie.db");
    std::fs::create_dir_all(copie.parent().unwrap()).unwrap();
    t.sauvegarder_vers(&tok, &copie).unwrap();
    assert!(matches!(t.sauvegarder_vers(&tok, &copie), Err(Erreur::Doublon(_))), "n'écrase jamais un fichier");

    // « Restaurer » = ouvrir la copie comme base principale
    let mut restauree = Tanana::ouvrir(&copie, None).unwrap();
    let tok2 = restauree.connexion("admin", MDP_ADMIN).unwrap().token;
    let clients = restauree.rechercher_clients(&tok2, "sauvegardé").unwrap();
    assert_eq!(clients.len(), 1);
    assert!(restauree.verifier_integrite_journal(&tok2).unwrap().intacte);
    assert!(t.journal(&tok, FiltreJournal::default()).unwrap().iter().any(|e| e.action == "sauvegarde.manuelle"));
}

#[test]
fn sauvegarde_automatique_quotidienne_avec_rotation() {
    let dir = tempfile::tempdir().unwrap();
    let temps = Temps::nouveau();
    let (t, _) = hotel_sur_disque(&dir.path().join("tanana.db"), None);
    let mut t = t.avec_horloge(temps.horloge());
    let dossier = dir.path().join("sauvegardes");

    let f = t.sauvegarde_automatique(&dossier, 3).unwrap();
    assert!(f.unwrap().ends_with("tanana-sauvegarde-2026-03-10.db"));
    assert_eq!(t.sauvegarde_automatique(&dossier, 3).unwrap(), None, "une seule par jour");

    for _ in 0..4 {
        temps.avancer(chrono::Duration::days(1));
        assert!(t.sauvegarde_automatique(&dossier, 3).unwrap().is_some());
    }
    let mut noms: Vec<_> =
        std::fs::read_dir(&dossier).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    noms.sort();
    assert_eq!(
        noms,
        ["tanana-sauvegarde-2026-03-12.db", "tanana-sauvegarde-2026-03-13.db", "tanana-sauvegarde-2026-03-14.db",]
    );
}

#[test]
fn la_sauvegarde_requiert_la_permission() {
    let dir = tempfile::tempdir().unwrap();
    let (mut t, admin) = hotel_sur_disque(&dir.path().join("tanana.db"), None);
    t.creer_utilisateur(
        &admin,
        NouvelUtilisateur {
            identifiant: "marie".into(),
            nom: "Marie".into(),
            mot_de_passe_temporaire: MDP_TEMP.into(),
            role_id: "role-reception".into(),
        },
    )
    .unwrap();
    let rec = t.connexion("marie", MDP_TEMP).unwrap().token;
    t.changer_mon_mot_de_passe(&rec, MDP_TEMP, MDP_DEFINITIF).unwrap();
    assert!(matches!(t.sauvegarder_vers(&rec, &dir.path().join("x.db")), Err(Erreur::PermissionRefusee { .. })));
    assert!(!dir.path().join("x.db").exists());
}

#[test]
fn migrations_idempotentes_a_la_reouverture() {
    let dir = tempfile::tempdir().unwrap();
    let chemin = dir.path().join("tanana.db");
    drop(hotel_sur_disque(&chemin, None));
    let mut t = Tanana::ouvrir(&chemin, None).unwrap();
    assert!(t.est_initialise().unwrap());
    assert!(t.connexion("admin", MDP_ADMIN).is_ok());
    tanana_core::db::migrations().validate().unwrap();
}

#[test]
fn mise_a_jour_d_une_base_installee_avec_la_premiere_version() {
    use rusqlite_migration::{M, Migrations};
    let dir = tempfile::tempdir().unwrap();
    let chemin = dir.path().join("tanana.db");
    {
        // Une base telle qu'elle existe chez un client resté à la version 1
        let mut conn = rusqlite::Connection::open(&chemin).unwrap();
        Migrations::new(vec![M::up(include_str!("../migrations/0001_initial.sql"))]).to_latest(&mut conn).unwrap();
        conn.execute_batch(
            "INSERT INTO utilisateurs (id, identifiant, nom, mot_de_passe_hash, role_id, cree_le)
               VALUES ('u1', 'ancien', 'Ancien', 'x', 'role-admin', '2026-01-01T00:00:00.000Z');
             INSERT INTO depenses (id, categorie, montant, description, date, utilisateur_id, cree_le)
               VALUES ('d1', 'Eau', 8000, NULL, '2026-03-05', 'u1', '2026-03-05T10:00:00.000Z');",
        )
        .unwrap();
    }
    // La nouvelle version s'ouvre dessus : la migration 0002 s'applique, rien n'est perdu
    let t = Tanana::ouvrir(&chemin, None).unwrap();
    let (montant, annulee): (i64, bool) = t
        .conn_brute()
        .query_row("SELECT montant, annulee_le IS NOT NULL FROM depenses WHERE id = 'd1'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((montant, annulee), (8_000, false));
    assert!(t.est_initialise().unwrap());
}

// ─────────────────────────────── Chiffrement ───────────────────────────────

#[cfg(feature = "chiffrement")]
mod chiffrement {
    use super::*;

    #[test]
    fn base_illisible_sans_la_cle() {
        let dir = tempfile::tempdir().unwrap();
        let chemin = dir.path().join("tanana.db");
        let cle = CleBase::generer().unwrap();
        drop(hotel_sur_disque(&chemin, Some(cle.clone())));

        // Le fichier ne ressemble pas à une base SQLite et ne contient aucun texte en clair
        let octets = std::fs::read(&chemin).unwrap();
        assert!(!octets.starts_with(b"SQLite format 3"));
        assert!(!octets.windows(17).any(|w| w == "Client sauvegardé".as_bytes()));

        assert!(Tanana::ouvrir(&chemin, None).is_err(), "sans clé");
        assert!(Tanana::ouvrir(&chemin, Some(CleBase::generer().unwrap())).is_err(), "mauvaise clé");
        let mut t = Tanana::ouvrir(&chemin, Some(cle)).unwrap();
        assert!(t.connexion("admin", MDP_ADMIN).is_ok(), "bonne clé");
    }

    #[test]
    fn la_sauvegarde_est_chiffree_avec_la_meme_cle() {
        let dir = tempfile::tempdir().unwrap();
        let cle = CleBase::generer().unwrap();
        let (mut t, tok) = hotel_sur_disque(&dir.path().join("tanana.db"), Some(cle.clone()));
        let copie = dir.path().join("copie.db");
        t.sauvegarder_vers(&tok, &copie).unwrap();
        assert!(!std::fs::read(&copie).unwrap().starts_with(b"SQLite format 3"));
        assert!(Tanana::ouvrir(&copie, None).is_err());
        let mut r = Tanana::ouvrir(&copie, Some(cle)).unwrap();
        assert!(r.connexion("admin", MDP_ADMIN).is_ok());
    }

    #[test]
    fn export_de_la_cle_pour_un_autre_ordinateur() {
        let dir = tempfile::tempdir().unwrap();
        let cle = CleBase::generer().unwrap();
        let (mut t, admin) = hotel_sur_disque(&dir.path().join("tanana.db"), Some(cle.clone()));
        assert!(matches!(t.exporter_cle_base(&admin, "mauvais123"), Err(Erreur::IdentifiantsInvalides)));
        let exportee = t.exporter_cle_base(&admin, MDP_ADMIN).unwrap().unwrap();
        assert_eq!(exportee, cle.en_hex());
        let e = t
            .journal(
                &admin,
                FiltreJournal { action_prefixe: Some("parametres.export_cle".into()), ..Default::default() },
            )
            .unwrap();
        assert!(e[0].sensible, "l'export de la clé est une action sensible");

        t.creer_utilisateur(
            &admin,
            NouvelUtilisateur {
                identifiant: "paul".into(),
                nom: "Paul".into(),
                mot_de_passe_temporaire: MDP_TEMP.into(),
                role_id: "role-gerant".into(),
            },
        )
        .unwrap();
        let g = t.connexion("paul", MDP_TEMP).unwrap().token;
        t.changer_mon_mot_de_passe(&g, MDP_TEMP, MDP_DEFINITIF).unwrap();
        assert!(matches!(t.exporter_cle_base(&g, MDP_DEFINITIF), Err(Erreur::PermissionRefusee { .. })));

        // Sur un « autre ordinateur » : la clé exportée ouvre la copie
        let copie = dir.path().join("copie.db");
        t.sauvegarder_vers(&admin, &copie).unwrap();
        assert!(Tanana::ouvrir(&copie, Some(CleBase::depuis_hex(&exportee).unwrap())).is_ok());
    }

    #[test]
    fn format_de_cle() {
        let c = CleBase::generer().unwrap();
        assert_eq!(c.en_hex().len(), 64);
        assert_eq!(format!("{c:?}"), "CleBase(***)", "jamais affichée dans les logs");
        assert!(CleBase::depuis_hex(c.en_hex()).is_ok());
        assert!(CleBase::depuis_hex("trop-court").is_err());
        assert!(CleBase::depuis_hex(&"z".repeat(64)).is_err());
        // Recopiée depuis le papier, par groupes de 4, sur deux lignes
        let groupes: Vec<String> =
            c.en_hex().as_bytes().chunks(4).map(|g| String::from_utf8_lossy(g).to_uppercase()).collect();
        let papier = format!("{}\n{}", groupes[..8].join(" "), groupes[8..].join("-"));
        assert_eq!(CleBase::depuis_hex(&papier).unwrap().en_hex(), c.en_hex());
    }
}
