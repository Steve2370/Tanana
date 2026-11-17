//! Réservations : anti-double-réservation, cycle de vie, planning.

mod commun;

use commun::*;
use tanana_core::modeles::*;
use tanana_core::{Erreur, Tanana};

#[test]
fn creation_avec_prix_par_defaut_ou_negocie() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-13").unwrap();
    assert_eq!(r.nuits, 3);
    assert_eq!(r.prix_total, 75_000);
    assert_eq!(r.solde, 75_000);
    assert_eq!(r.statut, StatutReservation::Confirmee);
    assert_eq!(r.chambre_numero, "101");
    assert_eq!(r.client_nom, "Mbarga");

    let r2 =
        h.t.creer_reservation(
            &h.admin,
            NouvelleReservation {
                chambre_id: ch.id.clone(),
                client_id: cl.id.clone(),
                date_arrivee: d("2026-04-01"),
                date_depart: d("2026-04-03"),
                prix_total: Some(40_000),
                notes: Some("  tarif fidélité  ".into()),
            },
        )
        .unwrap();
    assert_eq!(r2.prix_total, 40_000);
    assert_eq!(r2.notes.as_deref(), Some("tarif fidélité"));
}

#[test]
fn double_reservation_refusee() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r1 = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-15").unwrap();

    for (a, dep) in [
        ("2026-03-10", "2026-03-15"), // identique
        ("2026-03-12", "2026-03-13"), // incluse
        ("2026-03-08", "2026-03-11"), // déborde avant
        ("2026-03-14", "2026-03-20"), // déborde après
        ("2026-03-01", "2026-03-31"), // englobe
    ] {
        let e = h.resa(&ch.id, &cl.id, a, dep).unwrap_err();
        assert_eq!(e, Erreur::ChambreDejaReservee { reservation_en_conflit: Some(r1.id.clone()) }, "{a} → {dep}");
    }
    // Séjours qui se touchent : autorisés
    assert!(h.resa(&ch.id, &cl.id, "2026-03-15", "2026-03-17").is_ok(), "arrive le jour du départ");
    assert!(h.resa(&ch.id, &cl.id, "2026-03-07", "2026-03-10").is_ok(), "part le jour de l'arrivée");
}

#[test]
fn une_autre_chambre_reste_libre() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let ch2 = h.chambre("102", &ch.type_id);
    h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-15").unwrap();
    assert!(h.resa(&ch2.id, &cl.id, "2026-03-10", "2026-03-15").is_ok());
}

#[test]
fn le_trigger_sql_bloque_meme_sans_passer_par_le_code() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-15").unwrap();
    let admin_id = h.t.ma_session(&h.admin).unwrap().utilisateur.id;
    let r = h.t.conn_brute().execute(
        "INSERT INTO reservations (id, chambre_id, client_id, date_arrivee, date_depart, prix_total, cree_le, cree_par)
         VALUES ('pirate', ?1, ?2, '2026-03-12', '2026-03-14', 0, 'x', ?3)",
        [&ch.id, &cl.id, &admin_id],
    );
    assert!(r.unwrap_err().to_string().contains("CHAMBRE_DEJA_RESERVEE"));
    // Idem pour une modification de dates en SQL brut
    h.resa(&ch.id, &cl.id, "2026-03-20", "2026-03-22").unwrap();
    let r =
        h.t.conn_brute()
            .execute("UPDATE reservations SET date_arrivee = '2026-03-14' WHERE date_arrivee = '2026-03-20'", []);
    assert!(r.unwrap_err().to_string().contains("CHAMBRE_DEJA_RESERVEE"));
}

