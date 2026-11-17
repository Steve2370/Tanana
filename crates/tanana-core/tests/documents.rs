//! Reçus et factures : numérotation, contenu figé, duplicatas, droits, PDF.

mod commun;

use chrono::Duration;
use commun::*;
use tanana_core::Erreur;
use tanana_core::modeles::*;
use tanana_core::pdf::{self, Op, PAGE_H, PAGE_L, caractere_disponible, largeur};
use tanana_core::services::documents::{ContenuDocument, DocumentImprime};

/// Heure du Cameroun : UTC+1.
const DOUALA: i32 = 60;

fn encaisser(h: &mut Hotel, resa: &str, montant: i64, t: TypePaiement) -> Paiement {
    h.t.encaisser(
        &h.admin,
        Encaissement { reservation_id: resa.into(), montant, type_paiement: t, mode: ModePaiement::Mobile, note: None },
    )
    .unwrap()
}

/// Tous les textes imprimés, page par page.
fn textes(c: &ContenuDocument, duplicata: bool) -> Vec<String> {
    pdf::mise_en_page(c, duplicata)
        .into_iter()
        .map(|ops| {
            ops.into_iter()
                .filter_map(|o| if let Op::Texte { texte, .. } = o { Some(texte) } else { None })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

fn tout(d: &DocumentImprime) -> String {
    textes(&d.contenu, d.duplicata).join("\n").replace('\u{a0}', " ")
}

/// Hôtel complet : coordonnées, une réservation de 2 nuits (50 000), un acompte de 20 000.
fn sejour() -> (Hotel, Reservation, Paiement) {
    let mut h = hotel();
    let mut p = h.t.parametres(&h.admin).unwrap();
    p.adresse_hotel = Some("12 rue du Lac\nBonapriso, Douala".into());
    p.telephone_hotel = Some("+237 233 00 00 00".into());
    p.email_hotel = Some("accueil@hoteltanana.cm".into());
    p.mentions_documents = Some("NIU : M012345678901A · RCCM : RC/DLA/2020/B/1234".into());
    h.t.modifier_parametres(&h.admin, p).unwrap();
    let (ch, _) = h.base();
    let cl =
        h.t.creer_client(
            &h.admin,
            DonneesClient {
                nom: "Mbarga".into(),
                prenom: Some("Aline".into()),
                telephone: Some("690 00 00 00".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    let acompte = encaisser(&mut h, &r.id, 20_000, TypePaiement::Acompte);
    (h, r, acompte)
}

#[test]
fn recu_d_un_acompte() {
    let (mut h, r, acompte) = sejour();
    let d = h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    assert_eq!(d.document.numero, "R-2026-0001");
    assert_eq!(d.document.type_document, TypeDocument::Recu);
    assert_eq!(d.document.paiement_id.as_deref(), Some(acompte.id.as_str()));
    assert_eq!(d.document.emis_par_nom, "Beltran");
    assert!(!d.duplicata);
    assert!(d.pdf.starts_with(b"%PDF-"), "c'est un vrai PDF");

    let c = &d.contenu;
    assert_eq!((c.total_paye, c.reste), (20_000, 30_000));
    assert_eq!(c.heure, "15 h 00", "heure locale de l'hôtel, pas UTC");
    assert_eq!(c.client.nom, "Mbarga Aline");

    let texte = tout(&d);
    for attendu in [
        "Hôtel Tanana",
        "12 rue du Lac",
        "Bonapriso, Douala",
        "Tél. +237 233 00 00 00 · accueil@hoteltanana.cm",
        "REÇU",
        "N° R-2026-0001",
        "Le 10 mars 2026 à 15 h 00",
        "Mbarga Aline",
        "Chambre 101 · Standard",
        "Arrivée : mardi 10 mars 2026",
        "Départ : jeudi 12 mars 2026",
        "2 nuits",
        "Reçu de Mbarga Aline la somme de :",
        "20 000 FCFA",
        "Vingt mille francs CFA",
        "Acompte pour la chambre 101, du 10 au 12 mars 2026",
        "Mobile money",
        "Prix du séjour",
        "50 000 FCFA",
        "Reste à payer",
        "30 000 FCFA",
        "Signature et cachet de l'hôtel",
        "APRÈS CE PAIEMENT",
        "Reçu par Beltran",
        "NIU : M012345678901A",
        "Émis par Beltran le 10 mars 2026 à 15 h 00",
    ] {
        assert!(texte.contains(attendu), "« {attendu} » absent du reçu :\n{texte}");
    }
    assert!(!texte.contains("DUPLICATA"));
    assert!(!texte.contains("Page 1 sur"), "une seule page : pas de numéro de page");

    // Journal
    assert!(h.actions().contains(&"document.emission".to_string()));
    assert_eq!(h.t.documents_reservation(&h.admin, &r.id).unwrap(), vec![d.document]);
}

#[test]
fn reimprimer_donne_le_meme_recu_marque_duplicata() {
    let (mut h, _, acompte) = sejour();
    let premier = h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    // Plus tard, l'hôtel change d'adresse : le reçu déjà remis ne change pas.
    let mut p = h.t.parametres(&h.admin).unwrap();
    p.adresse_hotel = Some("Nouvelle adresse".into());
    h.t.modifier_parametres(&h.admin, p).unwrap();
    h.temps.avancer(Duration::days(3));
    h.admin = h.t.connexion("admin", MDP_ADMIN).unwrap().token;

    let copie = h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    assert!(copie.duplicata);
    assert_eq!(copie.document, premier.document, "même numéro, même date d'émission");
    assert_eq!(copie.contenu, premier.contenu, "contenu figé");
    let texte = tout(&copie);
    assert!(texte.contains("DUPLICATA") && texte.contains("Copie d'un document déjà remis"));
    assert!(texte.contains("12 rue du Lac") && !texte.contains("Nouvelle adresse"));
    assert!(h.actions().contains(&"document.reimpression".to_string()));
}

#[test]
fn le_recu_montre_la_situation_au_moment_du_paiement() {
    let (mut h, r, acompte) = sejour();
    let solde = encaisser(&mut h, &r.id, 30_000, TypePaiement::Solde);
    // Le reçu de l'acompte, imprimé après le solde, reste celui de l'acompte
    let d = h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    assert_eq!((d.contenu.total_paye, d.contenu.reste), (20_000, 30_000));
    let d = h.t.imprimer_recu(&h.admin, &solde.id, DOUALA).unwrap();
    assert_eq!(d.document.numero, "R-2026-0002");
    assert_eq!((d.contenu.total_paye, d.contenu.reste), (50_000, 0));
    assert!(tout(&d).contains("Séjour entièrement payé. Merci !"));
}

#[test]
fn recu_de_remboursement() {
    let (mut h, r, _) = sejour();
    let remb = h.t.rembourser(&h.admin, &r.id, 5_000, ModePaiement::Especes, "Climatisation en panne").unwrap();
    let d = h.t.imprimer_recu(&h.admin, &remb.id, DOUALA).unwrap();
    assert_eq!((d.contenu.total_paye, d.contenu.reste), (15_000, 35_000));
    let texte = tout(&d);
    for attendu in [
        "REÇU DE REMBOURSEMENT",
        "Remboursé à Mbarga Aline la somme de :",
        "5 000 FCFA",
        "Cinq mille francs CFA",
        "Motif",
        "Climatisation en panne",
        "Espèces",
        "APRÈS CE REMBOURSEMENT",
        "Remboursé par Beltran",
    ] {
        assert!(texte.contains(attendu), "« {attendu} » absent :\n{texte}");
    }
}

#[test]
fn facture_puis_duplicata_puis_facture_qui_remplace() {
    let (mut h, r, _) = sejour();
    let f1 = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    assert_eq!(f1.document.numero, "F-2026-0001");
    assert_eq!(f1.document.remplace_numero, None);
    let texte = tout(&f1);
    for attendu in [
        "FACTURE",
        "N° F-2026-0001",
        "Hébergement · chambre 101 (Standard)",
        "Du 10 au 12 mars 2026",
        "25 000 FCFA", // prix par nuit
        "Total du séjour",
        "Arrêtée la présente facture à la somme de : Cinquante mille francs CFA.",
        "Paiements reçus",
        "10 mars 2026",
        "Acompte",
        "Total payé",
        "20 000 FCFA",
        "Reste à payer",
        "30 000 FCFA",
    ] {
        assert!(texte.contains(attendu), "« {attendu} » absent de la facture :\n{texte}");
    }
    assert!(!texte.contains("ACQUITTÉE"));

    // Rien n'a changé : même facture, en duplicata
    let f2 = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    assert!(f2.duplicata);
    assert_eq!(f2.document, f1.document);

    // Le client paie le solde : nouvelle facture, qui remplace la première
    h.temps.avancer(Duration::hours(2));
    h.admin = h.t.connexion("admin", MDP_ADMIN).unwrap().token;
    encaisser(&mut h, &r.id, 30_000, TypePaiement::Solde);
    let f3 = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    assert!(!f3.duplicata);
    assert_eq!(f3.document.numero, "F-2026-0002");
    assert_eq!(f3.document.remplace_numero.as_deref(), Some("F-2026-0001"));
    let texte = tout(&f3);
    assert!(texte.contains("Cette facture remplace la facture N° F-2026-0001, qui n'est plus valable."));
    assert!(texte.contains("FACTURE ACQUITTÉE"));

    // La dernière facture est celle qu'on réimprime ensuite
    let f4 = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    assert!(f4.duplicata);
    assert_eq!(f4.document.numero, "F-2026-0002");

    let docs = h.t.documents_reservation(&h.admin, &r.id).unwrap();
    assert_eq!(docs.iter().map(|d| d.numero.as_str()).collect::<Vec<_>>(), ["F-2026-0002", "F-2026-0001"]);
}

#[test]
fn facture_sans_paiement_et_prix_non_divisible() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-13").unwrap();
    let r =
        h.t.modifier_reservation(
            &h.admin,
            &r.id,
            ModificationReservation {
                chambre_id: ch.id.clone(),
                date_arrivee: r.date_arrivee,
                date_depart: r.date_depart,
                prix_total: 70_000,
                notes: None,
            },
        )
        .unwrap();
    let f = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    let texte = tout(&f);
    assert!(texte.contains("Aucun paiement reçu pour le moment."));
    assert!(texte.contains("—"), "70 000 sur 3 nuits : pas de prix par nuit inventé");
    assert!(texte.contains("Soixante-dix mille francs CFA"));
}

#[test]
fn pas_de_facture_pour_une_reservation_annulee() {
    let (mut h, r, acompte) = sejour();
    h.t.annuler_reservation(&h.admin, &r.id, "Le client ne vient plus").unwrap();
    assert!(matches!(h.t.imprimer_facture(&h.admin, &r.id, DOUALA), Err(Erreur::StatutInvalide { .. })));
    // Le reçu de l'acompte reste disponible
    assert!(h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).is_ok());
}

#[test]
fn numerotation_sans_trou_par_type_et_par_annee() {
    let (mut h, r, acompte) = sejour();
    let solde = encaisser(&mut h, &r.id, 30_000, TypePaiement::Solde);
    assert_eq!(h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap().document.numero, "R-2026-0001");
    assert_eq!(h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap().document.numero, "F-2026-0001");
    assert_eq!(h.t.imprimer_recu(&h.admin, &solde.id, DOUALA).unwrap().document.numero, "R-2026-0002");

    // Nouvelle année (heure locale) : on repart à 1
    let ch2 = {
        let tc = h.type_chambre("Suite", 60_000);
        h.chambre("201", &tc.id)
    };
    let cl = h.client("Ngono");
    let r2 = h.resa(&ch2.id, &cl.id, "2027-01-05", "2027-01-06").unwrap();
    let p = encaisser(&mut h, &r2.id, 60_000, TypePaiement::Solde);
    // 31 décembre 2026 à 23 h 30 UTC = 1er janvier 2027 à 0 h 30 à Douala
    let maintenant = *h.temps.0.lock().unwrap();
    let cible = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 12, 31, 23, 30, 0).unwrap();
    h.temps.avancer(cible - maintenant);
    h.admin = h.t.connexion("admin", MDP_ADMIN).unwrap().token;
    let d = h.t.imprimer_recu(&h.admin, &p.id, DOUALA).unwrap();
    assert_eq!(d.document.numero, "R-2027-0001");
    assert!(tout(&d).contains("Le 1er janvier 2027 à 0 h 30"));

    // La base refuse un numéro qui sauterait ou reviendrait en arrière
    let c = h.t.conn_brute();
    let inserer = |sequence: i64| {
        c.execute(
            "INSERT INTO documents (id, type, annee, sequence, numero, reservation_id, paiement_id, contenu, empreinte,
                                    emis_le, emis_par)
             SELECT 'x' || ?1, 'facture', 2026, ?1, 'F-2026-X' || ?1, reservation_id, NULL, '{}', '', emis_le, emis_par
             FROM documents LIMIT 1",
            [sequence],
        )
    };
    for sequence in [1, 5] {
        let e = inserer(sequence).unwrap_err().to_string();
        assert!(e.contains("NUMEROTATION_INVALIDE") || e.contains("UNIQUE"), "{sequence} : {e}");
    }
}

#[test]
fn les_documents_sont_figes_dans_la_base() {
    let (mut h, _, acompte) = sejour();
    h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    let c = h.t.conn_brute();
    for sql in
        ["UPDATE documents SET contenu = '{}'", "UPDATE documents SET numero = 'R-2026-9999'", "DELETE FROM documents"]
    {
        let e = c.execute(sql, []).unwrap_err().to_string();
        assert!(e.contains("DOCUMENT_NON_MODIFIABLE"), "{sql} : {e}");
    }
}

#[test]
fn droits_necessaires() {
    let (mut h, r, acompte) = sejour();
    let (_, menage) = {
        let role =
            h.t.creer_role(&h.admin, "Lecture", vec![tanana_core::permissions::Permission::ReservationVoir]).unwrap();
        h.employe("paul", &role.id)
    };
    assert!(matches!(h.t.imprimer_recu(&menage, &acompte.id, DOUALA), Err(Erreur::PermissionRefusee { .. })));
    assert!(matches!(h.t.imprimer_facture(&menage, &r.id, DOUALA), Err(Erreur::PermissionRefusee { .. })));
    assert!(h.t.documents_reservation(&menage, &r.id).unwrap().is_empty(), "il peut voir la liste");
    // La réception, elle, encaisse et remet les reçus
    let (_, reception) = h.employe("marie", "role-reception");
    let d = h.t.imprimer_recu(&reception, &acompte.id, DOUALA).unwrap();
    assert_eq!(d.document.emis_par_nom, "Employé marie");
    // Paramètres invalides
    assert!(matches!(h.t.imprimer_recu(&reception, "inconnu", DOUALA), Err(Erreur::Introuvable(_))));
    assert!(matches!(h.t.imprimer_recu(&reception, &acompte.id, 24 * 60), Err(Erreur::Validation(_))));
}

#[test]
fn rien_ne_deborde_de_la_page_et_les_caracteres_existent() {
    let (mut h, r, acompte) = sejour();
    // Noms très longs et caractères inhabituels saisis par l'utilisateur
    let mut p = h.t.parametres(&h.admin).unwrap();
    p.nom_hotel = "Grand Hôtel International des Bords du Lac et de la Montagne Verte".into();
    p.mentions_documents = Some("Mentions ".repeat(42));
    h.t.modifier_parametres(&h.admin, p).unwrap();
    let d = h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    let f = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    for (doc, duplicata) in [(&d.contenu, true), (&f.contenu, false)] {
        for ops in pdf::mise_en_page(doc, duplicata) {
            for op in ops {
                if let Op::Texte { x, y, poids, taille, texte, .. } = op {
                    let fin = x + largeur(poids, taille, &texte);
                    assert!(x >= 40.0 && fin <= PAGE_L - 40.0, "« {texte} » sort de la page ({x} → {fin})");
                    assert!(y > 0.0 && y < PAGE_H - 20.0, "« {texte} » trop bas ({y})");
                    for ch in texte.chars() {
                        assert!(caractere_disponible(ch), "caractère absent de la police : {ch:?} dans « {texte} »");
                    }
                }
            }
        }
    }
}

#[test]
fn une_facture_avec_beaucoup_de_paiements_continue_sur_une_deuxieme_page() {
    let mut h = hotel();
    let tc = h.type_chambre("Longue durée", 100_000);
    let ch = h.chambre("301", &tc.id);
    let cl = h.client("Essomba");
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-04-09").unwrap(); // 30 nuits
    for _ in 0..30 {
        encaisser(&mut h, &r.id, 50_000, TypePaiement::Acompte);
    }
    let f = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    let pages = textes(&f.contenu, false);
    assert!(pages.len() >= 2, "{} page(s)", pages.len());
    assert!(pages[1].contains("Facture N° F-2026-0001 (suite)"));
    assert!(pages[1].contains("Date"), "l'en-tête du tableau est répété");
    assert!(pages.last().unwrap().contains("Reste à payer"));
    assert!(pages[0].contains(&format!("Page 1 sur {}", pages.len())));
    let lignes = pages.join("\n").matches("Acompte").count();
    assert_eq!(lignes, 30, "chaque paiement apparaît une fois");
    assert!(f.pdf.starts_with(b"%PDF-"));
}

#[test]
fn le_pdf_contient_la_police_et_le_titre() {
    let (mut h, _, acompte) = sejour();
    let d = h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    let brut = String::from_utf8_lossy(&d.pdf);
    assert!(brut.contains("Lexend"), "police intégrée");
    assert!(d.pdf.len() < 200_000, "fichier léger : {} octets", d.pdf.len());
}

fn taxe(libelle: &str, calcul: CalculTaxe, valeur: i64, comprise: bool) -> Taxe {
    Taxe { libelle: libelle.into(), calcul, valeur, comprise }
}

#[test]
fn facture_avec_tva_comprise_et_taxe_de_sejour_ajoutee() {
    let (mut h, r, _) = sejour(); // 2 nuits, 50 000, acompte 20 000
    h.t.modifier_taxes_reservation(
        &h.admin,
        &r.id,
        vec![
            taxe("TVA", CalculTaxe::Pourcentage, 2000, true),
            taxe("Taxe de séjour", CalculTaxe::ParNuit, 2_000, false),
        ],
    )
    .unwrap();
    let f = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    assert_eq!((f.contenu.total_a_payer(), f.contenu.reste), (54_000, 34_000));
    let texte = tout(&f);
    for attendu in [
        "Ce prix comprend : TVA (20 %)",
        "Taxe de séjour",
        "Par nuit",
        "2 000 FCFA", // par nuit
        "4 000 FCFA", // 2 nuits
        "Total à payer",
        "54 000 FCFA",
        "Montant hors taxes",
        "41 667 FCFA",
        "dont TVA (20 %)",
        "8 333 FCFA",
        "Arrêtée la présente facture à la somme de : Cinquante-quatre mille francs CFA.",
        "34 000 FCFA", // reste
    ] {
        assert!(texte.contains(attendu), "« {attendu} » absent :\n{texte}");
    }
    assert!(!texte.contains("Total du séjour"));
}

#[test]
fn tva_ajoutee_en_pourcentage_sur_la_facture() {
    let (mut h, r, _) = sejour();
    h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![taxe("TVA", CalculTaxe::Pourcentage, 1925, false)]).unwrap();
    let f = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    let texte = tout(&f);
    assert!(texte.contains("19,25 % du prix hors taxes (50 000 FCFA)"), "{texte}");
    assert!(texte.contains("9 625 FCFA") && texte.contains("59 625 FCFA"));
    assert!(texte.contains("Cinquante-neuf mille six cent vingt-cinq francs CFA"));
}

#[test]
fn changer_les_taxes_donne_une_nouvelle_facture() {
    let (mut h, r, _) = sejour();
    let f1 = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![taxe("Taxe de séjour", CalculTaxe::ParNuit, 2_000, false)])
        .unwrap();
    let f2 = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    assert!(!f2.duplicata);
    assert_eq!(f2.document.remplace_numero.as_deref(), Some(f1.document.numero.as_str()));
}

