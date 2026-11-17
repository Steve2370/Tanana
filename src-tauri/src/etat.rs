//! État partagé de l'application et logique de démarrage.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tanana_core::{CleBase, Erreur, Resultat, Tanana};
use ts_rs::TS;

use crate::coffre::Coffre;

/// Ce que l'interface doit afficher au lancement.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StatutDemarrage {
    /// Base ouverte : écran de premier lancement ou de connexion.
    Prete,
    /// La base existe mais la clé est absente ou ne correspond pas
    /// (nouvel ordinateur, restauration) : il faut la saisir.
    CleManquante,
    /// Problème grave (disque, trousseau) : message affiché tel quel.
    Erreur,
}

#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct EtatApplication {
    pub statut: StatutDemarrage,
    pub initialise: bool,
    /// Cet ordinateur consulte un hôtel à distance (copie en lecture seule).
    pub consultation_distance: bool,
    pub nom_hotel: Option<String>,
    pub message: Option<String>,
    pub version: String,
}

enum Interne {
    Prete(Box<Tanana>),
    CleManquante,
    Erreur(String),
}

pub struct Etat {
    interne: Mutex<Interne>,
    pub chemin_base: PathBuf,
    /// Copie d'un hôtel consulté à distance (à la place de la base de l'hôtel).
    pub chemin_copie: PathBuf,
    pub dossier_sauvegardes: PathBuf,
    coffre: Box<dyn Coffre>,
}

/// Ouvre la base avec la clé du trousseau, ou en crée une au tout premier lancement.
/// La clé est rangée dans le trousseau AVANT de créer la base : si le trousseau
/// refuse, on ne crée jamais une base que personne ne pourrait relire.
fn ouvrir(chemin: &Path, chemin_copie: &Path, coffre: &dyn Coffre) -> Interne {
    // Ordinateur d'un administrateur qui consulte un hôtel à distance
    if chemin_copie.exists() {
        return match coffre.lire() {
            Err(e) => Interne::Erreur(e),
            Ok(None) => Interne::CleManquante,
            Ok(Some(hex)) => {
                match CleBase::depuis_hex(&hex).and_then(|c| Tanana::ouvrir_copie(chemin_copie, Some(c))) {
                    Ok(t) => Interne::Prete(Box::new(t)),
                    Err(Erreur::Base(m)) if m.contains("clé incorrecte") => Interne::CleManquante,
                    Err(e) => Interne::Erreur(e.to_string()),
                }
            }
        };
    }
    let resultat = match coffre.lire() {
        Err(e) => return Interne::Erreur(e),
        Ok(Some(hex)) => match CleBase::depuis_hex(&hex) {
            Ok(cle) => Tanana::ouvrir(chemin, Some(cle)),
            Err(_) => return Interne::CleManquante,
        },
        Ok(None) if chemin.exists() => return Interne::CleManquante,
        Ok(None) => {
            let cle = match CleBase::generer() {
                Ok(c) => c,
                Err(e) => return Interne::Erreur(e.to_string()),
            };
            if let Err(e) = coffre.ecrire(cle.en_hex()) {
                return Interne::Erreur(e);
            }
            Tanana::ouvrir(chemin, Some(cle))
        }
    };
    match resultat {
        Ok(t) => Interne::Prete(Box::new(t)),
        // Clé présente mais qui n'ouvre pas ce fichier
        Err(Erreur::Base(m)) if m.contains("clé incorrecte") => Interne::CleManquante,
        Err(e) => Interne::Erreur(e.to_string()),
    }
}

impl Etat {
    pub fn demarrer(chemin_base: PathBuf, dossier_sauvegardes: PathBuf, coffre: Box<dyn Coffre>) -> Self {
        let chemin_copie = chemin_base.with_file_name("copie-distance.db");
        let interne = ouvrir(&chemin_base, &chemin_copie, coffre.as_ref());
        Self { interne: Mutex::new(interne), chemin_base, chemin_copie, dossier_sauvegardes, coffre }
    }

