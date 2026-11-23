//! Reçus et factures.
//!
//! Règles :
//! - un reçu par paiement ; une facture par réservation (une nouvelle facture
//!   n'est émise que si le séjour ou les paiements ont changé, et elle indique
//!   quelle facture elle remplace) ;
//! - la numérotation se suit sans trou, par type et par année (la base le vérifie) ;
//! - le contenu est figé à l'émission : réimprimer redonne exactement le même
//!   document, marqué « Duplicata », et la réimpression est tracée.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::audit;
use crate::domaine::calculer_taxes;
use crate::erreur::{Erreur, Resultat};
use crate::modeles::{
    DetailTaxes, DocumentEmis, FormatLogo, ModePaiement, Reservation, StatutReservation, TypeDocument, TypePaiement,
};
use crate::permissions::Permission;
use crate::services::clients::client;
use crate::services::reservations::{groupe, reservation, taxes_de};
use crate::{Acteur, Tanana, nouvel_id, pdf};

// ───────────────────────────── Contenu figé ─────────────────────────────

/// Tout ce qui est imprimé sur le document, copié au moment de l'émission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContenuDocument {
    pub type_document: TypeDocument,
    pub numero: String,
    pub hotel: HotelDoc,
    pub devise: String,
    pub decimales: u8,
    /// Date et heure locales de l'émission
    pub date: NaiveDate,
    pub heure: String,
    pub emis_par: String,
    pub client: ClientDoc,
    pub sejour: SejourDoc,
    /// Pour un reçu : le paiement reçu (ou remboursé).
    pub paiement: Option<LignePaiement>,
    /// Facture : tous les paiements. Reçu : ceux d'avant, plus celui-ci.
    pub paiements: Vec<LignePaiement>,
    /// Encaissé net (encaissements moins remboursements).
    pub total_paye: i64,
    /// Total à payer moins total payé.
    pub reste: i64,
    /// Taxes du séjour (absent sur les documents émis avant les taxes).
    #[serde(default)]
    pub taxes: Option<DetailTaxes>,
    /// Numéro de la facture remplacée, le cas échéant.
    pub remplace: Option<String>,
    /// Réservation de groupe : toutes ses chambres non annulées (vide pour une chambre seule).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chambres: Vec<SejourDoc>,
    /// Nom du groupe, s'il en a un.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groupe: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HotelDoc {
    pub nom: String,
    pub adresse: Option<String>,
    pub telephone: Option<String>,
    pub email: Option<String>,
    pub mentions: Option<String>,
    /// Logo en usage à l'émission (absent sur les documents émis avant les logos).
    #[serde(default)]
    pub logo: Option<LogoDoc>,
}

