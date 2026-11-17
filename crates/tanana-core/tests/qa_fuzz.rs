//! Fuzzing reproductible des fonctions qui lisent une saisie humaine.
//! Aucune dépendance : petit générateur déterministe, graine dans FUZZ_SEED.
//! Rejouer un échec : FUZZ_SEED=<graine> cargo test -p tanana-core --test qa_fuzz

use tanana_core::CleBase;
use tanana_core::distance::chiffrement::Secret;
use tanana_core::distance::normaliser_identifiant_hotel;
use tanana_core::domaine::{TAXES_MAX, calculer_taxes, valider_taxes, verifier_taxes_comprises};
use tanana_core::francais::montant_en_lettres;
use tanana_core::modeles::{CalculTaxe, Taxe};

const TOURS: usize = 3_000;

struct Hasard(u64);
impl Hasard {
    fn depuis_env() -> (u64, Self) {
        let graine = std::env::var("FUZZ_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(20_261_115);
        (graine, Self(graine ^ 0x9E37_79B9_7F4A_7C15))
    }
    fn suivant(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn entre(&mut self, min: i64, max: i64) -> i64 {
        min + (self.suivant() % ((max - min) as u64 + 1)) as i64
    }
    fn parmi<'a, T>(&mut self, l: &'a [T]) -> &'a T {
        &l[(self.suivant() % l.len() as u64) as usize]
    }
    /// Texte « sale » : tous les pièges d'une saisie au clavier.
    fn texte(&mut self) -> String {
        const MORCEAUX: &[&str] = &[
            "a",
            "Z",
            "0",
            "1",
            "8",
            "9",
            "f",
            "-",
            " ",
            "\n",
            "\t",
            "é",
            "É",
            "ß",
            "İ",
            "ﬁ",
            "👨‍👩‍👧",
            "\u{202E}",
            "\u{0}",
            "٣",
            "Ⅻ",
            "０",
            "<script>",
            "'",
            "\"",
            "%",
            "\\",
        ];
        let n = self.entre(0, 80);
        (0..n).map(|_| *self.parmi(MORCEAUX)).collect()
    }
    fn taxe(&mut self) -> Taxe {
        let calcul = *self.parmi(&[CalculTaxe::Pourcentage, CalculTaxe::ParNuit, CalculTaxe::ParSejour]);
        let valeur = match self.entre(0, 4) {
            0 => self.entre(-5, 5),
            1 => self.entre(1, 10_000),
            2 => self.entre(9_990, 10_010),
            3 => self.entre(999_999_999_990, 1_000_000_000_010),
            _ => self.entre(1, 100_000),
        };
        Taxe { libelle: self.texte(), calcul, valeur, comprise: self.suivant().is_multiple_of(2) }
    }
}

fn echec(graine: u64, tour: usize, quoi: String) -> ! {
    panic!("FUZZ_SEED={graine} tour {tour} : {quoi}")
}

#[test]
fn la_cle_de_base_ne_plante_jamais_et_reste_coherente() {
    let (graine, mut h) = Hasard::depuis_env();
    let vraie = "3ad442fb0269ec164acf7f89fce12e84bd2df4482866cdc6c0fef3115fcd3c4f";
    for tour in 0..TOURS {
        let saisie = h.texte();
        if let Ok(c) = CleBase::depuis_hex(&saisie) {
            let hex = c.en_hex();
            if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
                echec(graine, tour, format!("clé acceptée mal formée : {hex:?}"));
            }
        }
        // Une vraie clé recopiée avec des séparateurs et des majuscules reste la même clé.
        let mut recopiee = String::new();
        for (i, c) in vraie.chars().enumerate() {
            if i > 0 && i % 4 == 0 {
                recopiee.push_str(h.parmi(&[" ", "-", "\n", "  ", ""]));
            }
            recopiee.push(if h.suivant().is_multiple_of(2) { c.to_ascii_uppercase() } else { c });
        }
        match CleBase::depuis_hex(&recopiee) {
            Ok(c) if c.en_hex() == vraie => {}
            autre => echec(graine, tour, format!("clé recopiée {recopiee:?} mal lue : {autre:?}")),
        }
    }
}

#[test]
fn la_cle_secrete_se_relit_telle_qu_affichee() {
    let (graine, mut h) = Hasard::depuis_env();
    for tour in 0..TOURS {
        let _ = Secret::lire(&h.texte()); // jamais de panique
        let s = Secret::generer().unwrap();
        let affichee = s.afficher();
        let abimee: String = affichee
            .chars()
            .map(|c| match (c, h.entre(0, 5)) {
                ('-', 0) => ' ',
                ('O', 1) => '0',
                ('I', 1) => '1',
                ('B', 1) => '8',
                (c, 2) => c.to_ascii_lowercase(),
                (c, _) => c,
            })
            .collect();
        if Secret::lire(&abimee).ok().as_ref() != Some(&s) {
            echec(graine, tour, format!("{affichee} recopiée {abimee:?} n'est pas relue"));
        }
    }
}

#[test]
fn l_identifiant_d_hotel_est_stable() {
    let (graine, mut h) = Hasard::depuis_env();
    for tour in 0..TOURS {
        let saisie = h.texte();
        let une = normaliser_identifiant_hotel(&saisie);
        let deux = normaliser_identifiant_hotel(&une);
        if une != deux {
            echec(graine, tour, format!("pas idempotent : {saisie:?} → {une:?} → {deux:?}"));
        }
        if une.chars().any(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')) {
            echec(graine, tour, format!("caractère inattendu dans {une:?}"));
        }
    }
}

