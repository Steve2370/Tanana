//! Dialogue avec le relais : par HTTP en vrai, en mémoire pour les tests.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tanana_protocole::{
    Accuse, DemandeInscription, ErreurRelais, Inscription, LOTS_PAR_PAGE, LotChiffre, LotRecu, PageLots, Temoin,
    VERSION,
};

use crate::erreur::{Erreur, Resultat};

/// Ce que Tanana demande au relais.
pub trait Relais: Send + Sync {
    fn inscrire(&self, demande: &DemandeInscription) -> Resultat<Inscription>;
    fn envoyer(&self, hotel_id: &str, jeton_envoi: &str, lot: &LotChiffre) -> Resultat<Accuse>;
    fn temoin(&self, hotel_id: &str, jeton_lecture: &str) -> Resultat<Temoin>;
    fn lire(&self, hotel_id: &str, jeton_lecture: &str, apres: i64) -> Resultat<PageLots>;
    fn desinscrire(&self, hotel_id: &str, jeton_envoi: &str) -> Resultat<()>;
    /// « L'hôtel est en ligne, rien de neuf » : l'administrateur voit que tout va bien.
    fn signaler_presence(&self, hotel_id: &str, jeton_envoi: &str) -> Resultat<()>;
}

// ───────────────────────────── HTTP ─────────────────────────────

/// Relais joint par Internet (HTTPS).
pub struct RelaisHttp {
    adresse: String,
    agent: ureq::Agent,
}

impl RelaisHttp {
    /// `adresse` : « https://relais.exemple.com » (sans « /v1 »).
    pub fn new(adresse: &str) -> Resultat<Self> {
        let adresse = adresse.trim().trim_end_matches('/').to_string();
        let local = adresse.starts_with("http://127.0.0.1") || adresse.starts_with("http://localhost");
        if !adresse.starts_with("https://") && !local {
            return Err(Erreur::Validation("l'adresse du relais doit commencer par https://".into()));
        }
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_secs(120)))
            .http_status_as_error(false)
            .user_agent(concat!("Tanana/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Ok(Self { adresse, agent })
    }

    fn url(&self, chemin: &str) -> String {
        format!("{}/{VERSION}{chemin}", self.adresse)
    }

    fn reponse<T: serde::de::DeserializeOwned>(
        r: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> Resultat<T> {
        let mut r = r.map_err(|e| Erreur::RelaisInjoignable(e.to_string()))?;
        let statut = r.status().as_u16();
        if (200..300).contains(&statut) {
            return r
                .body_mut()
                .read_json::<T>()
                .map_err(|e| Erreur::RelaisInjoignable(format!("réponse illisible : {e}")));
        }
        let detail = r.body_mut().read_json::<ErreurRelais>().map(|e| e.erreur).unwrap_or_default();
        Err(match statut {
            401 | 403 => Erreur::RelaisRefuse(format!("accès refusé {detail}").trim().to_string()),
            404 => Erreur::HotelInconnuDuRelais,
            413 => Erreur::RelaisRefuse("envoi trop gros".into()),
            429 => Erreur::RelaisInjoignable("relais surchargé, nouvel essai plus tard".into()),
            _ => Erreur::RelaisInjoignable(format!("erreur {statut} {detail}").trim().to_string()),
        })
    }
}

#[derive(serde::Deserialize)]
struct Vide {}

impl Relais for RelaisHttp {
    fn signaler_presence(&self, hotel_id: &str, jeton_envoi: &str) -> Resultat<()> {
        Self::reponse::<Vide>(
            self.agent
                .post(&self.url(&format!("/hotels/{hotel_id}/presence")))
                .header("Authorization", &format!("Bearer {jeton_envoi}"))
                .send_json(serde_json::json!({})),
        )
        .map(|_| ())
    }

    fn inscrire(&self, demande: &DemandeInscription) -> Resultat<Inscription> {
        Self::reponse(self.agent.post(&self.url("/hotels")).send_json(demande))
    }

    fn envoyer(&self, hotel_id: &str, jeton_envoi: &str, lot: &LotChiffre) -> Resultat<Accuse> {
        Self::reponse(
            self.agent
                .post(&self.url(&format!("/hotels/{hotel_id}/lots")))
                .header("Authorization", &format!("Bearer {jeton_envoi}"))
                .send_json(lot),
        )
    }

    fn temoin(&self, hotel_id: &str, jeton_lecture: &str) -> Resultat<Temoin> {
        Self::reponse(
            self.agent
                .get(&self.url(&format!("/hotels/{hotel_id}/temoin")))
                .header("Authorization", &format!("Bearer {jeton_lecture}"))
                .call(),
        )
    }

    fn lire(&self, hotel_id: &str, jeton_lecture: &str, apres: i64) -> Resultat<PageLots> {
        Self::reponse(
            self.agent
                .get(&self.url(&format!("/hotels/{hotel_id}/lots?apres={apres}")))
                .header("Authorization", &format!("Bearer {jeton_lecture}"))
                .call(),
        )
    }

    fn desinscrire(&self, hotel_id: &str, jeton_envoi: &str) -> Resultat<()> {
        Self::reponse::<Vide>(
            self.agent
                .delete(&self.url(&format!("/hotels/{hotel_id}")))
                .header("Authorization", &format!("Bearer {jeton_envoi}"))
                .call(),
        )
        .map(|_| ())
    }
}

// ───────────────────────────── En mémoire (tests) ─────────────────────────────

#[derive(Default)]
struct HotelMemoire {
    jeton_envoi: String,
    jeton_lecture: String,
    temoin: Option<Temoin>,
    lots: Vec<LotRecu>,
    dernier_contact: Option<String>,
}

/// Relais en mémoire, avec les mêmes règles que le vrai (contrôle des jetons,
/// envois rejoués ignorés). Peut simuler une coupure d'Internet.
#[derive(Default)]
pub struct RelaisMemoire {
    hotels: Mutex<HashMap<String, HotelMemoire>>,
    pub hors_ligne: std::sync::atomic::AtomicBool,
    /// Nombre de « présences » reçues
    pub presences: std::sync::atomic::AtomicUsize,
}

impl RelaisMemoire {
    fn connecte(&self) -> Resultat<()> {
        if self.hors_ligne.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erreur::RelaisInjoignable("pas de connexion Internet".into()));
        }
        Ok(())
    }

    /// Tout ce que le relais a reçu, tel quel (pour vérifier qu'il ne voit rien de lisible).
    pub fn contenu_brut(&self) -> String {
        let h = self.hotels.lock().unwrap();
        h.values()
            .flat_map(|x| x.lots.iter().map(|l| format!("{}{}", l.lot.nonce, l.lot.chiffre)))
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn nombre_lots(&self) -> usize {
        self.hotels.lock().unwrap().values().map(|h| h.lots.len()).sum()
    }
}