/// Le logo d'un document : quelle image (table `logos`), et sa taille en pixels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LogoDoc {
    pub empreinte: String,
    pub format: FormatLogo,
    pub largeur: u32,
    pub hauteur: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClientDoc {
    pub nom: String,
    pub telephone: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SejourDoc {
    pub chambre_numero: String,
    pub chambre_type: String,
    pub arrivee: NaiveDate,
    pub depart: NaiveDate,
    pub nuits: i64,
    pub prix_total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LignePaiement {
    pub date: NaiveDate,
    pub type_paiement: TypePaiement,
    pub mode: ModePaiement,
    pub montant: i64,
    pub note: Option<String>,
}

impl ContenuDocument {
    /// Vrai pour le reçu ou la facture d'une réservation de groupe.
    pub fn est_un_groupe(&self) -> bool {
        !self.chambres.is_empty()
    }

    /// Les chambres du document : toutes celles du groupe, ou la seule chambre.
    pub fn sejours(&self) -> &[SejourDoc] {
        if self.est_un_groupe() { &self.chambres } else { std::slice::from_ref(&self.sejour) }
    }

    /// Prix de l'hébergement convenu, toutes chambres confondues.
    pub fn prix_hebergement(&self) -> i64 {
        self.sejours().iter().map(|s| s.prix_total).sum()
    }

    /// Ce que le client doit en tout : prix + taxes ajoutées.
    pub fn total_a_payer(&self) -> i64 {
        self.taxes.as_ref().map_or(self.prix_hebergement(), |t| t.total_a_payer)
    }

    /// Les taxes à afficher (vide s'il n'y en a pas).
    pub fn lignes_taxes(&self) -> &[crate::modeles::LigneTaxe] {
        self.taxes.as_ref().map_or(&[], |t| &t.lignes)
    }
}

impl LignePaiement {
    /// Montant signé : un remboursement compte en moins.
    pub fn net(&self) -> i64 {
        if self.type_paiement == TypePaiement::Remboursement { -self.montant } else { self.montant }
    }
}

/// Un document prêt à être enregistré et ouvert.
#[derive(Debug, Clone)]
pub struct DocumentImprime {
    pub document: DocumentEmis,
    /// Vrai si le document avait déjà été émis : c'est une réimpression.
    pub duplicata: bool,
    pub contenu: ContenuDocument,
    pub pdf: Vec<u8>,
}

impl DocumentImprime {
    /// Nom de fichier lisible et valable sur Windows comme sur Mac :
    /// « R-2026-0001 Mbarga Aline.pdf », « … (duplicata).pdf ».
    pub fn nom_fichier(&self) -> String {
        let client: String = self
            .contenu
            .client
            .nom
            .chars()
            .map(|c| {
                if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                    ' '
                } else {
                    c
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let client: String = client.chars().take(60).collect();
        let suffixe = if self.duplicata { " (duplicata)" } else { "" };
        format!("{} {}{suffixe}.pdf", self.document.numero, client.trim_end_matches('.'))
    }
}

// ───────────────────────────── Aides ─────────────────────────────

fn verifier_decalage(decalage_minutes: i32) -> Resultat<Duration> {
    if !(-14 * 60..=14 * 60).contains(&decalage_minutes) {
        return Err(Erreur::Validation("décalage horaire invalide".into()));
    }
    Ok(Duration::minutes(decalage_minutes as i64))
}

fn date_locale(horodatage: &str, decalage: Duration) -> Resultat<NaiveDate> {
    let t = DateTime::parse_from_rfc3339(horodatage).map_err(|e| Erreur::Base(format!("date illisible : {e}")))?;
    Ok((t.with_timezone(&Utc) + decalage).date_naive())
}

fn empreinte<T: Serialize>(v: &T) -> Resultat<String> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(v)?)))
}

fn hotel(conn: &Connection) -> Resultat<(HotelDoc, String, u8)> {
    let p = crate::services::systeme::lire_parametres(conn)?;
    let logo = crate::services::logo::logo_en_usage(conn)?.map(|(empreinte, l)| LogoDoc {
        empreinte,
        format: l.format,
        largeur: l.largeur,
        hauteur: l.hauteur,
    });
    Ok((
        HotelDoc {
            nom: p.nom_hotel,
            adresse: p.adresse_hotel,
            telephone: p.telephone_hotel,
            email: p.email_hotel,
            mentions: p.mentions_documents,
            logo,
        },
        p.devise,
        p.decimales_devise,
    ))
}

/// Les éléments communs aux reçus et factures : hôtel, client, séjour, paiements.
struct Base {
    hotel: HotelDoc,
    devise: String,
    decimales: u8,
    client: ClientDoc,
    sejour: SejourDoc,
    statut: StatutReservation,
    taxes: DetailTaxes,
    /// (id du paiement, ligne) dans l'ordre où ils ont été reçus
    paiements: Vec<(String, LignePaiement)>,
    /// Groupe : (id, nom) et toutes ses chambres non annulées.
    groupe: Option<(String, Option<String>)>,
    chambres: Vec<SejourDoc>,
}

fn sejour_doc(conn: &Connection, r: &Reservation) -> Resultat<SejourDoc> {
    let chambre_type: String = conn.query_row(
        "SELECT t.nom FROM chambres c JOIN types_chambre t ON t.id = c.type_id WHERE c.id = ?1",
        [&r.chambre_id],
        |l| l.get(0),
    )?;
    Ok(SejourDoc {
        chambre_numero: r.chambre_numero.clone(),
        chambre_type,
        arrivee: r.date_arrivee,
        depart: r.date_depart,
        nuits: r.nuits,
        prix_total: r.prix_total,
    })
}

