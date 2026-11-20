//! Réservations de groupe : plusieurs chambres pour un même client, un seul compte, une seule facture.

mod commun;

use commun::*;
use tanana_core::Erreur;
use tanana_core::modeles::*;
use tanana_core::pdf::{self, Op};
use tanana_core::services::documents::{ContenuDocument, DocumentImprime};

/// Hôtel avec 4 chambres Standard (101, 102, 103) à 25 000 et une Suite (201) à 60 000, et un client.
fn hotel_et_chambres() -> (Hotel, Vec<Chambre>, Client) {
    let mut h = hotel();
    let standard = h.type_chambre("Standard", 25_000);
    let suite = h.type_chambre("Suite", 60_000);
    let chambres = vec![
        h.chambre("101", &standard.id),
        h.chambre("102", &standard.id),
        h.chambre("103", &standard.id),
        h.chambre("201", &suite.id),
    ];
    let cl = h.client("SOGEA");
    (h, chambres, cl)
}

fn nouveau(cl: &Client, chambres: &[(&Chambre, Option<i64>)], a: &str, dep: &str) -> NouveauGroupe {
    NouveauGroupe {
        client_id: cl.id.clone(),
        chambres: chambres.iter().map(|(c, p)| ChambreDuGroupe { chambre_id: c.id.clone(), prix_total: *p }).collect(),
        date_arrivee: d(a),
        date_depart: d(dep),
        nom: Some("Équipe SOGEA".into()),
        notes: Some("Facture au nom de l'entreprise".into()),
    }
}

/// Groupe de 3 chambres du 10 au 12 mars : 101 et 102 au prix normal (50 000), la suite à 100 000 convenu.
fn groupe_de_trois() -> (Hotel, Groupe, Vec<Chambre>) {
    let (mut h, ch, cl) = hotel_et_chambres();
    let g =
        h.t.creer_groupe(
            &h.admin,
            nouveau(&cl, &[(&ch[0], None), (&ch[1], None), (&ch[3], Some(100_000))], "2026-03-10", "2026-03-12"),
        )
        .unwrap();
    (h, g, ch)
}

fn encaisser(h: &mut Hotel, resa: &str, montant: i64) -> tanana_core::Resultat<Paiement> {
    h.t.encaisser(
        &h.admin,
        Encaissement {
            reservation_id: resa.into(),
            montant,
            type_paiement: TypePaiement::Acompte,
            mode: ModePaiement::Virement,
            note: None,
        },
    )
}

fn tout(d: &DocumentImprime) -> String {
    texte(&d.contenu, d.duplicata)
}

fn texte(c: &ContenuDocument, duplicata: bool) -> String {
    pdf::mise_en_page(c, duplicata)
        .into_iter()
        .flat_map(|ops| {
            ops.into_iter().filter_map(|o| if let Op::Texte { texte, .. } = o { Some(texte) } else { None })
        })
        .collect::<Vec<_>>()
        .join("\n")
        .replace('\u{a0}', " ")
}

#[test]
fn un_groupe_reserve_plusieurs_chambres_aux_memes_dates() {
    let (mut h, g, ch) = groupe_de_trois();
    assert_eq!(g.nom.as_deref(), Some("Équipe SOGEA"));
    assert_eq!(g.client_nom, "SOGEA");
    let numeros: Vec<&str> = g.reservations.iter().map(|r| r.chambre_numero.as_str()).collect();
    assert_eq!(numeros, ["101", "102", "201"]);
    for r in &g.reservations {
        assert_eq!(r.groupe_id.as_deref(), Some(g.id.as_str()));
        assert_eq!(
            (r.date_arrivee, r.date_depart, r.statut),
            (d("2026-03-10"), d("2026-03-12"), StatutReservation::Confirmee)
        );
        assert_eq!(r.notes.as_deref(), Some("Facture au nom de l'entreprise"));
    }
    assert_eq!(g.reservations[2].prix_total, 100_000, "prix convenu pour la suite");
    assert_eq!((g.total_a_payer, g.total_encaisse, g.solde), (200_000, 0, 200_000));

    // Chaque chambre apparaît dans le planning, la quatrième reste libre
    let planning = h.t.planning(&h.admin, d("2026-03-01"), d("2026-04-01")).unwrap();
    assert_eq!(planning.len(), 3);
    let libres = h.t.chambres_disponibles(&h.admin, d("2026-03-10"), d("2026-03-12")).unwrap();
    assert_eq!(libres.iter().map(|c| c.id.clone()).collect::<Vec<_>>(), vec![ch[2].id.clone()]);

    assert_eq!(h.t.obtenir_groupe(&h.admin, &g.id).unwrap(), g);
    let actions = h.actions();
    assert_eq!(actions.iter().filter(|a| *a == "groupe.creation").count(), 1);
    assert_eq!(actions.iter().filter(|a| *a == "reservation.creation").count(), 3);
    // Le journal dit quel groupe, pour qui, quelles chambres et combien
    let entree =
        h.t.journal(&h.admin, FiltreJournal { action_prefixe: Some("groupe.".into()), ..Default::default() })
            .unwrap()
            .remove(0);
    assert_eq!(entree.entite_id.as_deref(), Some(g.id.as_str()));
    assert_eq!(
        entree.apres,
        Some(
            serde_json::json!({ "nom": "Équipe SOGEA", "client": "SOGEA", "chambres": ["101", "102", "201"], "total_a_payer": 200_000 })
        )
    );
}

