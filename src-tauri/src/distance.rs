//! Travail à distance, côté application.
//!
//! **Ordinateur principal** (celui de l'hôtel) : toutes les 15 secondes, il
//! envoie ses changements, dépose les coffres d'accès et, de temps en temps,
//! une copie complète. Un autre fil attend les modifications demandées à
//! distance, les exécute et répond.
//!
//! **Poste à distance** (le portable de l'administratrice) : sa copie se met
//! à jour toutes les 15 secondes ; ses modifications partent vers l'hôtel.
//!
//! Règle d'or : **jamais d'appel réseau pendant que la base est verrouillée**
//! (sauf « Retrouver mon hôtel », quand personne ne travaille encore ici).
//! Un envoi lent sur une connexion mobile ne fige jamais l'écran de la réception.

use std::time::{Duration, Instant};

use serde_json::{Map, Value};
use tanana_core::distance::BilanEnvoi;
use tanana_core::distance::relais::{Relais, RelaisHttp};
use tanana_core::modeles::SessionOuverte;
use tanana_core::{Erreur, Resultat};
use tauri::{AppHandle, Manager};

use crate::etat::Etat;

/// Rythme des envois et des mises à jour de la copie.
pub const INTERVALLE: Duration = Duration::from_secs(15);
/// Attente maximale d'une modification par l'ordinateur principal (secondes).
const ATTENTE_COMMANDES: u64 = 25;
/// Toutes les combien on garde la session ouverte sur l'hôtel (poste à distance).
const ENTRETIEN_SESSION: Duration = Duration::from_secs(4 * 60);

/// Note un refus du relais (« poste remplacé ») ou un échec, puis le renvoie.
fn echec(etat: &Etat, e: Erreur) -> Erreur {
    let _ = etat.avec(|t| t.envoi_refuse(&e));
    e
}

/// Envoie tout ce qui attend (ou signale la présence de l'hôtel s'il n'y a rien).
/// Sans effet si la consultation à distance n'est pas activée.
pub fn envoyer_tout(etat: &Etat) -> Resultat<BilanEnvoi> {
    let mut bilan = BilanEnvoi::default();
    while let Some(envoi) = etat.avec(|t| t.preparer_envoi())? {
        let resultat =
            RelaisHttp::new(&envoi.relais).and_then(|r| r.envoyer(&envoi.hotel_id, &envoi.jeton_envoi, &envoi.lot));
        match resultat {
            Ok(accuse) => {
                etat.avec(|t| t.envoi_reussi(envoi.lot.seq_fin, Some(accuse.numero)))?;
                bilan.lots += 1;
                bilan.changements += envoi.changements as i64;
            }
            Err(e) => return Err(echec(etat, e)),
        }
    }
    if bilan.lots == 0
        && let Some((adresse, hotel_id, jeton)) = etat.avec(|t| t.liaison_distance())?
    {
        match RelaisHttp::new(&adresse).and_then(|r| r.signaler_presence(&hotel_id, &jeton)) {
            Ok(()) => etat.avec(|t| t.envoi_reussi(0, None))?,
            Err(e) => return Err(echec(etat, e)),
        }
    }
    Ok(bilan)
}

/// Dépose (ou retire) les coffres d'accès des administrateurs.
pub fn deposer_acces(etat: &Etat) -> Resultat<()> {
    let Some(e) = etat.avec(|t| t.preparer_acces())? else { return Ok(()) };
    let relais = RelaisHttp::new(&e.relais)?;
    for (uid, recherche, coffre) in &e.depots {
        relais.deposer_acces(&e.hotel_id, &e.jeton_envoi, recherche, coffre).map_err(|x| echec(etat, x))?;
        etat.avec(|t| t.acces_depose(uid, coffre))?;
    }
    for recherche in &e.retraits {
        relais.retirer_acces(&e.hotel_id, &e.jeton_envoi, recherche).map_err(|x| echec(etat, x))?;
        etat.avec(|t| t.acces_retire(recherche))?;
    }
    Ok(())
}

/// Dépose une copie complète si c'est le moment.
pub fn deposer_instantane(etat: &Etat) -> Resultat<bool> {
    let dossier = etat.dossier_travail();
    let Some(e) = etat.avec(|t| t.preparer_instantane(&dossier))? else { return Ok(false) };
    RelaisHttp::new(&e.relais)?
        .deposer_instantane(&e.hotel_id, &e.jeton_envoi, &e.instantane)
        .map_err(|x| echec(etat, x))?;
    etat.avec(|t| t.instantane_depose(e.instantane.apres_numero))?;
    Ok(true)
}

