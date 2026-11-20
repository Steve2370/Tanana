//! Mise en page des reçus et factures, puis rendu PDF (A4).
//!
//! Deux étapes séparées :
//! 1. `mise_en_page` calcule, page par page, une liste d'opérations simples
//!    (texte à telle position, rectangle, trait). C'est là que se trouvent
//!    toutes les décisions, et c'est ce que les tests vérifient.
//! 2. `rendre` dessine ces opérations avec krilla. La police Lexend est
//!    intégrée au PDF (sous-ensemble), le texte reste copiable.

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};

use crate::erreur::{Erreur, Resultat};
use crate::francais::{date, date_longue, majuscule, montant, montant_en_lettres, pluriel, pourcentage};
use crate::modeles::{CalculTaxe, FormatLogo, ModePaiement, Taxe, TypeDocument, TypePaiement};
use crate::services::documents::{ContenuDocument, LignePaiement};

// ───────────────────────────── Polices et couleurs ─────────────────────────────

static LEXEND_REGULIER: &[u8] = include_bytes!("../polices/Lexend-Regular.ttf");
static LEXEND_DEMI: &[u8] = include_bytes!("../polices/Lexend-SemiBold.ttf");
static LEXEND_GRAS: &[u8] = include_bytes!("../polices/Lexend-Bold.ttf");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Poids {
    Regulier,
    Demi,
    Gras,
}

impl Poids {
    fn donnees(self) -> &'static [u8] {
        match self {
            Poids::Regulier => LEXEND_REGULIER,
            Poids::Demi => LEXEND_DEMI,
            Poids::Gras => LEXEND_GRAS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Couleur(pub u8, pub u8, pub u8);

// Les couleurs de Tanana, en version imprimable (texte bien contrasté).
const MARQUE: Couleur = Couleur(0x09, 0x59, 0x5a);
const ORANGE: Couleur = Couleur(0xc0, 0x5f, 0x10);
const ENCRE: Couleur = Couleur(0x1d, 0x26, 0x26);
const GRIS: Couleur = Couleur(0x55, 0x60, 0x60);
const FOND: Couleur = Couleur(0xf1, 0xf5, 0xf4);
const BORDURE: Couleur = Couleur(0xcf, 0xd9, 0xd7);
const BLANC: Couleur = Couleur(0xff, 0xff, 0xff);

/// Largeur d'un texte en points, mesurée avec la même mise en forme que le rendu.
pub fn largeur(poids: Poids, taille: f32, texte: &str) -> f32 {
    let face = rustybuzz::Face::from_slice(poids.donnees(), 0).expect("police intégrée valide");
    let mut tampon = rustybuzz::UnicodeBuffer::new();
    tampon.push_str(texte);
    let forme = rustybuzz::shape(&face, &[], tampon);
    let avance: i32 = forme.glyph_positions().iter().map(|p| p.x_advance).sum();
    avance as f32 * taille / face.units_per_em() as f32
}

/// Vrai si la police sait dessiner ce caractère (sinon il apparaîtrait comme un carré).
pub fn caractere_disponible(c: char) -> bool {
    c.is_whitespace() || {
        let face = rustybuzz::Face::from_slice(LEXEND_REGULIER, 0).expect("police intégrée valide");
        face.glyph_index(c).is_some()
    }
}

/// Remplace ce que la police ne sait pas dessiner (texte saisi par l'utilisateur).
fn imprimable(texte: &str) -> String {
    texte
        .chars()
        .map(|c| match c {
            '\u{202f}' => '\u{a0}',
            '\u{2018}' | '\u{2019}' => '\'',
            c if caractere_disponible(c) => c,
            _ => '?',
        })
        .collect()
}

// ───────────────────────────── Opérations de dessin ─────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// `y` est la ligne de base du texte.
    Texte {
        x: f32,
        y: f32,
        poids: Poids,
        taille: f32,
        couleur: Couleur,
        texte: String,
    },
    Rect {
        x: f32,
        y: f32,
        l: f32,
        h: f32,
        couleur: Couleur,
    },
    Cadre {
        x: f32,
        y: f32,
        l: f32,
        h: f32,
        couleur: Couleur,
        epaisseur: f32,
    },
    Ligne {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        couleur: Couleur,
        epaisseur: f32,
    },
    /// Le logo de l'hôtel, dans un cadre de `l` × `h` (coin haut gauche en `x`, `y`).
    Logo {
        x: f32,
        y: f32,
        l: f32,
        h: f32,
    },
}

pub const PAGE_L: f32 = 595.28;
pub const PAGE_H: f32 = 841.89;
pub const MARGE: f32 = 48.0;
const DROITE: f32 = PAGE_L - MARGE;
const LARGEUR_UTILE: f32 = PAGE_L - 2.0 * MARGE;
/// En dessous, on change de page (le pied de page commence après).
const BAS_CONTENU: f32 = 742.0;
/// Place maximale du logo en haut à gauche de la première page.
pub const LOGO_L_MAX: f32 = 110.0;
pub const LOGO_H_MAX: f32 = 60.0;
const LOGO_HAUT: f32 = 24.0;

#[derive(Clone, Copy)]
enum Alignement {
    Gauche,
    Droite,
    Centre,
}

struct Mise {
    pages: Vec<Vec<Op>>,
    y: f32,
}

impl Mise {
    fn ops(&mut self) -> &mut Vec<Op> {
        self.pages.last_mut().expect("au moins une page")
    }

