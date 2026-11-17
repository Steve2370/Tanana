//! Écriture en français pour les documents imprimés : montants, montants en
//! toutes lettres (« Arrêté le présent reçu à la somme de… »), dates et heures.
//!
//! Les montants sont des entiers dans la plus petite unité de la devise
//! (francs pour XAF, centimes pour EUR), comme partout dans Tanana.

use chrono::{Datelike, NaiveDate, Weekday};

/// Espace insécable : la police Lexend n'a pas l'espace fine insécable.
const INSECABLE: char = '\u{a0}';

/// Symbole affiché après le montant, comme dans l'interface (Intl fr-FR).
pub fn symbole_devise(code: &str) -> String {
    match code {
        "XAF" => "FCFA".into(),
        "XOF" => "F CFA".into(),
        "EUR" => "€".into(),
        "CAD" => "$CA".into(),
        "USD" => "$US".into(),
        autre => autre.into(),
    }
}

fn grouper_milliers(n: u64) -> String {
    let chiffres = n.to_string();
    let mut sortie = String::new();
    for (i, c) in chiffres.chars().enumerate() {
        if i > 0 && (chiffres.len() - i).is_multiple_of(3) {
            sortie.push(INSECABLE);
        }
        sortie.push(c);
    }
    sortie
}

/// 125050 (EUR, 2 décimales) → « 1 250,50 € » ; 50000 (XAF) → « 50 000 FCFA ».
pub fn montant(valeur: i64, devise: &str, decimales: u8) -> String {
    let signe = if valeur < 0 { "-" } else { "" };
    let absolu = valeur.unsigned_abs();
    let diviseur = 10u64.pow(decimales as u32);
    let entier = grouper_milliers(absolu / diviseur);
    let nombre = if decimales == 0 {
        entier
    } else {
        format!("{entier},{:0width$}", absolu % diviseur, width = decimales as usize)
    };
    format!("{signe}{nombre}{INSECABLE}{}", symbole_devise(devise))
}

// ───────────────────────────── En toutes lettres ─────────────────────────────

const UNITES: [&str; 17] = [
    "zéro", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix", "onze", "douze", "treize",
    "quatorze", "quinze", "seize",
];
const DIZAINES: [&str; 7] = ["", "dix", "vingt", "trente", "quarante", "cinquante", "soixante"];

/// 0 à 99, orthographe traditionnelle : « vingt et un », « soixante et onze »,
/// « quatre-vingts », « quatre-vingt-un ».
fn moins_de_cent(n: u64) -> String {
    match n {
        0..=16 => UNITES[n as usize].into(),
        17..=19 => format!("dix-{}", UNITES[(n - 10) as usize]),
        20..=69 => {
            let (d, u) = ((n / 10) as usize, n % 10);
            match u {
                0 => DIZAINES[d].into(),
                1 => format!("{} et un", DIZAINES[d]),
                _ => format!("{}-{}", DIZAINES[d], UNITES[u as usize]),
            }
        }
        70..=79 if n == 71 => "soixante et onze".into(),
        70..=79 => format!("soixante-{}", moins_de_cent(n - 60)),
        80 => "quatre-vingts".into(),
        _ => format!("quatre-vingt-{}", moins_de_cent(n - 80)),
    }
}

/// 0 à 999. `devant_mille` : « cent » et « vingt » ne prennent pas de « s »
/// devant « mille » (« quatre-vingt mille », « deux cent mille »).
fn moins_de_mille(n: u64, devant_mille: bool) -> String {
    let (c, reste) = (n / 100, n % 100);
    let mut mots = Vec::new();
    match c {
        0 => {}
        1 => mots.push("cent".to_string()),
        _ => mots.push(format!("{} cent{}", UNITES[c as usize], if reste == 0 && !devant_mille { "s" } else { "" })),
    }
    if reste > 0 || c == 0 {
        let mut r = moins_de_cent(reste);
        if devant_mille && reste == 80 {
            r = "quatre-vingt".into();
        }
        mots.push(r);
    }
    mots.join(" ")
}

