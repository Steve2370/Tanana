//! Le vrai relais, en HTTP, avec le vrai client de Tanana : hôtel → relais → copie de l'administrateur.

use std::net::SocketAddr;

use tanana_core::distance::chiffrement::Secret;
use tanana_core::distance::relais::{Relais as _, RelaisHttp};
use tanana_core::distance::verifier_cle;
use tanana_core::modeles::*;
use tanana_core::{Erreur, Tanana};
use tanana_relais::{Reglages, Relais};

const CODE: &str = "INVITATION-2026";

struct Serveur {
    adresse: String,
    base: std::path::PathBuf,
    _dossier: tempfile::TempDir,
}

/// Lance un relais sur un port libre, dans son propre fil.
fn lancer(reglages: Reglages) -> Serveur {
    let dossier = tempfile::tempdir().unwrap();
    let base = dossier.path().join("relais.db");
    let relais = Relais::ouvrir(&base, reglages).unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<SocketAddr>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let ecouteur = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(ecouteur.local_addr().unwrap()).unwrap();
            axum::serve(ecouteur, relais.routes()).await.unwrap();
        });
    });
    let adresse = format!("http://{}", rx.recv().unwrap());
    Serveur { adresse, base, _dossier: dossier }
}

fn reglages() -> Reglages {
    Reglages { codes_invitation: vec![CODE.into()], inscriptions_libres: false, hotels_max: 10 }
}

fn hotel() -> (Tanana, String) {
    let mut t = Tanana::en_memoire().unwrap();
    t.initialiser(ConfigurationInitiale {
        nom_hotel: "Hôtel du Lac".into(),
        devise: "MGA".into(),
        decimales_devise: 0,
        admin_identifiant: "admin".into(),
        admin_nom: "Beltran".into(),
        admin_mot_de_passe: "admin2026".into(),
    })
    .unwrap();
    let tok = t.connexion("admin", "admin2026").unwrap().token;
    t.creer_client(&tok, DonneesClient { nom: "Rakoto".into(), prenom: Some("Hery".into()), ..Default::default() })
        .unwrap();
    (t, tok)
}

#[test]
fn de_l_hotel_a_l_administrateur_par_le_vrai_relais() {
    let s = lancer(reglages());
    let client = RelaisHttp::new(&s.adresse).unwrap();
    let (mut t, tok) = hotel();

    let codes = t.activer_consultation_distance(&tok, &s.adresse, Some(CODE), &client).unwrap();
    assert_eq!(codes.hotel_id.len(), 9, "identifiant court : {}", codes.hotel_id);
    let bilan = t.synchroniser(&client).unwrap();
    assert!(bilan.lots >= 1 && bilan.changements > 10, "{bilan:?}");

    // Chez l'administrateur, à l'autre bout du monde
    let secret = Secret::lire(&codes.cle_secrete.to_lowercase()).unwrap();
    verifier_cle(&client, &codes.hotel_id, &secret).unwrap();
    let mut copie = Tanana::copie_en_memoire(&s.adresse, &codes.hotel_id, &secret).unwrap();
    assert!(copie.mettre_a_jour_copie(&client).unwrap() >= 1);
    copie.definir_acces_copie("consult2026").unwrap();
    let jeton = copie.connexion_copie("consult2026").unwrap().token;
    let clients = copie.rechercher_clients(&jeton, "Rakoto").unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].prenom.as_deref(), Some("Hery"));
    let etat = copie.etat_copie().unwrap();
    assert_eq!(etat.nom_hotel.as_deref(), Some("Hôtel du Lac"));
    assert!(etat.dernier_contact.is_some());

    // Ensuite, seulement les nouveautés ; et la présence quand il n'y a rien
    t.creer_client(&tok, DonneesClient { nom: "Rasoa".into(), ..Default::default() }).unwrap();
    assert_eq!(t.synchroniser(&client).unwrap().lots, 1);
    assert_eq!(t.synchroniser(&client).unwrap().lots, 0);
    assert_eq!(copie.mettre_a_jour_copie(&client).unwrap(), 1);
    assert_eq!(copie.rechercher_clients(&jeton, "Rasoa").unwrap().len(), 1);

    // Ce que le relais garde sur son disque : rien de lisible
    let brut = std::fs::read(&s.base).unwrap();
    let texte = String::from_utf8_lossy(&brut);
    for lisible in ["Rakoto", "Rasoa", "Hôtel du Lac", "Beltran", &codes.cle_secrete] {
        assert!(!texte.contains(lisible), "« {lisible} » lisible dans la base du relais");
    }
}

