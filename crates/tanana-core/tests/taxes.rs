//! Taxes de l'hôtel et de chaque séjour : TVA en %, taxe de séjour par nuit…

mod commun;

use commun::*;
use tanana_core::Erreur;
use tanana_core::modeles::*;

fn taxe(libelle: &str, calcul: CalculTaxe, valeur: i64, comprise: bool) -> Taxe {
    Taxe { libelle: libelle.into(), calcul, valeur, comprise }
}

fn taxe_sejour() -> Taxe {
    taxe("Taxe de séjour", CalculTaxe::ParNuit, 2_000, false)
}

fn tva_comprise() -> Taxe {
    taxe("TVA", CalculTaxe::Pourcentage, 2_000, true)
}

fn encaisser(h: &mut Hotel, resa: &str, montant: i64, t: TypePaiement) -> tanana_core::Resultat<Paiement> {
    h.t.encaisser(
        &h.admin,
        Encaissement {
            reservation_id: resa.into(),
            montant,
            type_paiement: t,
            mode: ModePaiement::Especes,
            note: None,
        },
    )
}

#[test]
fn les_nouvelles_reservations_recoivent_les_taxes_de_l_hotel() {
    let mut h = hotel();
    let (ch, cl) = h.base(); // 25 000 par nuit
    assert!(h.t.taxes_hotel(&h.admin).unwrap().is_empty());
    let avant = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    assert_eq!((avant.taxes_ajoutees, avant.total_a_payer, avant.solde), (0, 50_000, 50_000));

    let enregistrees = h.t.definir_taxes_hotel(&h.admin, vec![tva_comprise(), taxe_sejour()]).unwrap();
    assert_eq!(enregistrees.len(), 2);
    assert!(h.actions().contains(&"taxes.modification".to_string()));

    let r = h.resa(&ch.id, &cl.id, "2026-03-12", "2026-03-15").unwrap(); // 3 nuits : 75 000
    assert_eq!(r.prix_total, 75_000);
    assert_eq!(r.taxes_ajoutees, 6_000, "2 000 × 3 nuits ; la TVA est déjà dans le prix");
    assert_eq!((r.total_a_payer, r.solde), (81_000, 81_000));

    let d = h.t.taxes_reservation(&h.admin, &r.id).unwrap();
    assert_eq!((d.hors_taxes, d.total_comprises, d.total_ajoutees), (62_500, 12_500, 6_000));
    assert_eq!(d.lignes.iter().map(|l| l.montant).collect::<Vec<_>>(), [12_500, 6_000]);

    // La réservation faite avant garde ses taxes (aucune)
    let avant = h.t.obtenir_reservation(&h.admin, &avant.id).unwrap();
    assert_eq!(avant.total_a_payer, 50_000);
}

#[test]
fn le_client_doit_payer_les_taxes_ajoutees_avant_de_partir() {
    let mut h = hotel();
    h.t.definir_taxes_hotel(&h.admin, vec![taxe_sejour()]).unwrap();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap(); // 50 000 + 4 000
    h.t.enregistrer_arrivee(&h.admin, &r.id).unwrap();
    encaisser(&mut h, &r.id, 50_000, TypePaiement::Acompte).unwrap();
    let r2 = h.t.obtenir_reservation(&h.admin, &r.id).unwrap();
    assert_eq!((r2.total_encaisse, r2.solde), (50_000, 4_000));
    assert!(matches!(h.t.enregistrer_depart(&h.admin, &r.id), Err(Erreur::SoldeImpaye { montant: 4_000 })));
    assert!(matches!(encaisser(&mut h, &r.id, 4_001, TypePaiement::Solde), Err(Erreur::MontantInvalide(_))));
    encaisser(&mut h, &r.id, 4_000, TypePaiement::Solde).unwrap();
    h.t.enregistrer_depart(&h.admin, &r.id).unwrap();
}

#[test]
fn modifier_les_taxes_d_un_sejour() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    encaisser(&mut h, &r.id, 50_000, TypePaiement::Solde).unwrap();

    // TVA de 19,25 % ajoutée au prix : le client doit 9 625 de plus
    let r =
        h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![taxe("TVA", CalculTaxe::Pourcentage, 1925, false)])
            .unwrap();
    assert_eq!((r.taxes_ajoutees, r.total_a_payer, r.solde), (9_625, 59_625, 9_625));
    assert!(h.actions().contains(&"reservation.taxes".to_string()));

    // On retire la taxe : rien de plus à payer
    let r = h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![]).unwrap();
    assert_eq!((r.total_a_payer, r.solde), (50_000, 0));

    // Montant fixe pour le séjour, compris dans le prix : le total ne bouge pas
    let r =
        h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![taxe("Forfait", CalculTaxe::ParSejour, 1_000, true)])
            .unwrap();
    assert_eq!(r.total_a_payer, 50_000);
    assert_eq!(h.t.taxes_reservation(&h.admin, &r.id).unwrap().hors_taxes, 49_000);

    // Compris mais plus grand que le prix : refusé avec une explication
    let e =
        h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![taxe("Forfait", CalculTaxe::ParSejour, 60_000, true)])
            .unwrap_err();
    assert!(matches!(&e, Erreur::Validation(m) if m.contains("dépassent le prix")), "{e:?}");
}

