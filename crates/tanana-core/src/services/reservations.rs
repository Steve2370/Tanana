use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::json;

use crate::audit::{self, json as en_json};
use crate::domaine::{
    calculer_taxes, prix_sejour, texte_facultatif, total_encaisse, transition_valide, valider_sejour,
};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{
    Chambre, ModificationReservation, NouvelleReservation, Reservation, StatutChambre, StatutReservation, Taxe,
    TypePaiement,
};
use crate::permissions::Permission;
use crate::services::chambres::chambre;
use crate::services::clients::client;
use crate::{Tanana, nouvel_id};

const SELECT_RESERVATION: &str = "SELECT r.id, r.chambre_id, c.numero, r.client_id,
        trim(cl.nom || ' ' || coalesce(cl.prenom, '')), r.date_arrivee, r.date_depart,
        r.prix_total, r.statut, r.notes, r.cree_le
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

fn lire(conn: &Connection, sql_fin: &str, p: impl rusqlite::Params) -> Resultat<Vec<Reservation>> {
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
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    bruts
        .into_iter()
        .map(
            |(id, chambre_id, chambre_numero, client_id, client_nom, arrivee, depart, prix, statut, notes, cree_le)| {
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
                })
            },
        )
        .collect()
}

pub(crate) fn reservation(conn: &Connection, id: &str) -> Resultat<Reservation> {
    lire(conn, "WHERE r.id = ?1", [id])?.pop().ok_or_else(|| Erreur::Introuvable("réservation".into()))
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
            let nuits = valider_sejour(d.date_arrivee, d.date_depart)?;
            let ch = chambre(tx, &d.chambre_id)?;
            client(tx, &d.client_id)?;
            // Vérification explicite pour un message clair ; le trigger SQL reste
            // le garde-fou final. La transaction IMMEDIATE empêche deux postes
            // d'écrire en même temps.
            verifier_disponibilite(tx, &ch, d.date_arrivee, d.date_depart, None)?;
            let prix = match d.prix_total {
                Some(p) if p < 0 => return Err(Erreur::MontantInvalide("prix total négatif".into())),
                Some(p) => p,
                None => prix_sejour(ch.prix_nuit_defaut, nuits)?,
            };
            let id = nouvel_id();
            tx.execute(
                "INSERT INTO reservations
                   (id, chambre_id, client_id, date_arrivee, date_depart, prix_total, statut, notes, cree_le, cree_par)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'confirmee', ?7, ?8, ?9)",
                params![
                    id,
                    d.chambre_id,
                    d.client_id,
                    d.date_arrivee,
                    d.date_depart,
                    prix,
                    texte_facultatif(d.notes),
                    a.horodatage(),
                    a.utilisateur_id
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
        })
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
            if r.statut == StatutReservation::Arrivee && r.solde > 0 {
                return Err(Erreur::SoldeImpaye { montant: r.solde });
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
