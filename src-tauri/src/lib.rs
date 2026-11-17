//! Coquille Tauri de Tanana : fenêtre, trousseau, sauvegarde au démarrage,
//! et branchement des commandes sur `tanana-core`.

mod coffre;
mod commandes;
mod distance;
mod documents;
mod etat;

use tauri::Manager;

use crate::coffre::TrousseauSysteme;
use crate::etat::Etat;

/// Vrai si cette version sait chercher ses mises à jour (clé publique et adresse fournies).
pub fn mise_a_jour_configuree(config: &tauri::Config) -> bool {
    config.plugins.0.get("updater").is_some_and(|u| {
        let cle = u.get("pubkey").and_then(|p| p.as_str()).is_some_and(|p| !p.trim().is_empty());
        let adresses = u.get("endpoints").and_then(|e| e.as_array()).is_some_and(|e| !e.is_empty());
        cle && adresses
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            // Journal TECHNIQUE (erreurs, démarrages) dans le dossier de logs de
            // l'app. À ne pas confondre avec le journal d'audit des actions.
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(5 * 1024 * 1024)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepSome(5))
                .build(),
        )
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Les versions publiées par GitHub reçoivent la clé publique de mise à jour
            // (voir `scripts/config-publication.mjs`) ; une version de développement n'en a pas.
            if mise_a_jour_configuree(app.config()) {
                app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            let dossier = app.path().app_data_dir()?;
            let etat = Etat::demarrer(
                dossier.join("tanana.db"),
                dossier.join("sauvegardes"),
                Box::new(TrousseauSysteme::new(&app.config().identifier)),
            );
            log::info!("Tanana {} démarré : {:?}", env!("CARGO_PKG_VERSION"), etat.etat_application().statut);
            if let Some(f) = etat.sauvegarde_du_jour() {
                log::info!("sauvegarde automatique : {}", f.display());
            }
            app.manage(etat);
            distance::lancer_envois_automatiques(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commandes::etat_application,
            commandes::infos_mise_a_jour,
            commandes::sauvegarder_avant_mise_a_jour,
            commandes::fournir_cle,
            commandes::initialiser,
            commandes::recuperer_acces_admin,
            commandes::connexion,
            commandes::connexion_pin,
            commandes::deconnexion,
            commandes::ma_session,
            commandes::changer_mon_mot_de_passe,
            commandes::definir_mon_pin,
            commandes::lister_utilisateurs,
            commandes::creer_utilisateur,
            commandes::changer_role,
            commandes::changer_statut_utilisateur,
            commandes::reinitialiser_mot_de_passe,
            commandes::definir_permission_perso,
            commandes::permissions_perso,
            commandes::permissions_utilisateur,
            commandes::lister_roles,
            commandes::creer_role,
            commandes::modifier_permissions_role,
            commandes::sessions_actives,
            commandes::fermer_session,
            commandes::journal,
            commandes::verifier_integrite_journal,
            commandes::lister_types_chambre,
            commandes::creer_type_chambre,
            commandes::modifier_type_chambre,
            commandes::lister_chambres,
            commandes::creer_chambre,
            commandes::modifier_chambre,
            commandes::rechercher_clients,
            commandes::obtenir_client,
            commandes::creer_client,
            commandes::modifier_client,
            commandes::planning,
            commandes::obtenir_reservation,
            commandes::reservations_du_client,
            commandes::chambres_disponibles,
            commandes::creer_reservation,
            commandes::modifier_reservation,
            commandes::annuler_reservation,
            commandes::enregistrer_arrivee,
            commandes::enregistrer_depart,
            commandes::paiements_reservation,
            commandes::encaisser,
            commandes::rembourser,
            commandes::saisir_depense,
            commandes::lister_depenses,
            commandes::annuler_depense,
            commandes::lister_paiements,
            commandes::rapport_caisse,
            commandes::parametres,
            commandes::modifier_parametres,
            commandes::lister_sauvegardes,
            commandes::examiner_sauvegarde,
            commandes::restaurer_sauvegarde,
            commandes::etat_distance,
            commandes::activer_distance,
            commandes::envoyer_distance,
            commandes::afficher_cle_distance,
            commandes::renvoyer_tout_distance,
            commandes::desactiver_distance,
            commandes::consulter_hotel_distant,
            commandes::connexion_copie,
            commandes::oublier_hotel_distant,
            commandes::etat_copie,
            commandes::mettre_a_jour_copie,
            commandes::taxes_hotel,
            commandes::definir_taxes_hotel,
            commandes::taxes_reservation,
            commandes::modifier_taxes_reservation,
            commandes::calculer_taxes,
            commandes::documents_reservation,
            commandes::imprimer_recu,
            commandes::imprimer_facture,
            commandes::ouvrir_dossier_documents,
            commandes::sauvegarder_vers,
            commandes::exporter_cle_base,
        ])
        .run(tauri::generate_context!())
        .expect("impossible de lancer Tanana");
}

#[cfg(test)]
mod tests {
    use super::mise_a_jour_configuree;

    fn config(plugins: serde_json::Value) -> tauri::Config {
        serde_json::from_value(serde_json::json!({ "identifier": "ca.tanana.essai", "plugins": plugins })).unwrap()
    }

    #[test]
    fn la_mise_a_jour_n_est_active_que_dans_les_versions_publiees() {
        assert!(!mise_a_jour_configuree(&config(serde_json::json!({}))), "version de développement");
        assert!(!mise_a_jour_configuree(&config(
            serde_json::json!({ "updater": { "pubkey": "", "endpoints": ["https://x"] } })
        )));
        assert!(!mise_a_jour_configuree(&config(
            serde_json::json!({ "updater": { "pubkey": "abc", "endpoints": [] } })
        )));
        assert!(mise_a_jour_configuree(&config(
            serde_json::json!({ "updater": { "pubkey": "abc", "endpoints": ["https://github.com/a/b/releases/latest/download/latest.json"] } })
        )));
        // La configuration livrée dans le dépôt est celle du développement
        let livree: tauri::Config = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert!(!mise_a_jour_configuree(&livree));
    }
}
