//! Acomptes, soldes, remboursements, dépenses et rapport de caisse.

mod commun;

use chrono::Duration;
use commun::*;
use tanana_core::Erreur;
use tanana_core::modeles::*;

fn encaissement(resa: &str, montant: i64, t: TypePaiement, mode: ModePaiement) -> Encaissement {
    Encaissement { reservation_id: resa.into(), montant, type_paiement: t, mode, note: None }
}

#[test]
fn acompte_puis_solde() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-14").unwrap(); // 100 000
    let p = h.t.encaisser(&h.admin, encaissement(&r.id, 30_000, TypePaiement::Acompte, ModePaiement::Mobile)).unwrap();
    assert_eq!(p.type_paiement, TypePaiement::Acompte);
    assert_eq!(p.utilisateur_nom, "Beltran");

    let r = h.t.obtenir_reservation(&h.admin, &r.id).unwrap();
    assert_eq!((r.total_encaisse, r.solde), (30_000, 70_000));

    // Pas plus que le solde
    assert!(matches!(
        h.t.encaisser(&h.admin, encaissement(&r.id, 70_001, TypePaiement::Solde, ModePaiement::Especes)),
        Err(Erreur::MontantInvalide(_))
    ));
    h.t.encaisser(&h.admin, encaissement(&r.id, 70_000, TypePaiement::Solde, ModePaiement::Especes)).unwrap();
    assert_eq!(h.t.obtenir_reservation(&h.admin, &r.id).unwrap().solde, 0);
    assert_eq!(h.t.paiements_reservation(&h.admin, &r.id).unwrap().len(), 2);
}

#[test]
fn montants_invalides() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-11").unwrap();
    for m in [0, -100] {
        assert!(matches!(
            h.t.encaisser(&h.admin, encaissement(&r.id, m, TypePaiement::Acompte, ModePaiement::Carte)),
            Err(Erreur::MontantInvalide(_))
        ));
    }
    assert!(
        matches!(
            h.t.encaisser(&h.admin, encaissement(&r.id, 100, TypePaiement::Remboursement, ModePaiement::Carte)),
            Err(Erreur::Validation(_))
        ),
        "un remboursement ne passe pas par l'encaissement"
    );
    assert!(matches!(
        h.t.encaisser(&h.admin, encaissement("inconnue", 100, TypePaiement::Acompte, ModePaiement::Carte)),
        Err(Erreur::Introuvable(_))
    ));
}

#[test]
fn pas_d_encaissement_sur_une_reservation_annulee() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-11").unwrap();
    h.t.annuler_reservation(&h.admin, &r.id, "test").unwrap();
    assert!(matches!(
        h.t.encaisser(&h.admin, encaissement(&r.id, 100, TypePaiement::Acompte, ModePaiement::Carte)),
        Err(Erreur::StatutInvalide { .. })
    ));
}

#[test]
fn remboursement_apres_annulation() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    h.t.encaisser(&h.admin, encaissement(&r.id, 20_000, TypePaiement::Acompte, ModePaiement::Especes)).unwrap();
    h.t.annuler_reservation(&h.admin, &r.id, "Annulé par le client").unwrap();

    assert!(
        matches!(h.t.rembourser(&h.admin, &r.id, 20_001, ModePaiement::Especes, "x"), Err(Erreur::MontantInvalide(_))),
        "jamais plus que l'encaissé"
    );
    assert!(
        matches!(h.t.rembourser(&h.admin, &r.id, 100, ModePaiement::Especes, " "), Err(Erreur::Validation(_))),
        "motif obligatoire"
    );
    h.t.rembourser(&h.admin, &r.id, 15_000, ModePaiement::Especes, "Retenue de 5 000 selon conditions").unwrap();
    assert!(matches!(
        h.t.rembourser(&h.admin, &r.id, 5_001, ModePaiement::Especes, "x"),
        Err(Erreur::MontantInvalide(_))
    ));

    let r = h.t.obtenir_reservation(&h.admin, &r.id).unwrap();
    assert_eq!(r.total_encaisse, 5_000);
    let e =
        h.t.journal(
            &h.admin,
            FiltreJournal { action_prefixe: Some("paiement.remboursement".into()), ..Default::default() },
        )
        .unwrap();
    assert_eq!(e.len(), 1);
    assert!(e[0].sensible);
    assert_eq!(e[0].apres.as_ref().unwrap()["motif"], "Retenue de 5 000 selon conditions");
}

