//! Outils partagés par les tests d'intégration.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use tanana_core::Tanana;
use tanana_core::modeles::*;

pub const MDP_ADMIN: &str = "admin2026";
pub const MDP_TEMP: &str = "temp1234";
pub const MDP_DEFINITIF: &str = "definitif99";

/// Horloge que le test fait avancer à la main.
#[derive(Clone)]
pub struct Temps(pub Arc<Mutex<DateTime<Utc>>>);

impl Temps {
    pub fn nouveau() -> Self {
        Self(Arc::new(Mutex::new(Utc.with_ymd_and_hms(2026, 3, 10, 14, 0, 0).unwrap())))
    }
    pub fn avancer(&self, d: Duration) {
        *self.0.lock().unwrap() += d;
    }
    pub fn horloge(&self) -> tanana_core::Horloge {
        let t = self.0.clone();
        Arc::new(move || *t.lock().unwrap())
    }
}

pub fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

pub struct Hotel {
    pub t: Tanana,
    pub temps: Temps,
    pub admin: String,
    pub code_recuperation: String,
}

pub fn config() -> ConfigurationInitiale {
    ConfigurationInitiale {
        nom_hotel: "Hôtel Tanana".into(),
        devise: "xaf".into(),
        decimales_devise: 0,
        admin_identifiant: "admin".into(),
        admin_nom: "Beltran".into(),
        admin_mot_de_passe: MDP_ADMIN.into(),
    }
}

/// Hôtel initialisé avec un admin connecté.
pub fn hotel() -> Hotel {
    let temps = Temps::nouveau();
    let mut t = Tanana::en_memoire().unwrap().avec_horloge(temps.horloge());
    let code_recuperation = t.initialiser(config()).unwrap();
    let admin = t.connexion("admin", MDP_ADMIN).unwrap().token;
    Hotel { t, temps, admin, code_recuperation }
}

impl Hotel {
    /// Crée un employé, lui fait changer son mot de passe temporaire et
    /// retourne (id, token).
    pub fn employe(&mut self, identifiant: &str, role_id: &str) -> (String, String) {
        let u = self
            .t
            .creer_utilisateur(
                &self.admin,
                NouvelUtilisateur {
                    identifiant: identifiant.into(),
                    nom: format!("Employé {identifiant}"),
                    mot_de_passe_temporaire: MDP_TEMP.into(),
                    role_id: role_id.into(),
                },
            )
            .unwrap();
        let token = self.t.connexion(identifiant, MDP_TEMP).unwrap().token;
        self.t.changer_mon_mot_de_passe(&token, MDP_TEMP, MDP_DEFINITIF).unwrap();
        (u.id, token)
    }

    pub fn type_chambre(&mut self, nom: &str, prix: i64) -> TypeChambre {
        self.t
            .creer_type_chambre(
                &self.admin,
                NouveauTypeChambre { nom: nom.into(), prix_nuit_defaut: prix, capacite: 2 },
            )
            .unwrap()
    }

    pub fn chambre(&mut self, numero: &str, type_id: &str) -> Chambre {
        self.t
            .creer_chambre(
                &self.admin,
                DonneesChambre {
                    numero: numero.into(),
                    type_id: type_id.into(),
                    etage: Some(1),
                    statut: StatutChambre::Disponible,
                    notes: None,
                },
            )
            .unwrap()
    }

    pub fn client(&mut self, nom: &str) -> Client {
        self.t.creer_client(&self.admin, DonneesClient { nom: nom.into(), ..Default::default() }).unwrap()
    }

    /// Une chambre « 101 » à 25 000/nuit et un client, prêts à réserver.
    pub fn base(&mut self) -> (Chambre, Client) {
        let tc = self.type_chambre("Standard", 25_000);
        let ch = self.chambre("101", &tc.id);
        let cl = self.client("Mbarga");
        (ch, cl)
    }

    pub fn resa(&mut self, chambre: &str, client: &str, a: &str, dep: &str) -> tanana_core::Resultat<Reservation> {
        self.t.creer_reservation(
            &self.admin,
            NouvelleReservation {
                chambre_id: chambre.into(),
                client_id: client.into(),
                date_arrivee: d(a),
                date_depart: d(dep),
                prix_total: None,
                notes: None,
            },
        )
    }

    pub fn actions(&mut self) -> Vec<String> {
        let mut v: Vec<String> = self
            .t
            .journal(&self.admin, FiltreJournal { limite: Some(1000), ..Default::default() })
            .unwrap()
            .into_iter()
            .map(|e| e.action)
            .collect();
        v.reverse();
        v
    }
}
