use rusqlite::{Connection, OptionalExtension, params};

use crate::audit::{self, json};
use crate::domaine::{courriel_plausible, texte_facultatif, texte_requis};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{Client, DonneesClient};
use crate::permissions::Permission;
use crate::{Tanana, nouvel_id};

const SELECT_CLIENT: &str = "SELECT id, nom, prenom, telephone, email, piece_identite, notes, cree_le FROM clients";

fn ligne_client(r: &rusqlite::Row) -> rusqlite::Result<Client> {
    Ok(Client {
        id: r.get(0)?,
        nom: r.get(1)?,
        prenom: r.get(2)?,
        telephone: r.get(3)?,
        email: r.get(4)?,
        piece_identite: r.get(5)?,
        notes: r.get(6)?,
        cree_le: r.get(7)?,
    })
}

pub(crate) fn client(conn: &Connection, id: &str) -> Resultat<Client> {
    conn.query_row(&format!("{SELECT_CLIENT} WHERE id = ?1"), [id], ligne_client)
        .optional()?
        .ok_or_else(|| Erreur::Introuvable("client".into()))
}

struct ClientValide {
    nom: String,
    prenom: Option<String>,
    telephone: Option<String>,
    email: Option<String>,
    piece_identite: Option<String>,
    notes: Option<String>,
}

fn valider(d: DonneesClient) -> Resultat<ClientValide> {
    let email = texte_facultatif(d.email);
    if let Some(e) = &email
        && !courriel_plausible(e)
    {
        return Err(Erreur::Validation("courriel invalide".into()));
    }
    Ok(ClientValide {
        nom: texte_requis(&d.nom, "le nom")?,
        prenom: texte_facultatif(d.prenom),
        telephone: texte_facultatif(d.telephone),
        email,
        piece_identite: texte_facultatif(d.piece_identite),
        notes: texte_facultatif(d.notes),
    })
}

impl Tanana {
    /// Recherche par nom, prénom, téléphone ou courriel (vide = 200 derniers).
    pub fn rechercher_clients(&mut self, token: &str, recherche: &str) -> Resultat<Vec<Client>> {
        self.executer(token, Some(Permission::ClientVoir), |tx, _| {
            let motif = format!("%{}%", recherche.trim().replace(['%', '_'], ""));
            // Un numéro se retrouve quelle que soit la façon de l'écrire
            // (espaces, tirets, points, indicatif) : on compare les 8 derniers chiffres.
            let chiffres: String = recherche.chars().filter(char::is_ascii_digit).collect();
            let motif_tel = if chiffres.len() >= 6 {
                format!("%{}%", &chiffres[chiffres.len().saturating_sub(8)..])
            } else {
                String::new()
            };
            let mut stmt = tx.prepare(&format!(
                "{SELECT_CLIENT}
                 WHERE nom LIKE ?1 OR prenom LIKE ?1 OR telephone LIKE ?1 OR email LIKE ?1
                    OR (prenom || ' ' || nom) LIKE ?1 OR (nom || ' ' || prenom) LIKE ?1
                    OR (?2 <> '' AND replace(replace(replace(replace(replace(replace(
                          telephone, ' ', ''), '-', ''), '.', ''), '+', ''), '(', ''), ')', '') LIKE ?2)
                 ORDER BY nom COLLATE NOCASE, prenom COLLATE NOCASE LIMIT 200"
            ))?;
            let v = stmt.query_map([motif, motif_tel], ligne_client)?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    pub fn obtenir_client(&mut self, token: &str, id: &str) -> Resultat<Client> {
        self.executer(token, Some(Permission::ClientVoir), |tx, _| client(tx, id))
    }

    pub fn creer_client(&mut self, token: &str, d: DonneesClient) -> Resultat<Client> {
        self.executer(token, Some(Permission::ClientGerer), |tx, a| {
            let c = valider(d)?;
            let id = nouvel_id();
            tx.execute(
                "INSERT INTO clients (id, nom, prenom, telephone, email, piece_identite, notes, cree_le, modifie_le)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![id, c.nom, c.prenom, c.telephone, c.email, c.piece_identite, c.notes, a.horodatage()],
            )?;
            let cree = client(tx, &id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "client.creation",
                Some("client"),
                Some(&id),
                None,
                Some(&json(&cree)?),
            )?;
            Ok(cree)
        })
    }

    pub fn modifier_client(&mut self, token: &str, id: &str, d: DonneesClient) -> Resultat<Client> {
        self.executer(token, Some(Permission::ClientGerer), |tx, a| {
            let avant = client(tx, id)?;
            let c = valider(d)?;
            tx.execute(
                "UPDATE clients SET nom = ?2, prenom = ?3, telephone = ?4, email = ?5, piece_identite = ?6,
                        notes = ?7, modifie_le = ?8
                 WHERE id = ?1",
                params![id, c.nom, c.prenom, c.telephone, c.email, c.piece_identite, c.notes, a.horodatage()],
            )?;
            let apres = client(tx, id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "client.modification",
                Some("client"),
                Some(id),
                Some(&json(&avant)?),
                Some(&json(&apres)?),
            )?;
            Ok(apres)
        })
    }
}