    #[allow(clippy::too_many_arguments)]
    fn texte(&mut self, x: f32, y: f32, poids: Poids, taille: f32, couleur: Couleur, al: Alignement, texte: &str) {
        let texte = imprimable(texte);
        let l = largeur(poids, taille, &texte);
        let x = match al {
            Alignement::Gauche => x,
            Alignement::Droite => x - l,
            Alignement::Centre => x - l / 2.0,
        };
        self.ops().push(Op::Texte { x, y, poids, taille, couleur, texte });
    }

    fn rect(&mut self, x: f32, y: f32, l: f32, h: f32, couleur: Couleur) {
        self.ops().push(Op::Rect { x, y, l, h, couleur });
    }

    fn cadre(&mut self, x: f32, y: f32, l: f32, h: f32, couleur: Couleur, epaisseur: f32) {
        self.ops().push(Op::Cadre { x, y, l, h, couleur, epaisseur });
    }

    fn ligne(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, couleur: Couleur, epaisseur: f32) {
        self.ops().push(Op::Ligne { x1, y1, x2, y2, couleur, epaisseur });
    }

    /// Paragraphe coupé en lignes de `largeur_max` ; renvoie le nombre de lignes.
    #[allow(clippy::too_many_arguments)]
    fn paragraphe(
        &mut self,
        x: f32,
        y: f32,
        largeur_max: f32,
        poids: Poids,
        taille: f32,
        interligne: f32,
        couleur: Couleur,
        texte: &str,
    ) -> usize {
        let lignes = couper(texte, poids, taille, largeur_max);
        for (i, l) in lignes.iter().enumerate() {
            self.texte(x, y + i as f32 * interligne, poids, taille, couleur, Alignement::Gauche, l);
        }
        lignes.len()
    }
}

/// Coupe un texte en lignes (respecte les retours à la ligne saisis).
fn couper(texte: &str, poids: Poids, taille: f32, largeur_max: f32) -> Vec<String> {
    let mut lignes = Vec::new();
    for paragraphe in texte.lines() {
        let mut courante = String::new();
        for mot in paragraphe.split_whitespace() {
            let essai = if courante.is_empty() { mot.to_string() } else { format!("{courante} {mot}") };
            if largeur(poids, taille, &imprimable(&essai)) <= largeur_max || courante.is_empty() {
                courante = essai;
            } else {
                lignes.push(std::mem::take(&mut courante));
                courante = mot.to_string();
            }
        }
        if !courante.is_empty() {
            lignes.push(courante);
        }
    }
    lignes
}

// ───────────────────────────── Libellés ─────────────────────────────

fn libelle_mode(m: ModePaiement) -> &'static str {
    match m {
        ModePaiement::Especes => "Espèces",
        ModePaiement::Carte => "Carte bancaire",
        ModePaiement::Mobile => "Mobile money",
        ModePaiement::Virement => "Virement",
        ModePaiement::Autre => "Autre",
    }
}

fn libelle_type(t: TypePaiement) -> &'static str {
    match t {
        TypePaiement::Acompte => "Acompte",
        TypePaiement::Solde => "Solde",
        TypePaiement::Remboursement => "Remboursement",
    }
}

/// « TVA (20 %) », « Taxe de séjour (2 000 Ar par nuit) », « Frais de dossier ».
fn libelle_taxe(t: &Taxe, c: &ContenuDocument) -> String {
    match t.calcul {
        CalculTaxe::Pourcentage => format!("{} ({})", t.libelle, pourcentage(t.valeur)),
        CalculTaxe::ParNuit => format!("{} ({} par nuit)", t.libelle, montant(t.valeur, &c.devise, c.decimales)),
        CalculTaxe::ParSejour => t.libelle.clone(),
    }
}

/// « du 13 au 14 novembre 2026 », « du 30 novembre au 2 décembre 2026 »…
/// Période du document : celle de la chambre, ou du groupe (de la première arrivée au dernier départ).
fn periode(c: &ContenuDocument) -> String {
    let a = c.sejours().iter().map(|s| s.arrivee).min().unwrap_or(c.sejour.arrivee);
    let d = c.sejours().iter().map(|s| s.depart).max().unwrap_or(c.sejour.depart);
    periode_de(a, d)
}

/// Vrai si toutes les chambres ont les mêmes dates.
fn memes_dates(c: &ContenuDocument) -> bool {
    c.sejours().windows(2).all(|w| w[0].arrivee == w[1].arrivee && w[0].depart == w[1].depart)
}

/// « Chambres 101, 102 et 103 »
fn liste_chambres(c: &ContenuDocument) -> String {
    let numeros: Vec<&str> = c.sejours().iter().map(|s| s.chambre_numero.as_str()).collect();
    match numeros.as_slice() {
        [] => String::new(),
        [seule] => format!("chambre {seule}"),
        [debut @ .., fin] => format!("chambres {} et {fin}", debut.join(", ")),
    }
}

fn periode_de(a: chrono::NaiveDate, d: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    if a.year() == d.year() && a.month() == d.month() {
        format!("du {} au {}", if a.day() == 1 { "1er".into() } else { a.day().to_string() }, date(d))
    } else if a.year() == d.year() {
        let debut = date(a);
        let sans_annee = debut.rsplit_once(' ').map(|(x, _)| x.to_string()).unwrap_or(debut);
        format!("du {sans_annee} au {}", date(d))
    } else {
        format!("du {} au {}", date(a), date(d))
    }
}

pub fn titre(c: &ContenuDocument) -> &'static str {
    match (c.type_document, c.paiement.as_ref().map(|p| p.type_paiement)) {
        (TypeDocument::Facture, _) => "FACTURE",
        (TypeDocument::Recu, Some(TypePaiement::Remboursement)) => "REÇU DE REMBOURSEMENT",
        (TypeDocument::Recu, _) => "REÇU",
    }
}