#[test]
fn seuls_les_autorises_remboursent() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let (_, rec) = h.employe("marie", "role-reception");
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    h.t.encaisser(&rec, encaissement(&r.id, 10_000, TypePaiement::Acompte, ModePaiement::Especes)).unwrap();
    assert!(matches!(
        h.t.rembourser(&rec, &r.id, 10_000, ModePaiement::Especes, "x"),
        Err(Erreur::PermissionRefusee { .. })
    ));
}

#[test]
fn les_paiements_sont_immuables_dans_la_base() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    h.t.encaisser(&h.admin, encaissement(&r.id, 10_000, TypePaiement::Acompte, ModePaiement::Especes)).unwrap();
    let c = h.t.conn_brute();
    assert!(
        c.execute("UPDATE paiements SET montant = 1", []).unwrap_err().to_string().contains("PAIEMENT_NON_MODIFIABLE")
    );
    assert!(c.execute("DELETE FROM paiements", []).unwrap_err().to_string().contains("PAIEMENT_NON_MODIFIABLE"));
}

#[test]
fn trop_percu_apres_baisse_de_prix() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap(); // 50 000
    h.t.encaisser(&h.admin, encaissement(&r.id, 50_000, TypePaiement::Solde, ModePaiement::Carte)).unwrap();
    let m =
        h.t.modifier_reservation(
            &h.admin,
            &r.id,
            ModificationReservation {
                chambre_id: ch.id.clone(),
                date_arrivee: d("2026-03-10"),
                date_depart: d("2026-03-11"),
                prix_total: 25_000,
                notes: None,
            },
        )
        .unwrap();
    assert_eq!(m.solde, -25_000, "un solde négatif signale un trop-perçu à rembourser");
    h.t.rembourser(&h.admin, &r.id, 25_000, ModePaiement::Carte, "Séjour écourté").unwrap();
    assert_eq!(h.t.obtenir_reservation(&h.admin, &r.id).unwrap().solde, 0);
}

#[test]
fn depenses() {
    let mut h = hotel();
    let (_, ger) = h.employe("paul", "role-gerant");
    let dep =
        h.t.saisir_depense(
            &ger,
            NouvelleDepense {
                categorie: "Électricité".into(),
                montant: 45_000,
                description: Some("Facture ENEO".into()),
                date: d("2026-03-05"),
            },
        )
        .unwrap();
    assert_eq!(dep.utilisateur_nom, "Employé paul");
    h.t.saisir_depense(
        &ger,
        NouvelleDepense { categorie: "Ménage".into(), montant: 5_000, description: None, date: d("2026-04-01") },
    )
    .unwrap();
    assert!(matches!(
        h.t.saisir_depense(
            &ger,
            NouvelleDepense { categorie: "X".into(), montant: 0, description: None, date: d("2026-03-05") }
        ),
        Err(Erreur::MontantInvalide(_))
    ));
    assert!(matches!(
        h.t.saisir_depense(
            &ger,
            NouvelleDepense { categorie: " ".into(), montant: 1, description: None, date: d("2026-03-05") }
        ),
        Err(Erreur::Validation(_))
    ));

    let mars = h.t.lister_depenses(&ger, d("2026-03-01"), d("2026-04-01")).unwrap();
    assert_eq!(mars.len(), 1, "le 1er avril est exclu de mars");

    let (_, rec) = h.employe("marie", "role-reception");
    assert!(matches!(
        h.t.saisir_depense(
            &rec,
            NouvelleDepense { categorie: "X".into(), montant: 1, description: None, date: d("2026-03-05") }
        ),
        Err(Erreur::PermissionRefusee { .. })
    ));
}

#[test]
fn rapport_de_caisse() {
    let mut h = hotel(); // 10 mars 2026, 14 h UTC
    let (ch, cl) = h.base();
    let r1 = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-14").unwrap();
    h.t.encaisser(&h.admin, encaissement(&r1.id, 30_000, TypePaiement::Acompte, ModePaiement::Mobile)).unwrap();
    h.t.encaisser(&h.admin, encaissement(&r1.id, 20_000, TypePaiement::Solde, ModePaiement::Especes)).unwrap();
    h.t.rembourser(&h.admin, &r1.id, 5_000, ModePaiement::Especes, "geste commercial").unwrap();
    h.t.saisir_depense(
        &h.admin,
        NouvelleDepense { categorie: "Eau".into(), montant: 8_000, description: None, date: d("2026-03-10") },
    )
    .unwrap();

    let rap = h.t.rapport_caisse(&h.admin, d("2026-03-10"), d("2026-03-11"), 0).unwrap();
    assert_eq!(rap.encaissements, 50_000);
    assert_eq!(rap.remboursements, 5_000);
    assert_eq!(rap.depenses, 8_000);
    assert_eq!(rap.net, 37_000);
    assert_eq!(
        rap.encaissements_par_mode,
        vec![
            LigneMode { mode: ModePaiement::Especes, montant: 20_000 },
            LigneMode { mode: ModePaiement::Mobile, montant: 30_000 },
        ]
    );

    let vide = h.t.rapport_caisse(&h.admin, d("2026-03-11"), d("2026-03-12"), 0).unwrap();
    assert_eq!((vide.encaissements, vide.net), (0, 0));
    assert!(h.t.rapport_caisse(&h.admin, d("2026-03-11"), d("2026-03-11"), 0).is_err());
    assert!(h.t.rapport_caisse(&h.admin, d("2026-03-10"), d("2026-03-11"), 9999).is_err());
}

