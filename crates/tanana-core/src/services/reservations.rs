use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use crate::audit::{self, json as en_json};
use crate::domaine::{
    calculer_taxes, prix_sejour, texte_facultatif, total_encaisse, transition_valide, valider_sejour,
};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{
    Chambre, Groupe, ModificationReservation, NouveauGroupe, NouvelleReservation, Reservation, StatutChambre,
    StatutReservation, Taxe, TypePaiement,
};
use crate::permissions::Permission;
use crate::services::chambres::chambre;
use crate::services::clients::client;
use crate::{Tanana, nouvel_id};

const SELECT_RESERVATION: &str = "SELECT r.id, r.chambre_id, c.numero, r.client_id,
        trim(cl.nom || ' ' || coalesce(cl.prenom, '')), r.date_arrivee, r.date_depart,
        r.prix_total, r.statut, r.notes, r.cree_le, r.groupe_id
     FROM reservations r
     JOIN chambres c ON c.id = r.chambre_id
     JOIN clients cl ON cl.id = r.client_id";

pub(crate) fn paiements_de(conn: &Connection, reservation_id: &str) -> Resultat<Vec<(TypePaiement, i64)>> {
    let mut stmt = conn.prepare("SELECT type, montant FROM paiements WHERE reservation_id = ?1")?;
    let v = stmt.query_map([reservation_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// Taxes d'un séjour, dans l'ordre où elles ont été saisies.
pub(crate) fn taxes_de(conn: &Connection, reservation_id: &str) -> Resultat<Vec<Taxe>> {
    let mut stmt = conn.prepare(
        "SELECT libelle, calcul, valeur, comprise FROM taxes_reservation WHERE reservation_id = ?1 ORDER BY ordre",
    )?;
    let v = stmt
        .query_map([reservation_id], |r| {
            Ok(Taxe { libelle: r.get(0)?, calcul: r.get(1)?, valeur: r.get(2)?, comprise: r.get(3)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

/// Remplace les taxes d'un séjour.
pub(crate) fn ecrire_taxes(conn: &Connection, reservation_id: &str, taxes: &[Taxe]) -> Resultat<()> {
    conn.execute("DELETE FROM taxes_reservation WHERE reservation_id = ?1", [reservation_id])?;
    for (ordre, t) in taxes.iter().enumerate() {
        conn.execute(
            "INSERT INTO taxes_reservation (reservation_id, ordre, libelle, calcul, valeur, comprise)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![reservation_id, ordre as i64, t.libelle, t.calcul, t.valeur, t.comprise],
        )?;
    }
    Ok(())
}

/// Réservations, avec le reste à payer de leur groupe s'il y en a un.
fn lire(conn: &Connection, sql_fin: &str, p: impl rusqlite::Params) -> Resultat<Vec<Reservation>> {
    let mut v = lire_brut(conn, sql_fin, p)?;
    let mut soldes: std::collections::HashMap<String, i64> = Default::default();
    for r in &mut v {
        if let Some(g) = &r.groupe_id {
            let solde = match soldes.get(g) {
                Some(s) => *s,
                None => {
                    let s = solde_du_groupe(&lire_brut(conn, "WHERE r.groupe_id = ?1", [g])?);
                    soldes.insert(g.clone(), s);
                    s
                }
            };
            r.solde_groupe = Some(solde);
        }
    }
    Ok(v)
}

/// (à payer pour les chambres non annulées, encaissé net sur toutes les chambres)
fn totaux_du_groupe(chambres: &[Reservation]) -> (i64, i64) {
    let total = chambres.iter().filter(|r| r.statut != StatutReservation::Annulee).map(|r| r.total_a_payer).sum();
    (total, chambres.iter().map(|r| r.total_encaisse).sum())
}

fn solde_du_groupe(chambres: &[Reservation]) -> i64 {
    let (total, encaisse) = totaux_du_groupe(chambres);
    total - encaisse
}

fn lire_brut(conn: &Connection, sql_fin: &str, p: impl rusqlite::Params) -> Resultat<Vec<Reservation>> {
    let mut stmt = conn.prepare(&format!("{SELECT_RESERVATION} {sql_fin}"))?;
    let bruts = stmt
        .query_map(p, |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, NaiveDate>(5)?,
                r.get::<_, NaiveDate>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, StatutReservation>(8)?,
                r.get::<_, Option<String>>(9)?,
                r.get::<_, String>(10)?,
                r.get::<_, Option<String>>(11)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    bruts
        .into_iter()
        .map(
            |(id, chambre_id, chambre_numero, client_id, client_nom, arrivee, depart, prix, statut, notes, cree_le, groupe_id)| {
                let nuits = (depart - arrivee).num_days();
                let encaisse = total_encaisse(&paiements_de(conn, &id)?);
                let taxes = calculer_taxes(prix, nuits, &taxes_de(conn, &id)?);
                Ok(Reservation {
                    nuits,
                    taxes_ajoutees: taxes.total_ajoutees,
                    total_a_payer: taxes.total_a_payer,
                    total_encaisse: encaisse,
                    solde: taxes.total_a_payer.saturating_sub(encaisse),
                    id,
                    chambre_id,
                    chambre_numero,
                    client_id,
                    client_nom,
                    date_arrivee: arrivee,
                    date_depart: depart,
                    prix_total: prix,
                    statut,
                    notes,
                    cree_le,
                    groupe_id,
                    solde_groupe: None,
                })
            },
        )
        .collect()
}

pub(crate) fn reservation(conn: &Connection, id: &str) -> Resultat<Reservation> {
    lire(conn, "WHERE r.id = ?1", [id])?.pop().ok_or_else(|| Erreur::Introuvable("réservation".into()))
}

/// Un groupe, ses chambres et son compte.
pub(crate) fn groupe(conn: &Connection, id: &str) -> Resultat<Groupe> {
    let (nom, client_id, client_nom, cree_le) = conn
        .query_row(
            "SELECT g.nom, g.client_id, trim(cl.nom || ' ' || coalesce(cl.prenom, '')), g.cree_le
             FROM groupes g JOIN clients cl ON cl.id = g.client_id WHERE g.id = ?1",
            [id],
            |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)),
        )
        .optional()?
        .ok_or_else(|| Erreur::Introuvable("groupe".into()))?;
    let reservations = lire(conn, "WHERE r.groupe_id = ?1 ORDER BY length(c.numero), c.numero, r.cree_le", [id])?;
    let (total_a_payer, total_encaisse) = totaux_du_groupe(&reservations);
    Ok(Groupe {
        id: id.to_string(),
        nom,
        client_id,
        client_nom,
        reservations,
        total_a_payer,
        total_encaisse,
        solde: total_a_payer - total_encaisse,
        cree_le,
    })
}

/// Le compte qui compte pour une réservation : (reste à payer, encaissé net).
/// Pour une chambre d'un groupe, c'est celui du groupe entier.
pub(crate) fn compte(conn: &Connection, r: &Reservation) -> Resultat<(i64, i64)> {
    match &r.groupe_id {
        Some(g) => {
            let g = groupe(conn, g)?;
            Ok((g.solde, g.total_encaisse))
        }
        None => Ok((r.solde, r.total_encaisse)),
    }
}

/// Enregistre une chambre réservée (seule ou dans un groupe) avec les taxes habituelles.
#[allow(clippy::too_many_arguments)]
fn inserer_reservation(
    tx: &Connection,
    a: &crate::Acteur,
    chambre_id: &str,
    client_id: &str,
    arrivee: NaiveDate,
    depart: NaiveDate,
    prix_total: Option<i64>,
    notes: Option<String>,
    groupe_id: Option<&str>,
) -> Resultat<Reservation> {
    let nuits = valider_sejour(arrivee, depart)?;
    let ch = chambre(tx, chambre_id)?;
    // Vérification explicite pour un message clair ; le trigger SQL reste
    // le garde-fou final. La transaction IMMEDIATE empêche deux postes
    // d'écrire en même temps.
    verifier_disponibilite(tx, &ch, arrivee, depart, None)?;
    let prix = match prix_total {
        Some(p) if p < 0 => return Err(Erreur::MontantInvalide("prix total négatif".into())),
        Some(p) => p,
        None => prix_sejour(ch.prix_nuit_defaut, nuits)?,
    };
    let id = nouvel_id();
    tx.execute(
        "INSERT INTO reservations
           (id, chambre_id, client_id, date_arrivee, date_depart, prix_total, statut, notes, cree_le, cree_par, groupe_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'confirmee', ?7, ?8, ?9, ?10)",
        params![
            id,
            chambre_id,
            client_id,
            arrivee,
            depart,
            prix,
            texte_facultatif(notes),
            a.horodatage(),
            a.utilisateur_id,
            groupe_id
        ],
    )?;
    // Les taxes habituelles de l'hôtel s'appliquent ; elles restent modifiables ensuite.
    let taxes = crate::services::taxes::taxes_hotel_de(tx)?;
    crate::domaine::verifier_taxes_comprises(prix, nuits, &taxes)?;
    ecrire_taxes(tx, &id, &taxes)?;
    let r = reservation(tx, &id)?;
    audit::ecrire(
        tx,
        &a.horodatage(),
        Some(&a.utilisateur_id),
        "reservation.creation",
        Some("reservation"),
        Some(&id),
        None,
        Some(&en_json(&r)?),
    )?;
    Ok(r)
}

/// Résumé d'un groupe pour le journal (sans le détail de chaque chambre).
fn resume_groupe(g: &Groupe) -> serde_json::Value {
    json!({
        "nom": g.nom,
        "client": g.client_nom,
        "chambres": g.reservations.iter().filter(|r| r.statut != StatutReservation::Annulee)
            .map(|r| r.chambre_numero.clone()).collect::<Vec<_>>(),
        "total_a_payer": g.total_a_payer,
    })
}

/// Retourne l'id d'une réservation active qui chevauche la période, s'il y en a une.
fn conflit(
    conn: &Connection,
    chambre_id: &str,
    arrivee: NaiveDate,
    depart: NaiveDate,
    exclure: Option<&str>,
) -> Resultat<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT id FROM reservations
             WHERE chambre_id = ?1 AND statut <> 'annulee'
               AND date_arrivee < ?3 AND date_depart > ?2
               AND (?4 IS NULL OR id <> ?4)
             LIMIT 1",
            params![chambre_id, arrivee, depart, exclure],
            |r| r.get(0),
        )
        .optional()?)
}

fn verifier_disponibilite(
    conn: &Connection,
    ch: &Chambre,
    arrivee: NaiveDate,
    depart: NaiveDate,
    exclure: Option<&str>,
) -> Resultat<()> {
    if ch.statut != StatutChambre::Disponible {
        return Err(Erreur::ChambreIndisponible);
    }
    if let Some(id) = conflit(conn, &ch.id, arrivee, depart, exclure)? {
        return Err(Erreur::ChambreDejaReservee { reservation_en_conflit: Some(id) });
    }
    Ok(())
}

impl Tanana {
    /// Réservations actives qui touchent la période `[debut, fin)` : la base du planning.
    pub fn planning(&mut self, token: &str, debut: NaiveDate, fin: NaiveDate) -> Resultat<Vec<Reservation>> {
        self.executer(token, Some(Permission::ReservationVoir), |tx, _| {
            if fin <= debut || (fin - debut).num_days() > 366 {
                return Err(Erreur::Validation("période du planning invalide".into()));
            }
            lire(
                tx,
                "WHERE r.statut <> 'annulee' AND r.date_arrivee < ?2 AND r.date_depart > ?1
                 ORDER BY c.numero, r.date_arrivee",
                params![debut, fin],
            )
        })
    }

    pub fn obtenir_reservation(&mut self, token: &str, id: &str) -> Resultat<Reservation> {
        self.executer(token, Some(Permission::ReservationVoir), |tx, _| reservation(tx, id))
    }

    pub fn reservations_du_client(&mut self, token: &str, client_id: &str) -> Resultat<Vec<Reservation>> {
        self.executer(token, Some(Permission::ReservationVoir), |tx, _| {
            lire(tx, "WHERE r.client_id = ?1 ORDER BY r.date_arrivee DESC", [client_id])
        })
    }

    /// Chambres disponibles (statut et calendrier) pour la période demandée.
    pub fn chambres_disponibles(
        &mut self,
        token: &str,
        arrivee: NaiveDate,
        depart: NaiveDate,
    ) -> Resultat<Vec<Chambre>> {
        self.executer(token, Some(Permission::ReservationVoir), |tx, _| {
            valider_sejour(arrivee, depart)?;
            let mut stmt = tx.prepare(
                "SELECT c.id FROM chambres c
                 WHERE c.statut = 'disponible' AND NOT EXISTS (
                     SELECT 1 FROM reservations r
                     WHERE r.chambre_id = c.id AND r.statut <> 'annulee'
                       AND r.date_arrivee < ?2 AND r.date_depart > ?1)
                 ORDER BY c.etage IS NULL, c.etage, length(c.numero), c.numero",
            )?;
            let ids =
                stmt.query_map(params![arrivee, depart], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
            ids.iter().map(|id| chambre(tx, id)).collect()
        })
    }

    pub fn creer_reservation(&mut self, token: &str, d: NouvelleReservation) -> Resultat<Reservation> {
        self.executer(token, Some(Permission::ReservationCreer), |tx, a| {
            client(tx, &d.client_id)?;
            inserer_reservation(
                tx,
                a,
                &d.chambre_id,
                &d.client_id,
                d.date_arrivee,
                d.date_depart,
                d.prix_total,
                d.notes,
                None,
            )
        })
    }

    /// Plusieurs chambres pour un même client, aux mêmes dates : un seul compte, une seule facture.
    pub fn creer_groupe(&mut self, token: &str, d: NouveauGroupe) -> Resultat<Groupe> {
        self.executer(token, Some(Permission::ReservationCreer), |tx, a| {
            if d.chambres.len() < 2 {
                return Err(Erreur::Validation("un groupe compte au moins deux chambres".into()));
            }
            if d.chambres.len() > 200 {
                return Err(Erreur::Validation("un groupe compte au plus 200 chambres".into()));
            }
            let mut vues = std::collections::HashSet::new();
            if !d.chambres.iter().all(|c| vues.insert(c.chambre_id.as_str())) {
                return Err(Erreur::Validation("la même chambre est choisie deux fois".into()));
            }
            valider_sejour(d.date_arrivee, d.date_depart)?;
            client(tx, &d.client_id)?;
            let nom = texte_facultatif(d.nom);
            if nom.as_ref().is_some_and(|n| n.chars().count() > 80) {
                return Err(Erreur::Validation("nom du groupe : 80 caractères au plus".into()));
            }
            let id = nouvel_id();
            tx.execute(
                "INSERT INTO groupes (id, client_id, nom, cree_le, cree_par) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, d.client_id, nom, a.horodatage(), a.utilisateur_id],
            )?;
            for c in &d.chambres {
                inserer_reservation(
                    tx,
                    a,
                    &c.chambre_id,
                    &d.client_id,
                    d.date_arrivee,
                    d.date_depart,
                    c.prix_total,
                    d.notes.clone(),
                    Some(&id),
                )?;
            }
            let g = groupe(tx, &id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "groupe.creation",
                Some("groupe"),
                Some(&id),
                None,
                Some(&resume_groupe(&g)),
            )?;
            Ok(g)
        })
    }

    /// Ajoute une chambre à un groupe, aux dates du groupe.
    pub fn ajouter_au_groupe(
        &mut self,
        token: &str,
        groupe_id: &str,
        chambre_id: &str,
        prix_total: Option<i64>,
    ) -> Resultat<Groupe> {
        self.executer(token, Some(Permission::ReservationCreer), |tx, a| {
            let avant = groupe(tx, groupe_id)?;
            let modele = avant
                .reservations
                .iter()
                .find(|r| r.statut != StatutReservation::Annulee)
                .ok_or_else(|| Erreur::Validation("toutes les chambres de ce groupe sont annulées".into()))?;
            if avant.reservations.iter().all(|r| r.statut == StatutReservation::Partie || r.statut == StatutReservation::Annulee) {
                return Err(Erreur::Validation("ce groupe est déjà parti".into()));
            }
            inserer_reservation(
                tx,
                a,
                chambre_id,
                &avant.client_id,
                modele.date_arrivee,
                modele.date_depart,
                prix_total,
                modele.notes.clone(),
                Some(groupe_id),
            )?;
            let apres = groupe(tx, groupe_id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "groupe.ajout",
                Some("groupe"),
                Some(groupe_id),
                Some(&resume_groupe(&avant)),
                Some(&resume_groupe(&apres)),
            )?;
            Ok(apres)
        })
    }

    pub fn obtenir_groupe(&mut self, token: &str, id: &str) -> Resultat<Groupe> {
        self.executer(token, Some(Permission::ReservationVoir), |tx, _| groupe(tx, id))
    }

    /// Change la chambre, les dates ou le prix d'une réservation (action sensible).
    pub fn modifier_reservation(&mut self, token: &str, id: &str, d: ModificationReservation) -> Resultat<Reservation> {
        self.executer(token, Some(Permission::ReservationModifier), |tx, a| {
            let avant = reservation(tx, id)?;
            if !matches!(avant.statut, StatutReservation::Confirmee | StatutReservation::Arrivee) {
                return Err(Erreur::StatutInvalide {
                    de: avant.statut.texte().into(),
                    vers: "modification".into(),
                });
            }
            if avant.statut == StatutReservation::Arrivee && d.date_arrivee != avant.date_arrivee {
                return Err(Erreur::Validation("le client est déjà arrivé : la date d'arrivée est fixée".into()));
            }
            if d.prix_total < 0 {
                return Err(Erreur::MontantInvalide("prix total négatif".into()));
            }
            let nuits = valider_sejour(d.date_arrivee, d.date_depart)?;
            crate::domaine::verifier_taxes_comprises(d.prix_total, nuits, &taxes_de(tx, id)?)?;
            let ch = chambre(tx, &d.chambre_id)?;
            verifier_disponibilite(tx, &ch, d.date_arrivee, d.date_depart, Some(id))?;
            tx.execute(
                "UPDATE reservations SET chambre_id = ?2, date_arrivee = ?3, date_depart = ?4, prix_total = ?5, notes = ?6
                 WHERE id = ?1",
                params![id, d.chambre_id, d.date_arrivee, d.date_depart, d.prix_total, texte_facultatif(d.notes)],
            )?;
            let apres = reservation(tx, id)?;
            audit::ecrire(tx, &a.horodatage(), Some(&a.utilisateur_id), "reservation.modification",
                Some("reservation"), Some(id), Some(&en_json(&avant)?), Some(&en_json(&apres)?))?;
            Ok(apres)
        })
    }

    /// Annule une réservation non commencée. L'éventuel acompte n'est PAS
    /// remboursé automatiquement : c'est une décision séparée et tracée.
    pub fn annuler_reservation(&mut self, token: &str, id: &str, motif: &str) -> Resultat<Reservation> {
        self.executer(token, Some(Permission::ReservationAnnuler), |tx, a| {
            let motif = crate::domaine::texte_requis(motif, "le motif d'annulation")?;
            self::changer_statut(
                tx,
                a,
                id,
                StatutReservation::Annulee,
                "reservation.annulation",
                Some(json!({ "motif": motif })),
            )
        })
    }

    pub fn enregistrer_arrivee(&mut self, token: &str, id: &str) -> Resultat<Reservation> {
        self.executer(token, Some(Permission::SejourGerer), |tx, a| {
            let r = reservation(tx, id)?;
            if chambre(tx, &r.chambre_id)?.statut != StatutChambre::Disponible {
                return Err(Erreur::ChambreIndisponible);
            }
            if r.date_arrivee > a.maintenant.date_naive() + chrono::Duration::days(1) {
                return Err(Erreur::Validation("arrivée prévue plus tard : modifiez d'abord les dates".into()));
            }
            changer_statut(tx, a, id, StatutReservation::Arrivee, "sejour.arrivee", None)
        })
    }

    /// Départ du client : refusé tant que le solde n'est pas réglé.
    pub fn enregistrer_depart(&mut self, token: &str, id: &str) -> Resultat<Reservation> {
        self.executer(token, Some(Permission::SejourGerer), |tx, a| {
            let r = reservation(tx, id)?;
            // Dans un groupe, c'est le reste à payer du groupe entier qui compte.
            let (reste, _) = compte(tx, &r)?;
            if r.statut == StatutReservation::Arrivee && reste > 0 {
                return Err(Erreur::SoldeImpaye { montant: reste });
            }
            changer_statut(tx, a, id, StatutReservation::Partie, "sejour.depart", None)
        })
    }
}

fn changer_statut(
    tx: &Connection,
    a: &crate::Acteur,
    id: &str,
    vers: StatutReservation,
    action: &str,
    details: Option<serde_json::Value>,
) -> Resultat<Reservation> {
    let avant = reservation(tx, id)?;
    if !transition_valide(avant.statut, vers) {
        return Err(Erreur::StatutInvalide { de: avant.statut.texte().into(), vers: vers.texte().into() });
    }
    tx.execute("UPDATE reservations SET statut = ?2 WHERE id = ?1", params![id, vers])?;
    let apres = reservation(tx, id)?;
    let mut apres_json = en_json(&apres)?;
    if let (Some(obj), Some(serde_json::Value::Object(extra))) = (apres_json.as_object_mut(), details) {
        obj.extend(extra);
    }
    audit::ecrire(
        tx,
        &a.horodatage(),
        Some(&a.utilisateur_id),
        action,
        Some("reservation"),
        Some(id),
        Some(&json!({ "statut": avant.statut })),
        Some(&apres_json),
    )?;
    Ok(apres)
}