// ───────────────────────────── Mise en page ─────────────────────────────

pub fn mise_en_page(c: &ContenuDocument, duplicata: bool) -> Vec<Vec<Op>> {
    let mut m = Mise { pages: vec![Vec::new()], y: 0.0 };
    m.y = entete(&mut m, c, duplicata);
    let haut = m.y + 22.0;
    m.y = blocs_client_sejour(&mut m, c, haut);
    match c.type_document {
        TypeDocument::Recu => corps_recu(&mut m, c),
        TypeDocument::Facture => corps_facture(&mut m, c),
    }
    let total = m.pages.len();
    for (i, page) in m.pages.iter_mut().enumerate() {
        let mut p = Mise { pages: vec![std::mem::take(page)], y: 0.0 };
        pied_de_page(&mut p, c, i + 1, total);
        *page = p.pages.pop().expect("page");
    }
    m.pages
}

/// En-tête complet (première page) ; renvoie la position sous le trait.
fn entete(m: &mut Mise, c: &ContenuDocument, duplicata: bool) -> f32 {
    m.rect(0.0, 0.0, PAGE_L, 8.0, MARQUE);
    // À gauche : le logo (s'il y en a un), puis l'hôtel
    let (x_hotel, bas_logo) = match &c.hotel.logo {
        Some(logo) => {
            let (l, h) = taille_logo(logo.largeur, logo.hauteur);
            m.ops().push(Op::Logo { x: MARGE, y: LOGO_HAUT, l, h });
            (MARGE + l + 14.0, LOGO_HAUT + h)
        }
        None => (MARGE, 0.0),
    };
    let largeur_hotel = 280.0 - (x_hotel - MARGE).min(100.0);
    let nom = couper(&c.hotel.nom, Poids::Gras, 19.0, largeur_hotel);
    let mut y = 62.0;
    for l in &nom {
        m.texte(x_hotel, y, Poids::Gras, 19.0, MARQUE, Alignement::Gauche, l);
        y += 22.0;
    }
    y -= 4.0;
    if let Some(a) = &c.hotel.adresse {
        y += 13.0 * m.paragraphe(x_hotel, y, largeur_hotel, Poids::Regulier, 9.5, 13.0, GRIS, a) as f32;
    }
    let contact: Vec<String> = [c.hotel.telephone.as_ref().map(|t| format!("Tél. {t}")), c.hotel.email.clone()]
        .into_iter()
        .flatten()
        .collect();
    if !contact.is_empty() {
        y += 13.0
            * m.paragraphe(x_hotel, y, largeur_hotel, Poids::Regulier, 9.5, 13.0, GRIS, &contact.join(" · ")) as f32;
    }

    // À droite : le type de document, son numéro, sa date
    let titre = titre(c);
    let taille_titre = if titre.len() > 10 { 16.0 } else { 24.0 };
    m.texte(DROITE, 62.0, Poids::Gras, taille_titre, ENCRE, Alignement::Droite, titre);
    m.texte(DROITE, 82.0, Poids::Demi, 11.5, ENCRE, Alignement::Droite, &format!("N° {}", c.numero));
    m.texte(
        DROITE,
        97.0,
        Poids::Regulier,
        9.5,
        GRIS,
        Alignement::Droite,
        &format!("Le {} à {}", date(c.date), c.heure),
    );
    let mut y_droite = 97.0;
    if duplicata {
        let l = largeur(Poids::Gras, 10.0, "DUPLICATA") + 20.0;
        m.rect(DROITE - l, 106.0, l, 20.0, ORANGE);
        m.texte(DROITE - l / 2.0, 120.0, Poids::Gras, 10.0, BLANC, Alignement::Centre, "DUPLICATA");
        m.texte(DROITE, 138.0, Poids::Regulier, 8.5, GRIS, Alignement::Droite, "Copie d'un document déjà remis");
        y_droite = 138.0;
    }
    let bas = y.max(y_droite).max(bas_logo) + 12.0;
    m.ligne(MARGE, bas, DROITE, bas, BORDURE, 1.0);
    bas
}

/// Taille du logo sur le document : il tient dans LOGO_L_MAX × LOGO_H_MAX, sans être déformé.
pub fn taille_logo(largeur: u32, hauteur: u32) -> (f32, f32) {
    let (l, h) = (largeur.max(1) as f32, hauteur.max(1) as f32);
    let echelle = (LOGO_L_MAX / l).min(LOGO_H_MAX / h);
    (l * echelle, h * echelle)
}

/// En-tête réduit des pages suivantes.
fn entete_suite(m: &mut Mise, c: &ContenuDocument) -> f32 {
    m.rect(0.0, 0.0, PAGE_L, 8.0, MARQUE);
    m.texte(MARGE, 50.0, Poids::Gras, 13.0, MARQUE, Alignement::Gauche, &c.hotel.nom);
    m.texte(
        DROITE,
        50.0,
        Poids::Demi,
        11.0,
        ENCRE,
        Alignement::Droite,
        &format!("{} N° {} (suite)", majuscule(&titre(c).to_lowercase()), c.numero),
    );
    m.ligne(MARGE, 62.0, DROITE, 62.0, BORDURE, 1.0);
    84.0
}

fn nouvelle_page(m: &mut Mise, c: &ContenuDocument) {
    m.pages.push(Vec::new());
    m.y = entete_suite(m, c);
}