#[test]
fn une_chambre_offerte_a_un_prix_de_zero_est_acceptee() {
    let (mut h, ch, cl) = hotel_et_chambres();
    let g =
        h.t.creer_groupe(&h.admin, nouveau(&cl, &[(&ch[0], None), (&ch[1], Some(0))], "2026-03-10", "2026-03-12"))
            .unwrap();
    assert_eq!(g.reservations[1].prix_total, 0);
    assert_eq!(g.total_a_payer, 50_000);
}

#[test]
fn refuse_un_groupe_incorrect_sans_rien_creer() {
    let (mut h, ch, cl) = hotel_et_chambres();
    let essai = |h: &mut Hotel, g: NouveauGroupe| h.t.creer_groupe(&h.admin, g);

    // Une seule chambre, la même deux fois
    assert!(
        matches!(essai(&mut h, nouveau(&cl, &[(&ch[0], None)], "2026-03-10", "2026-03-12")), Err(Erreur::Validation(m)) if m.contains("deux chambres"))
    );
    assert!(
        matches!(essai(&mut h, nouveau(&cl, &[(&ch[0], None), (&ch[0], None)], "2026-03-10", "2026-03-12")), Err(Erreur::Validation(m)) if m.contains("deux fois"))
    );
    // Dates à l'envers, prix négatif, nom trop long, client inconnu
    assert!(essai(&mut h, nouveau(&cl, &[(&ch[0], None), (&ch[1], None)], "2026-03-12", "2026-03-10")).is_err());
    assert!(matches!(
        essai(&mut h, nouveau(&cl, &[(&ch[0], None), (&ch[1], Some(-5))], "2026-03-10", "2026-03-12")),
        Err(Erreur::MontantInvalide(_))
    ));
    let mut long = nouveau(&cl, &[(&ch[0], None), (&ch[1], None)], "2026-03-10", "2026-03-12");
    long.nom = Some("x".repeat(81));
    assert!(matches!(essai(&mut h, long), Err(Erreur::Validation(m)) if m.contains("80")));
    let mut inconnu = nouveau(&cl, &[(&ch[0], None), (&ch[1], None)], "2026-03-10", "2026-03-12");
    inconnu.client_id = "personne".into();
    assert!(matches!(essai(&mut h, inconnu), Err(Erreur::Introuvable(_))));

    // Une chambre déjà prise : tout le groupe est refusé, aucune chambre n'est bloquée
    h.resa(&ch[1].id, &cl.id, "2026-03-11", "2026-03-13").unwrap();
    assert!(matches!(
        essai(&mut h, nouveau(&cl, &[(&ch[0], None), (&ch[1], None)], "2026-03-10", "2026-03-12")),
        Err(Erreur::ChambreDejaReservee { .. })
    ));
    assert_eq!(h.t.planning(&h.admin, d("2026-03-01"), d("2026-04-01")).unwrap().len(), 1);
    assert!(!h.actions().contains(&"groupe.creation".to_string()));
}