#[test]
fn la_taxe_par_nuit_suit_les_changements_de_dates() {
    let mut h = hotel();
    h.t.definir_taxes_hotel(&h.admin, vec![taxe_sejour()]).unwrap();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    assert_eq!(r.taxes_ajoutees, 4_000);
    let r =
        h.t.modifier_reservation(
            &h.admin,
            &r.id,
            ModificationReservation {
                chambre_id: ch.id.clone(),
                date_arrivee: r.date_arrivee,
                date_depart: d("2026-03-15"),
                prix_total: 125_000,
                notes: None,
            },
        )
        .unwrap();
    assert_eq!((r.nuits, r.taxes_ajoutees, r.total_a_payer), (5, 10_000, 135_000));

    // Un prix trop bas pour une taxe comprise est refusé
    h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![taxe("Forfait", CalculTaxe::ParSejour, 100_000, true)])
        .unwrap();
    let e = h.t.modifier_reservation(
        &h.admin,
        &r.id,
        ModificationReservation {
            chambre_id: ch.id.clone(),
            date_arrivee: r.date_arrivee,
            date_depart: r.date_depart,
            prix_total: 50_000,
            notes: None,
        },
    );
    assert!(matches!(e, Err(Erreur::Validation(_))));
}

#[test]
fn le_remboursement_tient_compte_des_taxes() {
    let mut h = hotel();
    h.t.definir_taxes_hotel(&h.admin, vec![taxe_sejour()]).unwrap();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap(); // 54 000
    encaisser(&mut h, &r.id, 54_000, TypePaiement::Solde).unwrap();
    h.t.rembourser(&h.admin, &r.id, 4_000, ModePaiement::Especes, "Taxe non due").unwrap();
    let r = h.t.obtenir_reservation(&h.admin, &r.id).unwrap();
    assert_eq!((r.total_encaisse, r.solde), (50_000, 4_000));
}

#[test]
fn droits_et_validation() {
    let mut h = hotel();
    let (_, reception) = h.employe("marie", "role-reception");
    // Tout employé connecté voit les taxes de l'hôtel (pour la création d'une réservation)…
    assert!(h.t.taxes_hotel(&reception).unwrap().is_empty());
    // … mais seul l'administrateur les règle
    assert!(matches!(h.t.definir_taxes_hotel(&reception, vec![taxe_sejour()]), Err(Erreur::PermissionRefusee { .. })));
    for mauvaise in [
        taxe("", CalculTaxe::ParNuit, 2_000, false),
        taxe("TVA", CalculTaxe::Pourcentage, 0, false),
        taxe("TVA", CalculTaxe::Pourcentage, 12_000, false),
    ] {
        assert!(matches!(h.t.definir_taxes_hotel(&h.admin, vec![mauvaise]), Err(Erreur::Validation(_))));
    }
    assert!(h.t.taxes_hotel(&h.admin).unwrap().is_empty(), "rien n'a été enregistré");

    // La réception modifie les taxes d'un séjour (elle a le droit de modifier les réservations)
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    h.t.modifier_taxes_reservation(&reception, &r.id, vec![taxe_sejour()]).unwrap();
    // Pas sur une réservation annulée
    h.t.annuler_reservation(&h.admin, &r.id, "Plus de séjour").unwrap();
    assert!(matches!(h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![]), Err(Erreur::StatutInvalide { .. })));
}

#[test]
fn apercu_du_calcul_pour_l_ecran() {
    let mut h = hotel();
    let d = h.t.calculer_taxes(&h.admin, 50_000, 2, vec![tva_comprise(), taxe_sejour()]).unwrap();
    assert_eq!((d.hors_taxes, d.total_comprises, d.total_ajoutees, d.total_a_payer), (41_667, 8_333, 4_000, 54_000));
    assert!(matches!(h.t.calculer_taxes(&h.admin, 50_000, 0, vec![]), Err(Erreur::Validation(_))));
    assert!(matches!(h.t.calculer_taxes("faux", 50_000, 2, vec![]), Err(Erreur::NonAuthentifie)));
}

/// Limites exactes (mutants survivants de cargo-mutants, `prix < 0`).
#[test]
fn apercu_du_calcul_aux_limites() {
    let mut h = hotel();
    assert!(h.t.calculer_taxes(&h.admin, 0, 1, vec![taxe_sejour()]).is_ok(), "une chambre offerte reste calculable");
    assert!(matches!(h.t.calculer_taxes(&h.admin, -1, 1, vec![]), Err(Erreur::Validation(_))));
    assert!(h.t.calculer_taxes(&h.admin, 50_000, 365, vec![]).is_ok());
    assert!(matches!(h.t.calculer_taxes(&h.admin, 50_000, 366, vec![]), Err(Erreur::Validation(_))));
}

#[test]
fn ordre_des_taxes_conserve() {
    let mut h = hotel();
    let taxes = vec![taxe_sejour(), tva_comprise(), taxe("Frais", CalculTaxe::ParSejour, 500, false)];
    h.t.definir_taxes_hotel(&h.admin, taxes.clone()).unwrap();
    assert_eq!(h.t.taxes_hotel(&h.admin).unwrap(), taxes);
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-11").unwrap();
    let libelles: Vec<String> =
        h.t.taxes_reservation(&h.admin, &r.id).unwrap().lignes.into_iter().map(|l| l.taxe.libelle).collect();
    assert_eq!(libelles, ["Taxe de séjour", "TVA", "Frais"]);
}
