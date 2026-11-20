//! Logo de l'hôtel : vérification de l'image, droits, journal, impression sur les documents.

mod commun;

use commun::*;
use tanana_core::Erreur;
use tanana_core::modeles::*;
use tanana_core::pdf::{self, LOGO_H_MAX, LOGO_L_MAX, Op};

/// PNG 40 × 20.
const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAACgAAAAUCAYAAAD/Rn+7AAAAOUlEQVR4Xu3QQQ0AMAjAQMYPiZt/HyPBQj88egqanrrvx2K5uG0YSHmQ8iDlQcqDlAcpD1IepDxINcfFAeNBdYkVAAAAAElFTkSuQmCC";
/// JPEG 30 × 30.
const JPEG: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAYEBQYFBAYGBQYHBwYIChAKCgkJChQODwwQFxQYGBcUFhYaHSUfGhsjHBYWICwgIyYnKSopGR8tMC0oMCUoKSj/2wBDAQcHBwoIChMKChMoGhYaKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCj/wAARCAAeAB4DASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwDqKKKK/Mj9CCiiigAooooAKKKKAP/Z";
/// PNG 2100 × 10 : trop large.
const PNG_TROP_LARGE: &str = "iVBORw0KGgoAAAANSUhEUgAACDQAAAAKCAIAAADOqQskAAAAcElEQVR4Xu3ZQREAIAzAMDb8e+Yw0VdioAI6s/cAAAAAAABUtgoBAAAAAAB85gQAAAAAAJAyJwAAAAAAgJQ5AQAAAAAApMwJAAAAAAAgZU4AAAAAAAApcwIAAAAAAEiZEwAAAAAAQMqcAAAAAAAAUg9ZJAAa6Kc3IAAAAABJRU5ErkJggg==";

fn png() -> NouveauLogo {
    NouveauLogo { format: FormatLogo::Png, donnees: PNG.into() }
}

fn validation(r: tanana_core::Resultat<Option<Logo>>) -> String {
    match r {
        Err(Erreur::Validation(m)) => m,
        autre => panic!("refus attendu, obtenu {autre:?}"),
    }
}

#[test]
fn sans_logo_au_depart() {
    let mut h = hotel();
    assert_eq!(h.t.logo_hotel(&h.admin).unwrap(), None);
}

#[test]
fn enregistre_un_png_puis_un_jpeg_puis_le_retire() {
    let mut h = hotel();
    let l = h.t.definir_logo(&h.admin, Some(png())).unwrap().unwrap();
    assert_eq!((l.format, l.largeur, l.hauteur), (FormatLogo::Png, 40, 20));
    assert_eq!(h.t.logo_hotel(&h.admin).unwrap(), Some(l));

    let j = h.t.definir_logo(&h.admin, Some(NouveauLogo { format: FormatLogo::Jpeg, donnees: JPEG.into() })).unwrap();
    assert_eq!(j.as_ref().map(|j| (j.format, j.largeur, j.hauteur)), Some((FormatLogo::Jpeg, 30, 30)));

    assert_eq!(h.t.definir_logo(&h.admin, None).unwrap(), None);
    assert_eq!(h.t.logo_hotel(&h.admin).unwrap(), None);

    let logos = h.actions().iter().filter(|a| *a == "parametres.logo").count();
    assert_eq!(logos, 3, "chaque changement est au journal");
}

#[test]
fn remettre_le_meme_logo_ne_change_rien() {
    let mut h = hotel();
    h.t.definir_logo(&h.admin, Some(png())).unwrap();
    h.t.definir_logo(&h.admin, Some(png())).unwrap();
    assert_eq!(h.actions().iter().filter(|a| *a == "parametres.logo").count(), 1);
}