    /// Oublie l'hôtel consulté à distance, pour le rattacher à nouveau (mot de
    /// passe oublié, clé du trousseau perdue). Rien n'est perdu : la copie ne
    /// contient que ce que le relais garde chiffré. La base d'hôtel de cet
    /// ordinateur est effacée aussi : elle n'a jamais servi (`creer_copie`
    /// refuse un hôtel installé, et la copie s'ouvre à sa place ensuite).
    pub fn oublier_copie(&self) -> Resultat<()> {
        let mut interne = self.interne.lock().map_err(|_| verrou())?;
        if !self.chemin_copie.exists() {
            return Err(Erreur::Validation("cet ordinateur ne consulte aucun hôtel à distance".into()));
        }
        // Fermer les fichiers avant de les effacer
        *interne = Interne::CleManquante;
        for chemin in [&self.chemin_copie, &self.chemin_base] {
            for suffixe in ["", "-wal", "-shm"] {
                let f = PathBuf::from(format!("{}{suffixe}", chemin.display()));
                if f.exists() {
                    std::fs::remove_file(&f)
                        .map_err(|e| Erreur::Base(format!("effacement de {} : {e}", f.display())))?;
                }
            }
        }
        *interne = ouvrir(&self.chemin_base, &self.chemin_copie, self.coffre.as_ref());
        log::info!("consultation à distance : hôtel oublié, prêt à être rattaché à nouveau");
        Ok(())
    }

    /// Transforme cet ordinateur en poste de consultation à distance : crée la
    /// copie (clé secrète déjà vérifiée auprès du relais) protégée par le mot
    /// de passe choisi. Refusé si un hôtel est déjà installé ici.
    pub fn creer_copie(
        &self,
        adresse: &str,
        hotel_id: &str,
        secret: &tanana_core::distance::chiffrement::Secret,
        mot_de_passe: &str,
    ) -> Resultat<()> {
        let mut interne = self.interne.lock().map_err(|_| verrou())?;
        match &*interne {
            Interne::Prete(t) if t.est_une_copie() => {
                return Err(Erreur::Validation("cet ordinateur consulte déjà un hôtel à distance".into()));
            }
            Interne::Prete(t) if t.est_initialise()? => {
                return Err(Erreur::DejaInitialise);
            }
            Interne::Prete(_) => {}
            _ => return Err(Erreur::NonInitialise),
        }
        tanana_core::domaine::valider_mot_de_passe(mot_de_passe)?;
        let hex = self.coffre.lire().map_err(Erreur::Base)?.ok_or(Erreur::NonInitialise)?;
        let cle = CleBase::depuis_hex(&hex)?;
        let creation = Tanana::creer_copie(&self.chemin_copie, Some(cle), adresse, hotel_id, secret)
            .and_then(|mut t| t.definir_acces_copie(mot_de_passe).map(|_| t));
        match creation {
            Ok(t) => {
                *interne = Interne::Prete(Box::new(t));
                Ok(())
            }
            Err(e) => {
                // Pas de copie à moitié faite : on efface pour pouvoir recommencer.
                for suffixe in ["", "-wal", "-shm"] {
                    let _ = std::fs::remove_file(format!("{}{suffixe}", self.chemin_copie.display()));
                }
                Err(e)
            }
        }
    }

    pub fn etat_application(&self) -> EtatApplication {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let interne = self.interne.lock().unwrap_or_else(|p| p.into_inner());
        match &*interne {
            Interne::Prete(t) if t.est_une_copie() => EtatApplication {
                statut: StatutDemarrage::Prete,
                initialise: true,
                consultation_distance: true,
                nom_hotel: t.etat_copie().ok().and_then(|e| e.nom_hotel),
                message: None,
                version,
            },
            Interne::Prete(t) => EtatApplication {
                statut: StatutDemarrage::Prete,
                initialise: t.est_initialise().unwrap_or(false),
                consultation_distance: false,
                nom_hotel: t.nom_hotel().ok().flatten(),
                message: None,
                version,
            },
            Interne::CleManquante => EtatApplication {
                statut: StatutDemarrage::CleManquante,
                initialise: true,
                consultation_distance: self.chemin_copie.exists(),
                nom_hotel: None,
                message: None,
                version,
            },
            Interne::Erreur(m) => EtatApplication {
                statut: StatutDemarrage::Erreur,
                initialise: false,
                consultation_distance: false,
                nom_hotel: None,
                message: Some(m.clone()),
                version,
            },
        }
    }

