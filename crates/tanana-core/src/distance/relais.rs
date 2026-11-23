//! Dialogue avec le relais : par HTTP en vrai, en mémoire pour les tests.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tanana_protocole::{
    AccesOuvert, Accuse, CoffreAcces, CommandeChiffree, CommandesEnAttente, DemandeInscription,
    ERREUR_HOTEL_INJOIGNABLE, ERREUR_POSTE_REMPLACE, ErreurRelais, Inscription, Instantane, InstantaneRecu,
    LOTS_PAR_PAGE, LotChiffre, LotRecu, NouveauPrincipal, PageLots, PreuveAcces, ReponseChiffree, SelAcces, Temoin,
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

    // Accès par identifiant et mot de passe
    fn deposer_acces(&self, hotel_id: &str, jeton_envoi: &str, recherche: &str, coffre: &CoffreAcces) -> Resultat<()>;
    fn retirer_acces(&self, hotel_id: &str, jeton_envoi: &str, recherche: &str) -> Resultat<()>;
    fn sel_acces(&self, recherche: &str) -> Resultat<SelAcces>;
    /// Mauvaise preuve : `IdentifiantsInvalides`.
    fn ouvrir_acces(&self, recherche: &str, preuve: &PreuveAcces) -> Resultat<AccesOuvert>;

    // Instantané et ordinateur principal
    fn deposer_instantane(&self, hotel_id: &str, jeton_envoi: &str, instantane: &Instantane) -> Resultat<()>;
    /// Aucun instantané : `InstantaneAbsent`.
    fn lire_instantane(&self, hotel_id: &str, jeton_admin: &str) -> Resultat<InstantaneRecu>;
    fn devenir_principal(&self, hotel_id: &str, jeton_admin: &str) -> Resultat<NouveauPrincipal>;

    // Modifications à distance
    /// Envoie une modification et attend la réponse de l'hôtel.
    fn poster_commande(
        &self,
        hotel_id: &str,
        jeton_admin: &str,
        commande: &CommandeChiffree,
    ) -> Resultat<ReponseChiffree>;
    /// L'ordinateur principal attend les modifications (`attente` secondes au plus).
    fn lire_commandes(&self, hotel_id: &str, jeton_envoi: &str, attente: u64) -> Resultat<CommandesEnAttente>;
    fn repondre(&self, hotel_id: &str, jeton_envoi: &str, commande_id: &str, reponse: &ReponseChiffree)
    -> Resultat<()>;
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
            409 if detail == ERREUR_POSTE_REMPLACE => Erreur::PosteRemplace,
            409 if detail == ERREUR_HOTEL_INJOIGNABLE => Erreur::HotelInjoignable,
            504 => Erreur::PasDeReponseHotel,
            413 => Erreur::RelaisRefuse("envoi trop gros".into()),
            429 if detail == "trop d'essais" => Erreur::TropDEssais,
            429 => Erreur::RelaisInjoignable("relais surchargé, nouvel essai plus tard".into()),
            _ => Erreur::RelaisInjoignable(format!("erreur {statut} {detail}").trim().to_string()),
        })
    }

    fn avec_jeton<T: serde::de::DeserializeOwned>(
        &self,
        methode: &str,
        chemin: &str,
        jeton: Option<&str>,
        corps: Option<&serde_json::Value>,
    ) -> Resultat<T> {
        let url = self.url(chemin);
        let autorisation = jeton.map(|j| format!("Bearer {j}"));
        let r = match (methode, corps) {
            ("GET", _) => {
                let mut d = self.agent.get(&url);
                if let Some(a) = &autorisation {
                    d = d.header("Authorization", a);
                }
                d.call()
            }
            ("DELETE", _) => {
                let mut d = self.agent.delete(&url);
                if let Some(a) = &autorisation {
                    d = d.header("Authorization", a);
                }
                d.call()
            }
            (m, c) => {
                let mut d = if m == "PUT" { self.agent.put(&url) } else { self.agent.post(&url) };
                if let Some(a) = &autorisation {
                    d = d.header("Authorization", a);
                }
                d.send_json(c.cloned().unwrap_or(serde_json::json!({})))
            }
        };
        Self::reponse(r)
    }
}