/// Change de page si `hauteur` ne tient plus.
fn place_pour(m: &mut Mise, c: &ContenuDocument, hauteur: f32) {
    if m.y + hauteur > BAS_CONTENU {
        nouvelle_page(m, c);
    }
}

fn blocs_client_sejour(m: &mut Mise, c: &ContenuDocument, haut: f32) -> f32 {
    let l = (LARGEUR_UTILE - 16.0) / 2.0;
    let (x1, x2) = (MARGE, MARGE + l + 16.0);
    let interieur = l - 28.0;

    // Contenu des deux blocs : (poids, taille, texte)
    let mut client = vec![(Poids::Gras, 12.5, c.client.nom.clone())];
    if let Some(t) = &c.client.telephone {
        client.push((Poids::Regulier, 9.5, format!("Tél. {t}")));
    }
    if let Some(e) = &c.client.email {
        client.push((Poids::Regulier, 9.5, e.clone()));
    }
    let sejour = if c.est_un_groupe() {
        let mut v = vec![(
            Poids::Gras,
            12.5,
            match &c.groupe {
                Some(nom) => format!("{nom} · {}", pluriel(c.chambres.len() as i64, "chambre")),
                None => format!("Groupe de {}", pluriel(c.chambres.len() as i64, "chambre")),
            },
        )];
        v.push((Poids::Regulier, 9.5, majuscule(&liste_chambres(c))));
        if memes_dates(c) {
            v.push((Poids::Regulier, 9.5, format!("Arrivée : {}", date_longue(c.sejour.arrivee))));
            v.push((Poids::Regulier, 9.5, format!("Départ : {}", date_longue(c.sejour.depart))));
            v.push((Poids::Regulier, 9.5, format!("{} par chambre", pluriel(c.sejour.nuits, "nuit"))));
        } else {
            v.push((Poids::Regulier, 9.5, majuscule(&periode(c))));
        }
        v
    } else {
        vec![
            (Poids::Gras, 12.5, format!("Chambre {} · {}", c.sejour.chambre_numero, c.sejour.chambre_type)),
            (Poids::Regulier, 9.5, format!("Arrivée : {}", date_longue(c.sejour.arrivee))),
            (Poids::Regulier, 9.5, format!("Départ : {}", date_longue(c.sejour.depart))),
            (Poids::Regulier, 9.5, pluriel(c.sejour.nuits, "nuit")),
        ]
    };
    let lignes = |bloc: &[(Poids, f32, String)]| -> Vec<(Poids, f32, String)> {
        bloc.iter().flat_map(|(p, t, s)| couper(s, *p, *t, interieur).into_iter().map(move |l| (*p, *t, l))).collect()
    };
    let (client, sejour) = (lignes(&client), lignes(&sejour));
    let hauteur_bloc = |b: &[(Poids, f32, String)]| b.iter().map(|(_, t, _)| t * 1.45).sum::<f32>();
    let h = 34.0 + hauteur_bloc(&client).max(hauteur_bloc(&sejour)) + 6.0;

    for (x, etiquette, bloc) in [(x1, "CLIENT", &client), (x2, "SÉJOUR", &sejour)] {
        m.rect(x, haut, l, h, FOND);
        m.texte(x + 14.0, haut + 20.0, Poids::Demi, 8.5, GRIS, Alignement::Gauche, etiquette);
        let mut y = haut + 38.0;
        for (p, t, s) in bloc.iter() {
            m.texte(x + 14.0, y, *p, *t, if *p == Poids::Gras { ENCRE } else { GRIS }, Alignement::Gauche, s);
            y += t * 1.45;
        }
    }
    haut + h
}

/// Lignes « libellé …… montant » alignées à droite ; la dernière peut être en gras.
fn recapitulatif(m: &mut Mise, x: f32, lignes: &[(&str, String, bool)]) {
    for (libelle, valeur, fort) in lignes {
        let (poids, taille) = if *fort { (Poids::Gras, 13.0) } else { (Poids::Regulier, 10.5) };
        m.texte(x, m.y, poids, taille, ENCRE, Alignement::Gauche, libelle);
        m.texte(DROITE, m.y, poids, taille, ENCRE, Alignement::Droite, valeur);
        m.y += if *fort { 22.0 } else { 18.0 };
    }
}

fn libelle_reste(c: &ContenuDocument) -> (&'static str, String) {
    let fmt = |v: i64| montant(v, &c.devise, c.decimales);
    if c.reste < 0 { ("À rendre au client", fmt(-c.reste)) } else { ("Reste à payer", fmt(c.reste)) }
}