/// Paiements de plusieurs réservations, dans l'ordre où ils ont été reçus.
fn paiements_des(
    conn: &Connection,
    reservations: &[&str],
    decalage: Duration,
) -> Resultat<Vec<(String, LignePaiement)>> {
    let mut tous = Vec::new();
    let mut stmt =
        conn.prepare("SELECT id, date, type, mode, montant, note FROM paiements WHERE reservation_id = ?1")?;
    for id in reservations {
        let lignes = stmt
            .query_map([id], |l| {
                Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?, l.get(2)?, l.get(3)?, l.get(4)?, l.get(5)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        tous.extend(lignes);
    }
    tous.sort_by(|a, b| (&a.1, &a.0).cmp(&(&b.1, &b.0)));
    tous.into_iter()
        .map(|(id, date, type_paiement, mode, montant, note)| {
            Ok((id, LignePaiement { date: date_locale(&date, decalage)?, type_paiement, mode, montant, note }))
        })
        .collect()
}

/// Les taxes de plusieurs chambres, additionnées ligne par ligne (même taxe = même ligne).
pub fn cumuler_taxes(details: &[DetailTaxes]) -> DetailTaxes {
    let mut t = DetailTaxes::default();
    for d in details {
        t.prix += d.prix;
        t.nuits += d.nuits;
        t.total_comprises += d.total_comprises;
        t.total_ajoutees += d.total_ajoutees;
        t.hors_taxes += d.hors_taxes;
        t.total_a_payer += d.total_a_payer;
        for l in &d.lignes {
            match t.lignes.iter_mut().find(|x| x.taxe == l.taxe) {
                Some(x) => x.montant += l.montant,
                None => t.lignes.push(l.clone()),
            }
        }
    }
    t
}

fn client_doc(conn: &Connection, client_id: &str) -> Resultat<ClientDoc> {
    let c = client(conn, client_id)?;
    Ok(ClientDoc {
        nom: [Some(c.nom.clone()), c.prenom.clone()].into_iter().flatten().collect::<Vec<_>>().join(" "),
        telephone: c.telephone,
        email: c.email,
    })
}

/// Base d'un document de groupe : toutes les chambres, tous les paiements.
fn base_groupe(conn: &Connection, groupe_id: &str, decalage: Duration) -> Resultat<Base> {
    let g = groupe(conn, groupe_id)?;
    let actives: Vec<&Reservation> = g.reservations.iter().filter(|r| r.statut != StatutReservation::Annulee).collect();
    let chambres = actives.iter().map(|r| sejour_doc(conn, r)).collect::<Resultat<Vec<_>>>()?;
    let premiere = actives.first().copied().or(g.reservations.first()).ok_or(Erreur::Introuvable("groupe".into()))?;
    let ids: Vec<&str> = g.reservations.iter().map(|r| r.id.as_str()).collect();
    let taxes = actives
        .iter()
        .map(|r| Ok(calculer_taxes(r.prix_total, r.nuits, &taxes_de(conn, &r.id)?)))
        .collect::<Resultat<Vec<_>>>()?;
    let (hotel, devise, decimales) = hotel(conn)?;
    Ok(Base {
        hotel,
        devise,
        decimales,
        client: client_doc(conn, &g.client_id)?,
        sejour: sejour_doc(conn, premiere)?,
        statut: if actives.is_empty() { StatutReservation::Annulee } else { premiere.statut },
        taxes: cumuler_taxes(&taxes),
        paiements: paiements_des(conn, &ids, decalage)?,
        groupe: Some((g.id.clone(), g.nom.clone())),
        chambres,
    })
}

fn base(conn: &Connection, reservation_id: &str, decalage: Duration) -> Resultat<Base> {
    let r = reservation(conn, reservation_id)?;
    if let Some(g) = &r.groupe_id {
        return base_groupe(conn, g, decalage);
    }
    let (hotel, devise, decimales) = hotel(conn)?;
    Ok(Base {
        taxes: calculer_taxes(r.prix_total, r.nuits, &taxes_de(conn, reservation_id)?),
        hotel,
        devise,
        decimales,
        client: client_doc(conn, &r.client_id)?,
        sejour: sejour_doc(conn, &r)?,
        statut: r.statut,
        paiements: paiements_des(conn, &[reservation_id], decalage)?,
        groupe: None,
        chambres: Vec::new(),
    })
}

fn prefixe(t: TypeDocument) -> &'static str {
    match t {
        TypeDocument::Recu => "R",
        TypeDocument::Facture => "F",
    }
}

/// Prochain numéro de l'année : dernier + 1.
fn prochain_numero(conn: &Connection, t: TypeDocument, annee: i32) -> Resultat<(i64, String)> {
    let dernier: i64 = conn.query_row(
        "SELECT coalesce(max(sequence), 0) FROM documents WHERE type = ?1 AND annee = ?2",
        params![t, annee],
        |l| l.get(0),
    )?;
    let sequence = dernier + 1;
    Ok((sequence, format!("{}-{annee}-{sequence:04}", prefixe(t))))
}

const COLONNES_DOCUMENT: &str = "d.id, d.type, d.numero, d.reservation_id, d.paiement_id, d.emis_le, u.nom,
        (SELECT p.numero FROM documents p WHERE p.id = d.remplace_id)";
const DEPUIS_DOCUMENTS: &str = "FROM documents d JOIN utilisateurs u ON u.id = d.emis_par";

fn ligne_document(l: &rusqlite::Row) -> rusqlite::Result<DocumentEmis> {
    Ok(DocumentEmis {
        id: l.get(0)?,
        type_document: l.get(1)?,
        numero: l.get(2)?,
        reservation_id: l.get(3)?,
        paiement_id: l.get(4)?,
        emis_le: l.get(5)?,
        emis_par_nom: l.get(6)?,
        remplace_numero: l.get(7)?,
    })
}

/// Document existant : (fiche, contenu figé, empreinte).
fn lire_document(
    conn: &Connection,
    condition: &str,
    valeur: &str,
) -> Resultat<Option<(DocumentEmis, ContenuDocument, String)>> {
    let ligne = conn
        .query_row(
            &format!(
                "SELECT {COLONNES_DOCUMENT}, d.contenu, d.empreinte {DEPUIS_DOCUMENTS}
                 WHERE {condition} ORDER BY d.emis_le DESC, d.sequence DESC LIMIT 1"
            ),
            [valeur],
            |l| Ok((ligne_document(l)?, l.get::<_, String>(8)?, l.get::<_, String>(9)?)),
        )
        .optional()?;
    ligne.map(|(doc, contenu, emp)| Ok((doc, serde_json::from_str(&contenu)?, emp))).transpose()
}

/// Enregistre un nouveau document et l'écrit au journal.
#[allow(clippy::too_many_arguments)]
fn inserer(
    tx: &Connection,
    a: &Acteur,
    contenu: &ContenuDocument,
    sequence: i64,
    reservation_id: &str,
    paiement_id: Option<&str>,
    empreinte: &str,
    remplace_id: Option<&str>,
    groupe_id: Option<&str>,
) -> Resultat<DocumentEmis> {
    let id = nouvel_id();
    tx.execute(
        "INSERT INTO documents (id, type, annee, sequence, numero, reservation_id, paiement_id, contenu, empreinte,
                                remplace_id, emis_le, emis_par, groupe_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            contenu.type_document,
            contenu.date.year(),
            sequence,
            contenu.numero,
            reservation_id,
            paiement_id,
            serde_json::to_string(contenu)?,
            empreinte,
            remplace_id,
            a.horodatage(),
            a.utilisateur_id,
            groupe_id
        ],
    )?;
    let doc =
        tx.query_row(&format!("SELECT {COLONNES_DOCUMENT} {DEPUIS_DOCUMENTS} WHERE d.id = ?1"), [&id], ligne_document)?;
    audit::ecrire(
        tx,
        &a.horodatage(),
        Some(&a.utilisateur_id),
        "document.emission",
        Some("reservation"),
        Some(reservation_id),
        None,
        Some(&json!({
            "numero": doc.numero,
            "type": contenu.type_document,
            "montant": contenu.paiement.as_ref().map(|p| p.montant).unwrap_or(contenu.prix_hebergement()),
            "remplace": doc.remplace_numero,
            "groupe": groupe_id,
        })),
    )?;
    Ok(doc)
}

