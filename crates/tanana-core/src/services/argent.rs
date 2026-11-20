//! Paiements (acomptes, soldes, remboursements), dépenses et rapport de caisse.

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use crate::audit::{self, json as en_json};
use crate::domaine::{texte_facultatif, texte_requis, valider_encaissement, valider_remboursement};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{
    Depense, Encaissement, LigneMode, ModePaiement, NouvelleDepense, Paiement, PaiementDetaille, RapportCaisse,
    StatutReservation, TypePaiement,
};
use crate::permissions::Permission;
use crate::services::reservations::{compte, groupe, reservation};
use crate::{Tanana, nouvel_id};

fn lire_paiements(conn: &Connection, reservation_id: &str) -> Resultat<Vec<Paiement>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.reservation_id, p.montant, p.type, p.mode, p.date, u.nom, p.note
         FROM paiements p JOIN utilisateurs u ON u.id = p.utilisateur_id
         WHERE p.reservation_id = ?1 ORDER BY p.date, p.id",
    )?;
    let v = stmt
        .query_map([reservation_id], |r| {
            Ok(Paiement {
                id: r.get(0)?,
                reservation_id: r.get(1)?,
                montant: r.get(2)?,
                type_paiement: r.get(3)?,
                mode: r.get(4)?,
                date: r.get(5)?,
                utilisateur_nom: r.get(6)?,
                note: r.get(7)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

fn inserer_paiement(
    tx: &Connection,
    a: &crate::Acteur,
    reservation_id: &str,
    montant: i64,
    type_paiement: TypePaiement,
    mode: ModePaiement,
    note: Option<String>,
) -> Resultat<String> {
    let id = nouvel_id();
    tx.execute(
        "INSERT INTO paiements (id, reservation_id, montant, type, mode, date, utilisateur_id, note)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, reservation_id, montant, type_paiement, mode, a.horodatage(), a.utilisateur_id, note],
    )?;
    Ok(id)
}

const SELECT_DEPENSE: &str = "SELECT d.id, d.categorie, d.montant, d.description, d.date, u.nom,
        d.annulee_le IS NOT NULL, d.motif_annulation
     FROM depenses d JOIN utilisateurs u ON u.id = d.utilisateur_id";

fn ligne_depense(r: &rusqlite::Row) -> rusqlite::Result<Depense> {
    Ok(Depense {
        id: r.get(0)?,
        categorie: r.get(1)?,
        montant: r.get(2)?,
        description: r.get(3)?,
        date: r.get(4)?,
        utilisateur_nom: r.get(5)?,
        annulee: r.get(6)?,
        motif_annulation: r.get(7)?,
    })
}

fn depense(conn: &Connection, id: &str) -> Resultat<Depense> {
    conn.query_row(&format!("{SELECT_DEPENSE} WHERE d.id = ?1"), [id], ligne_depense)
        .optional()?
        .ok_or_else(|| Erreur::Introuvable("dépense".into()))
}

/// Bornes de période en texte, comparables aux horodatages RFC 3339 stockés.
fn bornes(debut: NaiveDate, fin: NaiveDate) -> Resultat<(String, String)> {
    if fin <= debut {
        return Err(Erreur::Validation("la fin de période doit être après le début".into()));
    }
    Ok((debut.format("%Y-%m-%d").to_string(), fin.format("%Y-%m-%d").to_string()))
}

/// Minuit local → instant UTC, au format des horodatages stockés.
fn bornes_instants(debut: NaiveDate, fin: NaiveDate, decalage_minutes: i32) -> Resultat<(String, String)> {
    if !(-14 * 60..=14 * 60).contains(&decalage_minutes) {
        return Err(Erreur::Validation("décalage horaire invalide".into()));
    }
    let vers_utc = |j: NaiveDate| {
        let minuit = j.and_hms_opt(0, 0, 0).expect("minuit existe toujours");
        crate::horodater((minuit - chrono::Duration::minutes(decalage_minutes as i64)).and_utc())
    };
    Ok((vers_utc(debut), vers_utc(fin)))
}

impl Tanana {
    /// Paiements d'une réservation ; pour une chambre d'un groupe, ceux de tout le groupe.
    pub fn paiements_reservation(&mut self, token: &str, reservation_id: &str) -> Resultat<Vec<Paiement>> {
        self.executer(token, Some(Permission::ReservationVoir), |tx, _| {
            let r = reservation(tx, reservation_id)?;
            let Some(g) = &r.groupe_id else {
                return lire_paiements(tx, reservation_id);
            };
            let mut tous = Vec::new();
            for chambre in groupe(tx, g)?.reservations {
                tous.extend(lire_paiements(tx, &chambre.id)?);
            }
            tous.sort_by(|a, b| (&a.date, &a.id).cmp(&(&b.date, &b.id)));
            Ok(tous)
        })
    }

    /// Encaisse un acompte ou un solde. Jamais plus que ce qui reste dû.
    pub fn encaisser(&mut self, token: &str, e: Encaissement) -> Resultat<Paiement> {
        self.executer(token, Some(Permission::PaiementEncaisser), |tx, a| {
            if e.type_paiement == TypePaiement::Remboursement {
                return Err(Erreur::Validation("utilisez la commande de remboursement".into()));
            }
            let r = reservation(tx, &e.reservation_id)?;
            if r.statut == StatutReservation::Annulee {
                return Err(Erreur::StatutInvalide { de: "annulee".into(), vers: "encaissement".into() });
            }
            // Dans un groupe, le paiement va au compte du groupe entier.
            let (reste, _) = compte(tx, &r)?;
            valider_encaissement(e.montant, reste)?;
            let id = inserer_paiement(tx, a, &r.id, e.montant, e.type_paiement, e.mode, texte_facultatif(e.note))?;
            let p = lire_paiements(tx, &r.id)?
                .into_iter()
                .find(|p| p.id == id)
                .ok_or(Erreur::Introuvable("paiement".into()))?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "paiement.encaissement",
                Some("reservation"),
                Some(&r.id),
                Some(&json!({ "solde": reste, "groupe": r.groupe_id })),
                Some(&json!({ "paiement": en_json(&p)?, "solde": reste - e.montant, "groupe": r.groupe_id })),
            )?;
            Ok(p)
        })
    }

    /// Remboursement : écriture inverse, plafonnée au total réellement encaissé.
    pub fn rembourser(
        &mut self,
        token: &str,
        reservation_id: &str,
        montant: i64,
        mode: ModePaiement,
        motif: &str,
    ) -> Resultat<Paiement> {
        self.executer(token, Some(Permission::PaiementRembourser), |tx, a| {
            let motif = texte_requis(motif, "le motif du remboursement")?;
            let r = reservation(tx, reservation_id)?;
            // Dans un groupe : jusqu'à ce que le groupe entier a payé.
            let (reste_avant, encaisse) = compte(tx, &r)?;
            valider_remboursement(montant, encaisse)?;
            let id = inserer_paiement(
                tx,
                a,
                reservation_id,
                montant,
                TypePaiement::Remboursement,
                mode,
                Some(motif.clone()),
            )?;
            let p = lire_paiements(tx, reservation_id)?
                .into_iter()
                .find(|p| p.id == id)
                .ok_or(Erreur::Introuvable("paiement".into()))?;
            let nouveau_solde = reste_avant + montant;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "paiement.remboursement",
                Some("reservation"),
                Some(reservation_id),
                Some(&json!({ "encaisse": encaisse, "solde": reste_avant, "groupe": r.groupe_id })),
                Some(
                    &json!({ "paiement": en_json(&p)?, "motif": motif, "solde": nouveau_solde, "groupe": r.groupe_id }),
                ),
            )?;
            Ok(p)
        })
    }

    // ───────────────────────────── Dépenses ─────────────────────────────

    pub fn saisir_depense(&mut self, token: &str, d: NouvelleDepense) -> Resultat<Depense> {
        self.executer(token, Some(Permission::DepenseSaisir), |tx, a| {
            if d.montant <= 0 {
                return Err(Erreur::MontantInvalide("le montant doit être positif".into()));
            }
            let categorie = texte_requis(&d.categorie, "la catégorie")?;
            let description = texte_facultatif(d.description);
            let id = nouvel_id();
            tx.execute(
                "INSERT INTO depenses (id, categorie, montant, description, date, utilisateur_id, cree_le)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![id, categorie, d.montant, description, d.date, a.utilisateur_id, a.horodatage()],
            )?;
            let dep = depense(tx, &id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "depense.saisie",
                Some("depense"),
                Some(&id),
                None,
                Some(&en_json(&dep)?),
            )?;
            Ok(dep)
        })
    }

    /// Dépenses de la période, annulées comprises (l'écran les affiche barrées).
    pub fn lister_depenses(&mut self, token: &str, debut: NaiveDate, fin: NaiveDate) -> Resultat<Vec<Depense>> {
        self.executer(token, Some(Permission::DepenseVoir), |tx, _| {
            let (debut, fin) = bornes(debut, fin)?;
            let mut stmt = tx.prepare(&format!(
                "{SELECT_DEPENSE} WHERE d.date >= ?1 AND d.date < ?2 ORDER BY d.date DESC, d.id DESC"
            ))?;
            let v = stmt.query_map(params![debut, fin], ligne_depense)?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    /// Annule une dépense saisie par erreur. Elle n'est pas effacée : elle reste
    /// visible avec la raison, mais ne compte plus dans les totaux.
    pub fn annuler_depense(&mut self, token: &str, id: &str, motif: &str) -> Resultat<Depense> {
        self.executer(token, Some(Permission::DepenseSaisir), |tx, a| {
            let motif = texte_requis(motif, "la raison de l'annulation")?;
            let avant = depense(tx, id)?;
            if avant.annulee {
                return Err(Erreur::Validation("cette dépense est déjà annulée".into()));
            }
            tx.execute(
                "UPDATE depenses SET annulee_le = ?2, annulee_par = ?3, motif_annulation = ?4 WHERE id = ?1",
                params![id, a.horodatage(), a.utilisateur_id, motif],
            )?;
            let apres = depense(tx, id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "depense.annulation",
                Some("depense"),
                Some(id),
                Some(&en_json(&avant)?),
                Some(&en_json(&apres)?),
            )?;
            Ok(apres)
        })
    }

    // ───────────────────────── Détail des paiements ─────────────────────────

    /// Tous les paiements (encaissements et remboursements) des jours locaux
    /// `[debut, fin)`, du plus récent au plus ancien, avec le client et la chambre.
    pub fn lister_paiements(
        &mut self,
        token: &str,
        debut: NaiveDate,
        fin: NaiveDate,
        decalage_utc_minutes: i32,
    ) -> Resultat<Vec<PaiementDetaille>> {
        self.executer(token, Some(Permission::RapportFinancierVoir), |tx, _| {
            bornes(debut, fin)?;
            let (d, f) = bornes_instants(debut, fin, decalage_utc_minutes)?;
            let mut stmt = tx.prepare(
                "SELECT p.id, p.reservation_id, trim(cl.nom || ' ' || coalesce(cl.prenom, '')), c.numero,
                        p.montant, p.type, p.mode, p.date, u.nom, p.note
                 FROM paiements p
                 JOIN reservations r ON r.id = p.reservation_id
                 JOIN clients cl ON cl.id = r.client_id
                 JOIN chambres c ON c.id = r.chambre_id
                 JOIN utilisateurs u ON u.id = p.utilisateur_id
                 WHERE p.date >= ?1 AND p.date < ?2
                 ORDER BY p.date DESC, p.id DESC",
            )?;
            let v = stmt
                .query_map(params![d, f], |r| {
                    Ok(PaiementDetaille {
                        id: r.get(0)?,
                        reservation_id: r.get(1)?,
                        client_nom: r.get(2)?,
                        chambre_numero: r.get(3)?,
                        montant: r.get(4)?,
                        type_paiement: r.get(5)?,
                        mode: r.get(6)?,
                        date: r.get(7)?,
                        utilisateur_nom: r.get(8)?,
                        note: r.get(9)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }

    // ─────────────────────────────── Rapport ───────────────────────────────

    /// Entrées et sorties d'argent sur les jours locaux `[debut, fin)`.
    /// `decalage_utc_minutes` : décalage du fuseau de l'hôtel (ex. -300 pour UTC−5),
    /// pour qu'un paiement de 22 h compte bien dans la journée locale.
    pub fn rapport_caisse(
        &mut self,
        token: &str,
        debut: NaiveDate,
        fin: NaiveDate,
        decalage_utc_minutes: i32,
    ) -> Resultat<RapportCaisse> {
        self.executer(token, Some(Permission::RapportFinancierVoir), |tx, _| {
            let (dj, fj) = bornes(debut, fin)?;
            let (d, f) = bornes_instants(debut, fin, decalage_utc_minutes)?;
            let mut par_mode = Vec::new();
            let mut encaissements = 0i64;
            for mode in ModePaiement::TOUTES {
                let m: i64 = tx.query_row(
                    "SELECT coalesce(sum(montant), 0) FROM paiements
                     WHERE type IN ('acompte', 'solde') AND mode = ?1 AND date >= ?2 AND date < ?3",
                    params![mode, d, f],
                    |r| r.get(0),
                )?;
                if m > 0 {
                    par_mode.push(LigneMode { mode: *mode, montant: m });
                }
                encaissements += m;
            }
            let remboursements: i64 = tx.query_row(
                "SELECT coalesce(sum(montant), 0) FROM paiements
                 WHERE type = 'remboursement' AND date >= ?1 AND date < ?2",
                params![d, f],
                |r| r.get(0),
            )?;
            let depenses: i64 = tx.query_row(
                "SELECT coalesce(sum(montant), 0) FROM depenses
                 WHERE date >= ?1 AND date < ?2 AND annulee_le IS NULL",
                params![dj, fj],
                |r| r.get(0),
            )?;
            Ok(RapportCaisse {
                debut,
                fin,
                encaissements,
                encaissements_par_mode: par_mode,
                remboursements,
                depenses,
                net: encaissements - remboursements - depenses,
            })
        })
    }
}