#[test]
fn un_seul_compte_pour_tout_le_groupe() {
    let (mut h, g, _) = groupe_de_trois();
    let (r1, r2) = (g.reservations[0].id.clone(), g.reservations[1].id.clone());

    // Un acompte plus gros que le prix d'une chambre : accepté, c'est le groupe qui compte
    encaisser(&mut h, &r1, 120_000).unwrap();
    let g1 = h.t.obtenir_groupe(&h.admin, &g.id).unwrap();
    assert_eq!((g1.total_encaisse, g1.solde), (120_000, 80_000));
    // Plus que le reste du groupe : refusé, même depuis une autre chambre
    assert!(encaisser(&mut h, &r2, 80_001).is_err());
    encaisser(&mut h, &r2, 80_000).unwrap();
    assert_eq!(h.t.obtenir_groupe(&h.admin, &g.id).unwrap().solde, 0);

    // Chaque chambre montre les paiements de tout le groupe, dans l'ordre
    for r in &g.reservations {
        let p = h.t.paiements_reservation(&h.admin, &r.id).unwrap();
        assert_eq!(p.iter().map(|p| p.montant).collect::<Vec<_>>(), [120_000, 80_000]);
    }

    // Remboursement : jusqu'à ce que le groupe a payé, depuis n'importe quelle chambre
    assert!(h.t.rembourser(&h.admin, &r2, 200_001, ModePaiement::Especes, "geste").is_err());
    h.t.rembourser(&h.admin, &r2, 90_000, ModePaiement::Especes, "geste commercial").unwrap();
    let g2 = h.t.obtenir_groupe(&h.admin, &g.id).unwrap();
    assert_eq!((g2.total_encaisse, g2.solde), (110_000, 90_000));
    // Le journal garde le reste du groupe avant et après le remboursement
    let entree =
        h.t.journal(
            &h.admin,
            FiltreJournal { action_prefixe: Some("paiement.remboursement".into()), ..Default::default() },
        )
        .unwrap()
        .remove(0);
    assert_eq!(entree.avant.as_ref().unwrap()["solde"], 0);
    assert_eq!(entree.avant.as_ref().unwrap()["encaisse"], 200_000);
    assert_eq!(entree.apres.as_ref().unwrap()["solde"], 90_000);
    assert_eq!(entree.apres.as_ref().unwrap()["groupe"], serde_json::json!(g.id));
}

#[test]
fn le_depart_attend_que_le_groupe_ait_tout_paye() {
    let (mut h, g, _) = groupe_de_trois();
    let ids: Vec<String> = g.reservations.iter().map(|r| r.id.clone()).collect();
    for id in &ids {
        h.t.enregistrer_arrivee(&h.admin, id).unwrap();
    }
    // La chambre 101 a payé « sa part », mais le groupe doit encore 150 000
    encaisser(&mut h, &ids[0], 50_000).unwrap();
    assert!(matches!(h.t.enregistrer_depart(&h.admin, &ids[0]), Err(Erreur::SoldeImpaye { montant: 150_000 })));
    encaisser(&mut h, &ids[0], 150_000).unwrap();
    for id in &ids {
        h.t.enregistrer_depart(&h.admin, id).unwrap();
    }
    let g = h.t.obtenir_groupe(&h.admin, &g.id).unwrap();
    assert!(g.reservations.iter().all(|r| r.statut == StatutReservation::Partie));
}

#[test]
fn annuler_une_chambre_garde_l_argent_dans_le_groupe() {
    let (mut h, g, _) = groupe_de_trois();
    let (r1, suite) = (g.reservations[0].id.clone(), g.reservations[2].id.clone());
    encaisser(&mut h, &suite, 150_000).unwrap();
    h.t.annuler_reservation(&h.admin, &suite, "un collègue ne vient plus").unwrap();
    let g1 = h.t.obtenir_groupe(&h.admin, &g.id).unwrap();
    assert_eq!(g1.reservations.len(), 3, "la chambre annulée reste visible");
    // Il reste 101 et 102 (100 000) ; 150 000 déjà payés : 50 000 à rendre
    assert_eq!((g1.total_a_payer, g1.total_encaisse, g1.solde), (100_000, 150_000, -50_000));
    assert!(encaisser(&mut h, &r1, 1).is_err(), "plus rien à payer");
    h.t.enregistrer_arrivee(&h.admin, &r1).unwrap();
    h.t.enregistrer_depart(&h.admin, &r1).unwrap();
    // Rendre le trop-perçu depuis une chambre encore active
    h.t.rembourser(&h.admin, &g1.reservations[1].id, 50_000, ModePaiement::Especes, "trop-perçu").unwrap();
    assert_eq!(h.t.obtenir_groupe(&h.admin, &g.id).unwrap().solde, 0);
}

