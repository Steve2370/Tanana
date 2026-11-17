//! Démonstration de la consultation à distance, sans toucher à un vrai hôtel.
//!
//! Lance un relais (avec la page web si `--pages` est donné), crée un hôtel
//! bien rempli (chambres, clients, réservations autour d'aujourd'hui,
//! encaissements, dépenses, taxe de séjour), l'envoie chiffré au relais,
//! puis affiche l'identifiant et la clé à taper dans la page.
//!
//!   npm run build:web
//!   cargo run -p tanana-relais --example demo -- --pages dist-web
//!   → ouvrir http://127.0.0.1:8787
//!
//! `--fixture fichier.json` : écrit aussi tout ce que la page doit retrouver
//! (lots chiffrés + valeurs calculées par Tanana), puis s'arrête. Sert aux
//! tests de la page (src/web/__fixtures__).

use std::net::SocketAddr;

use chrono::{Duration, Local, NaiveDate};
use serde_json::json;
use tanana_core::Tanana;
use tanana_core::distance::chiffrement::Secret;
use tanana_core::distance::relais::{Relais as _, RelaisHttp};
use tanana_core::modeles::*;
use tanana_relais::{Reglages, Relais};

fn argument(nom: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == nom).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let port: u16 = argument("--port").and_then(|p| p.parse().ok()).unwrap_or(8787);
    let pages = argument("--pages");
    let fixture = argument("--fixture");
    // Date fixe pour la fixture (résultats reproductibles), aujourd'hui sinon.
    let aujourdhui = argument("--date")
        .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Local::now().date_naive());

    let dossier = tempfile::tempdir().unwrap();
    let relais = Relais::ouvrir(
        &dossier.path().join("relais.db"),
        Reglages { codes_invitation: vec![], inscriptions_libres: true, hotels_max: 10 },
    )
    .unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<SocketAddr>();
    let pages_serveur = pages.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let ecouteur = tokio::net::TcpListener::bind(("127.0.0.1", port)).await.expect("port déjà pris ?");
            tx.send(ecouteur.local_addr().unwrap()).unwrap();
            let routes = match pages_serveur {
                Some(p) => relais.routes_avec_pages(std::path::Path::new(&p)),
                None => relais.routes(),
            };
            axum::serve(ecouteur, routes).await.unwrap();
        });
    });
    let adresse = format!("http://{}", rx.recv().unwrap());

    let (mut t, tok) = remplir(aujourdhui);
    let client = RelaisHttp::new(&adresse).unwrap();
    let codes = t.activer_consultation_distance(&tok, &adresse, None, &client).unwrap();
    t.synchroniser(&client).unwrap();

    if let Some(chemin) = fixture {
        let secret = Secret::lire(&codes.cle_secrete).unwrap();
        let mut pages_lots = vec![];
        let mut apres = 0;
        loop {
            let page = client.lire(&codes.hotel_id, &secret.jeton_lecture(), apres).unwrap();
            apres = page.lots.last().map_or(apres, |l| l.numero);
            let suite = page.suite;
            pages_lots.push(page);
            if !suite {
                break;
            }
        }
        let temoin = client.temoin(&codes.hotel_id, &secret.jeton_lecture()).unwrap();
        let j = |n: i64| aujourdhui + Duration::days(n);
        let planning = t.planning(&tok, j(-60), j(60)).unwrap();
        let caisse_jour = t.rapport_caisse(&tok, aujourdhui, j(1), 0).unwrap();
        let caisse_semaine = t.rapport_caisse(&tok, j(-6), j(1), 0).unwrap();
        let attendu = json!({
            "nom_hotel": t.parametres(&tok).unwrap().nom_hotel,
            "reservations": planning,
            "caisse_jour": caisse_jour,
            "caisse_semaine": caisse_semaine,
            "chambres": t.lister_chambres(&tok).unwrap().len(),
        });
        let contenu = json!({
            "aujourdhui": aujourdhui.to_string(),
            "hotel_id": codes.hotel_id,
            "cle_secrete": codes.cle_secrete,
            "temoin": temoin,
            "pages": pages_lots,
            "attendu": attendu,
        });
        std::fs::write(&chemin, serde_json::to_string_pretty(&contenu).unwrap() + "\n").unwrap();
        println!("Fixture écrite : {chemin}");
        return;
    }

    println!();
    println!("  Relais de démonstration : {adresse}");
    if pages.is_some() {
        println!("  Ouvrir {adresse} dans le navigateur, puis saisir :");
    }
    println!("    Identifiant de l'hôtel : {}", codes.hotel_id);
    println!("    Clé secrète            : {}", codes.cle_secrete);
    println!();
    println!("  (Ctrl+C pour arrêter ; les données disparaissent à l'arrêt.)");
    loop {
        std::thread::sleep(std::time::Duration::from_secs(60));
        let _ = t.synchroniser(&client);
    }
}

