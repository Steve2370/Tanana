use rusqlite::{Connection, OptionalExtension, params};

use crate::audit::{self, json};
use crate::domaine::{texte_facultatif, texte_requis};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{Chambre, DonneesChambre, NouveauTypeChambre, TypeChambre};
use crate::permissions::Permission;
use crate::{Tanana, nouvel_id};

const SELECT_CHAMBRE: &str = "SELECT c.id, c.numero, c.type_id, t.nom, t.prix_nuit_defaut, c.etage, c.statut, c.notes
     FROM chambres c JOIN types_chambre t ON t.id = c.type_id";

fn ligne_chambre(r: &rusqlite::Row) -> rusqlite::Result<Chambre> {
    Ok(Chambre {
        id: r.get(0)?,
        numero: r.get(1)?,
        type_id: r.get(2)?,
        type_nom: r.get(3)?,
        prix_nuit_defaut: r.get(4)?,
        etage: r.get(5)?,
        statut: r.get(6)?,
        notes: r.get(7)?,
    })
}

pub(crate) fn chambre(conn: &Connection, id: &str) -> Resultat<Chambre> {
    conn.query_row(&format!("{SELECT_CHAMBRE} WHERE c.id = ?1"), [id], ligne_chambre)
        .optional()?
        .ok_or_else(|| Erreur::Introuvable("chambre".into()))
}

fn type_chambre(conn: &Connection, id: &str) -> Resultat<TypeChambre> {
    conn.query_row("SELECT id, nom, prix_nuit_defaut, capacite, actif FROM types_chambre WHERE id = ?1", [id], |r| {
        Ok(TypeChambre {
            id: r.get(0)?,
            nom: r.get(1)?,
            prix_nuit_defaut: r.get(2)?,
            capacite: r.get(3)?,
            actif: r.get(4)?,
        })
    })
    .optional()?
    .ok_or_else(|| Erreur::Introuvable("type de chambre".into()))
}

fn valider_type(d: &NouveauTypeChambre) -> Resultat<String> {
    if d.prix_nuit_defaut < 0 {
        return Err(Erreur::MontantInvalide("prix par nuit négatif".into()));
    }
    if d.capacite < 1 {
        return Err(Erreur::Validation("la capacité doit être d'au moins 1".into()));
    }
    texte_requis(&d.nom, "le nom du type")
}

fn doublon(e: rusqlite::Error, quoi: String) -> Erreur {
    match Erreur::from(e) {
        Erreur::Doublon(_) => Erreur::Doublon(quoi),
        autre => autre,
    }
}

impl Tanana {
    pub fn lister_types_chambre(&mut self, token: &str) -> Resultat<Vec<TypeChambre>> {
        self.executer(token, Some(Permission::ChambreVoir), |tx, _| {
            let mut stmt = tx.prepare("SELECT id FROM types_chambre ORDER BY nom COLLATE NOCASE")?;
            let ids = stmt.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            ids.iter().map(|id| type_chambre(tx, id)).collect()
        })
    }

    pub fn creer_type_chambre(&mut self, token: &str, d: NouveauTypeChambre) -> Resultat<TypeChambre> {
        self.executer(token, Some(Permission::ChambreGerer), |tx, a| {
            let nom = valider_type(&d)?;
            let id = nouvel_id();
            tx.execute(
                "INSERT INTO types_chambre (id, nom, prix_nuit_defaut, capacite) VALUES (?1, ?2, ?3, ?4)",
                params![id, nom, d.prix_nuit_defaut, d.capacite],
            )
            .map_err(|e| doublon(e, format!("type « {nom} »")))?;
            let t = type_chambre(tx, &id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "type_chambre.creation",
                Some("type_chambre"),
                Some(&id),
                None,
                Some(&json(&t)?),
            )?;
            Ok(t)
        })
    }

    pub fn modifier_type_chambre(
        &mut self,
        token: &str,
        id: &str,
        d: NouveauTypeChambre,
        actif: bool,
    ) -> Resultat<TypeChambre> {
        self.executer(token, Some(Permission::ChambreGerer), |tx, a| {
            let avant = type_chambre(tx, id)?;
            let nom = valider_type(&d)?;
            tx.execute(
                "UPDATE types_chambre SET nom = ?2, prix_nuit_defaut = ?3, capacite = ?4, actif = ?5 WHERE id = ?1",
                params![id, nom, d.prix_nuit_defaut, d.capacite, actif],
            )
            .map_err(|e| doublon(e, format!("type « {nom} »")))?;
            let apres = type_chambre(tx, id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "type_chambre.modification",
                Some("type_chambre"),
                Some(id),
                Some(&json(&avant)?),
                Some(&json(&apres)?),
            )?;
            Ok(apres)
        })
    }

    pub fn lister_chambres(&mut self, token: &str) -> Resultat<Vec<Chambre>> {
        self.executer(token, Some(Permission::ChambreVoir), |tx, _| {
            // Tri naturel : 2 avant 10 quand les numéros sont numériques.
            let mut stmt = tx.prepare(&format!(
                "{SELECT_CHAMBRE} ORDER BY c.etage IS NULL, c.etage, length(c.numero), c.numero COLLATE NOCASE"
            ))?;
            let v = stmt.query_map([], ligne_chambre)?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    pub fn creer_chambre(&mut self, token: &str, d: DonneesChambre) -> Resultat<Chambre> {
        self.executer(token, Some(Permission::ChambreGerer), |tx, a| {
            let numero = texte_requis(&d.numero, "le numéro")?;
            type_chambre(tx, &d.type_id)?;
            let id = nouvel_id();
            tx.execute(
                "INSERT INTO chambres (id, numero, type_id, etage, statut, notes, cree_le)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, numero, d.type_id, d.etage, d.statut, texte_facultatif(d.notes), a.horodatage()],
            )
            .map_err(|e| doublon(e, format!("chambre « {numero} »")))?;
            let c = chambre(tx, &id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "chambre.creation",
                Some("chambre"),
                Some(&id),
                None,
                Some(&json(&c)?),
            )?;
            Ok(c)
        })
    }

    pub fn modifier_chambre(&mut self, token: &str, id: &str, d: DonneesChambre) -> Resultat<Chambre> {
        self.executer(token, Some(Permission::ChambreGerer), |tx, a| {
            let avant = chambre(tx, id)?;
            let numero = texte_requis(&d.numero, "le numéro")?;
            type_chambre(tx, &d.type_id)?;
            tx.execute(
                "UPDATE chambres SET numero = ?2, type_id = ?3, etage = ?4, statut = ?5, notes = ?6 WHERE id = ?1",
                params![id, numero, d.type_id, d.etage, d.statut, texte_facultatif(d.notes)],
            )
            .map_err(|e| doublon(e, format!("chambre « {numero} »")))?;
            let apres = chambre(tx, id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "chambre.modification",
                Some("chambre"),
                Some(id),
                Some(&json(&avant)?),
                Some(&json(&apres)?),
            )?;
            Ok(apres)
        })
    }
}