fn json<T: serde::Serialize>(v: &T) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or_default()
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

    fn deposer_acces(&self, hotel_id: &str, jeton_envoi: &str, recherche: &str, coffre: &CoffreAcces) -> Resultat<()> {
        self.avec_jeton::<Vide>(
            "PUT",
            &format!("/hotels/{hotel_id}/acces/{recherche}"),
            Some(jeton_envoi),
            Some(&json(coffre)),
        )
        .map(|_| ())
    }

    fn retirer_acces(&self, hotel_id: &str, jeton_envoi: &str, recherche: &str) -> Resultat<()> {
        self.avec_jeton::<Vide>("DELETE", &format!("/hotels/{hotel_id}/acces/{recherche}"), Some(jeton_envoi), None)
            .map(|_| ())
    }

    fn sel_acces(&self, recherche: &str) -> Resultat<SelAcces> {
        self.avec_jeton("GET", &format!("/acces/{recherche}/sel"), None, None)
    }

    fn ouvrir_acces(&self, recherche: &str, preuve: &PreuveAcces) -> Resultat<AccesOuvert> {
        self.avec_jeton("POST", &format!("/acces/{recherche}"), None, Some(&json(preuve))).map_err(|e| match e {
            Erreur::RelaisRefuse(_) => Erreur::IdentifiantsInvalides,
            e => e,
        })
    }

    fn deposer_instantane(&self, hotel_id: &str, jeton_envoi: &str, instantane: &Instantane) -> Resultat<()> {
        self.avec_jeton::<Vide>(
            "PUT",
            &format!("/hotels/{hotel_id}/instantane"),
            Some(jeton_envoi),
            Some(&json(instantane)),
        )
        .map(|_| ())
    }

    fn lire_instantane(&self, hotel_id: &str, jeton_admin: &str) -> Resultat<InstantaneRecu> {
        // Les autres 404 (hôtel inconnu) ne peuvent pas arriver : le coffre vient de donner l'hôtel.
        self.avec_jeton("GET", &format!("/hotels/{hotel_id}/instantane"), Some(jeton_admin), None).map_err(
            |e| match e {
                Erreur::HotelInconnuDuRelais => Erreur::InstantaneAbsent,
                e => e,
            },
        )
    }

    fn devenir_principal(&self, hotel_id: &str, jeton_admin: &str) -> Resultat<NouveauPrincipal> {
        self.avec_jeton("POST", &format!("/hotels/{hotel_id}/principal"), Some(jeton_admin), None)
    }

    fn poster_commande(
        &self,
        hotel_id: &str,
        jeton_admin: &str,
        commande: &CommandeChiffree,
    ) -> Resultat<ReponseChiffree> {
        self.avec_jeton("POST", &format!("/hotels/{hotel_id}/commandes"), Some(jeton_admin), Some(&json(commande)))
    }

    fn lire_commandes(&self, hotel_id: &str, jeton_envoi: &str, attente: u64) -> Resultat<CommandesEnAttente> {
        self.avec_jeton("GET", &format!("/hotels/{hotel_id}/commandes?attente={attente}"), Some(jeton_envoi), None)
    }

    fn repondre(
        &self,
        hotel_id: &str,
        jeton_envoi: &str,
        commande_id: &str,
        reponse: &ReponseChiffree,
    ) -> Resultat<()> {
        self.avec_jeton::<Vide>(
            "POST",
            &format!("/hotels/{hotel_id}/reponses/{commande_id}"),
            Some(jeton_envoi),
            Some(&json(reponse)),
        )
        .map(|_| ())
    }
}

// ───────────────────────────── En mémoire (tests) ─────────────────────────────

#[derive(Default)]
struct HotelMemoire {
    jeton_envoi: String,
    ancien_jeton_envoi: Option<String>,
    jeton_lecture: String,
    jeton_admin: Option<String>,
    epoque: i64,
    temoin: Option<Temoin>,
    /// (époque, lot)
    lots: Vec<(i64, LotRecu)>,
    dernier_contact: Option<String>,
    instantane: Option<InstantaneRecu>,
}