fn corps_recu(m: &mut Mise, c: &ContenuDocument) {
    let p = c.paiement.as_ref().expect("un reçu porte sur un paiement");
    let fmt = |v: i64| montant(v, &c.devise, c.decimales);
    let rembourse = p.type_paiement == TypePaiement::Remboursement;

    // Le montant, bien visible
    let haut = m.y + 22.0;
    let en_lettres = montant_en_lettres(p.montant, &c.devise, c.decimales);
    let place_lettres = LARGEUR_UTILE - 48.0 - largeur(Poids::Regulier, 9.0, "Somme en lettres :");
    let h = 92.0 + 15.0 * couper(&en_lettres, Poids::Demi, 10.5, place_lettres).len() as f32;
    m.cadre(MARGE, haut, LARGEUR_UTILE, h, MARQUE, 1.5);
    let phrase = if rembourse {
        format!("Remboursé à {} la somme de :", c.client.nom)
    } else {
        format!("Reçu de {} la somme de :", c.client.nom)
    };
    m.paragraphe(MARGE + 20.0, haut + 26.0, LARGEUR_UTILE - 40.0, Poids::Regulier, 11.0, 14.0, ENCRE, &phrase);
    m.texte(MARGE + 20.0, haut + 64.0, Poids::Gras, 28.0, MARQUE, Alignement::Gauche, &fmt(p.montant));
    let etiquette = "Somme en lettres :";
    m.texte(MARGE + 20.0, haut + 84.0, Poids::Regulier, 9.0, GRIS, Alignement::Gauche, etiquette);
    let x_lettres = MARGE + 20.0 + largeur(Poids::Regulier, 9.0, etiquette) + 8.0;
    let lignes_lettres = couper(&en_lettres, Poids::Demi, 10.5, DROITE - 20.0 - x_lettres);
    for (i, l) in lignes_lettres.iter().enumerate() {
        m.texte(x_lettres, haut + 84.0 + 15.0 * i as f32, Poids::Demi, 10.5, ENCRE, Alignement::Gauche, l);
    }
    m.y = haut + h + 26.0;

    // Le détail
    let objet = format!(
        "{} pour {}{}, {}",
        if rembourse { "Remboursement" } else { libelle_type(p.type_paiement) },
        if c.est_un_groupe() { "le groupe, " } else { "la " },
        liste_chambres(c),
        periode(c)
    );
    let mut details = vec![
        ("Objet", objet),
        (if rembourse { "Mode" } else { "Mode de paiement" }, libelle_mode(p.mode).to_string()),
        (if rembourse { "Date" } else { "Date du paiement" }, date(p.date)),
    ];
    if let Some(n) = &p.note {
        details.push((if rembourse { "Motif" } else { "Remarque" }, n.clone()));
    }
    for (libelle, valeur) in details {
        m.texte(MARGE, m.y, Poids::Regulier, 10.0, GRIS, Alignement::Gauche, libelle);
        let n = m.paragraphe(MARGE + 120.0, m.y, LARGEUR_UTILE - 120.0, Poids::Demi, 10.5, 14.0, ENCRE, &valeur);
        m.y += 14.0 * n as f32 + 6.0;
    }

    // La situation du séjour juste après ce paiement
    m.y += 14.0;
    place_pour(m, c, 120.0);
    let x = MARGE + LARGEUR_UTILE / 2.0;
    m.texte(
        x,
        m.y,
        Poids::Demi,
        9.0,
        GRIS,
        Alignement::Gauche,
        if rembourse { "APRÈS CE REMBOURSEMENT" } else { "APRÈS CE PAIEMENT" },
    );
    m.y += 8.0;
    m.ligne(x, m.y, DROITE, m.y, BORDURE, 1.0);
    m.y += 18.0;
    let (libelle, reste) = libelle_reste(c);
    let mut lignes = vec![("Prix du séjour", fmt(c.prix_hebergement()), false)];
    let ajoutees = c.taxes.as_ref().map_or(0, |t| t.total_ajoutees);
    if ajoutees > 0 {
        lignes.push(("Taxes ajoutées", fmt(ajoutees), false));
    }
    lignes.push(("Total payé", fmt(c.total_paye), false));
    lignes.push((libelle, reste, true));
    recapitulatif(m, x, &lignes);
    if c.reste == 0 {
        m.texte(DROITE, m.y, Poids::Demi, 10.0, MARQUE, Alignement::Droite, "Séjour entièrement payé. Merci !");
        m.y += 16.0;
    }

    // Signatures
    m.y += 26.0;
    place_pour(m, c, 80.0);
    let l = (LARGEUR_UTILE - 24.0) / 2.0;
    for (x, texte) in [(MARGE, "Signature du client"), (MARGE + l + 24.0, "Signature et cachet de l'hôtel")] {
        m.texte(x, m.y, Poids::Regulier, 9.0, GRIS, Alignement::Gauche, texte);
        m.ligne(x, m.y + 52.0, x + l, m.y + 52.0, BORDURE, 1.0);
    }
    m.texte(
        MARGE + l + 24.0,
        m.y + 66.0,
        Poids::Regulier,
        8.5,
        GRIS,
        Alignement::Gauche,
        &format!("{} par {}", if rembourse { "Remboursé" } else { "Reçu" }, c.emis_par),
    );
    m.y += 70.0;
}