#[test]
fn reservation_concurrente_depuis_plusieurs_connexions() {
    // Dix connexions sur le même fichier tentent la même chambre au même moment :
    // exactement une doit réussir.
    let dossier = tempfile::tempdir().unwrap();
    let chemin = dossier.path().join("tanana.db");
    let (ch, cl, token) = {
        let mut t = Tanana::ouvrir(&chemin, None).unwrap();
        t.initialiser(config()).unwrap();
        let token = t.connexion("admin", MDP_ADMIN).unwrap().token;
        let tc = t
            .creer_type_chambre(&token, NouveauTypeChambre { nom: "S".into(), prix_nuit_defaut: 1, capacite: 1 })
            .unwrap();
        let ch = t
            .creer_chambre(
                &token,
                DonneesChambre {
                    numero: "1".into(),
                    type_id: tc.id,
                    etage: None,
                    statut: StatutChambre::Disponible,
                    notes: None,
                },
            )
            .unwrap();
        let cl = t.creer_client(&token, DonneesClient { nom: "X".into(), ..Default::default() }).unwrap();
        (ch.id, cl.id, token)
    };

    let barriere = std::sync::Arc::new(std::sync::Barrier::new(10));
    let fils: Vec<_> = (0..10)
        .map(|_| {
            let (chemin, ch, cl, token, b) = (chemin.clone(), ch.clone(), cl.clone(), token.clone(), barriere.clone());
            std::thread::spawn(move || {
                let mut t = Tanana::ouvrir(&chemin, None).unwrap();
                b.wait();
                t.creer_reservation(
                    &token,
                    NouvelleReservation {
                        chambre_id: ch,
                        client_id: cl,
                        date_arrivee: d("2026-03-10"),
                        date_depart: d("2026-03-12"),
                        prix_total: None,
                        notes: None,
                    },
                )
            })
        })
        .collect();
    let resultats: Vec<_> = fils.into_iter().map(|f| f.join().unwrap()).collect();
    let reussies = resultats.iter().filter(|r| r.is_ok()).count();
    assert_eq!(reussies, 1, "{resultats:?}");
    assert!(resultats.iter().filter_map(|r| r.as_ref().err()).all(|e| matches!(e, Erreur::ChambreDejaReservee { .. })));
}