#[test]
fn rapport_respecte_le_fuseau_de_l_hotel() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-14").unwrap();
    // 10 mars, 23 h 30 à Montréal (UTC−5) = 11 mars 4 h 30 UTC
    h.temps.avancer(Duration::hours(14) + Duration::minutes(30));
    let tok = h.t.connexion("admin", MDP_ADMIN).unwrap().token;
    h.t.encaisser(&tok, encaissement(&r.id, 10_000, TypePaiement::Acompte, ModePaiement::Carte)).unwrap();

    let montreal_10 = h.t.rapport_caisse(&tok, d("2026-03-10"), d("2026-03-11"), -300).unwrap();
    assert_eq!(montreal_10.encaissements, 10_000, "compte dans la journée locale du 10");
    let utc_10 = h.t.rapport_caisse(&tok, d("2026-03-10"), d("2026-03-11"), 0).unwrap();
    assert_eq!(utc_10.encaissements, 0, "en UTC c'était déjà le 11");
    // Douala (UTC+1) : 5 h 30 le 11
    let douala_11 = h.t.rapport_caisse(&tok, d("2026-03-11"), d("2026-03-12"), 60).unwrap();
    assert_eq!(douala_11.encaissements, 10_000);
}

fn depense_test(h: &mut Hotel, token: &str, categorie: &str, montant: i64, jour: &str) -> Depense {
    h.t.saisir_depense(
        token,
        NouvelleDepense { categorie: categorie.into(), montant, description: None, date: d(jour) },
    )
    .unwrap()
}

#[test]
fn annulation_d_une_depense() {
    let mut h = hotel();
    let admin = h.admin.clone();
    let (_, ger) = h.employe("paul", "role-gerant");
    let (_, rec) = h.employe("marie", "role-reception");
    let erreur = depense_test(&mut h, &ger, "Eau", 80_000, "2026-03-10");
    let bonne = depense_test(&mut h, &ger, "Eau", 8_000, "2026-03-10");
    assert!(!erreur.annulee);
    assert_eq!(h.t.rapport_caisse(&admin, d("2026-03-10"), d("2026-03-11"), 0).unwrap().depenses, 88_000);

    // Raison obligatoire, permission exigée, dépense existante
    assert!(matches!(h.t.annuler_depense(&ger, &erreur.id, "  "), Err(Erreur::Validation(_))));
    assert!(matches!(h.t.annuler_depense(&rec, &erreur.id, "x"), Err(Erreur::PermissionRefusee { .. })));
    assert!(matches!(h.t.annuler_depense(&ger, "inconnue", "x"), Err(Erreur::Introuvable(_))));

    let annulee = h.t.annuler_depense(&ger, &erreur.id, "Un zéro en trop").unwrap();
    assert!(annulee.annulee);
    assert_eq!(annulee.motif_annulation.as_deref(), Some("Un zéro en trop"));
    assert_eq!(annulee.montant, 80_000, "le montant d'origine reste lisible");
    assert!(matches!(h.t.annuler_depense(&ger, &erreur.id, "encore"), Err(Erreur::Validation(_))), "une seule fois");

    // Elle sort des totaux mais reste dans la liste
    let rapport = h.t.rapport_caisse(&admin, d("2026-03-10"), d("2026-03-11"), 0).unwrap();
    assert_eq!(rapport.depenses, 8_000);
    assert_eq!(rapport.net, -8_000);
    let liste = h.t.lister_depenses(&ger, d("2026-03-10"), d("2026-03-11")).unwrap();
    assert_eq!(liste.len(), 2);
    assert_eq!(liste.iter().filter(|x| x.annulee).map(|x| x.id.as_str()).collect::<Vec<_>>(), [erreur.id.as_str()]);
    assert!(!liste.iter().find(|x| x.id == bonne.id).unwrap().annulee);

    // L'annulation est tracée comme action sensible, avec l'avant et l'après
    let e =
        h.t.journal(&admin, FiltreJournal { action_prefixe: Some("depense.annulation".into()), ..Default::default() })
            .unwrap();
    assert_eq!(e.len(), 1);
    assert!(e[0].sensible);
    assert_eq!(e[0].avant.as_ref().unwrap()["annulee"], false);
    assert_eq!(e[0].apres.as_ref().unwrap()["motif_annulation"], "Un zéro en trop");
}

