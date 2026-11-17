//! Rangement des reçus et factures sur l'ordinateur, puis ouverture avec le
//! lecteur PDF habituel (Aperçu sur Mac, Edge ou Acrobat sur Windows), d'où
//! la personne imprime comme d'habitude.
//!
//! Dossiers : Documents › Tanana › Reçus, et Documents › Tanana › Factures.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tanana_core::modeles::{DocumentEmis, TypeDocument};
use tanana_core::services::documents::DocumentImprime;
use tanana_core::{Erreur, Resultat};
use ts_rs::TS;

/// Ce que l'interface reçoit après une impression.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DocumentEnregistre {
    pub document: DocumentEmis,
    pub duplicata: bool,
    /// Chemin complet du fichier PDF
    pub chemin: String,
    /// Où le retrouver, en mots : « Documents › Tanana › Reçus »
    pub emplacement: String,
    /// Faux si le lecteur PDF n'a pas pu être lancé (le fichier existe quand même).
    pub ouvert: bool,
}

pub fn sous_dossier(t: TypeDocument) -> &'static str {
    match t {
        TypeDocument::Recu => "Reçus",
        TypeDocument::Facture => "Factures",
    }
}

/// Nom de fichier lisible et valable sur Windows comme sur Mac :
/// « R-2026-0001 Mbarga Aline.pdf », « … (duplicata).pdf ».
pub fn nom_fichier(d: &DocumentImprime) -> String {
    let client: String =
        d.contenu
            .client
            .nom
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                    ' '
                } else {
                    c
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
    let client: String = client.chars().take(60).collect();
    let suffixe = if d.duplicata { " (duplicata)" } else { "" };
    format!("{} {}{suffixe}.pdf", d.document.numero, client.trim_end_matches('.'))
}

/// Écrit le PDF dans `documents/Tanana/<Reçus|Factures>/` et renvoie son chemin.
/// L'écriture passe par un fichier temporaire : jamais de PDF à moitié écrit.
pub fn enregistrer(dossier_documents: &Path, d: &DocumentImprime) -> Resultat<PathBuf> {
    let dossier = dossier_documents.join("Tanana").join(sous_dossier(d.document.type_document));
    let erreur = |e: std::io::Error| Erreur::EnregistrementImpossible(format!("{} : {e}", dossier.display()));
    fs::create_dir_all(&dossier).map_err(erreur)?;
    let chemin = dossier.join(nom_fichier(d));
    let temporaire = chemin.with_extension("pdf.partiel");
    fs::write(&temporaire, &d.pdf).map_err(erreur)?;
    fs::rename(&temporaire, &chemin).map_err(erreur)?;
    Ok(chemin)
}

/// `racine` : comment s'appelle, pour la personne, le dossier où « Tanana » a été créé
/// (« Documents » d'habitude, « votre dossier personnel » s'il n'y a pas de Documents).
pub fn resultat(d: DocumentImprime, racine: &str, chemin: PathBuf, ouvert: bool) -> DocumentEnregistre {
    DocumentEnregistre {
        emplacement: format!("{racine} › Tanana › {}", sous_dossier(d.document.type_document)),
        document: d.document,
        duplicata: d.duplicata,
        chemin: chemin.display().to_string(),
        ouvert,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tanana_core::Tanana;
    use tanana_core::modeles::*;

    fn document() -> DocumentImprime {
        let mut t = Tanana::en_memoire().unwrap();
        t.initialiser(ConfigurationInitiale {
            nom_hotel: "Hôtel".into(),
            devise: "XAF".into(),
            decimales_devise: 0,
            admin_identifiant: "admin".into(),
            admin_nom: "Beltran".into(),
            admin_mot_de_passe: "admin2026".into(),
        })
        .unwrap();
        let tok = t.connexion("admin", "admin2026").unwrap().token;
        let tc = t
            .creer_type_chambre(
                &tok,
                NouveauTypeChambre { nom: "Standard".into(), prix_nuit_defaut: 25_000, capacite: 2 },
            )
            .unwrap();
        let ch = t
            .creer_chambre(
                &tok,
                DonneesChambre {
                    numero: "101".into(),
                    type_id: tc.id,
                    etage: None,
                    statut: StatutChambre::Disponible,
                    notes: None,
                },
            )
            .unwrap();
        let cl = t
            .creer_client(
                &tok,
                DonneesClient { nom: "Mbarga/Ngono".into(), prenom: Some("A: \"Aline\"".into()), ..Default::default() },
            )
            .unwrap();
        let aujourd_hui = chrono::Utc::now().date_naive();
        let r = t
            .creer_reservation(
                &tok,
                NouvelleReservation {
                    chambre_id: ch.id,
                    client_id: cl.id,
                    date_arrivee: aujourd_hui,
                    date_depart: aujourd_hui + chrono::Duration::days(1),
                    prix_total: None,
                    notes: None,
                },
            )
            .unwrap();
        t.imprimer_facture(&tok, &r.id, 60).unwrap()
    }

    #[test]
    fn nom_de_fichier_sans_caracteres_interdits() {
        let mut d = document();
        let nom = nom_fichier(&d);
        assert!(nom.ends_with(" Mbarga Ngono A Aline.pdf"), "{nom}");
        assert!(nom.starts_with("F-"), "{nom}");
        d.duplicata = true;
        assert!(nom_fichier(&d).ends_with("Aline (duplicata).pdf"));
    }

    #[test]
    fn range_le_pdf_dans_documents_tanana() {
        let dossier = tempfile::tempdir().unwrap();
        let d = document();
        let chemin = enregistrer(dossier.path(), &d).unwrap();
        assert!(chemin.starts_with(dossier.path().join("Tanana").join("Factures")));
        assert_eq!(fs::read(&chemin).unwrap(), d.pdf);
        // Rien d'autre (pas de fichier temporaire oublié)
        assert_eq!(fs::read_dir(chemin.parent().unwrap()).unwrap().count(), 1);
        // Réimprimer remplace le fichier au lieu d'échouer
        enregistrer(dossier.path(), &d).unwrap();
        let r = resultat(d, "Documents", chemin, false);
        assert_eq!(r.emplacement, "Documents › Tanana › Factures");
    }

    #[test]
    fn erreur_claire_si_le_dossier_est_inaccessible() {
        let dossier = tempfile::tempdir().unwrap();
        let fichier = dossier.path().join("pas-un-dossier");
        fs::write(&fichier, b"x").unwrap();
        assert!(matches!(enregistrer(&fichier, &document()), Err(Erreur::EnregistrementImpossible(_))));
    }
}
