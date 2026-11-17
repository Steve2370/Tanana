//! Règles métier pures : aucune base de données, aucune interface.
//! C'est ici que se trouvent les calculs qui coûtent de l'argent s'ils sont faux,
//! donc tout est couvert par des tests unitaires.

use chrono::NaiveDate;

use crate::erreur::{Erreur, Resultat};
use crate::modeles::{CalculTaxe, DetailTaxes, LigneTaxe, StatutReservation, Taxe, TypePaiement};

/// Durée maximale d'un séjour (garde-fou contre les fautes de frappe d'année).
pub const NUITS_MAX: i64 = 365;

/// Deux séjours `[arrivée, départ)` se chevauchent-ils ?
/// Le jour du départ est libre : un client peut arriver le jour où un autre part.
pub fn chevauchent(a1: NaiveDate, d1: NaiveDate, a2: NaiveDate, d2: NaiveDate) -> bool {
    a1 < d2 && d1 > a2
}

/// Vérifie les dates d'un séjour et retourne le nombre de nuits.
pub fn valider_sejour(arrivee: NaiveDate, depart: NaiveDate) -> Resultat<i64> {
    let nuits = (depart - arrivee).num_days();
    if nuits < 1 {
        return Err(Erreur::Validation("la date de départ doit être après la date d'arrivée".into()));
    }
    if nuits > NUITS_MAX {
        return Err(Erreur::Validation(format!("séjour de {nuits} nuits : maximum {NUITS_MAX}")));
    }
    Ok(nuits)
}

/// Prix d'un séjour, sans risque de dépassement d'entier.
pub fn prix_sejour(prix_nuit: i64, nuits: i64) -> Resultat<i64> {
    if prix_nuit < 0 {
        return Err(Erreur::MontantInvalide("prix par nuit négatif".into()));
    }
    prix_nuit.checked_mul(nuits).ok_or_else(|| Erreur::MontantInvalide("montant trop grand".into()))
}

/// Total réellement encaissé : acomptes + soldes − remboursements.
pub fn total_encaisse(paiements: &[(TypePaiement, i64)]) -> i64 {
    paiements.iter().fold(0i64, |acc, (t, m)| match t {
        TypePaiement::Acompte | TypePaiement::Solde => acc.saturating_add(*m),
        TypePaiement::Remboursement => acc.saturating_sub(*m),
    })
}

/// Ce qu'il reste à payer (négatif = trop-perçu à rembourser).
pub fn solde_restant(prix_total: i64, paiements: &[(TypePaiement, i64)]) -> i64 {
    prix_total.saturating_sub(total_encaisse(paiements))
}

/// Valide un encaissement (acompte ou solde) avant écriture.
pub fn valider_encaissement(montant: i64, solde: i64) -> Resultat<()> {
    if montant <= 0 {
        return Err(Erreur::MontantInvalide("le montant doit être positif".into()));
    }
    if montant > solde {
        return Err(Erreur::MontantInvalide(format!("montant {montant} supérieur au solde restant {solde}")));
    }
    Ok(())
}

/// Valide un remboursement : on ne rend jamais plus que ce qui a été encaissé.
pub fn valider_remboursement(montant: i64, encaisse: i64) -> Resultat<()> {
    if montant <= 0 {
        return Err(Erreur::MontantInvalide("le montant doit être positif".into()));
    }
    if montant > encaisse {
        return Err(Erreur::MontantInvalide(format!("remboursement {montant} supérieur au total encaissé {encaisse}")));
    }
    Ok(())
}

/// Cycle de vie d'une réservation :
/// `confirmee → arrivee → partie`, et `confirmee → annulee`.
/// Un client arrivé ne peut pas être « annulé » : on enregistre son départ.
pub fn transition_valide(de: StatutReservation, vers: StatutReservation) -> bool {
    use StatutReservation::*;
    matches!((de, vers), (Confirmee, Arrivee) | (Confirmee, Annulee) | (Arrivee, Partie))
}

/// Règles de mot de passe : simples mais suffisantes pour un poste de réception.
pub fn valider_mot_de_passe(mdp: &str) -> Resultat<()> {
    if mdp.chars().count() < 8 {
        return Err(Erreur::Validation("le mot de passe doit contenir au moins 8 caractères".into()));
    }
    if !mdp.chars().any(|c| c.is_alphabetic()) || !mdp.chars().any(|c| c.is_ascii_digit()) {
        return Err(Erreur::Validation("le mot de passe doit contenir au moins une lettre et un chiffre".into()));
    }
    Ok(())
}