/// Un hôtel avec de la vie autour d'aujourd'hui.
fn remplir(aujourdhui: NaiveDate) -> (Tanana, String) {
    let j = |n: i64| aujourdhui + Duration::days(n);
    let mut t = Tanana::en_memoire().unwrap();
    t.initialiser(ConfigurationInitiale {
        nom_hotel: "Hôtel du Lac".into(),
        devise: "XAF".into(),
        decimales_devise: 0,
        admin_identifiant: "admin".into(),
        admin_nom: "Beltran".into(),
        admin_mot_de_passe: "demo-2026!".into(),
    })
    .unwrap();
    let tok = t.connexion("admin", "demo-2026!").unwrap().token;
    t.definir_taxes_hotel(
        &tok,
        vec![Taxe { libelle: "Taxe de séjour".into(), calcul: CalculTaxe::ParNuit, valeur: 1_000, comprise: false }],
    )
    .unwrap();
    let standard = t
        .creer_type_chambre(&tok, NouveauTypeChambre { nom: "Standard".into(), prix_nuit_defaut: 25_000, capacite: 2 })
        .unwrap();
    let suite = t
        .creer_type_chambre(&tok, NouveauTypeChambre { nom: "Suite".into(), prix_nuit_defaut: 45_000, capacite: 3 })
        .unwrap();
    let mut chambre = |numero: &str, type_id: &str, statut: StatutChambre| {
        t.creer_chambre(
            &tok,
            DonneesChambre { numero: numero.into(), type_id: type_id.into(), etage: None, statut, notes: None },
        )
        .unwrap()
        .id
    };
    let c101 = chambre("101", &standard.id, StatutChambre::Disponible);
    let c102 = chambre("102", &standard.id, StatutChambre::Disponible);
    let c103 = chambre("103", &standard.id, StatutChambre::Disponible);
    chambre("104", &standard.id, StatutChambre::Maintenance);
    let c201 = chambre("201", &suite.id, StatutChambre::Disponible);
    let c202 = chambre("202", &suite.id, StatutChambre::Disponible);
    let mut client = |nom: &str, prenom: &str, tel: &str| {
        t.creer_client(
            &tok,
            DonneesClient {
                nom: nom.into(),
                prenom: Some(prenom.into()),
                telephone: Some(tel.into()),
                ..Default::default()
            },
        )
        .unwrap()
        .id
    };
    let mbarga = client("Mbarga", "Aline", "+237 6 99 12 34 56");
    let rakoto = client("Rakoto", "Hery", "+261 34 12 345 67");
    let ngono = client("Ngono", "Paul", "+237 6 77 00 11 22");
    let diallo = client("Diallo", "Fatou", "+221 77 123 45 67");
    let kouassi = client("Kouassi", "Jean", "+225 07 08 09 10 11");
    let mut resa = |ch: &str, cl: &str, a: i64, d: i64| {
        t.creer_reservation(
            &tok,
            NouvelleReservation {
                chambre_id: ch.into(),
                client_id: cl.into(),
                date_arrivee: j(a),
                date_depart: j(d),
                prix_total: None,
                notes: None,
            },
        )
        .unwrap()
    };
    let en_cours = resa(&c101, &mbarga, -2, 1);
    let arrive_aujourdhui = resa(&c102, &rakoto, 0, 3);
    let part_aujourdhui = resa(&c201, &ngono, -3, 0);
    let a_venir = resa(&c202, &diallo, 2, 5);
    let terminee = resa(&c103, &kouassi, -5, -2);
    let annulee = resa(&c103, &diallo, 1, 2);
    let mut payer = |r: &Reservation, montant: i64, type_paiement: TypePaiement, mode: ModePaiement| {
        t.encaisser(&tok, Encaissement { reservation_id: r.id.clone(), montant, type_paiement, mode, note: None })
            .unwrap();
    };
    payer(&en_cours, 30_000, TypePaiement::Acompte, ModePaiement::Mobile);
    payer(&part_aujourdhui, part_aujourdhui.total_a_payer, TypePaiement::Solde, ModePaiement::Carte);
    payer(&a_venir, 50_000, TypePaiement::Acompte, ModePaiement::Especes);
    payer(&terminee, terminee.total_a_payer, TypePaiement::Solde, ModePaiement::Especes);
    for r in [&en_cours, &part_aujourdhui, &terminee] {
        t.enregistrer_arrivee(&tok, &r.id).unwrap();
    }
    t.enregistrer_depart(&tok, &terminee.id).unwrap();
    t.annuler_reservation(&tok, &annulee.id, "Le client a changé ses dates").unwrap();
    let _ = arrive_aujourdhui;
    for (categorie, montant, jour) in [("Électricité", 8_000, 0), ("Blanchisserie", 3_500, -1)] {
        t.saisir_depense(
            &tok,
            NouvelleDepense { categorie: categorie.into(), montant, description: None, date: j(jour) },
        )
        .unwrap();
    }
    (t, tok)
}