impl HotelMemoire {
    fn envoi(&self, jeton: &str) -> Resultat<()> {
        if self.jeton_envoi == jeton {
            Ok(())
        } else if self.ancien_jeton_envoi.as_deref() == Some(jeton) {
            Err(Erreur::PosteRemplace)
        } else {
            Err(Erreur::RelaisRefuse("accès refusé".into()))
        }
    }

    fn admin(&self, jeton: &str) -> Resultat<()> {
        if self.jeton_admin.as_deref() == Some(jeton) {
            Ok(())
        } else {
            Err(Erreur::RelaisRefuse("accès refusé".into()))
        }
    }
}

/// Relais en mémoire, avec les mêmes règles que le vrai (contrôle des jetons,
/// envois rejoués ignorés). Peut simuler une coupure d'Internet.
#[derive(Default)]
pub struct RelaisMemoire {
    hotels: Mutex<HashMap<String, HotelMemoire>>,
    /// Coffres d'accès : recherche → (hôtel, coffre)
    acces: Mutex<HashMap<String, (String, CoffreAcces)>>,
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
        let mut tout: Vec<String> =
            h.values().flat_map(|x| x.lots.iter().map(|(_, l)| format!("{}{}", l.lot.nonce, l.lot.chiffre))).collect();
        tout.extend(h.values().filter_map(|x| x.instantane.as_ref().map(|i| i.chiffre.clone())));
        tout.extend(
            self.acces.lock().unwrap().iter().map(|(k, (_, c))| format!("{k}{}{}{}", c.sel, c.verif, c.chiffre)),
        );
        tout.join("\n")
    }

    /// Simule le temps qui passe sans nouvelles de l'ordinateur principal.
    pub fn oublier_contact(&self) {
        for h in self.hotels.lock().unwrap().values_mut() {
            h.dernier_contact = None;
        }
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
        hotel.envoi(jeton_envoi)?;
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
                jeton_admin: demande.jeton_admin.clone(),
                epoque: 1,
                temoin: Some(demande.temoin.clone()),
                ..Default::default()
            },
        );
        Ok(Inscription { hotel_id, jeton_envoi })
    }

    fn envoyer(&self, hotel_id: &str, jeton_envoi: &str, lot: &LotChiffre) -> Resultat<Accuse> {
        self.connecte()?;
        let mut h = self.hotels.lock().unwrap();
        let hotel = h.get_mut(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?;
        hotel.envoi(jeton_envoi)?;
        // Lot déjà reçu (accusé perdu en route) : on redonne le même numéro.
        let epoque = hotel.epoque;
        if let Some((_, l)) = hotel.lots.iter().find(|(e, l)| *e == epoque && l.lot.seq_debut == lot.seq_debut) {
            return Ok(Accuse { numero: l.numero });
        }
        let numero = hotel.lots.len() as i64 + 1;
        hotel.dernier_contact = Some(format!("2026-01-01T00:00:{:02}Z", numero % 60));
        hotel.lots.push((epoque, LotRecu { numero, recu_le: "2026-01-01T00:00:00Z".into(), lot: lot.clone() }));
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
        let restants: Vec<LotRecu> = hotel.lots.iter().map(|(_, l)| l).filter(|l| l.numero > apres).cloned().collect();
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
                self.acces.lock().unwrap().retain(|_, (hotel, _)| hotel != hotel_id);
                Ok(())
            }
            Some(_) => Err(Erreur::RelaisRefuse("accès refusé".into())),
            None => Ok(()),
        }
    }

    fn deposer_acces(&self, hotel_id: &str, jeton_envoi: &str, recherche: &str, coffre: &CoffreAcces) -> Resultat<()> {
        self.connecte()?;
        let h = self.hotels.lock().unwrap();
        h.get(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?.envoi(jeton_envoi)?;
        self.acces.lock().unwrap().insert(recherche.to_string(), (hotel_id.to_string(), coffre.clone()));
        Ok(())
    }

    fn retirer_acces(&self, hotel_id: &str, jeton_envoi: &str, recherche: &str) -> Resultat<()> {
        self.connecte()?;
        let h = self.hotels.lock().unwrap();
        h.get(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?.envoi(jeton_envoi)?;
        self.acces.lock().unwrap().retain(|k, (hotel, _)| !(k == recherche && hotel == hotel_id));
        Ok(())
    }

    fn sel_acces(&self, recherche: &str) -> Resultat<SelAcces> {
        self.connecte()?;
        let a = self.acces.lock().unwrap();
        Ok(SelAcces {
            sel: a.get(recherche).map(|(_, c)| c.sel.clone()).unwrap_or_else(|| "AAAAAAAAAAAAAAAAAAAAAA==".into()),
        })
    }

    fn ouvrir_acces(&self, recherche: &str, preuve: &PreuveAcces) -> Resultat<AccesOuvert> {
        use sha2::Digest;
        self.connecte()?;
        let a = self.acces.lock().unwrap();
        match a.get(recherche) {
            Some((hotel_id, c)) if c.verif == hex::encode(sha2::Sha256::digest(preuve.preuve.as_bytes())) => {
                Ok(AccesOuvert { hotel_id: hotel_id.clone(), nonce: c.nonce.clone(), chiffre: c.chiffre.clone() })
            }
            _ => Err(Erreur::IdentifiantsInvalides),
        }
    }

    fn deposer_instantane(&self, hotel_id: &str, jeton_envoi: &str, instantane: &Instantane) -> Resultat<()> {
        self.connecte()?;
        let mut h = self.hotels.lock().unwrap();
        let hotel = h.get_mut(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?;
        hotel.envoi(jeton_envoi)?;
        hotel.instantane = Some(InstantaneRecu {
            apres_numero: instantane.apres_numero,
            recu_le: "2026-01-01T00:00:00Z".into(),
            nonce: instantane.nonce.clone(),
            chiffre: instantane.chiffre.clone(),
        });
        Ok(())
    }

    fn lire_instantane(&self, hotel_id: &str, jeton_admin: &str) -> Resultat<InstantaneRecu> {
        self.connecte()?;
        let h = self.hotels.lock().unwrap();
        let hotel = h.get(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?;
        hotel.admin(jeton_admin)?;
        hotel.instantane.clone().ok_or(Erreur::InstantaneAbsent)
    }

    fn devenir_principal(&self, hotel_id: &str, jeton_admin: &str) -> Resultat<NouveauPrincipal> {
        self.connecte()?;
        let mut h = self.hotels.lock().unwrap();
        let hotel = h.get_mut(hotel_id).ok_or(Erreur::HotelInconnuDuRelais)?;
        hotel.admin(jeton_admin)?;
        hotel.epoque += 1;
        let jeton_envoi = format!("envoi-{hotel_id}-{}", hotel.epoque);
        hotel.ancien_jeton_envoi = Some(std::mem::replace(&mut hotel.jeton_envoi, jeton_envoi.clone()));
        Ok(NouveauPrincipal { jeton_envoi })
    }

    /// Le relais en mémoire ne transporte pas les modifications : les tests du
    /// canal passent par le vrai relais (crates/tanana-relais/tests).
    fn poster_commande(&self, _: &str, _: &str, _: &CommandeChiffree) -> Resultat<ReponseChiffree> {
        self.connecte()?;
        Err(Erreur::HotelInjoignable)
    }

    fn lire_commandes(&self, hotel_id: &str, jeton_envoi: &str, _attente: u64) -> Resultat<CommandesEnAttente> {
        self.signaler_presence(hotel_id, jeton_envoi)?;
        Ok(CommandesEnAttente::default())
    }

    fn repondre(&self, _: &str, _: &str, _: &str, _: &ReponseChiffree) -> Resultat<()> {
        Err(Erreur::RelaisRefuse("demande inconnue ou expirée".into()))
    }
}