#[test]
fn les_depenses_sont_figees_dans_la_base() {
    let mut h = hotel();
    let admin = h.admin.clone();
    let dep = depense_test(&mut h, &admin, "Eau", 8_000, "2026-03-10");
    h.t.annuler_depense(&admin, &dep.id, "erreur").unwrap();
    let c = h.t.conn_brute();
    for sql in [
        "UPDATE depenses SET montant = 1",
        "UPDATE depenses SET date = '2026-01-01'",
        "UPDATE depenses SET annulee_le = NULL", // on ne « désannule » pas en douce
        "DELETE FROM depenses",
    ] {
        assert!(c.execute(sql, []).unwrap_err().to_string().contains("DEPENSE_NON_MODIFIABLE"), "{sql}");
    }
}

#[test]
fn detail_des_paiements_de_la_periode() {
    let mut h = hotel(); // 10 mars 2026, 14 h UTC
    let admin = h.admin.clone();
    let (ch, cl) = h.base();
    let ch2 = h.chambre("102", &ch.type_id);
    let autre = h
        .t
        .creer_client(&admin, DonneesClient { nom: "Ngono".into(), prenom: Some("Paul".into()), ..Default::default() })
        .unwrap();
    let (_, rec) = h.employe("marie", "role-reception");
    let r1 = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-14").unwrap();
    let r2 = h.resa(&ch2.id, &autre.id, "2026-03-10", "2026-03-12").unwrap();

    h.t.encaisser(&rec, encaissement(&r1.id, 30_000, TypePaiement::Acompte, ModePaiement::Mobile)).unwrap();
    h.temps.avancer(Duration::minutes(5));
    h.t.encaisser(
        &admin,
        Encaissement {
            reservation_id: r2.id.clone(),
            montant: 50_000,
            type_paiement: TypePaiement::Solde,
            mode: ModePaiement::Especes,
            note: Some("Payé à l'arrivée".into()),
        },
    )
    .unwrap();
    h.temps.avancer(Duration::minutes(5));
    h.t.rembourser(&admin, &r1.id, 5_000, ModePaiement::Mobile, "geste commercial").unwrap();

    let liste = h.t.lister_paiements(&admin, d("2026-03-10"), d("2026-03-11"), 0).unwrap();
    // Du plus récent au plus ancien
    let resume: Vec<_> =
        liste.iter().map(|p| (p.type_paiement, p.montant, p.client_nom.as_str(), p.chambre_numero.as_str())).collect();
    assert_eq!(
        resume,
        [
            (TypePaiement::Remboursement, 5_000, "Mbarga", "101"),
            (TypePaiement::Solde, 50_000, "Ngono Paul", "102"),
            (TypePaiement::Acompte, 30_000, "Mbarga", "101"),
        ]
    );
    assert_eq!(liste[2].utilisateur_nom, "Employé marie");
    assert_eq!(liste[1].note.as_deref(), Some("Payé à l'arrivée"));
    assert_eq!(liste[0].note.as_deref(), Some("geste commercial"));
    assert_eq!(liste[0].reservation_id, r1.id);

    // Le détail et le rapport racontent la même chose
    let rapport = h.t.rapport_caisse(&admin, d("2026-03-10"), d("2026-03-11"), 0).unwrap();
    let entrees: i64 = liste.iter().filter(|p| p.type_paiement != TypePaiement::Remboursement).map(|p| p.montant).sum();
    let rendus: i64 = liste.iter().filter(|p| p.type_paiement == TypePaiement::Remboursement).map(|p| p.montant).sum();
    assert_eq!((entrees, rendus), (rapport.encaissements, rapport.remboursements));

    // Autre jour : rien. Avec un fuseau à UTC+12, 14 h UTC le 10 est déjà le 11 au matin.
    assert!(h.t.lister_paiements(&admin, d("2026-03-11"), d("2026-03-12"), 0).unwrap().is_empty());
    assert_eq!(h.t.lister_paiements(&admin, d("2026-03-11"), d("2026-03-12"), 12 * 60).unwrap().len(), 3);
    assert!(h.t.lister_paiements(&admin, d("2026-03-11"), d("2026-03-11"), 0).is_err());
    assert!(matches!(
        h.t.lister_paiements(&rec, d("2026-03-10"), d("2026-03-11"), 0),
        Err(Erreur::PermissionRefusee { .. })
    ));
}