/// Nombre entier en toutes lettres : 2_500_081 → « deux millions cinq cent mille quatre-vingt-un ».
pub fn nombre_en_lettres(n: u64) -> String {
    if n == 0 {
        return "zéro".into();
    }
    let mut mots = Vec::new();
    let mut groupe = |valeur: u64, singulier: &str, pluriel: &str| {
        if valeur > 0 {
            // Le groupe des billions n'a pas de plafond : au-delà de 999, on l'écrit
            // lui-même en lettres (« mille billions ») au lieu de planter.
            let nombre = if valeur < 1000 { moins_de_mille(valeur, false) } else { nombre_en_lettres(valeur) };
            mots.push(format!("{nombre} {}", if valeur > 1 { pluriel } else { singulier }));
        }
    };
    groupe(n / 1_000_000_000_000, "billion", "billions");
    groupe(n / 1_000_000_000 % 1000, "milliard", "milliards");
    groupe(n / 1_000_000 % 1000, "million", "millions");
    let milliers = n / 1000 % 1000;
    if milliers == 1 {
        mots.push("mille".into());
    } else if milliers > 1 {
        mots.push(format!("{} mille", moins_de_mille(milliers, true)));
    }
    if !n.is_multiple_of(1000) {
        mots.push(moins_de_mille(n % 1000, false));
    }
    mots.join(" ")
}

/// Nom de l'unité principale et de la subdivision : (singulier, pluriel).
fn noms_devise(code: &str) -> ((&'static str, &'static str), (&'static str, &'static str)) {
    match code {
        "XAF" | "XOF" => (("franc CFA", "francs CFA"), ("centime", "centimes")),
        "EUR" => (("euro", "euros"), ("centime", "centimes")),
        "CAD" => (("dollar canadien", "dollars canadiens"), ("cent", "cents")),
        "USD" => (("dollar américain", "dollars américains"), ("cent", "cents")),
        "MGA" => (("ariary", "ariary"), ("iraimbilanja", "iraimbilanja")),
        _ => (("", ""), ("", "")),
    }
}

fn avec_unite(n: u64, (singulier, pluriel): (&str, &str), code: &str) -> String {
    let lettres = nombre_en_lettres(n);
    if singulier.is_empty() {
        return format!("{lettres} {code}");
    }
    // « un million de francs », mais « un million deux francs »
    let de = if n >= 1_000_000 && n.is_multiple_of(1_000_000) { "de " } else { "" };
    format!("{lettres} {de}{}", if n > 1 { pluriel } else { singulier })
}

/// 50000 XAF → « Cinquante mille francs CFA » ;
/// 125050 EUR → « Mille deux cent cinquante euros et cinquante centimes ».
pub fn montant_en_lettres(valeur: i64, devise: &str, decimales: u8) -> String {
    let absolu = valeur.unsigned_abs();
    let diviseur = 10u64.pow(decimales as u32);
    let (principale, subdivision) = noms_devise(devise);
    let mut texte = avec_unite(absolu / diviseur, principale, devise);
    let fraction = absolu % diviseur;
    if fraction > 0 && !subdivision.0.is_empty() {
        // Les subdivisions se comptent en centièmes, quelle que soit la précision stockée.
        let centiemes = if decimales >= 2 { fraction / 10u64.pow(decimales as u32 - 2) } else { fraction * 10 };
        texte = format!("{texte} et {}", avec_unite(centiemes, subdivision, devise));
    }
    majuscule(&texte)
}

pub fn majuscule(texte: &str) -> String {
    let mut c = texte.chars();
    match c.next() {
        Some(premiere) => premiere.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

// ───────────────────────────────── Dates ─────────────────────────────────

const MOIS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];

fn jour_semaine(j: Weekday) -> &'static str {
    match j {
        Weekday::Mon => "lundi",
        Weekday::Tue => "mardi",
        Weekday::Wed => "mercredi",
        Weekday::Thu => "jeudi",
        Weekday::Fri => "vendredi",
        Weekday::Sat => "samedi",
        Weekday::Sun => "dimanche",
    }
}

/// « 13 novembre 2026 » (« 1er » pour le premier du mois).
pub fn date(d: NaiveDate) -> String {
    let jour = if d.day() == 1 { "1er".to_string() } else { d.day().to_string() };
    format!("{jour} {} {}", MOIS[d.month0() as usize], d.year())
}

/// « vendredi 13 novembre 2026 ».
pub fn date_longue(d: NaiveDate) -> String {
    format!("{} {}", jour_semaine(d.weekday()), date(d))
}

/// « 14 h 05 ».
pub fn heure(h: u32, m: u32) -> String {
    format!("{h} h {m:02}")
}

/// Pourcentage stocké en centièmes : 2000 → « 20 % », 1925 → « 19,25 % », 550 → « 5,5 % ».
pub fn pourcentage(centiemes: i64) -> String {
    let entier = centiemes / 100;
    let reste = centiemes % 100;
    let decimales = match reste {
        0 => String::new(),
        r if r % 10 == 0 => format!(",{}", r / 10),
        r => format!(",{r:02}"),
    };
    format!("{entier}{decimales}{INSECABLE}%")
}