#[test]
fn ajouter_une_chambre_au_groupe_a_ses_dates() {
    let (mut h, g, ch) = groupe_de_trois();
    let g2 = h.t.ajouter_au_groupe(&h.admin, &g.id, &ch[2].id, Some(40_000)).unwrap();
    assert_eq!(g2.reservations.len(), 4);
    let nouvelle = g2.reservations.iter().find(|r| r.chambre_id == ch[2].id).unwrap();
    assert_eq!(
        (nouvelle.date_arrivee, nouvelle.date_depart, nouvelle.prix_total),
        (d("2026-03-10"), d("2026-03-12"), 40_000)
    );
    assert_eq!(g2.total_a_payer, 240_000);
    assert!(h.actions().contains(&"groupe.ajout".to_string()));
    // Déjà dans le groupe : la chambre est prise
    assert!(matches!(h.t.ajouter_au_groupe(&h.admin, &g.id, &ch[2].id, None), Err(Erreur::ChambreDejaReservee { .. })));

    // Toutes les chambres annulées : on ne peut plus ajouter
    for r in &g2.reservations {
        h.t.annuler_reservation(&h.admin, &r.id, "annulé").unwrap();
    }
    assert!(matches!(h.t.ajouter_au_groupe(&h.admin, &g.id, &ch[2].id, None), Err(Erreur::Validation(_))));
    assert!(matches!(h.t.ajouter_au_groupe(&h.admin, "inconnu", &ch[2].id, None), Err(Erreur::Introuvable(_))));
}

#[test]
fn une_seule_facture_avec_toutes_les_chambres() {
    let (mut h, g, _) = groupe_de_trois();
    let ids: Vec<String> = g.reservations.iter().map(|r| r.id.clone()).collect();
    let p = encaisser(&mut h, &ids[1], 60_000).unwrap();

    let f = h.t.imprimer_facture(&h.admin, &ids[0], 60).unwrap();
    assert!(!f.duplicata);
    assert_eq!(f.contenu.chambres.len(), 3);
    assert_eq!(f.contenu.groupe.as_deref(), Some("Équipe SOGEA"));
    assert_eq!((f.contenu.total_a_payer(), f.contenu.total_paye, f.contenu.reste), (200_000, 60_000, 140_000));
    let t = tout(&f);
    for attendu in [
        "Équipe SOGEA · 3 chambres",
        "Chambres 101, 102 et 201",
        "2 nuits par chambre",
        "Hébergement · chambre 101 (Standard)",
        "Hébergement · chambre 102 (Standard)",
        "Hébergement · chambre 201 (Suite)",
        "100 000 FCFA",
        "200 000 FCFA",
        "140 000 FCFA",
    ] {
        assert!(t.contains(attendu), "manque « {attendu} » dans :\n{t}");
    }

    // Depuis n'importe quelle chambre : la même facture (duplicata)
    let bis = h.t.imprimer_facture(&h.admin, &ids[2], 60).unwrap();
    assert!(bis.duplicata);
    assert_eq!(bis.document.numero, f.document.numero);

    // Le reçu parle du groupe
    let recu = h.t.imprimer_recu(&h.admin, &p.id, 60).unwrap();
    let t = tout(&recu);
    assert!(
        t.replace('\n', " ").contains("Acompte pour le groupe, chambres 101, 102 et 201, du 10 au 12 mars 2026"),
        "{t}"
    );
    assert_eq!((recu.contenu.total_paye, recu.contenu.reste), (60_000, 140_000));

    // Un nouveau paiement : nouvelle facture qui remplace l'ancienne
    encaisser(&mut h, &ids[0], 140_000).unwrap();
    let f2 = h.t.imprimer_facture(&h.admin, &ids[1], 60).unwrap();
    assert!(!f2.duplicata);
    assert_eq!(f2.contenu.remplace.as_deref(), Some(f.document.numero.as_str()));
    assert!(tout(&f2).contains("FACTURE ACQUITTÉE"));

    // Chaque chambre voit les documents du groupe
    for id in &ids {
        let docs = h.t.documents_reservation(&h.admin, id).unwrap();
        assert_eq!(docs.len(), 3, "2 factures + 1 reçu pour la chambre {id}");
    }
    assert!(f2.pdf.starts_with(b"%PDF-"));
}

#[test]
fn la_chambre_annulee_n_est_pas_facturee_et_un_groupe_tout_annule_n_a_pas_de_facture() {
    let (mut h, g, _) = groupe_de_trois();
    let ids: Vec<String> = g.reservations.iter().map(|r| r.id.clone()).collect();
    h.t.annuler_reservation(&h.admin, &ids[2], "plus besoin").unwrap();
    let f = h.t.imprimer_facture(&h.admin, &ids[0], 60).unwrap();
    assert_eq!(f.contenu.chambres.len(), 2);
    assert_eq!(f.contenu.total_a_payer(), 100_000);
    assert!(!tout(&f).contains("chambre 201"));
    for id in &ids[..2] {
        h.t.annuler_reservation(&h.admin, id, "tout annulé").unwrap();
    }
    assert!(matches!(h.t.imprimer_facture(&h.admin, &ids[0], 60), Err(Erreur::StatutInvalide { .. })));
}

