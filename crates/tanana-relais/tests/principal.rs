//! Par le vrai relais, en HTTP : accès par identifiant et mot de passe,
//! « Retrouver mon hôtel », et modifications faites à distance.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};
use tanana_core::distance::acces::ouvrir_acces_distant;
use tanana_core::distance::chiffrement::recherche_compte;
use tanana_core::distance::principal::{reprendre_hotel, trouver_hotel};
use tanana_core::distance::relais::{Relais as _, RelaisHttp};
use tanana_core::modeles::*;
use tanana_core::{Erreur, Tanana};
use tanana_relais::{ECHECS_ACCES_PAR_COMPTE, Reglages, Relais};

const MDP: &str = "Madagascar2026";

fn lancer() -> (String, tempfile::TempDir) {
    let dossier = tempfile::tempdir().unwrap();
    let relais = Relais::ouvrir(
        &dossier.path().join("relais.db"),
        Reglages { codes_invitation: vec![], inscriptions_libres: true, hotels_max: 5 },
    )
    .unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<SocketAddr>();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let ecouteur = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            tx.send(ecouteur.local_addr().unwrap()).unwrap();
            axum::serve(ecouteur, relais.routes()).await.unwrap();
        });
    });
    (format!("http://{}", rx.recv().unwrap()), dossier)
}

/// Le portable de l'administratrice, relié au relais, avec tout envoyé.
fn portable(dossier: &std::path::Path, adresse: &str, client: &RelaisHttp) -> (Tanana, String) {
    let mut t = Tanana::ouvrir(&dossier.join("portable.db"), None).unwrap();
    t.initialiser(ConfigurationInitiale {
        nom_hotel: "Hôtel du Lac".into(),
        devise: "MGA".into(),
        decimales_devise: 0,
        admin_identifiant: "Hanta".into(),
        admin_nom: "Hanta".into(),
        admin_mot_de_passe: MDP.into(),
    })
    .unwrap();
    let tok = t.connexion("hanta", MDP).unwrap().token;
    t.creer_client(&tok, DonneesClient { nom: "Rakoto".into(), ..Default::default() }).unwrap();
    t.activer_consultation_distance(&tok, adresse, None, client).unwrap();
    t.synchroniser(client).unwrap();
    t.synchroniser_acces(client).unwrap();
    (t, tok)
}

#[test]
fn retrouver_l_hotel_sur_un_autre_ordinateur_par_le_vrai_relais() {
    let (adresse, d) = lancer();
    let client = RelaisHttp::new(&adresse).unwrap();
    let (mut t, tok) = portable(d.path(), &adresse, &client);
    assert!(t.synchroniser_instantane(&client, d.path()).unwrap());
    t.creer_client(&tok, DonneesClient { nom: "Rasoa".into(), ..Default::default() }).unwrap();
    t.synchroniser(&client).unwrap();

    let trouve = trouver_hotel(&client, "HANTA", MDP).unwrap();
    let chemin = d.path().join("hotel").join("tanana.db");
    let mut repris = reprendre_hotel(&client, &adresse, &trouve, &chemin).unwrap().tanana;
    let j = repris.connexion("hanta", MDP).unwrap().token;
    assert_eq!(repris.rechercher_clients(&j, "Ra").unwrap().len(), 2);

    // L'ancien ordinateur l'apprend par le relais (409 « poste remplacé »)
    t.creer_client(&tok, DonneesClient { nom: "Trop tard".into(), ..Default::default() }).unwrap();
    assert_eq!(t.synchroniser(&client).unwrap_err(), Erreur::PosteRemplace);
    assert!(t.poste_remplace().unwrap());
    // Le nouveau envoie sans problème
    repris.creer_client(&j, DonneesClient { nom: "Randria".into(), ..Default::default() }).unwrap();
    assert_eq!(repris.synchroniser(&client).unwrap().lots, 1);
}

#[test]
fn mots_de_passe_essayes_en_rafale() {
    let (adresse, d) = lancer();
    let client = RelaisHttp::new(&adresse).unwrap();
    let _ = portable(d.path(), &adresse, &client);
    for _ in 0..ECHECS_ACCES_PAR_COMPTE {
        assert_eq!(ouvrir_acces_distant(&client, "hanta", "mauvais").unwrap_err(), Erreur::IdentifiantsInvalides);
    }
    // Même le bon mot de passe attend maintenant
    assert_eq!(ouvrir_acces_distant(&client, "hanta", MDP).unwrap_err(), Erreur::TropDEssais);
    // Un compte inconnu a un sel, toujours le même : on ne devine pas qui existe
    let r = recherche_compte("inconnu");
    assert_eq!(client.sel_acces(&r).unwrap(), client.sel_acces(&r).unwrap());
    assert_ne!(client.sel_acces(&r).unwrap(), client.sel_acces(&recherche_compte("autre")).unwrap());
}

fn args(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

/// L'ordinateur principal, comme dans l'app : il attend les demandes, les fait, répond.
fn ecouter(t: Arc<Mutex<Tanana>>, adresse: String, arret: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let client = RelaisHttp::new(&adresse).unwrap();
        while !arret.load(Ordering::SeqCst) {
            let (_, hotel_id, jeton) = t.lock().unwrap().liaison_distance().unwrap().unwrap();
            let Ok(attente) = client.lire_commandes(&hotel_id, &jeton, 2) else { continue };
            for c in attente.commandes {
                let r = t.lock().unwrap().traiter_commande_distante(&c);
                if let Ok(r) = r {
                    t.lock().unwrap().synchroniser(&client).unwrap();
                    client.repondre(&hotel_id, &jeton, &c.id, &r).unwrap();
                }
            }
        }
    });
}

