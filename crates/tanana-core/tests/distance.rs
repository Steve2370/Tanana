//! Consultation à distance : de l'hôtel au relais, du relais à la copie de l'administrateur.

mod commun;

use std::sync::atomic::Ordering;

use chrono::Duration;
use commun::*;
use tanana_core::Erreur;
use tanana_core::Tanana;
use tanana_core::distance::chiffrement::{Secret, associe_lot};
use tanana_core::distance::relais::{Relais, RelaisMemoire};
use tanana_core::distance::{Lot, verifier_cle};
use tanana_core::modeles::*;

const ADRESSE: &str = "https://relais.tanana.test";
/// Mot de passe de consultation choisi par l'administrateur sur son ordinateur.
const MDP_COPIE: &str = "consult2026";

/// Hôtel avec un peu de vie : chambre, client, réservation, acompte, dépense, taxe, facture.
fn hotel_vivant() -> (Hotel, Reservation) {
    let mut h = hotel();
    h.t.definir_taxes_hotel(
        &h.admin,
        vec![Taxe { libelle: "Taxe de séjour".into(), calcul: CalculTaxe::ParNuit, valeur: 2_000, comprise: false }],
    )
    .unwrap();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();
    h.t.encaisser(
        &h.admin,
        Encaissement {
            reservation_id: r.id.clone(),
            montant: 20_000,
            type_paiement: TypePaiement::Acompte,
            mode: ModePaiement::Mobile,
            note: None,
        },
    )
    .unwrap();
    h.t.saisir_depense(
        &h.admin,
        NouvelleDepense { categorie: "Électricité".into(), montant: 8_000, description: None, date: d("2026-03-10") },
    )
    .unwrap();
    h.t.imprimer_facture(&h.admin, &r.id, 60).unwrap();
    (h, r)
}

/// Active la consultation, envoie tout, et crée la copie de l'administrateur.
fn relier(h: &mut Hotel, relais: &RelaisMemoire) -> (Tanana, Secret, String) {
    let codes = h.t.activer_consultation_distance(&h.admin, ADRESSE, None, relais).unwrap();
    h.t.synchroniser(relais).unwrap();
    let secret = Secret::lire(&codes.cle_secrete).unwrap();
    verifier_cle(relais, &codes.hotel_id, &secret).unwrap();
    let mut copie = Tanana::copie_en_memoire(&codes.relais, &codes.hotel_id, &secret).unwrap();
    copie.mettre_a_jour_copie(relais).unwrap();
    copie.definir_acces_copie(MDP_COPIE).unwrap();
    let jeton = copie.connexion_copie(MDP_COPIE).unwrap().token;
    (copie, secret, jeton)
}

/// Tous les lots, déchiffrés (pour inspecter ce qui est parti).
fn lots_dechiffres(relais: &RelaisMemoire, hotel_id: &str, secret: &Secret) -> Vec<Lot> {
    relais
        .lire(hotel_id, &secret.jeton_lecture(), 0)
        .unwrap()
        .lots
        .into_iter()
        .map(|l| {
            let clair = secret
                .dechiffrer(&l.lot.nonce, &l.lot.chiffre, &associe_lot(hotel_id, l.lot.seq_debut, l.lot.seq_fin))
                .unwrap();
            serde_json::from_slice(&clair).unwrap()
        })
        .collect()
}