#[test]
fn refuse_les_images_douteuses() {
    let mut h = hotel();
    let essai = |h: &mut Hotel, format, donnees: &str| {
        h.t.definir_logo(&h.admin, Some(NouveauLogo { format, donnees: donnees.into() }))
    };
    assert!(validation(essai(&mut h, FormatLogo::Png, "pas du base64 !")).contains("illisible"));
    assert!(validation(essai(&mut h, FormatLogo::Png, "")).contains("illisible"));
    // Un JPEG annoncé comme PNG, et l'inverse
    assert!(validation(essai(&mut h, FormatLogo::Png, JPEG)).contains("illisible"));
    assert!(validation(essai(&mut h, FormatLogo::Jpeg, PNG)).contains("illisible"));
    // La bonne signature, mais un contenu abîmé
    let mut abime = data_encoding::BASE64.decode(PNG.as_bytes()).unwrap();
    abime.truncate(30);
    assert!(validation(essai(&mut h, FormatLogo::Png, &data_encoding::BASE64.encode(&abime))).contains("illisible"));
    // Trop grand, trop lourd
    assert!(validation(essai(&mut h, FormatLogo::Png, PNG_TROP_LARGE)).contains("trop grande"));
    let lourd = data_encoding::BASE64.encode(&vec![0u8; 400 * 1024]);
    assert!(validation(essai(&mut h, FormatLogo::Png, &lourd)).contains("trop lourde"));

    assert_eq!(h.t.logo_hotel(&h.admin).unwrap(), None, "rien n'a été enregistré");
    assert!(!h.actions().contains(&"parametres.logo".to_string()));
}

#[test]
fn seul_un_compte_avec_les_reglages_peut_le_changer_mais_tout_le_monde_le_voit() {
    let mut h = hotel();
    h.t.definir_logo(&h.admin, Some(png())).unwrap();
    let (_, reception) = h.employe("marie", "role-reception");
    assert!(matches!(h.t.definir_logo(&reception, None), Err(Erreur::PermissionRefusee { .. })));
    assert!(h.t.logo_hotel(&reception).unwrap().is_some());
}

/// Le cadre du logo sur la première page, s'il y en a un.
fn cadre_logo(c: &tanana_core::services::documents::ContenuDocument) -> Option<(f32, f32)> {
    pdf::mise_en_page(c, false)[0].iter().find_map(|o| if let Op::Logo { l, h, .. } = o { Some((*l, *h)) } else { None })
}

#[test]
fn le_logo_est_imprime_et_les_anciens_documents_gardent_le_leur() {
    let mut h = hotel();
    let (ch, cl) = h.base();
    let r = h.resa(&ch.id, &cl.id, "2026-03-10", "2026-03-12").unwrap();

    let sans = h.t.imprimer_facture(&h.admin, &r.id, 60).unwrap();
    assert_eq!(sans.contenu.hotel.logo, None);
    assert_eq!(cadre_logo(&sans.contenu), None);

    h.t.definir_logo(&h.admin, Some(png())).unwrap();
    let p = h
        .t
        .encaisser(
            &h.admin,
            Encaissement {
                reservation_id: r.id.clone(),
                montant: 10_000,
                type_paiement: TypePaiement::Acompte,
                mode: ModePaiement::Especes,
                note: None,
            },
        )
        .unwrap();
    let recu = h.t.imprimer_recu(&h.admin, &p.id, 60).unwrap();
    let logo = recu.contenu.hotel.logo.clone().expect("le reçu a le logo");
    assert_eq!((logo.largeur, logo.hauteur), (40, 20));
    // 40 × 20 : limité par la largeur (110) → 110 × 55, sans déformation
    let (l, hh) = cadre_logo(&recu.contenu).unwrap();
    assert!((l - LOGO_L_MAX).abs() < 0.01 && (hh - 55.0).abs() < 0.01 && hh <= LOGO_H_MAX);
    assert!(recu.pdf.len() > sans.pdf.len(), "l'image est dans le PDF");

    // Nouveau logo : l'ancienne facture se réimprime telle quelle (sans logo), le reçu garde le PNG
    h.t.definir_logo(&h.admin, Some(NouveauLogo { format: FormatLogo::Jpeg, donnees: JPEG.into() })).unwrap();
    let recu_bis = h.t.imprimer_recu(&h.admin, &p.id, 60).unwrap();
    assert!(recu_bis.duplicata);
    assert_eq!(recu_bis.contenu.hotel.logo, Some(logo));
    assert!(recu_bis.pdf.starts_with(b"%PDF-"));

    // Les paiements ont changé : nouvelle facture, avec le logo actuel (JPEG carré → 60 × 60)
    let f = h.t.imprimer_facture(&h.admin, &r.id, 60).unwrap();
    assert!(!f.duplicata);
    assert_eq!(f.contenu.hotel.logo.as_ref().map(|l| l.format), Some(FormatLogo::Jpeg));
    let (l, hh) = cadre_logo(&f.contenu).unwrap();
    assert!((l - 60.0).abs() < 0.01 && (hh - 60.0).abs() < 0.01);
}