#[test]
fn le_recu_montre_les_taxes_ajoutees() {
    let mut h = hotel();
    h.t.definir_taxes_hotel(&h.admin, vec![taxe("Taxe de séjour", CalculTaxe::ParNuit, 2_000, false)]).unwrap();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    let p = encaisser(&mut h, &r.id, 20_000, TypePaiement::Acompte);
    let d = h.t.imprimer_recu(&h.admin, &p.id, DOUALA).unwrap();
    assert_eq!(d.contenu.reste, 34_000);
    let texte = tout(&d);
    assert!(
        texte.contains("Taxes ajoutées") && texte.contains("4 000 FCFA") && texte.contains("34 000 FCFA"),
        "{texte}"
    );
}

/// Aperçu à regarder à l'œil : `TANANA_APERCU=/tmp/apercu cargo test --test documents apercu -- --ignored`
#[test]
#[ignore]
fn apercu() {
    let dossier = std::path::PathBuf::from(std::env::var("TANANA_APERCU").unwrap_or_else(|_| "/tmp/apercu".into()));
    std::fs::create_dir_all(&dossier).unwrap();
    let (mut h, r, acompte) = sejour();
    let recu = h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    let copie = h.t.imprimer_recu(&h.admin, &acompte.id, DOUALA).unwrap();
    let f1 = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    encaisser(&mut h, &r.id, 30_000, TypePaiement::Solde);
    let f2 = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    let remb = h.t.rembourser(&h.admin, &r.id, 5_000, ModePaiement::Especes, "Climatisation en panne").unwrap();
    let rr = h.t.imprimer_recu(&h.admin, &remb.id, DOUALA).unwrap();
    h.t.modifier_taxes_reservation(
        &h.admin,
        &r.id,
        vec![
            taxe("TVA", CalculTaxe::Pourcentage, 2000, true),
            taxe("Taxe de séjour", CalculTaxe::ParNuit, 2_000, false),
        ],
    )
    .unwrap();
    let ft = h.t.imprimer_facture(&h.admin, &r.id, DOUALA).unwrap();
    for (nom, d) in [
        ("recu", recu),
        ("duplicata", copie),
        ("facture", f1),
        ("facture-acquittee", f2),
        ("remboursement", rr),
        ("facture-taxes", ft),
    ] {
        std::fs::write(dossier.join(format!("{nom}.pdf")), d.pdf).unwrap();
    }
}