    /// Clé saisie à la main (nouvel ordinateur). Enregistrée dans le trousseau
    /// seulement si elle ouvre vraiment la base.
    pub fn fournir_cle(&self, hex: &str) -> Resultat<()> {
        let cle = CleBase::depuis_hex(hex.trim())?;
        let t = if self.chemin_copie.exists() {
            Tanana::ouvrir_copie(&self.chemin_copie, Some(cle.clone()))
        } else {
            Tanana::ouvrir(&self.chemin_base, Some(cle.clone()))
        }
        .map_err(|_| Erreur::Validation("cette clé n'ouvre pas la base de données".into()))?;
        self.coffre.ecrire(cle.en_hex()).map_err(Erreur::Base)?;
        *self.interne.lock().map_err(|_| verrou())? = Interne::Prete(Box::new(t));
        Ok(())
    }

    /// Exécute une opération sur la base ouverte.
    pub fn avec<T>(&self, f: impl FnOnce(&mut Tanana) -> Resultat<T>) -> Resultat<T> {
        let mut interne = self.interne.lock().map_err(|_| verrou())?;
        match &mut *interne {
            Interne::Prete(t) => f(t),
            Interne::CleManquante => Err(Erreur::NonInitialise),
            Interne::Erreur(m) => Err(Erreur::Base(m.clone())),
        }
    }

    /// Sauvegarde quotidienne au démarrage ; une erreur est journalisée, jamais bloquante.
    pub fn sauvegarde_du_jour(&self) -> Option<PathBuf> {
        let dossier = self.dossier_sauvegardes.clone();
        // Une copie consultée à distance se reconstruit depuis le relais : pas de sauvegarde.
        match self.avec(|t| {
            if !t.est_une_copie() && t.est_initialise()? { t.sauvegarde_automatique(&dossier, 14) } else { Ok(None) }
        }) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("sauvegarde automatique impossible : {e}");
                None
            }
        }
    }
}

// ───────────────────────────── Remettre une copie de sécurité ─────────────────────────────

/// Qui demande à remettre une copie.
pub enum Demandeur {
    /// L'administrateur connecté (mot de passe déjà vérifié) : son identifiant.
    Administrateur(String),
    /// Personne n'est connecté : ordinateur neuf (hôtel pas encore installé)
    /// ou base illisible au démarrage.
    SansSession,
}

/// Une copie de sécurité trouvée dans le dossier des copies automatiques.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct SauvegardeDisponible {
    pub chemin: String,
    pub nom: String,
    /// Dernière modification du fichier (RFC 3339)
    pub date: String,
    pub octets: i64,
}

/// Ce qui a été fait, pour le dire à la personne.
#[derive(Debug, Clone, Serialize, TS, PartialEq)]
#[ts(export)]
pub struct RestaurationFaite {
    /// Où l'état d'avant a été mis de côté (s'il y en avait un)
    pub copie_avant: Option<String>,
    /// La consultation à distance était active : elle est arrêtée, à réactiver.
    pub distance_arretee: bool,
    /// Pour l'application seulement : liaisons à effacer chez le relais.
    #[serde(skip)]
    #[ts(skip)]
    pub liaisons: Vec<(String, String, String)>,
}

fn deplacer_base(de: &Path, vers: &Path) -> std::io::Result<()> {
    std::fs::rename(de, vers)?;
    for suffixe in ["-wal", "-shm"] {
        let f = PathBuf::from(format!("{}{suffixe}", de.display()));
        if f.exists() {
            std::fs::rename(&f, format!("{}{suffixe}", vers.display()))?;
        }
    }
    Ok(())
}

impl Etat {
    fn fichier_travail(&self) -> PathBuf {
        self.chemin_base.with_file_name("restauration-en-cours.db")
    }

    /// Clé tapée par la personne (copie venant d'un autre ordinateur), sinon celle de cet ordinateur.
    fn cle_pour(&self, cle_hex: Option<&str>) -> Resultat<Option<CleBase>> {
        match cle_hex.map(str::trim).filter(|c| !c.is_empty()) {
            Some(hex) => Ok(Some(CleBase::depuis_hex(hex).map_err(|_| {
                Erreur::Validation("la clé doit compter 64 caractères (chiffres et lettres de a à f)".into())
            })?)),
            None => Ok(self.coffre.lire().map_err(Erreur::Base)?.map(|h| CleBase::depuis_hex(&h)).transpose()?),
        }
    }