fn corps_facture(m: &mut Mise, c: &ContenuDocument) {
    let fmt = |v: i64| montant(v, &c.devise, c.decimales);
    m.y += 24.0;
    if let Some(ancienne) = &c.remplace {
        m.texte(
            MARGE,
            m.y,
            Poids::Demi,
            10.0,
            ORANGE,
            Alignement::Gauche,
            &format!("Cette facture remplace la facture N° {ancienne}, qui n'est plus valable."),
        );
        m.y += 20.0;
    }

    // Tableau des prestations
    use Alignement::{Droite, Gauche};
    let colonnes = [MARGE + 10.0, MARGE + 285.0, MARGE + 385.0, DROITE - 10.0];
    let colonnes_p = [MARGE + 10.0, MARGE + 190.0, MARGE + 330.0, DROITE - 10.0];
    // Chaque titre est aligné comme le contenu de sa colonne.
    let entete_tableau = |m: &mut Mise, titres: [(&str, f32, Alignement); 4]| {
        m.rect(MARGE, m.y, LARGEUR_UTILE, 24.0, MARQUE);
        for (titre, x, al) in titres {
            m.texte(x, m.y + 16.0, Poids::Demi, 9.5, BLANC, al, titre);
        }
        m.y += 24.0;
    };
    let titres_paiements = [
        ("Date", colonnes_p[0], Gauche),
        ("Nature", colonnes_p[1], Gauche),
        ("Mode", colonnes_p[2], Gauche),
        ("Montant", colonnes_p[3], Droite),
    ];
    entete_tableau(
        m,
        [
            ("Désignation", colonnes[0], Gauche),
            ("Nuits", colonnes[1], Droite),
            ("Prix par nuit", colonnes[2], Droite),
            ("Montant", colonnes[3], Droite),
        ],
    );
    let comprises: Vec<String> =
        c.lignes_taxes().iter().filter(|l| l.taxe.comprise).map(|l| libelle_taxe(&l.taxe, c)).collect();
    let titres_prestations = [
        ("Désignation", colonnes[0], Gauche),
        ("Nuits", colonnes[1], Droite),
        ("Prix par nuit", colonnes[2], Droite),
        ("Montant", colonnes[3], Droite),
    ];
    for (i, s) in c.sejours().iter().enumerate() {
        if i > 0 {
            if m.y + 46.0 > BAS_CONTENU {
                nouvelle_page(m, c);
                entete_tableau(m, titres_prestations);
            } else {
                m.ligne(MARGE, m.y, DROITE, m.y, BORDURE, 0.5);
            }
        }
        let prix_nuit =
            if s.nuits > 0 && s.prix_total % s.nuits == 0 { fmt(s.prix_total / s.nuits) } else { "—".into() };
        m.texte(
            colonnes[0],
            m.y + 20.0,
            Poids::Demi,
            10.5,
            ENCRE,
            Alignement::Gauche,
            &format!("Hébergement · chambre {} ({})", s.chambre_numero, s.chambre_type),
        );
        m.texte(
            colonnes[0],
            m.y + 35.0,
            Poids::Regulier,
            9.0,
            GRIS,
            Alignement::Gauche,
            &majuscule(&periode_de(s.arrivee, s.depart)),
        );
        m.texte(colonnes[1], m.y + 20.0, Poids::Regulier, 10.5, ENCRE, Alignement::Droite, &s.nuits.to_string());
        m.texte(colonnes[2], m.y + 20.0, Poids::Regulier, 10.5, ENCRE, Alignement::Droite, &prix_nuit);
        m.texte(colonnes[3], m.y + 20.0, Poids::Demi, 10.5, ENCRE, Alignement::Droite, &fmt(s.prix_total));
        m.y += 46.0;
    }
    if !comprises.is_empty() {
        let texte = format!("Ce prix comprend : {}", comprises.join(", "));
        let n = m.paragraphe(colonnes[0], m.y + 3.0, 270.0, Poids::Regulier, 9.0, 12.0, GRIS, &texte);
        m.y += 12.0 * n as f32 + 8.0;
    }

    // Taxes qui s'ajoutent au prix : une ligne chacune
    for l in c.lignes_taxes().iter().filter(|l| !l.taxe.comprise) {
        if m.y + 44.0 > BAS_CONTENU {
            nouvelle_page(m, c);
        }
        m.ligne(MARGE, m.y, DROITE, m.y, BORDURE, 0.5);
        let t = &l.taxe;
        m.texte(colonnes[0], m.y + 20.0, Poids::Demi, 10.5, ENCRE, Alignement::Gauche, &t.libelle);
        let detail = match t.calcul {
            // Dans un groupe, une taxe peut ne pas concerner toutes les chambres : pas de base affichée.
            CalculTaxe::Pourcentage if c.est_un_groupe() => format!("{} du prix hors taxes", pourcentage(t.valeur)),
            CalculTaxe::Pourcentage => {
                let ht = c.taxes.as_ref().map_or(c.sejour.prix_total, |d| d.hors_taxes);
                format!("{} du prix hors taxes ({})", pourcentage(t.valeur), fmt(ht))
            }
            CalculTaxe::ParNuit if c.est_un_groupe() => "Par nuit et par chambre".to_string(),
            CalculTaxe::ParNuit => "Par nuit".to_string(),
            CalculTaxe::ParSejour if c.est_un_groupe() => "Par chambre, pour le séjour".to_string(),
            CalculTaxe::ParSejour => "Pour le séjour".to_string(),
        };
        m.texte(colonnes[0], m.y + 33.0, Poids::Regulier, 9.0, GRIS, Alignement::Gauche, &detail);
        // Nuits concernées : toutes celles des chambres qui ont cette taxe.
        if t.calcul == CalculTaxe::ParNuit && t.valeur > 0 && l.montant % t.valeur == 0 {
            let nuits = l.montant / t.valeur;
            m.texte(colonnes[1], m.y + 20.0, Poids::Regulier, 10.5, ENCRE, Alignement::Droite, &nuits.to_string());
            m.texte(colonnes[2], m.y + 20.0, Poids::Regulier, 10.5, ENCRE, Alignement::Droite, &fmt(t.valeur));
        }
        m.texte(colonnes[3], m.y + 20.0, Poids::Demi, 10.5, ENCRE, Alignement::Droite, &fmt(l.montant));
        m.y += 42.0;
    }
    m.ligne(MARGE, m.y, DROITE, m.y, BORDURE, 1.0);
    m.y += 20.0;
    let total = c.total_a_payer();
    let libelle_total = if c.lignes_taxes().is_empty() { "Total du séjour" } else { "Total à payer" };
    m.texte(colonnes[2], m.y, Poids::Gras, 11.5, ENCRE, Alignement::Droite, libelle_total);
    m.texte(colonnes[3], m.y, Poids::Gras, 11.5, ENCRE, Alignement::Droite, &fmt(total));
    m.y += 18.0;

    // Détail fiscal : hors taxes, et part des taxes comprises
    if let Some(d) = c.taxes.as_ref().filter(|d| !d.lignes.is_empty()) {
        let mut lignes = vec![("Montant hors taxes".to_string(), d.hors_taxes)];
        lignes.extend(
            d.lignes
                .iter()
                .filter(|l| l.taxe.comprise)
                .map(|l| (format!("dont {}", libelle_taxe(&l.taxe, c)), l.montant)),
        );
        for (libelle, valeur) in lignes {
            m.texte(colonnes[2], m.y, Poids::Regulier, 9.5, GRIS, Alignement::Droite, &libelle);
            m.texte(colonnes[3], m.y, Poids::Regulier, 9.5, GRIS, Alignement::Droite, &fmt(valeur));
            m.y += 14.0;
        }
        m.y += 4.0;
    }
    m.y += 2.0;
    let lettres =
        format!("Arrêtée la présente facture à la somme de : {}.", montant_en_lettres(total, &c.devise, c.decimales));
    let n = m.paragraphe(MARGE, m.y, LARGEUR_UTILE, Poids::Regulier, 9.5, 13.0, GRIS, &lettres);
    m.y += 13.0 * n as f32 + 22.0;

    // Paiements reçus
    place_pour(m, c, 70.0);
    m.texte(MARGE, m.y, Poids::Gras, 12.0, ENCRE, Alignement::Gauche, "Paiements reçus");
    m.y += 12.0;
    if c.paiements.is_empty() {
        m.y += 14.0;
        m.texte(MARGE, m.y, Poids::Regulier, 10.0, GRIS, Alignement::Gauche, "Aucun paiement reçu pour le moment.");
        m.y += 24.0;
    } else {
        entete_tableau(m, titres_paiements);
        for (i, p) in c.paiements.iter().enumerate() {
            if m.y + 24.0 > BAS_CONTENU {
                nouvelle_page(m, c);
                entete_tableau(m, titres_paiements);
            }
            if i % 2 == 1 {
                m.rect(MARGE, m.y, LARGEUR_UTILE, 24.0, FOND);
            }
            let ligne = m.y + 16.0;
            m.texte(colonnes_p[0], ligne, Poids::Regulier, 10.0, ENCRE, Alignement::Gauche, &date(p.date));
            m.texte(
                colonnes_p[1],
                ligne,
                Poids::Regulier,
                10.0,
                ENCRE,
                Alignement::Gauche,
                libelle_type(p.type_paiement),
            );
            m.texte(colonnes_p[2], ligne, Poids::Regulier, 10.0, ENCRE, Alignement::Gauche, libelle_mode(p.mode));
            m.texte(colonnes_p[3], ligne, Poids::Demi, 10.0, ENCRE, Alignement::Droite, &fmt(LignePaiement::net(p)));
            m.y += 24.0;
        }
        m.y += 16.0;
    }

    // Récapitulatif : juste la place qu'il lui faut (trait, 3 lignes, tampon éventuel)
    let acquittee = c.reste == 0 && total > 0;
    place_pour(m, c, 20.0 + 18.0 * 2.0 + 22.0 + if acquittee { 34.0 } else { 0.0 });
    let x = MARGE + LARGEUR_UTILE / 2.0;
    m.ligne(x, m.y, DROITE, m.y, BORDURE, 1.0);
    m.y += 20.0;
    let (libelle, reste) = libelle_reste(c);
    recapitulatif(
        m,
        x,
        &[(libelle_total, fmt(total), false), ("Total payé", fmt(c.total_paye), false), (libelle, reste, true)],
    );
    if acquittee {
        // Tampon « Acquittée »
        let texte = "FACTURE ACQUITTÉE";
        let l = largeur(Poids::Gras, 11.0, texte) + 24.0;
        m.cadre(DROITE - l, m.y, l, 26.0, MARQUE, 1.5);
        m.texte(DROITE - l / 2.0, m.y + 17.5, Poids::Gras, 11.0, MARQUE, Alignement::Centre, texte);
        m.y += 34.0;
    }
}