#[test]
fn l_administrateur_retrouve_l_hotel_a_l_identique() {
    let (mut h, r) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let (mut copie, _, jeton) = relier(&mut h, &relais);
    assert!(copie.est_une_copie());

    // Mêmes réservations, mêmes montants, mêmes taxes
    assert_eq!(copie.obtenir_reservation(&jeton, &r.id).unwrap(), h.t.obtenir_reservation(&h.admin, &r.id).unwrap());
    assert_eq!(copie.taxes_reservation(&jeton, &r.id).unwrap(), h.t.taxes_reservation(&h.admin, &r.id).unwrap());
    assert_eq!(
        copie.planning(&jeton, d("2026-03-01"), d("2026-04-01")).unwrap(),
        h.t.planning(&h.admin, d("2026-03-01"), d("2026-04-01")).unwrap()
    );
    // Même caisse
    assert_eq!(
        copie.rapport_caisse(&jeton, d("2026-03-01"), d("2026-04-01"), 60).unwrap(),
        h.t.rapport_caisse(&h.admin, d("2026-03-01"), d("2026-04-01"), 60).unwrap()
    );
    assert_eq!(
        copie.lister_paiements(&jeton, d("2026-03-01"), d("2026-04-01"), 60).unwrap(),
        h.t.lister_paiements(&h.admin, d("2026-03-01"), d("2026-04-01"), 60).unwrap()
    );
    // Mêmes clients, documents, employés
    assert_eq!(copie.rechercher_clients(&jeton, "").unwrap(), h.t.rechercher_clients(&h.admin, "").unwrap());
    assert_eq!(
        copie.documents_reservation(&jeton, &r.id).unwrap(),
        h.t.documents_reservation(&h.admin, &r.id).unwrap()
    );
    assert_eq!(copie.lister_utilisateurs(&jeton).unwrap(), h.t.lister_utilisateurs(&h.admin).unwrap());

    // Le journal est copié tel quel : l'administrateur vérifie lui-même qu'il est intact.
    let rapport = copie.verifier_integrite_journal(&jeton).unwrap();
    assert!(rapport.intacte);
    assert!(rapport.entrees_verifiees > 5);

    let etat = copie.etat_copie().unwrap();
    assert_eq!(etat.nom_hotel.as_deref(), Some("Hôtel Tanana"));
    assert!(etat.donnees_du.is_some() && etat.mise_a_jour_le.is_some());
}

#[test]
fn le_logo_de_l_hotel_arrive_chez_l_administrateur_et_ne_se_change_pas_a_distance() {
    let (mut h, _) = hotel_vivant();
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAACgAAAAUCAYAAAD/Rn+7AAAAOUlEQVR4Xu3QQQ0AMAjAQMYPiZt/HyPBQj88egqanrrvx2K5uG0YSHmQ8iDlQcqDlAcpD1IepDxINcfFAeNBdYkVAAAAAElFTkSuQmCC";
    let logo = h.t.definir_logo(&h.admin, Some(NouveauLogo { format: FormatLogo::Png, donnees: PNG.into() })).unwrap();
    let relais = RelaisMemoire::default();
    let (mut copie, _, jeton) = relier(&mut h, &relais);
    assert_eq!(copie.logo_hotel(&jeton).unwrap(), logo);
    assert!(matches!(copie.definir_logo(&jeton, None), Err(Erreur::LectureSeule)));

    // Retiré à l'hôtel : retiré chez l'administrateur à l'envoi suivant
    h.t.definir_logo(&h.admin, None).unwrap();
    h.t.synchroniser(&relais).unwrap();
    copie.mettre_a_jour_copie(&relais).unwrap();
    assert_eq!(copie.logo_hotel(&jeton).unwrap(), None);
}

#[test]
fn un_groupe_arrive_chez_l_administrateur_avec_son_compte() {
    let (mut h, _) = hotel_vivant();
    let tc = h.type_chambre("Suite", 60_000);
    let (c2, c3) = (h.chambre("201", &tc.id), h.chambre("202", &tc.id));
    let cl = h.client("SOGEA");
    let g = h
        .t
        .creer_groupe(
            &h.admin,
            NouveauGroupe {
                client_id: cl.id,
                chambres: vec![
                    ChambreDuGroupe { chambre_id: c2.id, prix_total: None },
                    ChambreDuGroupe { chambre_id: c3.id, prix_total: Some(100_000) },
                ],
                date_arrivee: d("2026-03-20"),
                date_depart: d("2026-03-22"),
                nom: Some("Équipe SOGEA".into()),
                notes: None,
            },
        )
        .unwrap();
    h.t.encaisser(
        &h.admin,
        Encaissement {
            reservation_id: g.reservations[0].id.clone(),
            montant: 50_000,
            type_paiement: TypePaiement::Acompte,
            mode: ModePaiement::Virement,
            note: None,
        },
    )
    .unwrap();
    h.t.imprimer_facture(&h.admin, &g.reservations[1].id, 60).unwrap();
    let relais = RelaisMemoire::default();
    let (mut copie, _, jeton) = relier(&mut h, &relais);
    assert_eq!(copie.obtenir_groupe(&jeton, &g.id).unwrap(), h.t.obtenir_groupe(&h.admin, &g.id).unwrap());
    let r2 = &g.reservations[1].id;
    assert_eq!(copie.paiements_reservation(&jeton, r2).unwrap(), h.t.paiements_reservation(&h.admin, r2).unwrap());
    assert_eq!(copie.documents_reservation(&jeton, r2).unwrap(), h.t.documents_reservation(&h.admin, r2).unwrap());
    assert!(matches!(copie.ajouter_au_groupe(&jeton, &g.id, "x", None), Err(Erreur::LectureSeule)));
}