/// PIN de changement rapide d'utilisateur : 4 à 6 chiffres.
pub fn valider_pin(pin: &str) -> Resultat<()> {
    let ok = (4..=6).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit());
    if !ok {
        return Err(Erreur::Validation("le PIN doit contenir 4 à 6 chiffres".into()));
    }
    Ok(())
}

/// Champ texte obligatoire : retire les espaces et refuse le vide.
pub fn texte_requis(valeur: &str, champ: &str) -> Resultat<String> {
    let v = valeur.trim();
    if v.is_empty() {
        return Err(Erreur::Validation(format!("{champ} est obligatoire")));
    }
    Ok(v.to_string())
}

/// Contrôle simple : de quoi éviter une faute de frappe évidente, sans plus.
pub fn courriel_plausible(e: &str) -> bool {
    e.contains('@') && !e.starts_with('@') && !e.ends_with('@') && !e.contains(' ')
}

/// Champ texte facultatif : `Some("  ")` devient `None`.
pub fn texte_facultatif(valeur: Option<String>) -> Option<String> {
    valeur.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

// ───────────────────────────── Taxes ─────────────────────────────

pub const TAXES_MAX: usize = 10;
/// 100 % en centièmes de pourcent.
pub const CENT_POURCENT: i64 = 10_000;

/// Nettoie et vérifie une liste de taxes avant de l'enregistrer.
pub fn valider_taxes(taxes: Vec<Taxe>) -> Resultat<Vec<Taxe>> {
    if taxes.len() > TAXES_MAX {
        return Err(Erreur::Validation(format!("{TAXES_MAX} taxes au plus")));
    }
    taxes
        .into_iter()
        .map(|t| {
            let libelle = texte_requis(&t.libelle, "le nom de la taxe")?;
            if libelle.chars().count() > 60 {
                return Err(Erreur::Validation("nom de la taxe : 60 caractères au plus".into()));
            }
            if t.valeur <= 0 {
                return Err(Erreur::Validation(format!("{libelle} : le montant doit être plus grand que zéro")));
            }
            if t.calcul == CalculTaxe::Pourcentage && t.valeur > CENT_POURCENT {
                return Err(Erreur::Validation(format!("{libelle} : 100 % au plus")));
            }
            if t.valeur > 1_000_000_000_000 {
                return Err(Erreur::Validation(format!("{libelle} : montant trop grand")));
            }
            Ok(Taxe { libelle, ..t })
        })
        .collect()
}

/// Division entière arrondie au plus proche (valeurs positives).
fn arrondi(numerateur: i128, denominateur: i128) -> i128 {
    (2 * numerateur + denominateur) / (2 * denominateur)
}

fn vers_i64(v: i128) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

/// Montant fixe d'une taxe pour le séjour (par nuit × nuits, ou pour le séjour).
fn montant_fixe(t: &Taxe, nuits: i64) -> i128 {
    match t.calcul {
        CalculTaxe::ParNuit => t.valeur as i128 * nuits as i128,
        CalculTaxe::ParSejour => t.valeur as i128,
        CalculTaxe::Pourcentage => 0,
    }
}

/// Calcule les taxes d'un séjour, au franc près.
///
/// - Taxes **comprises** : elles sont déjà dans `prix`. On en retire d'abord
///   les montants fixes, puis on extrait les pourcentages :
///   hors taxes = reste ÷ (1 + somme des %).
/// - Taxes **ajoutées** : un pourcentage s'applique au prix hors taxes ;
///   un montant fixe s'ajoute tel quel (× nuits s'il est « par nuit »).
///
/// Ne échoue jamais : si les taxes comprises dépassent le prix, le hors-taxes
/// est ramené à zéro (la saisie, elle, est refusée par `verifier_taxes_comprises`).
pub fn calculer_taxes(prix: i64, nuits: i64, taxes: &[Taxe]) -> DetailTaxes {
    let prix_l = prix.max(0) as i128;
    let comprises_fixes: i128 = taxes.iter().filter(|t| t.comprise).map(|t| montant_fixe(t, nuits)).sum();
    let base_pct = (prix_l - comprises_fixes).max(0);
    let somme_pct: i128 =
        taxes.iter().filter(|t| t.comprise && t.calcul == CalculTaxe::Pourcentage).map(|t| t.valeur as i128).sum();
    let hors_taxes = arrondi(base_pct * CENT_POURCENT as i128, CENT_POURCENT as i128 + somme_pct);
    let pct_comprises_total = base_pct - hors_taxes;

    // Répartition des % compris : chacun arrondi, le dernier prend l'écart d'arrondi.
    let indices_pct: Vec<usize> = taxes
        .iter()
        .enumerate()
        .filter(|(_, t)| t.comprise && t.calcul == CalculTaxe::Pourcentage)
        .map(|(i, _)| i)
        .collect();
    let mut montants = vec![0i128; taxes.len()];
    let mut reparti = 0i128;
    for (n, &i) in indices_pct.iter().enumerate() {
        montants[i] = if n + 1 == indices_pct.len() {
            pct_comprises_total - reparti
        } else {
            arrondi(hors_taxes * taxes[i].valeur as i128, CENT_POURCENT as i128)
        };
        reparti += montants[i];
    }
    for (i, t) in taxes.iter().enumerate() {
        match (t.comprise, t.calcul) {
            (true, CalculTaxe::Pourcentage) => {}
            (false, CalculTaxe::Pourcentage) => {
                montants[i] = arrondi(hors_taxes * t.valeur as i128, CENT_POURCENT as i128);
            }
            _ => montants[i] = montant_fixe(t, nuits),
        }
    }
    let total_ajoutees: i128 = taxes.iter().zip(&montants).filter(|(t, _)| !t.comprise).map(|(_, m)| m).sum();
    let total_comprises: i128 = taxes.iter().zip(&montants).filter(|(t, _)| t.comprise).map(|(_, m)| m).sum();
    DetailTaxes {
        prix,
        nuits,
        lignes: taxes
            .iter()
            .zip(&montants)
            .map(|(t, m)| LigneTaxe { taxe: t.clone(), montant: vers_i64(*m) })
            .collect(),
        total_comprises: vers_i64(total_comprises.min(prix_l)),
        total_ajoutees: vers_i64(total_ajoutees),
        hors_taxes: vers_i64(hors_taxes),
        total_a_payer: vers_i64(prix_l + total_ajoutees),
    }
}

/// Les montants fixes « compris » ne peuvent pas dépasser le prix du séjour.
pub fn verifier_taxes_comprises(prix: i64, nuits: i64, taxes: &[Taxe]) -> Resultat<()> {
    let comprises: i128 = taxes.iter().filter(|t| t.comprise).map(|t| montant_fixe(t, nuits)).sum();
    if comprises > prix as i128 {
        return Err(Erreur::Validation(
            "les taxes comprises dans le prix dépassent le prix du séjour : vérifiez les montants, ou cochez « ajoutée au prix »"
                .into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use TypePaiement::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn chevauchement_cas_limites() {
        // Séjour de référence : du 10 au 15
        let (a, b) = (d("2026-03-10"), d("2026-03-15"));
        assert!(chevauchent(a, b, d("2026-03-12"), d("2026-03-13")), "inclus");
        assert!(chevauchent(a, b, d("2026-03-08"), d("2026-03-11")), "déborde avant");
        assert!(chevauchent(a, b, d("2026-03-14"), d("2026-03-20")), "déborde après");
        assert!(chevauchent(a, b, d("2026-03-01"), d("2026-03-30")), "englobe");
        assert!(chevauchent(a, b, a, b), "identique");
        assert!(!chevauchent(a, b, d("2026-03-15"), d("2026-03-18")), "arrive le jour du départ");
        assert!(!chevauchent(a, b, d("2026-03-05"), d("2026-03-10")), "part le jour de l'arrivée");
        assert!(!chevauchent(a, b, d("2026-04-01"), d("2026-04-02")), "sans rapport");
    }

    #[test]
    fn chevauchement_est_symetrique() {
        let cas = [
            ("2026-01-01", "2026-01-05", "2026-01-04", "2026-01-08"),
            ("2026-01-01", "2026-01-05", "2026-01-05", "2026-01-08"),
            ("2026-01-01", "2026-01-31", "2026-01-10", "2026-01-11"),
        ];
        for (a1, d1, a2, d2) in cas {
            assert_eq!(chevauchent(d(a1), d(d1), d(a2), d(d2)), chevauchent(d(a2), d(d2), d(a1), d(d1)));
        }
    }

    #[test]
    fn sejour_valide_compte_les_nuits() {
        assert_eq!(valider_sejour(d("2026-03-10"), d("2026-03-11")).unwrap(), 1);
        assert_eq!(valider_sejour(d("2026-12-30"), d("2027-01-02")).unwrap(), 3);
        // année bissextile 2028
        assert_eq!(valider_sejour(d("2028-02-28"), d("2028-03-01")).unwrap(), 2);
    }

    #[test]
    fn sejour_invalide() {
        assert!(valider_sejour(d("2026-03-10"), d("2026-03-10")).is_err());
        assert!(valider_sejour(d("2026-03-10"), d("2026-03-09")).is_err());
        assert!(valider_sejour(d("2026-01-01"), d("2027-01-02")).is_err());
        assert!(valider_sejour(d("2026-01-01"), d("2027-01-01")).is_ok());
    }

    #[test]
    fn prix_sejour_et_debordement() {
        assert_eq!(prix_sejour(25_000, 3).unwrap(), 75_000);
        assert_eq!(prix_sejour(0, 3).unwrap(), 0);
        assert!(prix_sejour(-1, 3).is_err());
        assert!(prix_sejour(i64::MAX, 2).is_err());
    }

    #[test]
    fn solde_avec_acompte_solde_et_remboursement() {
        assert_eq!(solde_restant(100_000, &[]), 100_000);
        assert_eq!(solde_restant(100_000, &[(Acompte, 30_000)]), 70_000);
        assert_eq!(solde_restant(100_000, &[(Acompte, 30_000), (Solde, 70_000)]), 0);
        assert_eq!(solde_restant(100_000, &[(Acompte, 30_000), (Solde, 70_000), (Remboursement, 10_000)]), 10_000);
        assert_eq!(total_encaisse(&[(Acompte, 30_000), (Remboursement, 30_000)]), 0);
    }

    #[test]
    fn encaissement_borne_par_le_solde() {
        assert!(valider_encaissement(5_000, 10_000).is_ok());
        assert!(valider_encaissement(10_000, 10_000).is_ok());
        assert!(valider_encaissement(10_001, 10_000).is_err());
        assert!(valider_encaissement(0, 10_000).is_err());
        assert!(valider_encaissement(-5, 10_000).is_err());
    }

    #[test]
    fn remboursement_borne_par_l_encaisse() {
        assert!(valider_remboursement(10_000, 10_000).is_ok());
        assert!(valider_remboursement(10_001, 10_000).is_err());
        assert!(valider_remboursement(0, 10_000).is_err());
    }

    #[test]
    fn transitions_de_statut() {
        use StatutReservation::*;
        let permises = [(Confirmee, Arrivee), (Confirmee, Annulee), (Arrivee, Partie)];
        for de in StatutReservation::TOUTES {
            for vers in StatutReservation::TOUTES {
                assert_eq!(transition_valide(*de, *vers), permises.contains(&(*de, *vers)), "{de:?} → {vers:?}");
            }
        }
    }

    #[test]
    fn regles_mot_de_passe_et_pin() {
        assert!(valider_mot_de_passe("abc12345").is_ok());
        assert!(valider_mot_de_passe("Hôtel2026").is_ok());
        assert!(valider_mot_de_passe("court1").is_err());
        assert!(valider_mot_de_passe("sanschiffre").is_err());
        assert!(valider_mot_de_passe("12345678").is_err());
        assert!(valider_pin("1234").is_ok());
        assert!(valider_pin("123456").is_ok());
        assert!(valider_pin("123").is_err());
        assert!(valider_pin("1234567").is_err());
        assert!(valider_pin("12a4").is_err());
    }

    #[test]
    fn textes() {
        assert_eq!(texte_requis("  Ana ", "nom").unwrap(), "Ana");
        assert!(texte_requis("   ", "nom").is_err());
        assert_eq!(texte_facultatif(Some("  ".into())), None);
        assert_eq!(texte_facultatif(Some(" x ".into())), Some("x".into()));
        assert_eq!(texte_facultatif(None), None);
    }

    // ─────────────── Taxes ───────────────

    fn taxe(libelle: &str, calcul: CalculTaxe, valeur: i64, comprise: bool) -> Taxe {
        Taxe { libelle: libelle.into(), calcul, valeur, comprise }
    }

    #[test]
    fn sans_taxe_rien_ne_change() {
        let d = calculer_taxes(50_000, 2, &[]);
        assert_eq!((d.hors_taxes, d.total_ajoutees, d.total_comprises, d.total_a_payer), (50_000, 0, 0, 50_000));
        assert!(d.lignes.is_empty());
    }

    #[test]
    fn taxe_de_sejour_par_nuit_ajoutee_madagascar() {
        // 2 000 Ar par nuit, 3 nuits
        let d = calculer_taxes(150_000, 3, &[taxe("Taxe de séjour", CalculTaxe::ParNuit, 2_000, false)]);
        assert_eq!(d.lignes[0].montant, 6_000);
        assert_eq!((d.hors_taxes, d.total_ajoutees, d.total_a_payer), (150_000, 6_000, 156_000));
    }

    #[test]
    fn tva_ajoutee_en_pourcentage_avec_decimales() {
        // 19,25 % de 50 000 = 9 625
        let d = calculer_taxes(50_000, 2, &[taxe("TVA", CalculTaxe::Pourcentage, 1925, false)]);
        assert_eq!(d.lignes[0].montant, 9_625);
        assert_eq!(d.total_a_payer, 59_625);
    }

    #[test]
    fn tva_comprise_dans_le_prix() {
        // 60 000 TTC à 20 % : HT 50 000, TVA 10 000 ; le client paie toujours 60 000
        let d = calculer_taxes(60_000, 2, &[taxe("TVA", CalculTaxe::Pourcentage, 2000, true)]);
        assert_eq!((d.hors_taxes, d.lignes[0].montant, d.total_comprises), (50_000, 10_000, 10_000));
        assert_eq!((d.total_ajoutees, d.total_a_payer), (0, 60_000));
        // Arrondi : 50 000 à 19,25 % → HT 41 929, TVA 8 071 (la somme retombe juste)
        let d = calculer_taxes(50_000, 1, &[taxe("TVA", CalculTaxe::Pourcentage, 1925, true)]);
        assert_eq!(d.hors_taxes + d.lignes[0].montant, 50_000);
        assert_eq!(d.hors_taxes, 41_929);
    }

    #[test]
    fn plusieurs_taxes_comprises_et_ajoutees_ensemble() {
        let taxes = [
            taxe("TVA", CalculTaxe::Pourcentage, 2000, true),
            taxe("Taxe communale", CalculTaxe::Pourcentage, 100, true),
            taxe("Taxe de séjour", CalculTaxe::ParNuit, 1_000, false),
            taxe("Frais de dossier", CalculTaxe::ParSejour, 500, true),
        ];
        let d = calculer_taxes(121_500, 2, &taxes);
        // 121 500 − 500 (fixe compris) = 121 000 = HT × 1,21 → HT 100 000
        assert_eq!(d.hors_taxes, 100_000);
        let montants: Vec<i64> = d.lignes.iter().map(|l| l.montant).collect();
        assert_eq!(montants, [20_000, 1_000, 2_000, 500]);
        assert_eq!((d.total_comprises, d.total_ajoutees, d.total_a_payer), (21_500, 2_000, 123_500));
        assert_eq!(d.hors_taxes + d.total_comprises, d.prix, "rien ne se perd à l'arrondi");
    }

    #[test]
    fn les_arrondis_ne_perdent_jamais_un_franc() {
        let taxes = [
            taxe("A", CalculTaxe::Pourcentage, 777, true),
            taxe("B", CalculTaxe::Pourcentage, 333, true),
            taxe("C", CalculTaxe::Pourcentage, 1925, true),
        ];
        for prix in [1, 7, 999, 12_345, 50_001, 1_234_567] {
            let d = calculer_taxes(prix, 1, &taxes);
            assert_eq!(d.hors_taxes + d.total_comprises, prix, "prix {prix}");
            assert!(d.lignes.iter().all(|l| l.montant >= 0));
        }
    }

    #[test]
    fn prix_nul_ou_taxes_comprises_trop_grandes() {
        let d = calculer_taxes(0, 2, &[taxe("Taxe de séjour", CalculTaxe::ParNuit, 2_000, false)]);
        assert_eq!(d.total_a_payer, 4_000, "chambre offerte : la taxe reste due");
        let trop = [taxe("Forfait", CalculTaxe::ParSejour, 60_000, true)];
        assert_eq!(calculer_taxes(50_000, 1, &trop).hors_taxes, 0);
        assert!(matches!(verifier_taxes_comprises(50_000, 1, &trop), Err(Erreur::Validation(_))));
        assert!(verifier_taxes_comprises(60_000, 1, &trop).is_ok());
    }

    #[test]
    fn validation_des_taxes() {
        let ok = valider_taxes(vec![taxe("  TVA ", CalculTaxe::Pourcentage, 1925, false)]).unwrap();
        assert_eq!(ok[0].libelle, "TVA");
        for mauvaise in [
            taxe(" ", CalculTaxe::ParNuit, 100, false),
            taxe("TVA", CalculTaxe::Pourcentage, 0, false),
            taxe("TVA", CalculTaxe::Pourcentage, 10_001, false),
            taxe("Taxe", CalculTaxe::ParNuit, -5, false),
            taxe(&"x".repeat(61), CalculTaxe::ParNuit, 5, false),
        ] {
            assert!(matches!(valider_taxes(vec![mauvaise]), Err(Erreur::Validation(_))));
        }
        let trop = vec![taxe("T", CalculTaxe::ParNuit, 1, false); TAXES_MAX + 1];
        assert!(valider_taxes(trop).is_err());
    }
}