fn pied_de_page(m: &mut Mise, c: &ContenuDocument, page: usize, total: usize) {
    let mut y = 760.0;
    m.ligne(MARGE, y, DROITE, y, BORDURE, 1.0);
    y += 14.0;
    if let Some(mentions) = &c.hotel.mentions {
        for l in couper(mentions, Poids::Regulier, 8.5, LARGEUR_UTILE).into_iter().take(3) {
            m.texte(MARGE, y, Poids::Regulier, 8.5, GRIS, Alignement::Gauche, &l);
            y += 11.5;
        }
    }
    let bas = PAGE_H - 30.0;
    m.texte(
        MARGE,
        bas,
        Poids::Regulier,
        8.0,
        GRIS,
        Alignement::Gauche,
        &format!("Émis par {} le {} à {} · Créé avec Tanana", c.emis_par, date(c.date), c.heure),
    );
    if total > 1 {
        m.texte(DROITE, bas, Poids::Regulier, 8.0, GRIS, Alignement::Droite, &format!("Page {page} sur {total}"));
    }
}

// ───────────────────────────── Rendu ─────────────────────────────

fn peinture(c: Couleur) -> krilla::paint::Paint {
    rgb::Color::new(c.0, c.1, c.2).into()
}

fn remplissage(c: Couleur) -> Fill {
    Fill { paint: peinture(c), opacity: NormalizedF32::ONE, rule: Default::default() }
}