#[test]
fn le_montant_en_lettres_ne_plante_pour_aucun_montant() {
    let (graine, mut h) = Hasard::depuis_env();
    let extremes = [0, 1, -1, 99, 100, 1_000_000, i64::MAX, i64::MIN, i64::MIN + 1];
    for tour in 0..TOURS {
        let valeur = if tour < extremes.len() { extremes[tour] } else { h.suivant() as i64 >> h.entre(0, 62) };
        for devise in ["XAF", "XOF", "MGA", "EUR", "USD", "CAD", "???", ""] {
            for decimales in 0..=3u8 {
                let t = montant_en_lettres(valeur, devise, decimales);
                if t.trim().is_empty() || t.contains("  ") {
                    echec(graine, tour, format!("{valeur} {devise} ({decimales}) → {t:?}"));
                }
            }
        }
    }
}

#[test]
fn les_taxes_refusent_le_mauvais_et_calculent_juste_le_reste() {
    let (graine, mut h) = Hasard::depuis_env();
    for tour in 0..TOURS {
        let n = h.entre(0, TAXES_MAX as i64 + 2) as usize;
        let brutes: Vec<Taxe> = (0..n).map(|_| h.taxe()).collect();
        let Ok(taxes) = valider_taxes(brutes.clone()) else { continue };
        // Ce qui est accepté respecte bien chaque règle.
        if taxes.len() > TAXES_MAX {
            echec(graine, tour, "plus de 10 taxes acceptées".into());
        }
        for t in &taxes {
            let ok = !t.libelle.trim().is_empty()
                && t.libelle.chars().count() <= 60
                && t.valeur > 0
                && t.valeur <= 1_000_000_000_000
                && (t.calcul != CalculTaxe::Pourcentage || t.valeur <= 10_000);
            if !ok {
                echec(graine, tour, format!("taxe acceptée à tort : {t:?}"));
            }
        }
        let prix = h.entre(0, 50_000_000);
        let nuits = h.entre(1, 365);
        let d = calculer_taxes(prix, nuits, &taxes);
        let somme_comprises: i64 = d.lignes.iter().filter(|l| l.taxe.comprise).map(|l| l.montant).sum();
        let somme_ajoutees: i64 = d.lignes.iter().filter(|l| !l.taxe.comprise).map(|l| l.montant).sum();
        let mut defauts = vec![];
        if d.lignes.iter().any(|l| l.montant < 0) {
            defauts.push("montant négatif");
        }
        if d.total_a_payer != prix + d.total_ajoutees {
            defauts.push("total à payer ≠ prix + taxes ajoutées");
        }
        if somme_ajoutees != d.total_ajoutees {
            defauts.push("lignes ajoutées ≠ total ajouté");
        }
        if verifier_taxes_comprises(prix, nuits, &taxes).is_ok() {
            if d.hors_taxes + d.total_comprises != prix {
                defauts.push("hors taxes + taxes comprises ≠ prix");
            }
            if somme_comprises != d.total_comprises {
                defauts.push("lignes comprises ≠ total compris");
            }
        }
        if !defauts.is_empty() {
            echec(graine, tour, format!("{defauts:?} pour prix {prix}, {nuits} nuits, {taxes:?} → {d:?}"));
        }
    }
}

// ─── Limites exactes de valider_taxes (mutants survivants de cargo-mutants) ───

fn taxe(libelle: &str, calcul: CalculTaxe, valeur: i64) -> Taxe {
    Taxe { libelle: libelle.into(), calcul, valeur, comprise: false }
}

#[test]
fn valider_taxes_accepte_exactement_les_limites() {
    // 10 taxes : accepté ; 11 : refusé
    let dix: Vec<Taxe> = (0..TAXES_MAX).map(|i| taxe(&format!("T{i}"), CalculTaxe::ParSejour, 1)).collect();
    assert!(valider_taxes(dix.clone()).is_ok(), "10 taxes doivent passer");
    let mut onze = dix;
    onze.push(taxe("T10", CalculTaxe::ParSejour, 1));
    assert!(valider_taxes(onze).is_err(), "11 taxes doivent être refusées");
    // 60 caractères (accentués) : accepté ; 61 : refusé
    assert!(valider_taxes(vec![taxe(&"é".repeat(60), CalculTaxe::ParSejour, 1)]).is_ok());
    assert!(valider_taxes(vec![taxe(&"é".repeat(61), CalculTaxe::ParSejour, 1)]).is_err());
    // 100 % : accepté ; 100,01 % : refusé
    assert!(valider_taxes(vec![taxe("TVA", CalculTaxe::Pourcentage, 10_000)]).is_ok());
    assert!(valider_taxes(vec![taxe("TVA", CalculTaxe::Pourcentage, 10_001)]).is_err());
    // Montant fixe : 1 000 milliards accepté, au-delà refusé (même bien au-delà)
    assert!(valider_taxes(vec![taxe("Séjour", CalculTaxe::ParNuit, 1_000_000_000_000)]).is_ok());
    assert!(valider_taxes(vec![taxe("Séjour", CalculTaxe::ParNuit, 1_000_000_000_001)]).is_err());
    assert!(valider_taxes(vec![taxe("Séjour", CalculTaxe::ParSejour, 5_000_000_000_000)]).is_err());
}