fn tracer_reimpression(tx: &Connection, a: &Acteur, doc: &DocumentEmis) -> Resultat<()> {
    audit::ecrire(
        tx,
        &a.horodatage(),
        Some(&a.utilisateur_id),
        "document.reimpression",
        Some("reservation"),
        Some(&doc.reservation_id),
        None,
        Some(&json!({ "numero": doc.numero })),
    )
}

fn horaire(a: &Acteur, decalage: Duration) -> (NaiveDate, String) {
    let local = a.maintenant + decalage;
    (local.date_naive(), crate::francais::heure(local.hour(), local.minute()))
}

// ───────────────────────────── Services ─────────────────────────────

impl Tanana {
    /// Dessine le PDF, avec le logo que le document avait à son émission.
    fn rendre_document(&mut self, contenu: &ContenuDocument, duplicata: bool) -> Resultat<Vec<u8>> {
        let octets = match &contenu.hotel.logo {
            Some(l) => crate::services::logo::octets_logo(self.conn_brute(), &l.empreinte)?,
            None => None,
        };
        pdf::rendre_avec_logo(contenu, duplicata, octets.as_deref())
    }

    /// Reçu d'un paiement (encaissement ou remboursement). Le premier appel
    /// l'émet ; les suivants redonnent le même reçu, marqué « Duplicata ».
    pub fn imprimer_recu(
        &mut self,
        token: &str,
        paiement_id: &str,
        decalage_minutes: i32,
    ) -> Resultat<DocumentImprime> {
        let decalage = verifier_decalage(decalage_minutes)?;
        let (document, contenu, duplicata) = self.executer(token, Some(Permission::PaiementEncaisser), |tx, a| {
            if let Some((doc, contenu, _)) = lire_document(tx, "d.paiement_id = ?1", paiement_id)? {
                tracer_reimpression(tx, a, &doc)?;
                return Ok((doc, contenu, true));
            }
            let reservation_id: String = tx
                .query_row("SELECT reservation_id FROM paiements WHERE id = ?1", [paiement_id], |l| l.get(0))
                .optional()?
                .ok_or_else(|| Erreur::Introuvable("paiement".into()))?;
            let b = base(tx, &reservation_id, decalage)?;
            let position = b
                .paiements
                .iter()
                .position(|(id, _)| id == paiement_id)
                .ok_or_else(|| Erreur::Introuvable("paiement".into()))?;
            // Le reçu montre la situation juste après ce paiement, pas celle d'aujourd'hui.
            let jusque_la: Vec<LignePaiement> = b.paiements[..=position].iter().map(|(_, l)| l.clone()).collect();
            let total_paye: i64 = jusque_la.iter().map(LignePaiement::net).sum();
            let (date, heure) = horaire(a, decalage);
            let (sequence, numero) = prochain_numero(tx, TypeDocument::Recu, date.year())?;
            let contenu = ContenuDocument {
                type_document: TypeDocument::Recu,
                numero,
                hotel: b.hotel,
                devise: b.devise,
                decimales: b.decimales,
                date,
                heure,
                emis_par: a.nom.clone(),
                client: b.client,
                reste: b.taxes.total_a_payer - total_paye,
                sejour: b.sejour,
                paiement: Some(b.paiements[position].1.clone()),
                paiements: jusque_la,
                total_paye,
                taxes: Some(b.taxes).filter(|t| !t.lignes.is_empty()),
                remplace: None,
                chambres: b.chambres,
                groupe: b.groupe.as_ref().and_then(|(_, n)| n.clone()),
            };
            let emp = empreinte(&contenu)?;
            let groupe_id = b.groupe.as_ref().map(|(id, _)| id.as_str());
            let doc = inserer(tx, a, &contenu, sequence, &reservation_id, Some(paiement_id), &emp, None, groupe_id)?;
            Ok((doc, contenu, false))
        })?;
        let pdf = self.rendre_document(&contenu, duplicata)?;
        Ok(DocumentImprime { document, duplicata, contenu, pdf })
    }

