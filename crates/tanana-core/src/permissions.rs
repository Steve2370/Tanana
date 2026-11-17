use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Liste fermée des permissions. Toute nouvelle permission doit aussi être
/// ajoutée par une migration (un test vérifie que les deux restent alignés).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum Permission {
    #[serde(rename = "chambre.voir")]
    ChambreVoir,
    #[serde(rename = "chambre.gerer")]
    ChambreGerer,
    #[serde(rename = "client.voir")]
    ClientVoir,
    #[serde(rename = "client.gerer")]
    ClientGerer,
    #[serde(rename = "reservation.voir")]
    ReservationVoir,
    #[serde(rename = "reservation.creer")]
    ReservationCreer,
    #[serde(rename = "reservation.modifier")]
    ReservationModifier,
    #[serde(rename = "reservation.annuler")]
    ReservationAnnuler,
    #[serde(rename = "sejour.gerer")]
    SejourGerer,
    #[serde(rename = "paiement.encaisser")]
    PaiementEncaisser,
    #[serde(rename = "paiement.rembourser")]
    PaiementRembourser,
    #[serde(rename = "depense.voir")]
    DepenseVoir,
    #[serde(rename = "depense.saisir")]
    DepenseSaisir,
    #[serde(rename = "rapport.financier.voir")]
    RapportFinancierVoir,
    #[serde(rename = "utilisateur.gerer")]
    UtilisateurGerer,
    #[serde(rename = "journal.voir")]
    JournalVoir,
    #[serde(rename = "parametres.gerer")]
    ParametresGerer,
    #[serde(rename = "sauvegarde.faire")]
    SauvegardeFaire,
}

impl Permission {
    pub const TOUTES: [Permission; 18] = [
        Permission::ChambreVoir,
        Permission::ChambreGerer,
        Permission::ClientVoir,
        Permission::ClientGerer,
        Permission::ReservationVoir,
        Permission::ReservationCreer,
        Permission::ReservationModifier,
        Permission::ReservationAnnuler,
        Permission::SejourGerer,
        Permission::PaiementEncaisser,
        Permission::PaiementRembourser,
        Permission::DepenseVoir,
        Permission::DepenseSaisir,
        Permission::RapportFinancierVoir,
        Permission::UtilisateurGerer,
        Permission::JournalVoir,
        Permission::ParametresGerer,
        Permission::SauvegardeFaire,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Permission::ChambreVoir => "chambre.voir",
            Permission::ChambreGerer => "chambre.gerer",
            Permission::ClientVoir => "client.voir",
            Permission::ClientGerer => "client.gerer",
            Permission::ReservationVoir => "reservation.voir",
            Permission::ReservationCreer => "reservation.creer",
            Permission::ReservationModifier => "reservation.modifier",
            Permission::ReservationAnnuler => "reservation.annuler",
            Permission::SejourGerer => "sejour.gerer",
            Permission::PaiementEncaisser => "paiement.encaisser",
            Permission::PaiementRembourser => "paiement.rembourser",
            Permission::DepenseVoir => "depense.voir",
            Permission::DepenseSaisir => "depense.saisir",
            Permission::RapportFinancierVoir => "rapport.financier.voir",
            Permission::UtilisateurGerer => "utilisateur.gerer",
            Permission::JournalVoir => "journal.voir",
            Permission::ParametresGerer => "parametres.gerer",
            Permission::SauvegardeFaire => "sauvegarde.faire",
        }
    }

    pub fn depuis_code(code: &str) -> Option<Self> {
        Self::TOUTES.into_iter().find(|p| p.code() == code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_uniques_et_aller_retour() {
        let codes: HashSet<_> = Permission::TOUTES.iter().map(|p| p.code()).collect();
        assert_eq!(codes.len(), Permission::TOUTES.len());
        for p in Permission::TOUTES {
            assert_eq!(Permission::depuis_code(p.code()), Some(p));
        }
        assert_eq!(Permission::depuis_code("inconnue"), None);
    }

    #[test]
    fn serde_utilise_le_code() {
        for p in Permission::TOUTES {
            let json = serde_json::to_string(&p).unwrap();
            assert_eq!(json, format!("\"{}\"", p.code()));
            assert_eq!(serde_json::from_str::<Permission>(&json).unwrap(), p);
        }
    }

    #[test]
    fn la_base_et_le_code_sont_alignes() {
        let conn = crate::db::ouvrir_en_memoire().unwrap();
        let mut stmt = conn.prepare("SELECT code FROM permissions").unwrap();
        let en_base: HashSet<String> = stmt.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
        let en_code: HashSet<String> = Permission::TOUTES.iter().map(|p| p.code().to_string()).collect();
        assert_eq!(en_base, en_code);
    }

    #[test]
    fn l_admin_a_toutes_les_permissions() {
        let conn = crate::db::ouvrir_en_memoire().unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM role_permissions WHERE role_id = 'role-admin'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n as usize, Permission::TOUTES.len());
    }
}