/// Relie l'hôtel au relais si c'était demandé à sa création (et qu'Internet est là).
pub fn activation_auto(etat: &Etat) -> Resultat<bool> {
    let Some((adresse, prep)) = etat.avec(|t| t.preparer_activation_auto())? else { return Ok(false) };
    let inscription = RelaisHttp::new(&adresse)?.inscrire(&prep.demande)?;
    etat.avec(|t| t.terminer_activation_auto(&adresse, &prep, &inscription))?;
    log::info!("travail à distance : hôtel relié au serveur {adresse}");
    Ok(true)
}

/// Ordinateur principal : attend les modifications demandées à distance
/// (jusqu'à 25 s), les exécute, envoie les changements, puis répond.
pub fn traiter_commandes(etat: &Etat) -> Resultat<usize> {
    let Some((adresse, hotel_id, jeton)) =
        etat.avec(|t| if t.est_une_copie() { Ok(None) } else { t.liaison_distance() })?
    else {
        return Ok(0);
    };
    let relais = RelaisHttp::new(&adresse)?;
    let attente = relais.lire_commandes(&hotel_id, &jeton, ATTENTE_COMMANDES).map_err(|e| echec(etat, e))?;
    let mut faites = 0;
    for c in attente.commandes {
        match etat.avec(|t| t.traiter_commande_distante(&c)) {
            Ok(reponse) => {
                // La copie de l'administratrice doit voir le changement dès la réponse
                if let Err(e) = envoyer_tout(etat) {
                    log::warn!("modification à distance : envoi des changements impossible ({e})");
                }
                relais.repondre(&hotel_id, &jeton, &c.id, &reponse).map_err(|e| echec(etat, e))?;
                faites += 1;
            }
            Err(e) => log::warn!("demande à distance ignorée : {e}"),
        }
    }
    Ok(faites)
}

/// Poste à distance : télécharge et applique les nouveautés de l'hôtel.
/// Renvoie le nombre de lots appliqués.
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

/// Poste à distance : envoie une demande à l'hôtel et attend sa réponse.
/// Ensuite, la copie est mise à jour (l'hôtel a envoyé ses changements avant de répondre).
pub fn envoyer_commande(etat: &Etat, commande: &str, args: Map<String, Value>) -> Resultat<Value> {
    let (hotel_id, jeton_admin, c) = etat.avec(|t| t.preparer_commande_distante(commande, args))?;
    let adresse = etat.avec(|t| t.preparer_lecture_copie().map(|p| p.relais))?;
    let reponse = RelaisHttp::new(&adresse)?.poster_commande(&hotel_id, &jeton_admin, &c)?;
    let v = etat.avec(|t| t.lire_reponse_distante(&c.id, &reponse))?;
    if let Err(e) = mettre_a_jour_copie(etat) {
        log::warn!("mise à jour de la copie après une modification : {e}");
    }
    Ok(v)
}

/// Poste à distance : ouvre une session sur l'ordinateur de l'hôtel.
pub fn connecter_a_l_hotel(etat: &Etat, identifiant: &str, mot_de_passe: &str) -> Resultat<SessionOuverte> {
    let mut args = Map::new();
    args.insert("identifiant".into(), Value::String(identifiant.into()));
    args.insert("mot_de_passe".into(), Value::String(mot_de_passe.into()));
    let s: SessionOuverte = serde_json::from_value(envoyer_commande(etat, "connexion", args)?)?;
    if let Ok(mut d) = etat.distant.lock() {
        d.session_hotel = Some(s.clone());
    }
    Ok(s)
}

/// Rouvre la session sur l'hôtel avec l'identifiant et le mot de passe gardés en mémoire.
pub fn reconnecter(etat: &Etat) -> Resultat<SessionOuverte> {
    let (identifiant, mdp) = {
        let d = etat.distant.lock().map_err(|_| Erreur::Base("verrou".into()))?;
        match (&d.identifiant, &d.mot_de_passe) {
            (Some(i), Some(m)) => (i.clone(), m.clone()),
            _ => return Err(Erreur::NonAuthentifie),
        }
    };
    connecter_a_l_hotel(etat, &identifiant, &mdp)
}

/// Session ouverte sur l'hôtel (jeton), en la rouvrant si besoin.
pub fn jeton_hotel(etat: &Etat) -> Resultat<String> {
    let existant = etat.distant.lock().ok().and_then(|d| d.session_hotel.as_ref().map(|s| s.token.clone()));
    match existant {
        Some(j) => Ok(j),
        None => reconnecter(etat).map(|s| s.token),
    }
}