#[test]
fn le_relais_refuse_ce_qu_il_doit_refuser() {
    let s = lancer(reglages());
    let client = RelaisHttp::new(&s.adresse).unwrap();

    // Sans code d'invitation, ou avec un faux : pas d'inscription
    for code in [None, Some("FAUX-CODE-123")] {
        let (mut t, tok) = hotel();
        let e = t.activer_consultation_distance(&tok, &s.adresse, code, &client).unwrap_err();
        assert!(matches!(&e, Erreur::RelaisRefuse(m) if m.contains("accès refusé")), "{e:?}");
        assert!(!t.etat_consultation_distance(&tok).unwrap().active, "rien d'activé après un refus");
    }

    let (mut t, tok) = hotel();
    let codes = t.activer_consultation_distance(&tok, &s.adresse, Some(CODE), &client).unwrap();
    let envoi = t.preparer_envoi().unwrap().unwrap();
    let secret = Secret::lire(&codes.cle_secrete).unwrap();

    // Mauvais jetons
    assert!(matches!(client.envoyer(&codes.hotel_id, "faux", &envoi.lot), Err(Erreur::RelaisRefuse(_))));
    assert!(matches!(
        client.envoyer(&codes.hotel_id, &secret.jeton_lecture(), &envoi.lot),
        Err(Erreur::RelaisRefuse(_))
    ));
    assert!(matches!(client.lire(&codes.hotel_id, &envoi.jeton_envoi, 0), Err(Erreur::RelaisRefuse(_))));
    assert!(matches!(client.lire("ZZZZ-ZZZZ", &secret.jeton_lecture(), 0), Err(Erreur::HotelInconnuDuRelais)));
    assert!(matches!(
        verifier_cle(&client, &codes.hotel_id, &Secret::generer().unwrap()),
        Err(Erreur::CleDistanceIncorrecte)
    ));

    // Un envoi répété (accusé perdu) garde le même numéro
    let a = client.envoyer(&codes.hotel_id, &envoi.jeton_envoi, &envoi.lot).unwrap();
    let b = client.envoyer(&codes.hotel_id, &envoi.jeton_envoi, &envoi.lot).unwrap();
    assert_eq!(a, b);

    // Désinscription : tout est effacé chez le relais
    client.desinscrire(&codes.hotel_id, &envoi.jeton_envoi).unwrap();
    assert!(matches!(client.lire(&codes.hotel_id, &secret.jeton_lecture(), 0), Err(Erreur::HotelInconnuDuRelais)));
}

#[test]
fn relais_eteint_ou_adresse_invalide() {
    let (mut t, tok) = hotel();
    // Personne n'écoute sur ce port
    let client = RelaisHttp::new("http://127.0.0.1:9").unwrap();
    assert!(matches!(
        t.activer_consultation_distance(&tok, "http://127.0.0.1:9", Some(CODE), &client),
        Err(Erreur::RelaisInjoignable(_))
    ));
    // Sur Internet, HTTPS obligatoire
    assert!(matches!(RelaisHttp::new("http://relais.exemple.com"), Err(Erreur::Validation(_))));
    assert!(RelaisHttp::new("https://relais.exemple.com/").is_ok());
}

#[test]
fn nombre_d_hotels_limite() {
    let s = lancer(Reglages { codes_invitation: vec![], inscriptions_libres: true, hotels_max: 1 });
    let client = RelaisHttp::new(&s.adresse).unwrap();
    let (mut t1, tok1) = hotel();
    t1.activer_consultation_distance(&tok1, &s.adresse, None, &client).unwrap();
    let (mut t2, tok2) = hotel();
    assert!(t2.activer_consultation_distance(&tok2, &s.adresse, None, &client).is_err());
}
