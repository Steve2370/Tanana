//! Paramètres de l'hôtel et sauvegardes.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::audit::{self, json as en_json};
use crate::auth::{definir_parametre, parametre, parametre_nombre};
use crate::domaine::{courriel_plausible, texte_facultatif, texte_requis};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::Parametres;
use crate::permissions::Permission;
use crate::{Tanana, db, horodater};

pub(crate) fn lire_parametres(conn: &rusqlite::Connection) -> Resultat<Parametres> {
    Ok(Parametres {
        nom_hotel: parametre(conn, "nom_hotel")?.unwrap_or_default(),
        devise: parametre(conn, "devise")?.unwrap_or_else(|| "XAF".into()),
        decimales_devise: parametre_nombre(conn, "decimales_devise", 0)?.clamp(0, 3) as u8,
        delai_inactivite_minutes: parametre_nombre(conn, "delai_inactivite_minutes", 15)?.clamp(1, 240) as u32,
        adresse_hotel: texte_facultatif(parametre(conn, "adresse_hotel")?),
        telephone_hotel: texte_facultatif(parametre(conn, "telephone_hotel")?),
        email_hotel: texte_facultatif(parametre(conn, "email_hotel")?),
        mentions_documents: texte_facultatif(parametre(conn, "mentions_documents")?),
    })
}

impl Tanana {
    /// Lisible par tout utilisateur connecté (devise et nom pour l'affichage).
    pub fn parametres(&mut self, token: &str) -> Resultat<Parametres> {
        let copie = self.copie;
        self.executer(token, None, |tx, _| {
            let mut p = lire_parametres(tx)?;
            // En consultation à distance, l'écran doit suivre le délai de la copie, pas celui de la réception.
            if copie {
                p.delai_inactivite_minutes = crate::distance::DELAI_COPIE_MINUTES as u32;
            }
            Ok(p)
        })
    }

    /// Nom de l'hôtel pour l'écran de connexion (aucune session requise).
    pub fn nom_hotel(&self) -> Resultat<Option<String>> {
        parametre(self.conn_brute(), "nom_hotel")
    }

    pub fn modifier_parametres(&mut self, token: &str, p: Parametres) -> Resultat<Parametres> {
        self.executer(token, Some(Permission::ParametresGerer), |tx, a| {
            let avant = lire_parametres(tx)?;
            let nom = texte_requis(&p.nom_hotel, "le nom de l'hôtel")?;
            let devise = p.devise.trim().to_uppercase();
            if devise.len() != 3 || !devise.chars().all(|c| c.is_ascii_alphabetic()) {
                return Err(Erreur::Validation("devise : code ISO à 3 lettres".into()));
            }
            if p.decimales_devise > 3 {
                return Err(Erreur::Validation("décimales de la devise : 0 à 3".into()));
            }
            if !(1..=240).contains(&p.delai_inactivite_minutes) {
                return Err(Erreur::Validation("délai d'inactivité : 1 à 240 minutes".into()));
            }
            definir_parametre(tx, "nom_hotel", &nom)?;
            definir_parametre(tx, "devise", &devise)?;
            definir_parametre(tx, "decimales_devise", &p.decimales_devise.to_string())?;
            definir_parametre(tx, "delai_inactivite_minutes", &p.delai_inactivite_minutes.to_string())?;
            let email = texte_facultatif(p.email_hotel);
            if let Some(e) = &email
                && !courriel_plausible(e)
            {
                return Err(Erreur::Validation("courriel de l'hôtel invalide".into()));
            }
            for (cle, valeur, max) in [
                ("adresse_hotel", texte_facultatif(p.adresse_hotel), 200),
                ("telephone_hotel", texte_facultatif(p.telephone_hotel), 60),
                ("email_hotel", email, 120),
                ("mentions_documents", texte_facultatif(p.mentions_documents), 400),
            ] {
                let v = valeur.unwrap_or_default();
                if v.chars().count() > max {
                    return Err(Erreur::Validation(format!("{cle} : {max} caractères au plus")));
                }
                definir_parametre(tx, cle, &v)?;
            }
            let apres = lire_parametres(tx)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "parametres.modification",
                Some("parametres"),
                None,
                Some(&en_json(&avant)?),
                Some(&en_json(&apres)?),
            )?;
            Ok(apres)
        })
    }

    /// Sauvegarde manuelle vers un fichier choisi (clé USB, disque externe…).
    pub fn sauvegarder_vers(&mut self, token: &str, destination: &Path) -> Resultat<()> {
        // 1. Vérifier session et permission (transaction courte)
        let auteur = self.executer(token, Some(Permission::SauvegardeFaire), |_, a| Ok(a.utilisateur_id.clone()))?;
        // 2. Copier hors transaction : l'API backup lit une base au repos
        db::sauvegarder(self.conn_brute(), destination, self.cle.as_ref())?;
        // 3. Tracer
        let quand = horodater(self.maintenant());
        let tx = self.transaction()?;
        audit::ecrire(
            &tx,
            &quand,
            Some(&auteur),
            "sauvegarde.manuelle",
            None,
            None,
            None,
            Some(&json!({ "fichier": destination.display().to_string() })),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Révèle la clé de chiffrement à un administrateur qui confirme son mot de
    /// passe. Indispensable pour ouvrir une sauvegarde sur un autre ordinateur.
    pub fn exporter_cle_base(&mut self, token: &str, mot_de_passe: &str) -> Resultat<Option<String>> {
        let cle = self.cle.as_ref().map(|c| c.en_hex().to_string());
        self.executer(token, Some(Permission::ParametresGerer), |tx, a| {
            let hash: String =
                tx.query_row("SELECT mot_de_passe_hash FROM utilisateurs WHERE id = ?1", [&a.utilisateur_id], |r| {
                    r.get(0)
                })?;
            if !crate::auth::verifier_hash(mot_de_passe, &hash) {
                return Err(Erreur::IdentifiantsInvalides);
            }
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "parametres.export_cle",
                None,
                None,
                None,
                None,
            )?;
            Ok(cle)
        })
    }

    /// Sauvegarde du jour, appelée au démarrage de l'app. Garde les `a_garder` dernières.
    pub fn sauvegarde_automatique(&mut self, dossier: &Path, a_garder: usize) -> Resultat<Option<PathBuf>> {
        let maintenant = self.maintenant();
        let cle = self.cle.clone();
        let fichier =
            db::sauvegarde_automatique(self.conn_brute(), dossier, cle.as_ref(), maintenant.date_naive(), a_garder)?;
        if let Some(f) = &fichier {
            let tx = self.transaction()?;
            audit::ecrire(
                &tx,
                &horodater(maintenant),
                None,
                "sauvegarde.automatique",
                None,
                None,
                None,
                Some(&json!({ "fichier": f.display().to_string() })),
            )?;
            tx.commit()?;
        }
        Ok(fichier)
    }
}