/// Une modification à distance, avec la session de l'hôtel. Si l'hôtel a
/// fermé la session (inactivité, redémarrage), on la rouvre une fois.
pub fn modifier_a_distance(etat: &Etat, commande: &str, args: Map<String, Value>) -> Resultat<Value> {
    let avec_jeton = |jeton: String| {
        let mut a = args.clone();
        a.insert("token".into(), Value::String(jeton));
        a
    };
    match envoyer_commande(etat, commande, avec_jeton(jeton_hotel(etat)?)) {
        Err(Erreur::NonAuthentifie | Erreur::SessionExpiree) => {
            let s = reconnecter(etat)?;
            envoyer_commande(etat, commande, avec_jeton(s.token))
        }
        r => r,
    }
}

/// Une étape du poste à distance : copie à jour, session sur l'hôtel entretenue.
fn entretenir_poste(etat: &Etat, dernier_entretien: &mut Option<Instant>) {
    if let Err(e) = mettre_a_jour_copie(etat) {
        log::debug!("mise à jour de la copie : {e}");
    }
    let (session, identifiants) = match etat.distant.lock() {
        Ok(d) => (d.session_hotel.clone(), d.mot_de_passe.is_some()),
        Err(_) => return,
    };
    match session {
        // Pas encore de session (hôtel injoignable à la connexion) : on réessaie
        None if identifiants => {
            if dernier_entretien.is_none_or(|t| t.elapsed() >= Duration::from_secs(60)) {
                *dernier_entretien = Some(Instant::now());
                if reconnecter(etat).is_ok() {
                    log::info!("travail à distance : l'hôtel est joignable, les modifications sont possibles");
                }
            }
        }
        Some(s) if dernier_entretien.is_none_or(|t| t.elapsed() >= ENTRETIEN_SESSION) => {
            *dernier_entretien = Some(Instant::now());
            let mut args = Map::new();
            args.insert("token".into(), Value::String(s.token));
            match envoyer_commande(etat, "ma_session", args) {
                Err(Erreur::NonAuthentifie | Erreur::SessionExpiree) => {
                    if let Ok(mut d) = etat.distant.lock() {
                        d.session_hotel = None;
                    }
                    let _ = reconnecter(etat);
                }
                Err(e) => log::debug!("entretien de la session sur l'hôtel : {e}"),
                Ok(_) => {}
            }
        }
        _ => {}
    }
}

/// Une étape de l'ordinateur principal : activation, envois, coffres, copie complète.
fn envoyer_etape(etat: &Etat) -> Resultat<BilanEnvoi> {
    activation_auto(etat)?;
    let bilan = envoyer_tout(etat)?;
    deposer_acces(etat)?;
    if deposer_instantane(etat)? {
        log::info!("travail à distance : copie complète de l'hôtel déposée");
    }
    Ok(bilan)
}

/// Fils de travail à distance : envois (ou mise à jour de la copie) toutes les
/// 15 secondes, et écoute des modifications demandées à distance.
pub fn lancer_envois_automatiques(app: AppHandle) {
    let a = app.clone();
    std::thread::Builder::new()
        .name("tanana-envois".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(5));
            let mut dernier_echec: Option<String> = None;
            let mut entretien: Option<Instant> = None;
            loop {
                let etat = a.state::<Etat>();
                match etat.avec(|t| Ok(t.est_une_copie())) {
                    Ok(true) => entretenir_poste(&etat, &mut entretien),
                    Ok(false) => match envoyer_etape(&etat) {
                        Ok(b) => {
                            if b.lots > 0 {
                                log::info!(
                                    "travail à distance : {} lot(s), {} changement(s) envoyés",
                                    b.lots,
                                    b.changements
                                );
                            }
                            if dernier_echec.take().is_some() {
                                log::info!("travail à distance : connexion rétablie");
                            }
                        }
                        Err(Erreur::NonInitialise) => {}
                        Err(e) => {
                            let message = e.to_string();
                            if dernier_echec.as_deref() != Some(message.as_str()) {
                                log::warn!("travail à distance : envoi impossible ({message})");
                                dernier_echec = Some(message);
                            }
                        }
                    },
                    Err(_) => {}
                }
                std::thread::sleep(INTERVALLE);
            }
        })
        .expect("fil des envois");

    std::thread::Builder::new()
        .name("tanana-ecoute".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(8));
            loop {
                let etat = app.state::<Etat>();
                let debut = Instant::now();
                match traiter_commandes(&etat) {
                    Ok(n) if n > 0 => log::info!("travail à distance : {n} modification(s) faite(s) à distance"),
                    Ok(_) => {}
                    Err(e) => log::debug!("écoute des modifications à distance : {e}"),
                }
                // Rien à écouter, ou erreur rapide : on ne tourne pas à vide
                if debut.elapsed() < Duration::from_secs(2) {
                    std::thread::sleep(INTERVALLE);
                }
            }
        })
        .expect("fil d'écoute");
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