    /// Facture d'une réservation. Si rien n'a changé depuis la dernière facture,
    /// c'est elle qui est redonnée (duplicata) ; sinon une nouvelle facture est
    /// émise et indique laquelle elle remplace.
    pub fn imprimer_facture(
        &mut self,
        token: &str,
        reservation_id: &str,
        decalage_minutes: i32,
    ) -> Resultat<DocumentImprime> {
        let decalage = verifier_decalage(decalage_minutes)?;
        let (document, contenu, duplicata) = self.executer(token, Some(Permission::PaiementEncaisser), |tx, a| {
            let b = base(tx, reservation_id, decalage)?;
            if b.statut == StatutReservation::Annulee {
                return Err(Erreur::StatutInvalide { de: "annulee".into(), vers: "facture".into() });
            }
            let paiements: Vec<LignePaiement> = b.paiements.into_iter().map(|(_, l)| l).collect();
            let total_paye: i64 = paiements.iter().map(LignePaiement::net).sum();
            // Ce qui fait qu'une facture est « la même » : client, séjour, paiements.
            let taxes = Some(b.taxes).filter(|t| !t.lignes.is_empty());
            // (sans taxes, même empreinte qu'avant leur arrivée : pas de facture de remplacement inutile)
            let groupe_id = b.groupe.as_ref().map(|(id, _)| id.clone());
            let emp = match (&taxes, &groupe_id) {
                (None, None) => empreinte(&json!([b.client, b.sejour, paiements]))?,
                (Some(t), None) => empreinte(&json!([b.client, b.sejour, paiements, t]))?,
                (t, Some(_)) => empreinte(&json!([b.client, b.chambres, paiements, t, b.groupe]))?,
            };
            let precedente = match &groupe_id {
                Some(g) => lire_document(tx, "d.type = 'facture' AND d.groupe_id = ?1", g)?,
                None => lire_document(
                    tx,
                    "d.type = 'facture' AND d.reservation_id = ?1 AND d.groupe_id IS NULL",
                    reservation_id,
                )?,
            };
            if let Some((doc, contenu, emp_precedente)) = &precedente
                && *emp_precedente == emp
            {
                tracer_reimpression(tx, a, doc)?;
                return Ok((doc.clone(), contenu.clone(), true));
            }
            let (date, heure) = horaire(a, decalage);
            let (sequence, numero) = prochain_numero(tx, TypeDocument::Facture, date.year())?;
            let contenu = ContenuDocument {
                type_document: TypeDocument::Facture,
                numero,
                hotel: b.hotel,
                devise: b.devise,
                decimales: b.decimales,
                date,
                heure,
                emis_par: a.nom.clone(),
                client: b.client,
                reste: 0,
                sejour: b.sejour,
                paiement: None,
                paiements,
                total_paye,
                taxes,
                remplace: precedente.as_ref().map(|(d, _, _)| d.numero.clone()),
                chambres: b.chambres,
                groupe: b.groupe.as_ref().and_then(|(_, n)| n.clone()),
            };
            let contenu = ContenuDocument { reste: contenu.total_a_payer() - total_paye, ..contenu };
            let remplace_id = precedente.as_ref().map(|(d, _, _)| d.id.as_str());
            let doc =
                inserer(tx, a, &contenu, sequence, reservation_id, None, &emp, remplace_id, groupe_id.as_deref())?;
            Ok((doc, contenu, false))
        })?;
        let pdf = self.rendre_document(&contenu, duplicata)?;
        Ok(DocumentImprime { document, duplicata, contenu, pdf })
    }

    /// Reçus et factures déjà émis pour une réservation, du plus récent au plus ancien.
    pub fn documents_reservation(&mut self, token: &str, reservation_id: &str) -> Resultat<Vec<DocumentEmis>> {
        self.executer(token, Some(Permission::ReservationVoir), |tx, _| {
            let mut stmt = tx.prepare(&format!(
                "SELECT {COLONNES_DOCUMENT} {DEPUIS_DOCUMENTS}
                 WHERE d.reservation_id = ?1
                    OR d.groupe_id = (SELECT groupe_id FROM reservations WHERE id = ?1)
                 ORDER BY d.emis_le DESC, d.numero DESC"
            ))?;
            let v = stmt.query_map([reservation_id], ligne_document)?.collect::<Result<Vec<_>, _>>()?;
            Ok(v)
        })
    }
}