    /// Un hôtel est installé et ouvert ici : il faut un administrateur pour le remplacer.
    pub fn hotel_installe(&self) -> bool {
        self.interne
            .lock()
            .map(|i| matches!(&*i, Interne::Prete(t) if !t.est_une_copie() && t.est_initialise().unwrap_or(false)))
            .unwrap_or(false)
    }

    /// Copies automatiques de cet ordinateur, la plus récente d'abord.
    pub fn sauvegardes_disponibles(&self) -> Vec<SauvegardeDisponible> {
        let Ok(entrees) = std::fs::read_dir(&self.dossier_sauvegardes) else { return vec![] };
        let mut liste: Vec<SauvegardeDisponible> = entrees
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.starts_with("tanana-") && n.ends_with(".db")))
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                let date: chrono::DateTime<chrono::Utc> = m.modified().ok()?.into();
                Some(SauvegardeDisponible {
                    chemin: e.path().display().to_string(),
                    nom: e.file_name().to_string_lossy().into_owned(),
                    date: tanana_core::horodater(date),
                    octets: m.len() as i64,
                })
            })
            .collect();
        liste.sort_by(|a, b| b.date.cmp(&a.date));
        liste
    }

    /// Vérifie une copie et dit ce qu'elle contient, sans rien changer.
    pub fn examiner_sauvegarde(
        &self,
        source: &Path,
        cle_hex: Option<&str>,
    ) -> Resultat<tanana_core::restauration::ApercuSauvegarde> {
        let cle = self.cle_pour(cle_hex)?;
        let travail = self.chemin_base.with_file_name("examen-en-cours.db");
        let prep = tanana_core::restauration::preparer(source, &travail, cle.as_ref());
        for suffixe in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffixe}", travail.display()));
        }
        Ok(prep?.apercu)
    }

    /// Remet une copie de sécurité à la place de la base. L'état d'avant est
    /// mis de côté dans le dossier des copies ; rien n'est effacé.
    pub fn restaurer(&self, source: &Path, cle_hex: Option<&str>, demandeur: Demandeur) -> Resultat<RestaurationFaite> {
        let mut interne = self.interne.lock().map_err(|_| verrou())?;
        let horodatage = chrono::Local::now().format("%Y-%m-%d-%Hh%M%S").to_string();

        // 1. Qui a le droit ?
        let installe = match &*interne {
            Interne::Prete(t) if t.est_une_copie() => {
                return Err(Erreur::Validation("cet ordinateur consulte un hôtel à distance".into()));
            }
            Interne::Prete(t) => t.est_initialise()?,
            // Clé perdue, fichier abîmé : la base actuelle est mise de côté, pas effacée
            Interne::Erreur(_) | Interne::CleManquante if self.chemin_copie.exists() => {
                return Err(Erreur::Validation("cet ordinateur consulte un hôtel à distance".into()));
            }
            Interne::Erreur(_) | Interne::CleManquante => false,
        };
        let auteur = match demandeur {
            Demandeur::Administrateur(id) => Some(id),
            Demandeur::SansSession if installe => return Err(Erreur::DejaInitialise),
            Demandeur::SansSession => None,
        };
        let illisible = matches!(&*interne, Interne::Erreur(_) | Interne::CleManquante);

        // 2. Vérifier la copie avant de toucher à quoi que ce soit
        let cle = self.cle_pour(cle_hex)?;
        let prep = tanana_core::restauration::preparer(source, &self.fichier_travail(), cle.as_ref())?;
        let annuler = |e: Erreur| {
            for suffixe in ["", "-wal", "-shm"] {
                let _ = std::fs::remove_file(format!("{}{suffixe}", prep.fichier.display()));
            }
            e
        };
        let mut liaisons: Vec<(String, String, String)> = prep.liaison_distance.clone().into_iter().collect();

        // 3. Mettre l'état actuel de côté
        std::fs::create_dir_all(&self.dossier_sauvegardes).map_err(|e| annuler(Erreur::Base(e.to_string())))?;
        let mut copie_avant = None;
        if let Interne::Prete(t) = &*interne
            && installe
        {
            let avant = self.dossier_sauvegardes.join(format!("tanana-avant-restauration-{horodatage}.db"));
            t.mettre_de_cote(&avant).map_err(annuler)?;
            liaisons.extend(t.liaison_distance_brute().map_err(annuler)?);
            copie_avant = Some(avant);
        }
        *interne = Interne::CleManquante; // la base est fermée
        if illisible && self.chemin_base.exists() {
            let garde = self.dossier_sauvegardes.join(format!("tanana-illisible-{horodatage}.db"));
            if let Err(e) = deplacer_base(&self.chemin_base, &garde) {
                *interne = ouvrir(&self.chemin_base, &self.chemin_copie, self.coffre.as_ref());
                return Err(annuler(Erreur::Base(format!("mise de côté de la base illisible : {e}"))));
            }
            copie_avant = Some(garde);
        }

        // 4. La clé de la copie devient celle de cet ordinateur
        let ancienne_cle = self.coffre.lire().map_err(Erreur::Base)?;
        let nouvelle_cle = cle.as_ref().map(|c| c.en_hex().to_string());
        if let Some(n) = &nouvelle_cle
            && Some(n) != ancienne_cle.as_ref()
            && let Err(e) = self.coffre.ecrire(n)
        {
            *interne = ouvrir(&self.chemin_base, &self.chemin_copie, self.coffre.as_ref());
            return Err(annuler(Erreur::Base(e)));
        }

        // 5. Remplacer puis rouvrir
        for suffixe in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffixe}", self.chemin_base.display()));
        }
        if let Err(e) = std::fs::rename(&prep.fichier, &self.chemin_base) {
            // Revenir exactement à l'état d'avant
            if let Some(ancienne) = &ancienne_cle {
                let _ = self.coffre.ecrire(ancienne);
            }
            if let Some(avant) = &copie_avant {
                let _ = std::fs::copy(avant, &self.chemin_base);
            }
            *interne = ouvrir(&self.chemin_base, &self.chemin_copie, self.coffre.as_ref());
            return Err(annuler(Erreur::Base(format!("remplacement de la base impossible : {e}"))));
        }
        *interne = ouvrir(&self.chemin_base, &self.chemin_copie, self.coffre.as_ref());
        let nom = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match &mut *interne {
            Interne::Prete(t) => t.noter_restauration(auteur.as_deref(), &nom)?,
            _ => return Err(Erreur::Base("la copie remise ne s'ouvre pas : relancez Tanana".into())),
        }
        log::info!("copie de sécurité remise : {nom}");

        liaisons.sort();
        liaisons.dedup();
        Ok(RestaurationFaite {
            copie_avant: copie_avant.map(|p| p.display().to_string()),
            distance_arretee: !liaisons.is_empty(),
            liaisons,
        })
    }
}