pub fn pluriel(n: i64, mot: &str) -> String {
    format!("{n} {mot}{}", if n > 1 { "s" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trouvé par le fuzzing (qa_fuzz) : au-delà de 999 billions, l'index sortait du tableau.
    #[test]
    fn tres_grands_nombres_sans_plantage() {
        assert_eq!(nombre_en_lettres(1_000_000_000_000_000), "mille billions");
        assert_eq!(nombre_en_lettres(2_500_000_000_000_000), "deux mille cinq cents billions");
        assert!(montant_en_lettres(i64::MAX, "XAF", 0).starts_with("Neuf millions deux cent vingt-trois mille"));
        assert!(!montant_en_lettres(i64::MIN, "EUR", 2).is_empty());
    }

    #[test]
    fn nombres_en_lettres() {
        let cas = [
            (0, "zéro"),
            (1, "un"),
            (16, "seize"),
            (17, "dix-sept"),
            (21, "vingt et un"),
            (22, "vingt-deux"),
            (70, "soixante-dix"),
            (71, "soixante et onze"),
            (77, "soixante-dix-sept"),
            (80, "quatre-vingts"),
            (81, "quatre-vingt-un"),
            (91, "quatre-vingt-onze"),
            (99, "quatre-vingt-dix-neuf"),
            (100, "cent"),
            (101, "cent un"),
            (200, "deux cents"),
            (201, "deux cent un"),
            (280, "deux cent quatre-vingts"),
            (1000, "mille"),
            (1001, "mille un"),
            (2000, "deux mille"),
            (80_000, "quatre-vingt mille"),
            (200_000, "deux cent mille"),
            (25_000, "vingt-cinq mille"),
            (50_000, "cinquante mille"),
            (1_000_000, "un million"),
            (2_500_081, "deux millions cinq cent mille quatre-vingt-un"),
            (200_000_000, "deux cents millions"),
            (1_000_000_000, "un milliard"),
        ];
        for (n, attendu) in cas {
            assert_eq!(nombre_en_lettres(n), attendu, "{n}");
        }
    }

    #[test]
    fn montants_en_lettres_selon_la_devise() {
        assert_eq!(montant_en_lettres(50_000, "XAF", 0), "Cinquante mille francs CFA");
        assert_eq!(montant_en_lettres(1, "XAF", 0), "Un franc CFA");
        assert_eq!(montant_en_lettres(1_000_000, "XAF", 0), "Un million de francs CFA");
        assert_eq!(montant_en_lettres(1_000_500, "XOF", 0), "Un million cinq cents francs CFA");
        assert_eq!(montant_en_lettres(125_050, "EUR", 2), "Mille deux cent cinquante euros et cinquante centimes");
        assert_eq!(montant_en_lettres(100, "EUR", 2), "Un euro");
        assert_eq!(montant_en_lettres(10_001, "CAD", 2), "Cent dollars canadiens et un cent");
        assert_eq!(montant_en_lettres(5_000, "MGA", 0), "Cinq mille ariary");
        assert_eq!(montant_en_lettres(42, "GHS", 0), "Quarante-deux GHS");
        assert_eq!(montant_en_lettres(-3_000, "XAF", 0), "Trois mille francs CFA");
    }

    #[test]
    fn montants_en_chiffres_comme_dans_l_interface() {
        assert_eq!(montant(50_000, "XAF", 0), "50\u{a0}000\u{a0}FCFA");
        assert_eq!(montant(1_234_567, "XOF", 0), "1\u{a0}234\u{a0}567\u{a0}F CFA");
        assert_eq!(montant(125_050, "EUR", 2), "1\u{a0}250,50\u{a0}€");
        assert_eq!(montant(5, "CAD", 2), "0,05\u{a0}$CA");
        assert_eq!(montant(-60_000, "XAF", 0), "-60\u{a0}000\u{a0}FCFA");
        assert_eq!(montant(0, "XAF", 0), "0\u{a0}FCFA");
    }

    #[test]
    fn dates_en_francais() {
        let d = NaiveDate::from_ymd_opt(2026, 11, 13).unwrap();
        assert_eq!(date(d), "13 novembre 2026");
        assert_eq!(date_longue(d), "vendredi 13 novembre 2026");
        assert_eq!(date(NaiveDate::from_ymd_opt(2027, 1, 1).unwrap()), "1er janvier 2027");
        assert_eq!(heure(9, 5), "9 h 05");
        assert_eq!(pluriel(1, "nuit"), "1 nuit");
        assert_eq!(pluriel(3, "nuit"), "3 nuits");
        assert_eq!(pourcentage(2000), "20\u{a0}%");
        assert_eq!(pourcentage(1925), "19,25\u{a0}%");
        assert_eq!(pourcentage(550), "5,5\u{a0}%");
        assert_eq!(pourcentage(5), "0,05\u{a0}%");
    }
}
