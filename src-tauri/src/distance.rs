//! Consultation à distance, côté application.
//!
//! Règle d'or : **jamais d'appel réseau pendant que la base est verrouillée**.
//! Chaque échange se fait en trois temps (préparer sous verrou, échanger
//! sans verrou, terminer sous verrou) : un envoi lent sur une connexion
//! mobile ne fige jamais l'écran de la réception.

use std::time::Duration;

use tanana_core::distance::BilanEnvoi;
use tanana_core::distance::relais::{Relais, RelaisHttp};
use tanana_core::{Erreur, Resultat};
use tauri::{AppHandle, Manager};

use crate::etat::Etat;

/// Envoie tout ce qui attend (ou signale la présence de l'hôtel s'il n'y a rien).
/// Sans effet si la consultation à distance n'est pas activée.
pub fn envoyer_tout(etat: &Etat) -> Resultat<BilanEnvoi> {
    let mut bilan = BilanEnvoi::default();
    while let Some(envoi) = etat.avec(|t| t.preparer_envoi())? {
        let resultat =
            RelaisHttp::new(&envoi.relais).and_then(|r| r.envoyer(&envoi.hotel_id, &envoi.jeton_envoi, &envoi.lot));
        match resultat {
            Ok(_) => {
                etat.avec(|t| t.envoi_reussi(envoi.lot.seq_fin))?;
                bilan.lots += 1;
                bilan.changements += envoi.changements as i64;
            }
            Err(e) => {
                let _ = etat.avec(|t| t.envoi_echoue(&e.to_string()));
                return Err(e);
            }
        }
    }
    if bilan.lots == 0
        && let Some((adresse, hotel_id, jeton)) = etat.avec(|t| t.liaison_distance())?
    {
        match RelaisHttp::new(&adresse).and_then(|r| r.signaler_presence(&hotel_id, &jeton)) {
            Ok(()) => etat.avec(|t| t.envoi_reussi(0))?,
            Err(e) => {
                let _ = etat.avec(|t| t.envoi_echoue(&e.to_string()));
                return Err(e);
            }
        }
    }
    Ok(bilan)
}

/// Télécharge et applique les nouveautés de l'hôtel consulté. Renvoie le nombre de lots appliqués.
pub fn mettre_a_jour_copie(etat: &Etat) -> Resultat<usize> {
    let mut total = 0;
    loop {
        let p = etat.avec(|t| t.preparer_lecture_copie())?;
        let page = RelaisHttp::new(&p.relais)?.lire(&p.hotel_id, &p.jeton_lecture, p.apres)?;
        total += etat.avec(|t| t.appliquer_page_copie(&page))?;
        if !page.suite {
            return Ok(total);
        }
    }
}