fn verrou() -> Erreur {
    Erreur::Base("état de l'application verrouillé".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coffre::CoffreMemoire;
    use tanana_core::modeles::ConfigurationInitiale;

    fn config() -> ConfigurationInitiale {
        ConfigurationInitiale {
            nom_hotel: "Hôtel Test".into(),
            devise: "XAF".into(),
            decimales_devise: 0,
            admin_identifiant: "admin".into(),
            admin_nom: "Admin".into(),
            admin_mot_de_passe: "admin2026".into(),
        }
    }

    fn demarrer(dir: &Path, coffre: CoffreMemoire) -> Etat {
        Etat::demarrer(dir.join("tanana.db"), dir.join("sauvegardes"), Box::new(coffre))
    }

    #[test]
    fn premier_lancement_cree_la_cle_puis_la_base() {
        let dir = tempfile::tempdir().unwrap();
        let etat = demarrer(dir.path(), CoffreMemoire::default());
        let e = etat.etat_application();
        assert_eq!(e.statut, StatutDemarrage::Prete);
        assert!(!e.initialise);
        assert!(etat.chemin_base.exists());
        assert!(!std::fs::read(&etat.chemin_base).unwrap().starts_with(b"SQLite format 3"), "base chiffrée");
    }

    #[test]
    fn relance_avec_la_cle_du_trousseau() {
        let dir = tempfile::tempdir().unwrap();
        let cle = {
            let coffre = CoffreMemoire::default();
            let etat = Etat::demarrer(dir.path().join("tanana.db"), dir.path().join("s"), Box::new(coffre));
            etat.avec(|t| t.initialiser(config())).unwrap();
            etat.coffre.lire().unwrap().unwrap()
        };
        let etat = demarrer(dir.path(), CoffreMemoire { contenu: Some(cle).into(), ..Default::default() });
        let e = etat.etat_application();
        assert_eq!(e.statut, StatutDemarrage::Prete);
        assert!(e.initialise);
        assert_eq!(e.nom_hotel.as_deref(), Some("Hôtel Test"));
    }

    #[test]
    fn nouvel_ordinateur_demande_la_cle() {
        let dir = tempfile::tempdir().unwrap();
        let cle = {
            let etat = demarrer(dir.path(), CoffreMemoire::default());
            etat.avec(|t| t.initialiser(config())).unwrap();
            etat.coffre.lire().unwrap().unwrap()
        };
        // Trousseau vide (autre ordinateur) : la base existe → clé demandée
        let etat = demarrer(dir.path(), CoffreMemoire::default());
        assert_eq!(etat.etat_application().statut, StatutDemarrage::CleManquante);
        assert_eq!(etat.avec(|t| t.est_initialise()), Err(Erreur::NonInitialise));

        // Mauvaise clé refusée et non enregistrée
        let autre = CleBase::generer().unwrap();
        assert!(etat.fournir_cle(autre.en_hex()).is_err());
        assert_eq!(etat.coffre.lire().unwrap(), None);
        assert!(etat.fournir_cle("pas une clé").is_err());

        // Bonne clé : base ouverte et clé mémorisée
        etat.fournir_cle(&format!("  {}  ", cle.to_uppercase())).unwrap();
        assert_eq!(etat.etat_application().statut, StatutDemarrage::Prete);
        assert_eq!(etat.coffre.lire().unwrap(), Some(cle));
    }

    #[test]
    fn cle_du_trousseau_qui_ne_correspond_pas() {
        let dir = tempfile::tempdir().unwrap();
        drop(demarrer(dir.path(), CoffreMemoire::default()));
        let mauvaise = CleBase::generer().unwrap().en_hex().to_string();
        let etat = demarrer(dir.path(), CoffreMemoire { contenu: Some(mauvaise).into(), ..Default::default() });
        assert_eq!(etat.etat_application().statut, StatutDemarrage::CleManquante);
    }

    #[test]
    fn trousseau_refuse_donc_aucune_base_creee() {
        let dir = tempfile::tempdir().unwrap();
        let etat = demarrer(dir.path(), CoffreMemoire { refuser_ecriture: true, ..Default::default() });
        let e = etat.etat_application();
        assert_eq!(e.statut, StatutDemarrage::Erreur);
        assert!(e.message.is_some());
        assert!(!etat.chemin_base.exists(), "pas de base illisible créée");
    }

    #[test]
    fn sauvegarde_du_jour_seulement_apres_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let etat = demarrer(dir.path(), CoffreMemoire::default());
        assert_eq!(etat.sauvegarde_du_jour(), None, "rien à sauvegarder avant la configuration");
        etat.avec(|t| t.initialiser(config())).unwrap();
        let f = etat.sauvegarde_du_jour().unwrap();
        assert!(f.exists());
        assert_eq!(etat.sauvegarde_du_jour(), None, "une seule par jour");
    }

    #[test]
    fn un_ordinateur_neuf_peut_devenir_poste_de_consultation() {
        use tanana_core::distance::chiffrement::Secret;
        let dir = tempfile::tempdir().unwrap();
        let coffre = CoffreMemoire::default();
        let etat = demarrer(dir.path(), coffre);
        let secret = Secret::generer().unwrap();
        // Mot de passe trop faible : rien n'est créé
        assert!(matches!(
            etat.creer_copie("https://r.test", "HK7Q-F29X", &secret, "court"),
            Err(Erreur::Validation(_))
        ));
        assert!(!etat.chemin_copie.exists());

        etat.creer_copie("https://r.test", "HK7Q-F29X", &secret, "consult2026").unwrap();
        let e = etat.etat_application();
        assert!(e.consultation_distance && e.initialise);
        assert!(etat.avec(|t| Ok(t.est_une_copie())).unwrap());
        assert!(etat.sauvegarde_du_jour().is_none(), "pas de sauvegarde d'une copie");
        assert!(etat.creer_copie("https://r.test", "HK7Q-F29X", &secret, "consult2026").is_err(), "une seule copie");

        // Au redémarrage, Tanana s'ouvre en consultation, protégé par le mot de passe
        let cle = etat.coffre.lire().unwrap();
        drop(etat);
        let etat = demarrer(dir.path(), CoffreMemoire { contenu: cle.into(), ..Default::default() });
        assert!(etat.etat_application().consultation_distance);
        assert!(etat.avec(|t| t.connexion_copie("mauvais12")).is_err());
        assert!(etat.avec(|t| t.connexion_copie("consult2026")).is_ok());
    }

    #[test]
    fn un_hotel_installe_ne_devient_pas_poste_de_consultation() {
        let dir = tempfile::tempdir().unwrap();
        let etat = demarrer(dir.path(), CoffreMemoire::default());
        etat.avec(|t| t.initialiser(config())).unwrap();
        let secret = tanana_core::distance::chiffrement::Secret::generer().unwrap();
        assert_eq!(etat.creer_copie("https://r.test", "X", &secret, "consult2026"), Err(Erreur::DejaInitialise));
        assert!(!etat.chemin_copie.exists());
    }

    #[test]
    fn un_poste_de_consultation_peut_tout_oublier_et_recommencer() {
        use tanana_core::distance::chiffrement::Secret;
        let dir = tempfile::tempdir().unwrap();
        let etat = demarrer(dir.path(), CoffreMemoire::default());
        assert!(matches!(etat.oublier_copie(), Err(Erreur::Validation(_))), "rien à oublier");
        let secret = Secret::generer().unwrap();
        etat.creer_copie("https://r.test", "HK7Q-F29X", &secret, "consult2026").unwrap();
        drop(etat);

        // Trousseau perdu : la clé est demandée… et le propriétaire ne l'a pas.
        let etat = demarrer(dir.path(), CoffreMemoire::default());
        let e = etat.etat_application();
        assert_eq!(e.statut, StatutDemarrage::CleManquante);
        assert!(e.consultation_distance);

        // Il oublie l'hôtel : retour au premier lancement, prêt à rattacher
        etat.oublier_copie().unwrap();
        assert!(!etat.chemin_copie.exists());
        let e = etat.etat_application();
        assert_eq!(e.statut, StatutDemarrage::Prete);
        assert!(!e.initialise && !e.consultation_distance);
        etat.creer_copie("https://r.test", "HK7Q-F29X", &secret, "nouveau2026").unwrap();
        assert!(etat.avec(|t| t.connexion_copie("nouveau2026")).is_ok());
    }

    // ───────────── Remettre une copie de sécurité ─────────────

    fn installer(etat: &Etat) -> (String, String) {
        etat.avec(|t| t.initialiser(config())).unwrap();
        let s = etat.avec(|t| t.connexion("admin", "admin2026")).unwrap();
        (s.token, s.utilisateur.id)
    }

    fn client(etat: &Etat, tok: &str, nom: &str) {
        use tanana_core::modeles::DonneesClient;
        etat.avec(|t| t.creer_client(tok, DonneesClient { nom: nom.into(), ..Default::default() })).unwrap();
    }

    fn clients(etat: &Etat, tok: &str) -> Vec<String> {
        etat.avec(|t| t.rechercher_clients(tok, "")).unwrap().into_iter().map(|c| c.nom).collect()
    }

    #[test]
    fn revenir_en_arriere_sur_le_meme_ordinateur() {
        let dir = tempfile::tempdir().unwrap();
        let etat = demarrer(dir.path(), CoffreMemoire::default());
        let (tok, admin) = installer(&etat);
        client(&etat, &tok, "Avant");
        let copie = etat.sauvegarde_du_jour().expect("copie du jour");
        client(&etat, &tok, "Apres");
        assert_eq!(etat.sauvegardes_disponibles().len(), 1);

        let apercu = etat.examiner_sauvegarde(&copie, None).unwrap();
        assert_eq!((apercu.nom_hotel.as_str(), apercu.clients), ("Hôtel Test", 1));
        assert!(matches!(etat.restaurer(&copie, None, Demandeur::SansSession), Err(Erreur::DejaInitialise)));

        let faite = etat.restaurer(&copie, None, Demandeur::Administrateur(admin)).unwrap();
        assert!(!faite.distance_arretee);
        // Tout le monde est déconnecté ; l'hôtel est revenu à l'état de la copie
        assert!(etat.avec(|t| t.ma_session(&tok)).is_err());
        let tok = etat.avec(|t| t.connexion("admin", "admin2026")).unwrap().token;
        assert_eq!(clients(&etat, &tok), vec!["Avant".to_string()]);
        let journal = etat.avec(|t| t.journal(&tok, Default::default())).unwrap();
        assert!(journal.iter().any(|e| e.action == "sauvegarde.restauration" && e.utilisateur_nom.is_some()));

        // L'état d'avant n'est pas perdu : il est dans le dossier des copies
        let avant = PathBuf::from(faite.copie_avant.expect("état d'avant mis de côté"));
        assert!(avant.starts_with(&etat.dossier_sauvegardes));
        assert_eq!(etat.examiner_sauvegarde(&avant, None).unwrap().clients, 2);
        assert!(!etat.chemin_base.with_file_name("restauration-en-cours.db").exists());
    }

    #[test]
    fn ordinateur_neuf_apres_une_panne() {
        let dir = tempfile::tempdir().unwrap();
        let (copie, cle) = {
            let ancien = demarrer(&dir.path().join("ancien"), CoffreMemoire::default());
            let (tok, _) = installer(&ancien);
            client(&ancien, &tok, "Rakoto");
            let copie = dir.path().join("cle-usb.db");
            ancien.avec(|t| t.sauvegarder_vers(&tok, &copie)).unwrap();
            (copie, ancien.coffre.lire().unwrap().unwrap())
        };

        let neuf = demarrer(&dir.path().join("neuf"), CoffreMemoire::default());
        assert!(!neuf.hotel_installe());
        let cle_du_neuf = neuf.coffre.lire().unwrap();

        // Sans la bonne clé : refus, rien ne change
        assert!(matches!(neuf.examiner_sauvegarde(&copie, None), Err(Erreur::Validation(_))));
        let mauvaise = CleBase::generer().unwrap().en_hex().to_string();
        assert!(neuf.restaurer(&copie, Some(&mauvaise), Demandeur::SansSession).is_err());
        assert!(neuf.restaurer(&copie, Some("pas une clé"), Demandeur::SansSession).is_err());
        assert_eq!(neuf.coffre.lire().unwrap(), cle_du_neuf);
        assert!(!neuf.etat_application().initialise);

        // Avec la clé notée sur papier
        assert_eq!(neuf.examiner_sauvegarde(&copie, Some(&cle)).unwrap().clients, 1);
        let faite = neuf.restaurer(&copie, Some(&format!(" {} ", cle.to_uppercase())), Demandeur::SansSession).unwrap();
        assert!(faite.copie_avant.is_none(), "la base vide n'est pas gardée");
        assert_eq!(neuf.coffre.lire().unwrap(), Some(cle.clone()));
        let e = neuf.etat_application();
        assert!(e.initialise && e.statut == StatutDemarrage::Prete);
        let tok = neuf.avec(|t| t.connexion("admin", "admin2026")).unwrap().token;
        assert_eq!(clients(&neuf, &tok), vec!["Rakoto".to_string()]);

        // Au redémarrage, la clé du trousseau ouvre l'hôtel remis
        let cle_rangee = neuf.coffre.lire().unwrap();
        drop(neuf);
        let relance =
            demarrer(&dir.path().join("neuf"), CoffreMemoire { contenu: cle_rangee.into(), ..Default::default() });
        assert!(relance.etat_application().initialise);
    }

    #[test]
    fn base_abimee_mise_de_cote_puis_remplacee() {
        let dir = tempfile::tempdir().unwrap();
        let etat = demarrer(dir.path(), CoffreMemoire::default());
        let (tok, _) = installer(&etat);
        let copie = dir.path().join("copie.db");
        etat.avec(|t| t.sauvegarder_vers(&tok, &copie)).unwrap();
        let cle = etat.coffre.lire().unwrap();
        drop(etat);
        // Le disque abîme la base
        std::fs::write(dir.path().join("tanana.db"), vec![7u8; 8192]).unwrap();
        let _ = std::fs::remove_file(dir.path().join("tanana.db-wal"));
        let etat = demarrer(dir.path(), CoffreMemoire { contenu: cle.into(), ..Default::default() });
        assert_eq!(etat.etat_application().statut, StatutDemarrage::CleManquante);

        let faite = etat.restaurer(&copie, None, Demandeur::SansSession).unwrap();
        let garde = PathBuf::from(faite.copie_avant.unwrap());
        assert!(garde.file_name().unwrap().to_string_lossy().starts_with("tanana-illisible-"));
        assert_eq!(std::fs::read(&garde).unwrap(), vec![7u8; 8192], "rien n'est effacé");
        assert!(etat.etat_application().initialise);
    }
}