#[test]
fn les_taxes_du_groupe_s_additionnent() {
    let (mut h, ch, cl) = hotel_et_chambres();
    h.t.definir_taxes_hotel(
        &h.admin,
        vec![
            Taxe { libelle: "Taxe de séjour".into(), calcul: CalculTaxe::ParNuit, valeur: 2_000, comprise: false },
            Taxe { libelle: "TVA".into(), calcul: CalculTaxe::Pourcentage, valeur: 1_925, comprise: true },
        ],
    )
    .unwrap();
    let g =
        h.t.creer_groupe(
            &h.admin,
            nouveau(&cl, &[(&ch[0], None), (&ch[1], None), (&ch[2], None)], "2026-03-10", "2026-03-13"),
        )
        .unwrap();
    // 3 chambres × 3 nuits × 25 000 = 225 000, plus 9 nuits × 2 000 de taxe de séjour
    assert_eq!(g.total_a_payer, 225_000 + 18_000);
    let f = h.t.imprimer_facture(&h.admin, &g.reservations[0].id, 60).unwrap();
    let taxes = f.contenu.taxes.clone().unwrap();
    assert_eq!(taxes.lignes.len(), 2, "une ligne par taxe, pas par chambre");
    assert_eq!(taxes.lignes[0].montant, 18_000);
    assert_eq!(taxes.total_a_payer, 243_000);
    let t = tout(&f);
    assert!(t.contains("Par nuit et par chambre"), "{t}");
    assert!(t.contains("\n9\n"), "9 nuits concernées : {t}");
    assert!(t.contains("Ce prix comprend : TVA (19,25 %)") && t.contains("dont TVA (19,25 %)"), "{t}");
}

#[test]
fn droits_et_consultation_a_distance() {
    let (mut h, g, ch) = groupe_de_trois();
    let (_, reception) = h.employe("marie", "role-reception");
    // La réception peut créer et voir un groupe
    assert!(h.t.obtenir_groupe(&reception, &g.id).is_ok());
    assert!(h.t.ajouter_au_groupe(&reception, &g.id, &ch[2].id, None).is_ok());
    // Sans le droit de voir les réservations : refusé
    let role = h.t.creer_role(&h.admin, "Ménage", vec![tanana_core::Permission::ChambreVoir]).unwrap();
    let (_, menage) = h.employe("awa", &role.id);
    assert!(matches!(h.t.obtenir_groupe(&menage, &g.id), Err(Erreur::PermissionRefusee { .. })));
    assert!(matches!(h.t.ajouter_au_groupe(&menage, &g.id, &ch[2].id, None), Err(Erreur::PermissionRefusee { .. })));
}