impl Relais for RelaisMemoire {
    fn signaler_presence(&self, hotel_id: &str, jeton_envoi: &str) -> Resultat<()> {
        self.connecte()?;
        let mut h = self.hotels.lock().unwrap();
        let hotel = h.get_mut(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?;
        if hotel.jeton_envoi != jeton_envoi {
            return Err(Erreur::RelaisRefuse("accès refusé".into()));
        }
        hotel.dernier_contact = Some("2026-01-01T12:00:00Z".into());
        self.presences.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn inscrire(&self, demande: &DemandeInscription) -> Resultat<Inscription> {
        self.connecte()?;
        let mut h = self.hotels.lock().unwrap();
        let hotel_id = format!("H{}", h.len() + 1);
        let jeton_envoi = format!("envoi-{}", hotel_id);
        h.insert(
            hotel_id.clone(),
            HotelMemoire {
                jeton_envoi: jeton_envoi.clone(),
                jeton_lecture: demande.jeton_lecture.clone(),
                temoin: Some(demande.temoin.clone()),
                lots: vec![],
                dernier_contact: None,
            },
        );
        Ok(Inscription { hotel_id, jeton_envoi })
    }

    fn envoyer(&self, hotel_id: &str, jeton_envoi: &str, lot: &LotChiffre) -> Resultat<Accuse> {
        self.connecte()?;
        let mut h = self.hotels.lock().unwrap();
        let hotel = h.get_mut(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?;
        if hotel.jeton_envoi != jeton_envoi {
            return Err(Erreur::RelaisRefuse("accès refusé".into()));
        }
        // Lot déjà reçu (accusé perdu en route) : on redonne le même numéro.
        if let Some(l) = hotel.lots.iter().find(|l| l.lot.seq_debut == lot.seq_debut) {
            return Ok(Accuse { numero: l.numero });
        }
        let numero = hotel.lots.len() as i64 + 1;
        hotel.dernier_contact = Some(format!("2026-01-01T00:00:{:02}Z", numero % 60));
        hotel.lots.push(LotRecu { numero, recu_le: "2026-01-01T00:00:00Z".into(), lot: lot.clone() });
        Ok(Accuse { numero })
    }

    fn temoin(&self, hotel_id: &str, jeton_lecture: &str) -> Resultat<Temoin> {
        self.connecte()?;
        let h = self.hotels.lock().unwrap();
        let hotel = h.get(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?;
        if hotel.jeton_lecture != jeton_lecture {
            return Err(Erreur::RelaisRefuse("accès refusé".into()));
        }
        hotel.temoin.clone().ok_or(Erreur::HotelInconnuDuRelais)
    }

    fn lire(&self, hotel_id: &str, jeton_lecture: &str, apres: i64) -> Resultat<PageLots> {
        self.connecte()?;
        let h = self.hotels.lock().unwrap();
        let hotel = h.get(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?;
        if hotel.jeton_lecture != jeton_lecture {
            return Err(Erreur::RelaisRefuse("accès refusé".into()));
        }
        let restants: Vec<LotRecu> = hotel.lots.iter().filter(|l| l.numero > apres).cloned().collect();
        Ok(PageLots {
            suite: restants.len() > LOTS_PAR_PAGE,
            lots: restants.into_iter().take(LOTS_PAR_PAGE).collect(),
            dernier_contact: hotel.dernier_contact.clone(),
        })
    }

    fn desinscrire(&self, hotel_id: &str, jeton_envoi: &str) -> Resultat<()> {
        self.connecte()?;
        let mut h = self.hotels.lock().unwrap();
        match h.get(hotel_id) {
            Some(x) if x.jeton_envoi == jeton_envoi => {
                h.remove(hotel_id);
                Ok(())
            }
            Some(_) => Err(Erreur::RelaisRefuse("accès refusé".into())),
            None => Ok(()),
        }
    }
}
