//! Taxes : celles de l'hôtel (réglées une fois) et celles de chaque séjour.

use rusqlite::{Connection, params};
use serde_json::json;

use crate::Tanana;
use crate::audit::{self, json as en_json};
use crate::domaine::{calculer_taxes, valider_taxes, verifier_taxes_comprises};
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{DetailTaxes, Reservation, StatutReservation, Taxe};
use crate::permissions::Permission;
use crate::services::reservations::{ecrire_taxes, reservation, taxes_de};

pub(crate) fn taxes_hotel_de(conn: &Connection) -> Resultat<Vec<Taxe>> {
    let mut stmt = conn.prepare("SELECT libelle, calcul, valeur, comprise FROM taxes_hotel ORDER BY ordre")?;
    let v = stmt
        .query_map([], |r| Ok(Taxe { libelle: r.get(0)?, calcul: r.get(1)?, valeur: r.get(2)?, comprise: r.get(3)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(v)
}

impl Tanana {
    /// Taxes habituelles de l'hôtel (lisibles par tout employé connecté :
    /// la création d'une réservation les affiche).
    pub fn taxes_hotel(&mut self, token: &str) -> Resultat<Vec<Taxe>> {
        self.executer(token, None, |tx, _| taxes_hotel_de(tx))
    }

    /// Remplace les taxes habituelles. Les réservations déjà faites gardent les leurs.
    pub fn definir_taxes_hotel(&mut self, token: &str, taxes: Vec<Taxe>) -> Resultat<Vec<Taxe>> {
        self.executer(token, Some(Permission::ParametresGerer), |tx, a| {
            let taxes = valider_taxes(taxes)?;
            let avant = taxes_hotel_de(tx)?;
            tx.execute("DELETE FROM taxes_hotel", [])?;
            for (ordre, t) in taxes.iter().enumerate() {
                tx.execute(
                    "INSERT INTO taxes_hotel (ordre, libelle, calcul, valeur, comprise) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![ordre as i64, t.libelle, t.calcul, t.valeur, t.comprise],
                )?;
            }
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "taxes.modification",
                Some("parametres"),
                None,
                Some(&en_json(&avant)?),
                Some(&en_json(&taxes)?),
            )?;
            Ok(taxes)
        })
    }

    /// Détail des taxes d'un séjour (montants calculés).
    pub fn taxes_reservation(&mut self, token: &str, reservation_id: &str) -> Resultat<DetailTaxes> {
        self.executer(token, Some(Permission::ReservationVoir), |tx, _| {
            let r = reservation(tx, reservation_id)?;
            Ok(calculer_taxes(r.prix_total, r.nuits, &taxes_de(tx, reservation_id)?))
        })
    }

    /// Change les taxes d'un séjour (avant la facture, par exemple). Le reste à
    /// payer suit : une taxe ajoutée au prix augmente ce que le client doit.
    pub fn modifier_taxes_reservation(
        &mut self,
        token: &str,
        reservation_id: &str,
        taxes: Vec<Taxe>,
    ) -> Resultat<Reservation> {
        self.executer(token, Some(Permission::ReservationModifier), |tx, a| {
            let avant = reservation(tx, reservation_id)?;
            if avant.statut == StatutReservation::Annulee {
                return Err(Erreur::StatutInvalide { de: "annulee".into(), vers: "taxes".into() });
            }
            let taxes = valider_taxes(taxes)?;
            verifier_taxes_comprises(avant.prix_total, avant.nuits, &taxes)?;
            let taxes_avant = taxes_de(tx, reservation_id)?;
            ecrire_taxes(tx, reservation_id, &taxes)?;
            let apres = reservation(tx, reservation_id)?;
            audit::ecrire(
                tx,
                &a.horodatage(),
                Some(&a.utilisateur_id),
                "reservation.taxes",
                Some("reservation"),
                Some(reservation_id),
                Some(&json!({ "taxes": taxes_avant, "total_a_payer": avant.total_a_payer })),
                Some(&json!({ "taxes": taxes, "total_a_payer": apres.total_a_payer })),
            )?;
            Ok(apres)
        })
    }

    /// Aperçu du calcul, pour l'écran (aucune écriture). Une seule règle de
    /// calcul dans toute l'application : celle-ci.
    pub fn calculer_taxes(&mut self, token: &str, prix: i64, nuits: i64, taxes: Vec<Taxe>) -> Resultat<DetailTaxes> {
        self.executer(token, None, |_, _| {
            if !(1..=crate::domaine::NUITS_MAX).contains(&nuits) || prix < 0 {
                return Err(Erreur::Validation("séjour invalide".into()));
            }
            Ok(calculer_taxes(prix, nuits, &valider_taxes(taxes)?))
        })
    }
}