/// L'image du logo, prête pour krilla.
fn image_logo(format: FormatLogo, octets: &[u8]) -> Resultat<Image> {
    let donnees: krilla::Data = octets.to_vec().into();
    match format {
        FormatLogo::Png => Image::from_png(donnees, true),
        FormatLogo::Jpeg => Image::from_jpeg(donnees, true),
    }
    .map_err(|_| Erreur::Validation("cette image est illisible : choisissez un fichier PNG ou JPEG".into()))
}

/// Vérifie qu'une image s'ouvre et peut être mise dans un PDF ; renvoie sa taille en pixels.
pub fn verifier_image(format: FormatLogo, octets: &[u8]) -> Resultat<(u32, u32)> {
    let image = image_logo(format, octets)?;
    let taille = image.size();
    let mut doc = Document::new();
    let reglages = PageSettings::from_wh(100.0, 100.0).ok_or_else(|| Erreur::Base("format de page".into()))?;
    {
        let mut page = doc.start_page_with(reglages);
        let mut s = page.surface();
        if let Some(t) = Size::from_wh(50.0, 50.0) {
            s.draw_image(image, t);
        }
        s.finish();
        page.finish();
    }
    doc.finish().map_err(|_| Erreur::Validation("cette image est illisible : choisissez un fichier PNG ou JPEG".into()))?;
    if taille.0 == 0 || taille.1 == 0 {
        return Err(Erreur::Validation("cette image est vide".into()));
    }
    Ok(taille)
}

/// Produit le fichier PDF du document, sans logo.
pub fn rendre(c: &ContenuDocument, duplicata: bool) -> Resultat<Vec<u8>> {
    rendre_avec_logo(c, duplicata, None)
}

/// Produit le fichier PDF du document. `logo` : l'image désignée par `c.hotel.logo`
/// (sans elle, le document est produit sans logo).
pub fn rendre_avec_logo(c: &ContenuDocument, duplicata: bool, logo: Option<&[u8]>) -> Resultat<Vec<u8>> {
    let image = match (&c.hotel.logo, logo) {
        (Some(l), Some(octets)) => Some(image_logo(l.format, octets)?),
        _ => None,
    };
    let polices = [
        (Poids::Regulier, Font::new(LEXEND_REGULIER.into(), 0)),
        (Poids::Demi, Font::new(LEXEND_DEMI.into(), 0)),
        (Poids::Gras, Font::new(LEXEND_GRAS.into(), 0)),
    ];
    let police = |p: Poids| -> Resultat<Font> {
        polices
            .iter()
            .find(|(q, _)| *q == p)
            .and_then(|(_, f)| f.clone())
            .ok_or_else(|| Erreur::Base("police du PDF illisible".into()))
    };

    let mut doc = Document::new();
    let titre_pdf = format!("{} {} — {}", majuscule(&titre(c).to_lowercase()), c.numero, c.hotel.nom);
    doc.set_metadata(
        Metadata::new()
            .title(titre_pdf)
            .creator("Tanana".into())
            .language("fr".into())
            .authors(vec![c.hotel.nom.clone()]),
    );
    let reglages = PageSettings::from_wh(PAGE_L, PAGE_H).ok_or_else(|| Erreur::Base("format de page".into()))?;
    for ops in mise_en_page(c, duplicata) {
        let mut page = doc.start_page_with(reglages.clone());
        let mut s = page.surface();
        for op in ops {
            match op {
                Op::Texte { x, y, poids, taille, couleur, texte } => {
                    s.set_stroke(None);
                    s.set_fill(Some(remplissage(couleur)));
                    s.draw_text(Point::from_xy(x, y), police(poids)?, taille, &texte, false, TextDirection::Auto);
                }
                Op::Rect { x, y, l, h, couleur } => {
                    let mut pb = PathBuilder::new();
                    if let Some(r) = Rect::from_xywh(x, y, l, h) {
                        pb.push_rect(r);
                    }
                    if let Some(chemin) = pb.finish() {
                        s.set_stroke(None);
                        s.set_fill(Some(remplissage(couleur)));
                        s.draw_path(&chemin);
                    }
                }
                Op::Cadre { x, y, l, h, couleur, epaisseur } => {
                    let mut pb = PathBuilder::new();
                    if let Some(r) = Rect::from_xywh(x, y, l, h) {
                        pb.push_rect(r);
                    }
                    if let Some(chemin) = pb.finish() {
                        s.set_fill(None);
                        s.set_stroke(Some(Stroke { paint: peinture(couleur), width: epaisseur, ..Default::default() }));
                        s.draw_path(&chemin);
                    }
                }
                Op::Logo { x, y, l, h } => {
                    if let (Some(image), Some(taille)) = (&image, Size::from_wh(l, h)) {
                        s.push_transform(&Transform::from_translate(x, y));
                        s.draw_image(image.clone(), taille);
                        s.pop();
                    }
                }
                Op::Ligne { x1, y1, x2, y2, couleur, epaisseur } => {
                    let mut pb = PathBuilder::new();
                    pb.move_to(x1, y1);
                    pb.line_to(x2, y2);
                    if let Some(chemin) = pb.finish() {
                        s.set_fill(None);
                        s.set_stroke(Some(Stroke { paint: peinture(couleur), width: epaisseur, ..Default::default() }));
                        s.draw_path(&chemin);
                    }
                }
            }
        }
        s.finish();
        page.finish();
    }
    doc.finish().map_err(|e| Erreur::Base(format!("création du PDF : {e:?}")))
}
