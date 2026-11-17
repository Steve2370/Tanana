//! Lance le relais.
//!
//! Variables d'environnement :
//! - `TANANA_RELAIS_ECOUTE` : adresse d'écoute (défaut `127.0.0.1:8080` ; le
//!   HTTPS est assuré devant par Caddy ou nginx) ;
//! - `TANANA_RELAIS_BASE` : fichier de données (défaut `relais.db`) ;
//! - `TANANA_RELAIS_CODES` : codes d'invitation acceptés, séparés par des virgules ;
//! - `TANANA_RELAIS_HOTELS_MAX` : nombre maximal d'hôtels (défaut 500) ;
//! - `TANANA_RELAIS_INSCRIPTIONS_LIBRES=oui` : essai local uniquement.

use tanana_relais::{Reglages, Relais};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).init();
    let var = |nom: &str| std::env::var(nom).ok().filter(|v| !v.trim().is_empty());
    let ecoute = var("TANANA_RELAIS_ECOUTE").unwrap_or_else(|| "127.0.0.1:8080".into());
    let base = var("TANANA_RELAIS_BASE").unwrap_or_else(|| "relais.db".into());
    let reglages = Reglages {
        codes_invitation: var("TANANA_RELAIS_CODES")
            .map(|c| c.split(',').map(|x| x.trim().to_string()).filter(|x| x.len() >= 8).collect())
            .unwrap_or_default(),
        inscriptions_libres: var("TANANA_RELAIS_INSCRIPTIONS_LIBRES").as_deref() == Some("oui"),
        hotels_max: var("TANANA_RELAIS_HOTELS_MAX").and_then(|v| v.parse().ok()).unwrap_or(500),
    };
    if reglages.codes_invitation.is_empty() && !reglages.inscriptions_libres {
        tracing::warn!("aucun code d'invitation (TANANA_RELAIS_CODES) : aucun nouvel hôtel ne pourra s'inscrire");
    }
    let relais = Relais::ouvrir(std::path::Path::new(&base), reglages).expect("ouverture de la base du relais");
    let ecouteur = tokio::net::TcpListener::bind(&ecoute).await.expect("adresse d'écoute");
    tracing::info!("relais Tanana à l'écoute sur {ecoute}");
    axum::serve(ecouteur, relais.routes())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("serveur");
}