/// Envois automatiques à l'hôtel : une minute après le démarrage, puis toutes les minutes.
/// Les échecs (pas d'Internet) sont notés dans l'état, et journalisés une fois par changement.
pub fn lancer_envois_automatiques(app: AppHandle) {
    std::thread::Builder::new()
        .name("tanana-envois".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(20));
            let mut dernier_echec: Option<String> = None;
            loop {
                let etat = app.state::<Etat>();
                let copie = etat.avec(|t| Ok(t.est_une_copie())).unwrap_or(true);
                if !copie {
                    match envoyer_tout(&etat) {
                        Ok(b) => {
                            if b.lots > 0 {
                                log::info!(
                                    "consultation à distance : {} lot(s), {} changement(s) envoyés",
                                    b.lots,
                                    b.changements
                                );
                            }
                            if dernier_echec.take().is_some() {
                                log::info!("consultation à distance : connexion rétablie");
                            }
                        }
                        // Base pas prête (clé manquante…) : rien à faire
                        Err(Erreur::NonInitialise) => {}
                        Err(e) => {
                            let message = e.to_string();
                            if dernier_echec.as_deref() != Some(message.as_str()) {
                                log::warn!("consultation à distance : envoi impossible ({message})");
                                dernier_echec = Some(message);
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(60));
            }
        })
        .expect("fil des envois");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffre::CoffreMemoire;
    use tanana_core::distance::chiffrement::Secret;
    use tanana_core::distance::verifier_cle;
    use tanana_core::modeles::{ConfigurationInitiale, DonneesClient};

    /// Un vrai relais sur un port libre, dans son propre fil.
    fn lancer_relais(dossier: &std::path::Path) -> String {
        let relais = tanana_relais::Relais::ouvrir(
            &dossier.join("relais.db"),
            tanana_relais::Reglages { codes_invitation: vec![], inscriptions_libres: true, hotels_max: 5 },
        )
        .unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new().unwrap().block_on(async move {
                let ecouteur = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                tx.send(ecouteur.local_addr().unwrap()).unwrap();
                axum::serve(ecouteur, relais.routes()).await.unwrap();
            });
        });
        format!("http://{}", rx.recv().unwrap())
    }

    fn etat(dossier: &std::path::Path) -> Etat {
        Etat::demarrer(dossier.join("tanana.db"), dossier.join("sauvegardes"), Box::new(CoffreMemoire::default()))
    }

    /// Un hôtel installé, un client, et la consultation à distance activée
    /// (comme la commande `activer_distance` : réseau hors verrou).
    fn hotel_relie(dossier: &std::path::Path, adresse: &str) -> (Etat, String, tanana_core::distance::CodesDistance) {
        let hotel = etat(dossier);
        hotel
            .avec(|t| {
                t.initialiser(ConfigurationInitiale {
                    nom_hotel: "Hôtel du Lac".into(),
                    devise: "MGA".into(),
                    decimales_devise: 0,
                    admin_identifiant: "admin".into(),
                    admin_nom: "Beltran".into(),
                    admin_mot_de_passe: "admin2026".into(),
                })
            })
            .unwrap();
        let tok = hotel.avec(|t| t.connexion("admin", "admin2026")).unwrap().token;
        hotel
            .avec(|t| {
                t.creer_client(
                    &tok,
                    DonneesClient { nom: "Rakoto".into(), prenom: Some("Hery".into()), ..Default::default() },
                )
            })
            .unwrap();
        let relais = RelaisHttp::new(adresse).unwrap();
        let prep = hotel.avec(|t| t.preparer_activation_distance(&tok, None)).unwrap();
        let inscription = relais.inscrire(&prep.demande).unwrap();
        let codes = hotel.avec(|t| t.terminer_activation_distance(&tok, adresse, &prep, &inscription)).unwrap();
        (hotel, tok, codes)
    }

    #[test]
    fn de_l_hotel_au_poste_du_proprietaire_par_le_vrai_relais() {
        let dossier = tempfile::tempdir().unwrap();
        let adresse = lancer_relais(dossier.path());
        let (hotel, tok, codes) = hotel_relie(&dossier.path().join("hotel"), &adresse);

        let bilan = envoyer_tout(&hotel).unwrap();
        assert!(bilan.lots >= 1 && bilan.changements > 10, "{bilan:?}");
        let e = hotel.avec(|t| t.etat_consultation_distance(&tok)).unwrap();
        assert_eq!(e.en_attente, 0);
        assert!(e.dernier_envoi_le.is_some() && e.derniere_erreur.is_none());

        // Rien de neuf : l'hôtel signale seulement qu'il est vivant
        assert_eq!(envoyer_tout(&hotel).unwrap(), BilanEnvoi::default());

        // Poste du propriétaire, sur un autre ordinateur
        let poste = etat(&dossier.path().join("proprietaire"));
        let secret = Secret::lire(&codes.cle_secrete).unwrap();
        verifier_cle(&RelaisHttp::new(&adresse).unwrap(), &codes.hotel_id, &secret).unwrap();
        poste.creer_copie(&adresse, &codes.hotel_id, &secret, "proprio2026").unwrap();
        assert!(mettre_a_jour_copie(&poste).unwrap() >= 1);
        let jeton = poste.avec(|t| t.connexion_copie("proprio2026")).unwrap().token;
        let clients = poste.avec(|t| t.rechercher_clients(&jeton, "Rakoto")).unwrap();
        assert_eq!(clients.len(), 1);
        assert!(poste.avec(|t| t.etat_copie()).unwrap().dernier_contact.is_some());

        // Un nouveau client à l'hôtel arrive chez le propriétaire à la mise à jour suivante
        hotel.avec(|t| t.creer_client(&tok, DonneesClient { nom: "Rasoa".into(), ..Default::default() })).unwrap();
        assert_eq!(envoyer_tout(&hotel).unwrap().lots, 1);
        assert_eq!(mettre_a_jour_copie(&poste).unwrap(), 1);
        assert_eq!(poste.avec(|t| t.rechercher_clients(&jeton, "Rasoa")).unwrap().len(), 1);
        assert_eq!(mettre_a_jour_copie(&poste).unwrap(), 0, "rien de neuf");

        // Rien ne s'écrit depuis le poste du propriétaire
        let refus =
            poste.avec(|t| t.creer_client(&jeton, DonneesClient { nom: "Pirate".into(), ..Default::default() }));
        assert!(refus.is_err());
    }

    #[test]
    fn sans_internet_l_hotel_continue_et_rattrape_ensuite() {
        let dossier = tempfile::tempdir().unwrap();
        let adresse = lancer_relais(dossier.path());
        let (hotel, tok, _) = hotel_relie(&dossier.path().join("hotel"), &adresse);
        envoyer_tout(&hotel).unwrap();

        // Le serveur devient injoignable (adresse d'un port fermé)
        let ferme = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", l.local_addr().unwrap())
        };
        hotel.avec(|t| Ok(t.conn_brute().execute("UPDATE sync_config SET relais = ?1", [&ferme])?)).unwrap();
        hotel.avec(|t| t.creer_client(&tok, DonneesClient { nom: "Rasoa".into(), ..Default::default() })).unwrap();
        assert!(matches!(envoyer_tout(&hotel), Err(Erreur::RelaisInjoignable(_))));

        // L'hôtel travaille normalement, l'échec est noté, rien n'est perdu
        let e = hotel.avec(|t| t.etat_consultation_distance(&tok)).unwrap();
        assert!(e.derniere_erreur.is_some());
        assert!(e.en_attente >= 1);
        assert_eq!(hotel.avec(|t| t.rechercher_clients(&tok, "Rasoa")).unwrap().len(), 1);

        // Internet revient
        hotel.avec(|t| Ok(t.conn_brute().execute("UPDATE sync_config SET relais = ?1", [&adresse])?)).unwrap();
        assert_eq!(envoyer_tout(&hotel).unwrap().lots, 1);
        let e = hotel.avec(|t| t.etat_consultation_distance(&tok)).unwrap();
        assert_eq!((e.en_attente, e.derniere_erreur), (0, None));
    }

    #[test]
    fn sans_consultation_activee_rien_ne_part() {
        let dossier = tempfile::tempdir().unwrap();
        let e = etat(dossier.path());
        assert_eq!(envoyer_tout(&e).unwrap(), BilanEnvoi::default());
    }
}