#[test]
fn dates_et_references_invalides() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    assert!(matches!(h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-10"), Err(Erreur::Validation(_))));
    assert!(matches!(h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-09"), Err(Erreur::Validation(_))));
    assert!(matches!(h.resa(&ch.id, &cl.id, "2026-03-10", "2027-03-12"), Err(Erreur::Validation(_))));
    assert!(matches!(h.resa("inconnue", &cl.id, "2026-03-10", "2026-03-11"), Err(Erreur::Introuvable(_))));
    assert!(matches!(h.resa(&ch.id, "inconnu", "2026-03-10", "2026-03-11"), Err(Erreur::Introuvable(_))));
    let r = h.t.creer_reservation(
        &h.admin,
        NouvelleReservation {
            chambre_id: ch.id.clone(),
            client_id: cl.id.clone(),
            date_arrivee: d("2026-03-10"),
            date_depart: d("2026-03-11"),
            prix_total: Some(-1),
            notes: None,
        },
    );
    assert!(matches!(r, Err(Erreur::MontantInvalide(_))));
}

#[test]
fn chambre_en_maintenance_non_reservable() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    h.t.modifier_chambre(
        &h.admin,
        &ch.id,
        DonneesChambre {
            numero: "101".into(),
            type_id: ch.type_id.clone(),
            etage: Some(1),
            statut: StatutChambre::Maintenance,
            notes: Some("fuite".into()),
        },
    )
    .unwrap();
    assert_eq!(h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-11").map(|_| ()), Err(Erreur::ChambreIndisponible));
    assert!(h.t.chambres_disponibles(&h.admin, d("2026-03-10"), d("2026-03-11")).unwrap().is_empty());
}

#[test]
fn annulation_libere_la_chambre() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-15").unwrap();
    assert!(matches!(h.t.annuler_reservation(&h.admin, &r.id, "  "), Err(Erreur::Validation(_))), "motif obligatoire");
    let annulee = h.t.annuler_reservation(&h.admin, &r.id, "Client malade").unwrap();
    assert_eq!(annulee.statut, StatutReservation::Annulee);
    assert!(h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-15").is_ok());

    let e =
        h.t.journal(
            &h.admin,
            FiltreJournal { action_prefixe: Some("reservation.annulation".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(e[0].apres.as_ref().unwrap()["motif"], "Client malade");
    assert!(e[0].sensible);
}

#[test]
fn cycle_arrivee_depart_et_solde() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();

    // Départ avant arrivée : impossible
    assert!(matches!(h.t.enregistrer_depart(&h.admin, &r.id), Err(Erreur::StatutInvalide { .. })));
    h.t.enregistrer_arrivee(&h.admin, &r.id).unwrap();
    // Un client arrivé ne s'annule pas
    assert!(matches!(h.t.annuler_reservation(&h.admin, &r.id, "x"), Err(Erreur::StatutInvalide { .. })));
    // Départ refusé tant que le solde n'est pas payé
    assert_eq!(h.t.enregistrer_depart(&h.admin, &r.id).map(|_| ()), Err(Erreur::SoldeImpaye { montant: 50_000 }));
    h.t.encaisser(
        &h.admin,
        Encaissement {
            reservation_id: r.id.clone(),
            montant: 50_000,
            type_paiement: TypePaiement::Solde,
            mode: ModePaiement::Especes,
            note: None,
        },
    )
    .unwrap();
    let partie = h.t.enregistrer_depart(&h.admin, &r.id).unwrap();
    assert_eq!(partie.statut, StatutReservation::Partie);
    assert_eq!(partie.solde, 0);
    assert!(matches!(h.t.enregistrer_arrivee(&h.admin, &r.id), Err(Erreur::StatutInvalide { .. })));
}

#[test]
fn arrivee_trop_tot_refusee() {
    let mut h = hotel(); // nous sommes le 2026-03-10
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-20", "2026-03-22").unwrap();
    assert!(matches!(h.t.enregistrer_arrivee(&h.admin, &r.id), Err(Erreur::Validation(_))));
    let r = h.resa(&ch.id, &cl.id, "2026-03-11", "2026-03-12").unwrap();
    assert!(h.t.enregistrer_arrivee(&h.admin, &r.id).is_ok(), "un jour d'avance est toléré");
}

#[test]
fn modification_de_reservation() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let ch2 = h.chambre("102", &ch.type_id);
    let r1 = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-15").unwrap();
    let r2 = h.resa(&ch2.id, &cl.id, "2026-03-12", "2026-03-14").unwrap();

    let modif = |ch: &str, a: &str, dep: &str| ModificationReservation {
        chambre_id: ch.into(),
        date_arrivee: d(a),
        date_depart: d(dep),
        prix_total: 100_000,
        notes: None,
    };
    // Prolonger r1 sur ses propres dates : pas de conflit avec elle-même
    let m = h.t.modifier_reservation(&h.admin, &r1.id, modif(&ch.id, "2026-03-10", "2026-03-17")).unwrap();
    assert_eq!(m.nuits, 7);
    assert_eq!(m.prix_total, 100_000);
    // Déplacer r1 dans la chambre de r2 : conflit
    assert_eq!(
        h.t.modifier_reservation(&h.admin, &r1.id, modif(&ch2.id, "2026-03-10", "2026-03-17")).map(|_| ()),
        Err(Erreur::ChambreDejaReservee { reservation_en_conflit: Some(r2.id.clone()) })
    );

    // Client arrivé : la date d'arrivée est figée, le départ peut bouger
    h.t.enregistrer_arrivee(&h.admin, &r1.id).unwrap();
    assert!(matches!(
        h.t.modifier_reservation(&h.admin, &r1.id, modif(&ch.id, "2026-03-11", "2026-03-17")),
        Err(Erreur::Validation(_))
    ));
    assert!(h.t.modifier_reservation(&h.admin, &r1.id, modif(&ch.id, "2026-03-10", "2026-03-16")).is_ok());

    let e =
        h.t.journal(
            &h.admin,
            FiltreJournal { action_prefixe: Some("reservation.modification".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(e.len(), 2);
    assert_eq!(e[1].avant.as_ref().unwrap()["date_depart"], "2026-03-15", "l'avant est conservé");
    assert_eq!(e[1].apres.as_ref().unwrap()["date_depart"], "2026-03-17");
}

#[test]
fn planning_et_disponibilites() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let ch2 = h.chambre("102", &ch.type_id);
    let ch3 = h.chambre("103", &ch.type_id);
    h.resa(&ch.id, &cl.id, "2026-03-01", "2026-03-11").unwrap(); // finit dans la fenêtre
    h.resa(&ch2.id, &cl.id, "2026-03-14", "2026-03-20").unwrap(); // commence dans la fenêtre
    h.resa(&ch3.id, &cl.id, "2026-03-20", "2026-03-25").unwrap(); // hors fenêtre
    let annulee = h.resa(&ch3.id, &cl.id, "2026-03-11", "2026-03-12").unwrap();
    h.t.annuler_reservation(&h.admin, &annulee.id, "test").unwrap();

    let p = h.t.planning(&h.admin, d("2026-03-10"), d("2026-03-17")).unwrap();
    let numeros: Vec<_> = p.iter().map(|r| r.chambre_numero.as_str()).collect();
    assert_eq!(numeros, ["101", "102"]);
    assert!(h.t.planning(&h.admin, d("2026-03-10"), d("2026-03-10")).is_err());

    let libres: Vec<_> =
        h.t.chambres_disponibles(&h.admin, d("2026-03-10"), d("2026-03-14"))
            .unwrap()
            .into_iter()
            .map(|c| c.numero)
            .collect();
    assert_eq!(libres, ["102", "103"]);
    let libres: Vec<_> =
        h.t.chambres_disponibles(&h.admin, d("2026-03-11"), d("2026-03-14"))
            .unwrap()
            .into_iter()
            .map(|c| c.numero)
            .collect();
    assert_eq!(libres, ["101", "102", "103"]);
}

#[test]
fn historique_client() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let autre = h.client("Ngono");
    h.resa(&ch.id, &cl.id, "2026-03-01", "2026-03-02").unwrap();
    h.resa(&ch.id, &cl.id, "2026-05-01", "2026-05-02").unwrap();
    h.resa(&ch.id, &autre.id, "2026-04-01", "2026-04-02").unwrap();
    let v = h.t.reservations_du_client(&h.admin, &cl.id).unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].date_arrivee, d("2026-05-01"), "plus récente d'abord");
}

#[test]
fn permissions_de_la_reception() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let (_, rec) = h.employe("marie", "role-reception");
    let r =
        h.t.creer_reservation(
            &rec,
            NouvelleReservation {
                chambre_id: ch.id.clone(),
                client_id: cl.id.clone(),
                date_arrivee: d("2026-03-10"),
                date_depart: d("2026-03-11"),
                prix_total: None,
                notes: None,
            },
        )
        .unwrap();
    assert!(matches!(h.t.annuler_reservation(&rec, &r.id, "x"), Err(Erreur::PermissionRefusee { .. })));
    assert!(h.t.enregistrer_arrivee(&rec, &r.id).is_ok());
}

// ─────────────────────────── Chambres et clients ───────────────────────────

#[test]
fn chambres_et_types() {
    let mut h = hotel();
    let tc = h.type_chambre("Suite", 60_000);
    assert!(matches!(
        h.t.creer_type_chambre(&h.admin, NouveauTypeChambre { nom: "suite".into(), prix_nuit_defaut: 1, capacite: 1 }),
        Err(Erreur::Doublon(_))
    ));
    assert!(matches!(
        h.t.creer_type_chambre(&h.admin, NouveauTypeChambre { nom: "X".into(), prix_nuit_defaut: -1, capacite: 1 }),
        Err(Erreur::MontantInvalide(_))
    ));
    assert!(matches!(
        h.t.creer_type_chambre(&h.admin, NouveauTypeChambre { nom: "X".into(), prix_nuit_defaut: 1, capacite: 0 }),
        Err(Erreur::Validation(_))
    ));
    for n in ["10", "2", "1"] {
        h.chambre(n, &tc.id);
    }
    let numeros: Vec<_> = h.t.lister_chambres(&h.admin).unwrap().into_iter().map(|c| c.numero).collect();
    assert_eq!(numeros, ["1", "2", "10"], "tri naturel");
    assert!(matches!(
        h.t.creer_chambre(
            &h.admin,
            DonneesChambre {
                numero: "10".into(),
                type_id: tc.id.clone(),
                etage: None,
                statut: StatutChambre::Disponible,
                notes: None
            }
        ),
        Err(Erreur::Doublon(_))
    ));
    let modif =
        h.t.modifier_type_chambre(
            &h.admin,
            &tc.id,
            NouveauTypeChambre { nom: "Suite royale".into(), prix_nuit_defaut: 80_000, capacite: 4 },
            true,
        )
        .unwrap();
    assert_eq!(modif.prix_nuit_defaut, 80_000);
    assert_eq!(h.t.lister_types_chambre(&h.admin).unwrap().len(), 1);
}

#[test]
fn changer_le_prix_d_un_type_ne_change_pas_les_reservations_existantes() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    h.t.modifier_type_chambre(
        &h.admin,
        &ch.type_id,
        NouveauTypeChambre { nom: "Standard".into(), prix_nuit_defaut: 99_000, capacite: 2 },
        true,
    )
    .unwrap();
    assert_eq!(h.t.obtenir_reservation(&h.admin, &r.id).unwrap().prix_total, 50_000);
}

#[test]
fn clients_recherche_et_validation() {
    let mut h = hotel();
    let c =
        h.t.creer_client(
            &h.admin,
            DonneesClient {
                nom: " Essomba ".into(),
                prenom: Some("Aline".into()),
                telephone: Some("+237 690 00 00 00".into()),
                email: Some("aline@exemple.cm".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(c.nom, "Essomba");
    h.client("Mbarga");

    for q in ["esso", "ALINE", "Aline Essomba", "690", "exemple.cm"] {
        let r = h.t.rechercher_clients(&h.admin, q).unwrap();
        assert_eq!(r.len(), 1, "recherche « {q} »");
    }
    // Le numéro se retrouve quelle que soit la façon de l'écrire
    for q in ["690000000", "690-00-00-00", "00237 690 00 00 00", "(237) 690.00.00.00"] {
        let r = h.t.rechercher_clients(&h.admin, q).unwrap();
        assert_eq!(r.len(), 1, "recherche du numéro « {q} »");
    }
    assert!(h.t.rechercher_clients(&h.admin, "691000000").unwrap().is_empty());
    assert_eq!(h.t.rechercher_clients(&h.admin, "").unwrap().len(), 2);
    assert_eq!(h.t.rechercher_clients(&h.admin, "%").unwrap().len(), 2, "les jokers SQL sont neutralisés");

    assert!(matches!(
        h.t.creer_client(&h.admin, DonneesClient { nom: "".into(), ..Default::default() }),
        Err(Erreur::Validation(_))
    ));
    assert!(matches!(
        h.t.creer_client(
            &h.admin,
            DonneesClient { nom: "X".into(), email: Some("pas-un-mail".into()), ..Default::default() }
        ),
        Err(Erreur::Validation(_))
    ));

    let m =
        h.t.modifier_client(
            &h.admin,
            &c.id,
            DonneesClient { nom: "Essomba".into(), telephone: Some("".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(m.telephone, None, "champ vidé");
    let e = h
        .t
        .journal(&h.admin, FiltreJournal { action_prefixe: Some("client.modification".into()), ..Default::default() })
        .unwrap();
    assert_eq!(e[0].avant.as_ref().unwrap()["telephone"], "+237 690 00 00 00");
}