#[test]
fn les_secrets_ne_quittent_jamais_l_hotel_et_le_relais_ne_lit_rien() {
    let (mut h, _) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let codes = h.t.activer_consultation_distance(&h.admin, ADRESSE, None, &relais).unwrap();
    h.t.synchroniser(&relais).unwrap();
    let secret = Secret::lire(&codes.cle_secrete).unwrap();

    // Le relais ne voit que du chiffré : ni nom de client, ni montant, ni nom d'hôtel
    let brut = relais.contenu_brut();
    for lisible in ["Mbarga", "Hôtel Tanana", "Beltran", "Électricité", "20000"] {
        assert!(!brut.contains(lisible), "« {lisible} » lisible sur le relais");
    }

    // Une fois déchiffré : rien de secret
    let tout = serde_json::to_string(&lots_dechiffres(&relais, &codes.hotel_id, &secret)).unwrap();
    for secret_local in ["$argon2", "code_recuperation_hash", "token_hash", "jeton_envoi", "envoi-H"] {
        assert!(!tout.contains(secret_local), "« {secret_local} » est parti");
    }
    assert!(tout.contains(r#""mot_de_passe_hash":"*""#), "seulement un marqueur à la place du mot de passe");
    assert!(tout.contains("Mbarga"), "les données utiles, elles, partent");
    assert!(!tout.contains(&codes.cle_secrete), "la clé secrète ne part pas");
}

#[test]
fn seuls_les_changements_partent_ensuite_et_tout_se_retrouve() {
    let (mut h, r) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let (mut copie, _, jeton) = relier(&mut h, &relais);
    let lots_avant = relais.nombre_lots();

    // L'hôtel continue sa journée
    h.t.encaisser(
        &h.admin,
        Encaissement {
            reservation_id: r.id.clone(),
            montant: 34_000,
            type_paiement: TypePaiement::Solde,
            mode: ModePaiement::Especes,
            note: None,
        },
    )
    .unwrap();
    h.t.modifier_taxes_reservation(&h.admin, &r.id, vec![]).unwrap(); // efface des lignes
    let ngono = h.client("Ngono");
    let bilan = h.t.synchroniser(&relais).unwrap();
    assert_eq!(bilan.lots, 1, "un seul petit lot");
    assert!(bilan.changements < 20, "{bilan:?}");
    assert_eq!(relais.nombre_lots(), lots_avant + 1);
    assert_eq!(h.t.etat_consultation_distance(&h.admin).unwrap().en_attente, 0);

    assert_eq!(copie.mettre_a_jour_copie(&relais).unwrap(), 1);
    let vue = copie.obtenir_reservation(&jeton, &r.id).unwrap();
    assert_eq!(vue, h.t.obtenir_reservation(&h.admin, &r.id).unwrap());
    assert_eq!((vue.total_a_payer, vue.total_encaisse, vue.solde), (50_000, 54_000, -4_000));
    assert!(copie.taxes_reservation(&jeton, &r.id).unwrap().lignes.is_empty(), "l'effacement est arrivé");
    assert_eq!(copie.obtenir_client(&jeton, &ngono.id).unwrap().nom, "Ngono");
    assert!(copie.verifier_integrite_journal(&jeton).unwrap().intacte);

    // Rien de nouveau : rien n'est envoyé, mais l'hôtel signale qu'il est en ligne
    let presences = relais.presences.load(Ordering::SeqCst);
    assert_eq!(h.t.synchroniser(&relais).unwrap().lots, 0);
    assert_eq!(relais.presences.load(Ordering::SeqCst), presences + 1);
    assert_eq!(copie.mettre_a_jour_copie(&relais).unwrap(), 0);
}

#[test]
fn sans_internet_l_hotel_continue_et_rattrape_ensuite() {
    let (mut h, r) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let (mut copie, _, jeton) = relier(&mut h, &relais);

    relais.hors_ligne.store(true, Ordering::SeqCst);
    h.client("Pendant la coupure");
    h.t.annuler_reservation(&h.admin, &r.id, "Le client a annulé").unwrap(); // l'hôtel travaille normalement
    assert!(matches!(h.t.synchroniser(&relais), Err(Erreur::RelaisInjoignable(_))));
    let etat = h.t.etat_consultation_distance(&h.admin).unwrap();
    assert!(etat.en_attente > 0);
    assert!(etat.derniere_erreur.unwrap().contains("pas de connexion"));
    assert!(etat.derniere_tentative_le.is_some());

    relais.hors_ligne.store(false, Ordering::SeqCst);
    h.t.synchroniser(&relais).unwrap();
    let etat = h.t.etat_consultation_distance(&h.admin).unwrap();
    assert_eq!((etat.en_attente, etat.derniere_erreur), (0, None));
    copie.mettre_a_jour_copie(&relais).unwrap();
    assert_eq!(copie.obtenir_reservation(&jeton, &r.id).unwrap().statut, StatutReservation::Annulee);
    assert!(copie.rechercher_clients(&jeton, "coupure").unwrap().len() == 1);
}

#[test]
fn un_accuse_perdu_ne_cree_ni_trou_ni_doublon() {
    let (mut h, _) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let (mut copie, _, jeton) = relier(&mut h, &relais);
    h.client("Essomba");
    // Le lot part, mais la réponse se perd : l'hôtel ne sait pas qu'il est arrivé
    let envoi = h.t.preparer_envoi().unwrap().unwrap();
    relais.envoyer(&envoi.hotel_id, &envoi.jeton_envoi, &envoi.lot).unwrap();
    let lots = relais.nombre_lots();
    // Au prochain passage, il le renvoie : le relais le reconnaît
    h.t.synchroniser(&relais).unwrap();
    assert_eq!(relais.nombre_lots(), lots, "pas de doublon");
    copie.mettre_a_jour_copie(&relais).unwrap();
    assert_eq!(copie.rechercher_clients(&jeton, "Essomba").unwrap().len(), 1);
}

#[test]
fn la_copie_ne_se_modifie_pas() {
    let (mut h, r) = hotel_vivant();
    h.employe("marie", "role-reception");
    let relais = RelaisMemoire::default();
    let (mut copie, _, jeton) = relier(&mut h, &relais);
    let tentatives: Vec<tanana_core::Resultat<()>> = vec![
        copie.creer_client(&jeton, DonneesClient { nom: "Pirate".into(), ..Default::default() }).map(|_| ()),
        copie.annuler_reservation(&jeton, &r.id, "test").map(|_| ()),
        copie
            .encaisser(
                &jeton,
                Encaissement {
                    reservation_id: r.id.clone(),
                    montant: 1,
                    type_paiement: TypePaiement::Solde,
                    mode: ModePaiement::Especes,
                    note: None,
                },
            )
            .map(|_| ()),
        copie.definir_taxes_hotel(&jeton, vec![]).map(|_| ()),
        copie.desactiver_utilisateur_test(&jeton),
    ];
    for t in tentatives {
        assert!(matches!(t, Err(Erreur::LectureSeule)), "{t:?}");
    }
    assert!(copie.rechercher_clients(&jeton, "Pirate").unwrap().is_empty());
    // Une copie n'envoie rien
    assert!(copie.preparer_envoi().is_err() || copie.preparer_envoi().unwrap().is_none());
}

trait EssaiEcriture {
    fn desactiver_utilisateur_test(&mut self, jeton: &str) -> tanana_core::Resultat<()>;
}
impl EssaiEcriture for Tanana {
    /// Écriture qui exige un droit accordé à la copie (gérer les employés) : c'est la base qui refuse.
    fn desactiver_utilisateur_test(&mut self, jeton: &str) -> tanana_core::Resultat<()> {
        let id = self.lister_utilisateurs(jeton)?.into_iter().find(|u| u.identifiant == "marie").unwrap().id;
        self.changer_statut_utilisateur(jeton, &id, StatutUtilisateur::Suspendu).map(|_| ())
    }
}

#[test]
fn mauvaise_cle_ou_mauvais_hotel() {
    let (mut h, _) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let codes = h.t.activer_consultation_distance(&h.admin, ADRESSE, None, &relais).unwrap();
    let autre = Secret::generer().unwrap();
    assert!(matches!(verifier_cle(&relais, &codes.hotel_id, &autre), Err(Erreur::CleDistanceIncorrecte)));
    let bonne = Secret::lire(&codes.cle_secrete).unwrap();
    assert!(matches!(verifier_cle(&relais, "H999", &bonne), Err(Erreur::HotelInconnuDuRelais)));
    // Une copie montée avec une mauvaise clé ne lit rien et reste vide
    h.t.synchroniser(&relais).unwrap();
    let mut copie = Tanana::copie_en_memoire(ADRESSE, &codes.hotel_id, &autre).unwrap();
    assert!(copie.mettre_a_jour_copie(&relais).is_err());
}

#[test]
fn droits_activation_et_desactivation() {
    let (mut h, _) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let (_, reception) = h.employe("marie", "role-reception");
    assert!(matches!(
        h.t.activer_consultation_distance(&reception, ADRESSE, None, &relais),
        Err(Erreur::PermissionRefusee { .. })
    ));
    assert!(!h.t.etat_consultation_distance(&h.admin).unwrap().active);
    // Sans activation, rien ne s'accumule
    h.client("Avant");
    assert!(h.t.preparer_envoi().unwrap().is_none());

    let codes = h.t.activer_consultation_distance(&h.admin, ADRESSE, None, &relais).unwrap();
    assert!(h.actions().contains(&"distance.activation".to_string()));
    assert!(matches!(h.t.activer_consultation_distance(&h.admin, ADRESSE, None, &relais), Err(Erreur::Validation(_))));
    let etat = h.t.etat_consultation_distance(&h.admin).unwrap();
    assert!(etat.active && etat.en_attente > 0);
    assert_eq!(etat.hotel_id.as_deref(), Some(codes.hotel_id.as_str()));

    // Réafficher la clé : mot de passe exigé, action journalisée
    assert!(matches!(h.t.afficher_cle_distance(&h.admin, "faux"), Err(Erreur::IdentifiantsInvalides)));
    assert_eq!(h.t.afficher_cle_distance(&h.admin, MDP_ADMIN).unwrap(), codes);
    assert!(h.actions().contains(&"distance.cle_affichee".to_string()));

    // Désactivation : plus de capture
    let liaison = h.t.desactiver_consultation_distance(&h.admin).unwrap().unwrap();
    assert_eq!(liaison.1, codes.hotel_id);
    h.client("Après");
    assert!(h.t.preparer_envoi().unwrap().is_none());
    assert!(!h.t.etat_consultation_distance(&h.admin).unwrap().active);
}

#[test]
fn un_gros_hotel_part_en_plusieurs_lots_et_revient_en_plusieurs_pages() {
    let mut h = hotel();
    for i in 0..1_200 {
        h.client(&format!("Client {i:04}"));
    }
    let relais = RelaisMemoire::default();
    let (mut copie, _, jeton) = relier(&mut h, &relais);
    assert!(relais.nombre_lots() >= 2, "{} lot(s)", relais.nombre_lots());
    assert_eq!(copie.rechercher_clients(&jeton, "Client 1199").unwrap().len(), 1);
    assert_eq!(copie.rechercher_clients(&jeton, "Client").unwrap().len(), 200, "limite d'affichage habituelle");
    assert!(copie.verifier_integrite_journal(&jeton).unwrap().intacte);
}

#[test]
fn la_capture_survit_a_la_fermeture_et_la_copie_se_rouvre() {
    let dossier = tempfile::tempdir().unwrap();
    let base_hotel = dossier.path().join("hotel.db");
    let base_copie = dossier.path().join("copie.db");
    let cle = tanana_core::CleBase::generer().unwrap();
    let cle_copie = tanana_core::CleBase::generer().unwrap();
    let relais = RelaisMemoire::default();

    let codes = {
        let mut t = Tanana::ouvrir(&base_hotel, Some(cle.clone())).unwrap();
        t.initialiser(config()).unwrap();
        let tok = t.connexion("admin", MDP_ADMIN).unwrap().token;
        let codes = t.activer_consultation_distance(&tok, ADRESSE, None, &relais).unwrap();
        t.synchroniser(&relais).unwrap();
        codes
    };
    {
        // Tanana redémarre : la capture fonctionne toujours
        let mut t = Tanana::ouvrir(&base_hotel, Some(cle.clone())).unwrap();
        let tok = t.connexion("admin", MDP_ADMIN).unwrap().token;
        t.creer_client(&tok, DonneesClient { nom: "Après redémarrage".into(), ..Default::default() }).unwrap();
        assert_eq!(t.synchroniser(&relais).unwrap().lots, 1);
    }
    let secret = Secret::lire(&codes.cle_secrete).unwrap();
    {
        let mut c =
            Tanana::creer_copie(&base_copie, Some(cle_copie.clone()), ADRESSE, &codes.hotel_id, &secret).unwrap();
        c.definir_acces_copie(MDP_COPIE).unwrap();
        c.mettre_a_jour_copie(&relais).unwrap();
    }
    let mut c = Tanana::ouvrir_copie(&base_copie, Some(cle_copie)).unwrap();
    assert!(c.acces_copie_defini().unwrap());
    let jeton = c.connexion_copie(MDP_COPIE).unwrap().token;
    assert_eq!(c.rechercher_clients(&jeton, "redémarrage").unwrap().len(), 1);
    assert!(matches!(
        c.creer_client(&jeton, DonneesClient { nom: "X".into(), ..Default::default() }),
        Err(Erreur::LectureSeule)
    ));
    // La base de l'hôtel n'est pas une copie : refus clair
    assert!(matches!(Tanana::ouvrir_copie(&base_hotel, Some(cle)), Err(Erreur::Validation(_))));
}

#[test]
fn toutes_les_tables_sont_classees() {
    // Une nouvelle table doit être explicitement envoyée ou gardée à l'hôtel :
    // jamais envoyée par défaut, jamais oubliée en silence.
    let h = hotel();
    let tables: Vec<String> =
        h.t.conn_brute()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
    use tanana_core::distance::capture::{TABLES_ENVOYEES, TABLES_LOCALES};
    for t in &tables {
        assert!(
            TABLES_ENVOYEES.contains(&t.as_str()) || TABLES_LOCALES.contains(&t.as_str()),
            "table « {t} » ni envoyée ni déclarée locale"
        );
    }
}

#[test]
fn l_etat_de_la_copie_dit_de_quand_datent_les_donnees() {
    let (mut h, _) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let (mut copie, _, _) = relier(&mut h, &relais);
    let premier = copie.etat_copie().unwrap().donnees_du.unwrap();
    h.temps.avancer(Duration::hours(3));
    h.admin = h.t.connexion("admin", MDP_ADMIN).unwrap().token;
    h.client("Plus tard");
    h.t.synchroniser(&relais).unwrap();
    copie.mettre_a_jour_copie(&relais).unwrap();
    let ensuite = copie.etat_copie().unwrap().donnees_du.unwrap();
    assert!(ensuite > premier, "{premier} → {ensuite}");
}

#[test]
fn la_copie_est_protegee_par_son_mot_de_passe() {
    let (mut h, r) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let codes = h.t.activer_consultation_distance(&h.admin, ADRESSE, None, &relais).unwrap();
    h.t.synchroniser(&relais).unwrap();
    let secret = Secret::lire(&codes.cle_secrete).unwrap();
    let temps = Temps::nouveau();
    let mut copie = Tanana::copie_en_memoire(ADRESSE, &codes.hotel_id, &secret).unwrap().avec_horloge(temps.horloge());
    copie.mettre_a_jour_copie(&relais).unwrap();
    assert!(!copie.acces_copie_defini().unwrap());
    // Sans mot de passe choisi, personne n'entre
    assert!(matches!(copie.connexion_copie("nimporte1"), Err(Erreur::IdentifiantsInvalides)));
    assert!(matches!(copie.definir_acces_copie("court"), Err(Erreur::Validation(_))));
    copie.definir_acces_copie(MDP_COPIE).unwrap();

    // Sans session : rien
    assert!(matches!(copie.obtenir_reservation("au-hasard", &r.id), Err(Erreur::NonAuthentifie)));

    let session = copie.connexion_copie(MDP_COPIE).unwrap();
    assert_eq!(session.utilisateur.nom, "Consultation à distance");
    assert!(session.permissions.contains(&tanana_core::Permission::RapportFinancierVoir));
    assert!(!session.permissions.contains(&tanana_core::Permission::PaiementEncaisser));
    assert_eq!(copie.ma_session(&session.token).unwrap().token, session.token);
    assert!(copie.obtenir_reservation(&session.token, &r.id).is_ok());

    // Inactivité : le propriétaire a 4 heures (la réception de l'hôtel, elle, garde ses 15 minutes)
    assert_eq!(copie.parametres(&session.token).unwrap().delai_inactivite_minutes, 240);
    temps.avancer(Duration::minutes(239));
    assert!(copie.obtenir_reservation(&session.token, &r.id).is_ok(), "encore actif après 3 h 59");
    temps.avancer(Duration::minutes(241));
    assert!(matches!(copie.obtenir_reservation(&session.token, &r.id), Err(Erreur::SessionExpiree)));
    assert!(matches!(copie.ma_session(&session.token), Err(Erreur::NonAuthentifie)));

    // Déconnexion
    let s2 = copie.connexion_copie(MDP_COPIE).unwrap();
    copie.deconnexion(&s2.token).unwrap();
    assert!(matches!(copie.obtenir_reservation(&s2.token, &r.id), Err(Erreur::NonAuthentifie)));

    // Cinq essais ratés : blocage, même avec le bon mot de passe ensuite
    for _ in 0..4 {
        assert!(matches!(copie.connexion_copie("mauvais99"), Err(Erreur::IdentifiantsInvalides)));
    }
    assert!(matches!(copie.connexion_copie("mauvais99"), Err(Erreur::CompteBloque { .. })));
    assert!(matches!(copie.connexion_copie(MDP_COPIE), Err(Erreur::CompteBloque { .. })));
    temps.avancer(Duration::minutes(16));
    assert!(copie.connexion_copie(MDP_COPIE).is_ok());
}

#[test]
fn tout_renvoyer_reconstruit_une_copie_neuve() {
    let (mut h, r) = hotel_vivant();
    let relais = RelaisMemoire::default();
    let (_, secret, _) = relier(&mut h, &relais);
    let hotel_id = h.t.etat_consultation_distance(&h.admin).unwrap().hotel_id.unwrap();
    // Le relais a « perdu » ses données : un nouveau relais, même hôtel côté hôtel
    let nb = h.t.renvoyer_tout_distance(&h.admin).unwrap();
    assert!(nb > 10);
    assert!(h.actions().contains(&"distance.renvoi_complet".to_string()));
    assert!(h.t.etat_consultation_distance(&h.admin).unwrap().en_attente >= nb);
    h.t.synchroniser(&relais).unwrap();
    // Une copie neuve, construite depuis le début, est complète
    let mut copie = Tanana::copie_en_memoire(ADRESSE, &hotel_id, &secret).unwrap();
    copie.mettre_a_jour_copie(&relais).unwrap();
    copie.definir_acces_copie(MDP_COPIE).unwrap();
    let jeton = copie.connexion_copie(MDP_COPIE).unwrap().token;
    assert_eq!(copie.obtenir_reservation(&jeton, &r.id).unwrap(), h.t.obtenir_reservation(&h.admin, &r.id).unwrap());
    assert!(copie.verifier_integrite_journal(&jeton).unwrap().intacte);
    // Réservé à l'administrateur, et seulement si c'est activé
    let (_, reception) = h.employe("paul", "role-reception");
    assert!(matches!(h.t.renvoyer_tout_distance(&reception), Err(Erreur::PermissionRefusee { .. })));
    h.t.desactiver_consultation_distance(&h.admin).unwrap();
    assert!(matches!(h.t.renvoyer_tout_distance(&h.admin), Err(Erreur::Validation(_))));
}

#[test]
fn identifiant_d_hotel_recopie_a_la_main() {
    use tanana_core::distance::normaliser_identifiant_hotel as n;
    assert_eq!(n("AFOJ-WRFX"), "AFOJ-WRFX");
    assert_eq!(n(" af0j wrfx "), "AFOJ-WRFX");
    assert_eq!(n("hk7qf29x"), "HK7Q-F29X");
    assert_eq!(n("1B8C-DEFG"), "IBBC-DEFG");
    assert_eq!(n("abc"), "ABC");
}
