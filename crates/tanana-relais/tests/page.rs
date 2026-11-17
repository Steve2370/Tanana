//! La page web de consultation, servie par le relais lui-même.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use tanana_relais::{Reglages, Relais};
use tower::ServiceExt;

fn relais_avec_page() -> (axum::Router, tempfile::TempDir) {
    let dossier = tempfile::tempdir().unwrap();
    let pages = dossier.path().join("pages");
    std::fs::create_dir_all(pages.join("assets")).unwrap();
    std::fs::write(pages.join("index.html"), "<!doctype html><title>Consulter mon hôtel</title>").unwrap();
    std::fs::write(pages.join("assets/page.js"), "console.log('tanana')").unwrap();
    let relais = Relais::ouvrir(&dossier.path().join("relais.db"), Reglages::default()).unwrap();
    (relais.routes_avec_pages(&pages), dossier)
}

async fn get(routes: &axum::Router, chemin: &str) -> (StatusCode, axum::http::HeaderMap, String) {
    let r = routes.clone().oneshot(Request::get(chemin).body(Body::empty()).unwrap()).await.unwrap();
    let (statut, entetes) = (r.status(), r.headers().clone());
    let corps = String::from_utf8(to_bytes(r.into_body(), 1 << 20).await.unwrap().to_vec()).unwrap();
    (statut, entetes, corps)
}

#[tokio::test]
async fn la_page_est_servie_avec_une_politique_de_securite_stricte() {
    let (routes, _d) = relais_avec_page();
    let (statut, entetes, corps) = get(&routes, "/").await;
    assert_eq!(statut, StatusCode::OK);
    assert!(corps.contains("Consulter mon hôtel"));
    let csp = entetes[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    for regle in ["default-src 'self'", "script-src 'self'", "connect-src 'self'", "frame-ancestors 'none'"] {
        assert!(csp.contains(regle), "{regle} absent de {csp}");
    }
    assert!(!csp.contains("unsafe-eval") && !csp.contains("script-src 'self' 'unsafe-inline'"));
    assert_eq!(entetes[header::REFERRER_POLICY], "no-referrer");
    assert_eq!(entetes[header::X_FRAME_OPTIONS], "DENY");

    let (statut, entetes, corps) = get(&routes, "/assets/page.js").await;
    assert_eq!(statut, StatusCode::OK);
    assert!(entetes[header::CONTENT_TYPE].to_str().unwrap().contains("javascript"));
    assert!(corps.contains("tanana"));
}

#[tokio::test]
async fn l_api_et_la_sante_restent_prioritaires_sur_la_page() {
    let (routes, _d) = relais_avec_page();
    assert_eq!(get(&routes, "/sante").await.2, "ok");
    // Sans jeton : refus de l'API, pas la page
    let (statut, _, corps) = get(&routes, "/v1/hotels/HK7Q-F29X/temoin").await;
    assert_eq!(statut, StatusCode::UNAUTHORIZED);
    assert!(!corps.contains("<title>"));
}

#[tokio::test]
async fn aucun_fichier_hors_du_dossier_de_la_page() {
    let (routes, _d) = relais_avec_page();
    for chemin in ["/../relais.db", "/%2e%2e/relais.db", "/assets/../../relais.db"] {
        let (_, _, corps) = get(&routes, chemin).await;
        assert!(!corps.contains("SQLite"), "{chemin} a renvoyé la base du relais");
    }
}