fn envoyer(poste: &Tanana, client: &RelaisHttp, commande: &str, a: Map<String, Value>) -> Result<Value, Erreur> {
    let (hotel_id, jeton_admin, c) = poste.preparer_commande_distante(commande, a)?;
    let r = client.poster_commande(&hotel_id, &jeton_admin, &c)?;
    poste.lire_reponse_distante(&c.id, &r)
}

#[test]
fn modifier_depuis_le_canada_par_le_vrai_relais() {
    let (adresse, d) = lancer();
    let client = RelaisHttp::new(&adresse).unwrap();
    let (t, _) = portable(d.path(), &adresse, &client);
    let hotel = Arc::new(Mutex::new(t));

    let (hotel_id, secret) = ouvrir_acces_distant(&client, "hanta", MDP).unwrap();
    let mut poste = Tanana::copie_en_memoire(&adresse, &hotel_id, &secret).unwrap();
    poste.mettre_a_jour_copie(&client).unwrap();

    let arret = Arc::new(AtomicBool::new(false));
    ecouter(hotel.clone(), adresse.clone(), arret.clone());

    let session: SessionOuverte = serde_json::from_value(
        envoyer(&poste, &client, "connexion", args(json!({ "identifiant": "hanta", "mot_de_passe": MDP }))).unwrap(),
    )
    .unwrap();
    let c: Client = serde_json::from_value(
        envoyer(
            &poste,
            &client,
            "creer_client",
            args(json!({ "token": session.token, "donnees": { "nom": "Andry", "prenom": null, "telephone": null,
                "email": null, "piece_identite": null, "notes": null } })),
        )
        .unwrap(),
    )
    .unwrap();
    // L'hôtel a envoyé ses lots avant de répondre : la copie l'a tout de suite
    poste.mettre_a_jour_copie(&client).unwrap();
    poste.definir_acces_copie(MDP).unwrap();
    let jc = poste.connexion_copie(MDP).unwrap().token;
    assert_eq!(poste.obtenir_client(&jc, &c.id).unwrap().nom, "Andry");

    // Une erreur de l'hôtel revient telle quelle
    let e = envoyer(&poste, &client, "creer_client", args(json!({ "token": "faux", "donnees": { "nom": "X" } })));
    assert_eq!(e.unwrap_err(), Erreur::NonAuthentifie);

    arret.store(true, Ordering::SeqCst);
}

#[test]
fn hotel_jamais_en_ligne_injoignable() {
    let (adresse, _d) = lancer();
    let client = RelaisHttp::new(&adresse).unwrap();
    // Hôtel inscrit à la main, qui n'a encore jamais rien envoyé
    let mut t = Tanana::en_memoire().unwrap();
    t.initialiser(ConfigurationInitiale {
        nom_hotel: "H".into(),
        devise: "MGA".into(),
        decimales_devise: 0,
        admin_identifiant: "hanta".into(),
        admin_nom: "Hanta".into(),
        admin_mot_de_passe: MDP.into(),
    })
    .unwrap();
    let tok = t.connexion("hanta", MDP).unwrap().token;
    let codes = t.activer_consultation_distance(&tok, &adresse, None, &client).unwrap();
    let secret = tanana_core::distance::chiffrement::Secret::lire(&codes.cle_secrete).unwrap();
    let poste = Tanana::copie_en_memoire(&adresse, &codes.hotel_id, &secret).unwrap();
    let e = envoyer(&poste, &client, "ma_session", args(json!({ "token": "x" })));
    assert_eq!(e.unwrap_err(), Erreur::HotelInjoignable);
}

#[test]
fn une_base_de_l_ancien_relais_est_mise_de_cote_sans_rien_effacer() {
    let d = tempfile::tempdir().unwrap();
    let chemin = d.path().join("relais.db");
    {
        let c = rusqlite::Connection::open(&chemin).unwrap();
        c.execute_batch(
            "CREATE TABLE hotels (id TEXT PRIMARY KEY, jeton_envoi_hash TEXT NOT NULL, jeton_lecture_hash TEXT NOT NULL,
                 temoin_nonce TEXT NOT NULL, temoin_chiffre TEXT NOT NULL, cree_le TEXT NOT NULL, dernier_contact TEXT);
             CREATE TABLE lots (hotel_id TEXT NOT NULL REFERENCES hotels(id), numero INTEGER NOT NULL, seq_debut INTEGER NOT NULL,
                 seq_fin INTEGER NOT NULL, nonce TEXT NOT NULL, chiffre TEXT NOT NULL, recu_le TEXT NOT NULL,
                 PRIMARY KEY (hotel_id, numero), UNIQUE (hotel_id, seq_debut));
             INSERT INTO hotels VALUES ('HK7Q-F29X', 'a', 'b', 'n', 'c', '2026-11-01', NULL);
             INSERT INTO lots VALUES ('HK7Q-F29X', 1, 1, 5, 'n', 'c', '2026-11-01');",
        )
        .unwrap();
    }
    Relais::ouvrir(&chemin, Reglages::default()).unwrap();
    let c = rusqlite::Connection::open(&chemin).unwrap();
    let anciennes: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name LIKE 'ancien_%' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(anciennes.len(), 2, "{anciennes:?}");
    let n: i64 = c.query_row("SELECT count(*) FROM hotels", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 0, "nouveau départ");
    // Une deuxième ouverture ne touche plus à rien
    Relais::ouvrir(&chemin, Reglages::default()).unwrap();
}