#[test]
fn taille_du_logo_sur_le_document() {
    for (l, h) in [(1, 1), (1000, 10), (10, 1000), (500, 300), (110, 60)] {
        let (pl, ph) = pdf::taille_logo(l, h);
        assert!(pl <= LOGO_L_MAX + 0.01 && ph <= LOGO_H_MAX + 0.01, "{l}×{h} déborde");
        assert!((pl / ph - l as f32 / h as f32).abs() < 0.01 * (l as f32 / h as f32), "{l}×{h} déformé");
        assert!((pl - LOGO_L_MAX).abs() < 0.01 || (ph - LOGO_H_MAX).abs() < 0.01, "{l}×{h} : prend toute la place possible");
    }
    // Image vide (protection) : pas de division par zéro
    let (pl, ph) = pdf::taille_logo(0, 0);
    assert!(pl.is_finite() && ph.is_finite());
}

/// PNG gris de 2000 × 1, 1 × 2000 et 1 × 2001.
const PNG_2000_1: &str = "iVBORw0KGgoAAAANSUhEUgAAB9AAAAABCAAAAACjlmqhAAAAGElEQVR42mNsYBgFo2AUjIJRMApGwVAHAPfPAIIWImnCAAAAAElFTkSuQmCC";
const PNG_1_2000: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAfQCAAAAAAmu2pRAAAAHklEQVR42u3CMQ0AAAwDoKbKK30y9hDIGgAAAODXAekPEB/OVQrKAAAAAElFTkSuQmCC";
const PNG_1_2001: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAfRCAAAAADt57n0AAAAHklEQVR42u3CMQ0AAAwDoKbKK30y9hDIGgAAAODbAQlgECHtAho/AAAAAElFTkSuQmCC";

#[test]
fn limites_exactes_de_taille() {
    use tanana_core::services::logo::{COTE_MAX, OCTETS_MAX};
    assert_eq!((COTE_MAX, OCTETS_MAX), (2_000, 300 * 1024));
    let mut h = hotel();
    let essai = |h: &mut Hotel, donnees: String| {
        h.t.definir_logo(&h.admin, Some(NouveauLogo { format: FormatLogo::Png, donnees }))
    };
    // 2000 pixels de côté : accepté, dans un sens comme dans l'autre ; 2001 : refusé
    assert_eq!(essai(&mut h, PNG_2000_1.into()).unwrap().map(|l| (l.largeur, l.hauteur)), Some((2_000, 1)));
    assert_eq!(essai(&mut h, PNG_1_2000.into()).unwrap().map(|l| (l.largeur, l.hauteur)), Some((1, 2_000)));
    assert!(validation(essai(&mut h, PNG_1_2001.into())).contains("trop grande"));

    // Exactement 300 Ko : la taille passe (l'image, fausse, est ensuite refusée comme illisible) ; un octet de plus : trop lourde
    let image = |n: usize| {
        let mut o = b"\x89PNG\r\n\x1a\n".to_vec();
        o.resize(n, 0);
        o
    };
    assert!(validation(essai(&mut h, data_encoding::BASE64.encode(&image(OCTETS_MAX)))).contains("illisible"));
    assert!(validation(essai(&mut h, data_encoding::BASE64.encode(&image(OCTETS_MAX + 1)))).contains("trop lourde"));
    // Le texte reçu peut avoir quelques espaces autour (8 au plus) ; au-delà, refusé sans même le décoder
    let texte = data_encoding::BASE64.encode(&image(OCTETS_MAX));
    assert_eq!(texte.len(), OCTETS_MAX * 4 / 3);
    assert!(validation(essai(&mut h, format!("    {texte}    "))).contains("illisible"));
    assert!(validation(essai(&mut h, format!("     {texte}    "))).contains("trop lourde"));
}