#[test]
fn une_chambre_seule_n_a_pas_de_groupe_et_sa_facture_ne_change_pas() {
    let (mut h, ch, cl) = hotel_et_chambres();
    let r = h.resa(&ch[0].id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    assert_eq!(r.groupe_id, None);
    let f = h.t.imprimer_facture(&h.admin, &r.id, 60).unwrap();
    assert!(f.contenu.chambres.is_empty() && f.contenu.groupe.is_none());
    let json = serde_json::to_value(&f.contenu).unwrap();
    assert!(json.get("chambres").is_none() && json.get("groupe").is_none(), "le contenu figé ne change pas de forme");
    assert!(tout(&f).contains("Chambre 101 · Standard"));
}

#[test]
fn limites_du_nombre_de_chambres_et_du_nom() {
    let (mut h, ch, cl) = hotel_et_chambres();
    let faux = |n: usize| -> Vec<ChambreDuGroupe> {
        (0..n).map(|i| ChambreDuGroupe { chambre_id: format!("inconnue-{i}"), prix_total: None }).collect()
    };
    let mut g = nouveau(&cl, &[], "2026-03-10", "2026-03-12");
    // 200 chambres : le nombre est accepté (puis la première chambre inconnue est refusée)
    g.chambres = faux(200);
    assert!(matches!(h.t.creer_groupe(&h.admin, g.clone()), Err(Erreur::Introuvable(_))));
    g.chambres = faux(201);
    assert!(matches!(h.t.creer_groupe(&h.admin, g), Err(Erreur::Validation(m)) if m.contains("200")));
    // Un nom de 80 caractères exactement est accepté
    let mut g = nouveau(&cl, &[(&ch[0], None), (&ch[1], None)], "2026-03-10", "2026-03-12");
    g.nom = Some("é".repeat(80));
    assert_eq!(h.t.creer_groupe(&h.admin, g).unwrap().nom.map(|n| n.chars().count()), Some(80));
}

#[test]
fn un_groupe_parti_ne_prend_plus_de_chambre() {
    let (mut h, g, ch) = groupe_de_trois();
    let ids: Vec<String> = g.reservations.iter().map(|r| r.id.clone()).collect();
    // Une chambre annulée, les deux autres parties
    h.t.annuler_reservation(&h.admin, &ids[2], "plus besoin").unwrap();
    for id in &ids[..2] {
        h.t.enregistrer_arrivee(&h.admin, id).unwrap();
    }
    encaisser(&mut h, &ids[0], 100_000).unwrap();
    h.t.enregistrer_depart(&h.admin, &ids[0]).unwrap();
    // Encore une chambre présente : on peut ajouter
    assert!(h.t.ajouter_au_groupe(&h.admin, &g.id, &ch[2].id, Some(0)).is_ok());
    let tout = h.t.obtenir_groupe(&h.admin, &g.id).unwrap();
    for r in tout.reservations.iter().filter(|r| r.statut == StatutReservation::Confirmee) {
        h.t.annuler_reservation(&h.admin, &r.id, "finalement non").unwrap();
    }
    h.t.enregistrer_depart(&h.admin, &ids[1]).unwrap();
    assert!(
        matches!(h.t.ajouter_au_groupe(&h.admin, &g.id, &ch[2].id, None), Err(Erreur::Validation(m)) if m.contains("déjà parti"))
    );
}

#[test]
fn chaque_chambre_connait_le_reste_de_son_groupe() {
    let (mut h, g, ch) = groupe_de_trois();
    encaisser(&mut h, &g.reservations[0].id, 120_000).unwrap();
    let planning = h.t.planning(&h.admin, d("2026-03-01"), d("2026-04-01")).unwrap();
    for r in &planning {
        assert_eq!(r.solde_groupe, Some(80_000), "chambre {}", r.chambre_numero);
    }
    // La chambre qui porte le paiement a « trop payé » pour elle seule : c'est le groupe qui compte
    assert_eq!(planning[0].solde, 50_000 - 120_000);
    let seule = h.resa(&ch[2].id, &g.client_id, "2026-03-10", "2026-03-12").unwrap();
    assert_eq!(seule.solde_groupe, None);
    let du_client = h.t.reservations_du_client(&h.admin, &g.client_id).unwrap();
    assert_eq!(du_client.iter().filter(|r| r.solde_groupe == Some(80_000)).count(), 3);
}

#[test]
fn les_taxes_de_plusieurs_chambres_s_additionnent_champ_par_champ() {
    use tanana_core::domaine::calculer_taxes;
    use tanana_core::services::documents::cumuler_taxes;
    let sejour =
        Taxe { libelle: "Taxe de séjour".into(), calcul: CalculTaxe::ParNuit, valeur: 1_000, comprise: false };
    let tva = Taxe { libelle: "TVA".into(), calcul: CalculTaxe::Pourcentage, valeur: 1_000, comprise: true };
    let a = calculer_taxes(110_000, 2, &[sejour.clone(), tva.clone()]);
    let b = calculer_taxes(55_000, 3, std::slice::from_ref(&sejour));
    let t = cumuler_taxes(&[a.clone(), b.clone()]);
    assert_eq!(t.prix, 165_000);
    assert_eq!(t.nuits, 5);
    assert_eq!(t.total_comprises, a.total_comprises + b.total_comprises);
    assert_eq!(t.total_comprises, 10_000);
    assert_eq!(t.total_ajoutees, 5_000);
    assert_eq!(t.hors_taxes, 100_000 + 55_000);
    assert_eq!(t.total_a_payer, 170_000);
    assert_eq!(t.lignes.len(), 2);
    assert_eq!((t.lignes[0].taxe.clone(), t.lignes[0].montant), (sejour, 5_000));
    assert_eq!((t.lignes[1].taxe.clone(), t.lignes[1].montant), (tva, 10_000));
    assert_eq!(cumuler_taxes(&[]), DetailTaxes::default());
}
