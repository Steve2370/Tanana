//! Logo de l'hôtel : affiché dans l'application et imprimé sur les reçus et factures.
//!
//! Chaque image enregistrée reste dans la table `logos` (elle ne change plus) ;
//! le paramètre `logo_hotel` désigne celle en usage. Un document émis garde
//! l'empreinte de son logo : il se réimprime à l'identique.

use data_encoding::BASE64;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::audit;
use crate::auth::{definir_parametre, parametre};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{FormatLogo, Logo, NouveauLogo};
use crate::permissions::Permission;
use crate::{Tanana, pdf};

/// Taille maximale de l'image reçue (l'interface la réduit avant de l'envoyer).
pub const OCTETS_MAX: usize = 300 * 1024;
/// Plus grand côté accepté, en pixels.
pub const COTE_MAX: u32 = 2_000;

fn lire(conn: &Connection, empreinte: &str) -> Resultat<Option<Logo>> {
    Ok(conn
        .query_row(
            "SELECT format, donnees, largeur, hauteur FROM logos WHERE empreinte = ?1",
            [empreinte],
            |r| {
                let format: String = r.get(0)?;
                Ok((format, r.get::<_, String>(1)?, r.get::<_, u32>(2)?, r.get::<_, u32>(3)?))
            },
        )
        .optional()?
        .and_then(|(format, donnees, largeur, hauteur)| {
            FormatLogo::depuis_texte(&format).map(|format| Logo { format, donnees, largeur, hauteur })
        }))
}

/// Le logo en usage : (empreinte, logo).
pub(crate) fn logo_en_usage(conn: &Connection) -> Resultat<Option<(String, Logo)>> {
    let Some(empreinte) = parametre(conn, "logo_hotel")?.filter(|e| !e.is_empty()) else {
        return Ok(None);
    };
    Ok(lire(conn, &empreinte)?.map(|l| (empreinte, l)))
}

/// Les octets d'un logo enregistré (pour dessiner un document).
pub(crate) fn octets_logo(conn: &Connection, empreinte: &str) -> Resultat<Option<Vec<u8>>> {
    lire(conn, empreinte)?
        .map(|l| BASE64.decode(l.donnees.as_bytes()).map_err(|_| Erreur::Base("logo enregistré illisible".into())))
        .transpose()
}

/// Vérifie l'image reçue ; renvoie ses octets et sa taille.
fn verifier(n: &NouveauLogo) -> Resultat<(Vec<u8>, u32, u32)> {
    let illisible = || Erreur::Validation("cette image est illisible : choisissez un fichier PNG ou JPEG".into());
    // Une image de 300 Ko fait environ 400 000 caractères en base64.
    if n.donnees.len() > OCTETS_MAX * 4 / 3 + 8 {
        return Err(Erreur::Validation("cette image est trop lourde (300 Ko au plus)".into()));
    }
    let octets = BASE64.decode(n.donnees.trim().as_bytes()).map_err(|_| illisible())?;
    if octets.is_empty() {
        return Err(illisible());
    }
    if octets.len() > OCTETS_MAX {
        return Err(Erreur::Validation("cette image est trop lourde (300 Ko au plus)".into()));
    }
    let signature_attendue: &[u8] = match n.format {
        FormatLogo::Png => b"\x89PNG\r\n\x1a\n",
        FormatLogo::Jpeg => b"\xff\xd8\xff",
    };
    if !octets.starts_with(signature_attendue) {
        return Err(illisible());
    }
    let (largeur, hauteur) = pdf::verifier_image(n.format, &octets)?;
    if largeur > COTE_MAX || hauteur > COTE_MAX {
        return Err(Erreur::Validation(format!("cette image est trop grande ({COTE_MAX} pixels de côté au plus)")));
    }
    Ok((octets, largeur, hauteur))
}

impl Tanana {
    /// Le logo de l'hôtel, s'il y en a un. Lisible par tout utilisateur connecté.
    pub fn logo_hotel(&mut self, token: &str) -> Resultat<Option<Logo>> {
        self.executer(token, None, |tx, _| Ok(logo_en_usage(tx)?.map(|(_, l)| l)))
    }

    /// Change le logo (`None` : le retire). Les documents déjà émis gardent le leur.
    pub fn definir_logo(&mut self, token: &str, nouveau: Option<NouveauLogo>) -> Resultat<Option<Logo>> {
        self.executer(token, Some(Permission::ParametresGerer), |tx, a| {
            let avant = logo_en_usage(tx)?;
            let empreinte = match &nouveau {
                None => None,
                Some(n) => {
                    let (octets, largeur, hauteur) = verifier(n)?;
                    let empreinte = hex::encode(Sha256::digest(&octets));
                    tx.execute(
                        "INSERT OR IGNORE INTO logos (empreinte, format, donnees, largeur, hauteur, ajoute_le)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                        params![empreinte, n.format.texte(), BASE64.encode(&octets), largeur, hauteur, a.horodatage()],
                    )?;
                    Some(empreinte)
                }
            };
            if avant.as_ref().map(|(e, _)| e) == empreinte.as_ref() {
                return Ok(avant.map(|(_, l)| l));
            }
            definir_parametre(tx, "logo_hotel", empreinte.as_deref().unwrap_or(""))?;
            let resume = |l: &Option<(String, Logo)>| {
                l.as_ref().map(|(e, l)| json!({ "empreinte": e, "largeur": l.largeur, "hauteur": l.hauteur }))
            };
            let apres = logo_en_usage(tx)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "parametres.logo",
                Some("parametres"),
                None,
                Some(&json!({ "logo": resume(&avant) })),
                Some(&json!({ "logo": resume(&apres) })),
            )?;
            Ok(apres.map(|(_, l)| l))
        })
    }
}
