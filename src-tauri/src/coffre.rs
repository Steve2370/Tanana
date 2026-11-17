//! Où vit la clé de chiffrement de la base : dans le trousseau du système
//! (Trousseau macOS, Gestionnaire d'identification Windows), jamais dans un
//! fichier à côté de la base.

pub trait Coffre: Send + Sync {
    fn lire(&self) -> Result<Option<String>, String>;
    fn ecrire(&self, cle: &str) -> Result<(), String>;
}

pub struct TrousseauSysteme {
    service: String,
}

impl TrousseauSysteme {
    /// `service` = identifiant de l'app (ex. `ca.tanana.gestion`).
    pub fn new(service: &str) -> Self {
        Self { service: service.to_string() }
    }

    fn entree(&self) -> Result<keyring::Entry, String> {
        keyring::Entry::new(&self.service, "cle-base-de-donnees").map_err(|e| e.to_string())
    }
}

impl Coffre for TrousseauSysteme {
    fn lire(&self) -> Result<Option<String>, String> {
        match self.entree()?.get_password() {
            Ok(c) => Ok(Some(c)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("trousseau illisible : {e}")),
        }
    }

    fn ecrire(&self, cle: &str) -> Result<(), String> {
        self.entree()?.set_password(cle).map_err(|e| format!("trousseau : écriture impossible : {e}"))
    }
}

/// Coffre en mémoire, pour les tests.
#[cfg(test)]
#[derive(Default)]
pub struct CoffreMemoire {
    pub contenu: std::sync::Mutex<Option<String>>,
    pub refuser_ecriture: bool,
}

#[cfg(test)]
impl Coffre for CoffreMemoire {
    fn lire(&self) -> Result<Option<String>, String> {
        Ok(self.contenu.lock().unwrap().clone())
    }
    fn ecrire(&self, cle: &str) -> Result<(), String> {
        if self.refuser_ecriture {
            return Err("refusé".into());
        }
        *self.contenu.lock().unwrap() = Some(cle.to_string());
        Ok(())
    }
}
