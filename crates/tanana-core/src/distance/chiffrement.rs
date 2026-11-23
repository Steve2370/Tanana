//! Chiffrement de bout en bout des envois.
//!
//! Une **clé secrète** aléatoire de 128 bits est créée à l'activation, puis
//! montrée une fois à l'administrateur (à noter sur papier). On en tire deux
//! valeurs indépendantes (HKDF-SHA256) :
//! - la clé de chiffrement (XChaCha20-Poly1305), qui ne quitte jamais
//!   l'hôtel ni l'ordinateur de l'administrateur ;
//! - le jeton de lecture, donné au relais pour qu'il sache qui peut
//!   télécharger. Le connaître ne permet pas de déchiffrer.
//!
//! Chaque lot est compressé puis chiffré. Les données associées (hôtel,
//! numéros de début et de fin) empêchent le relais de mélanger, rejouer ou
//! réordonner des lots sans que cela se voie.

use std::io::{Read, Write};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use data_encoding::{BASE32_NOPAD, BASE64};
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use hkdf::Hkdf;
use sha2::Sha256;

use crate::erreur::{Erreur, Resultat};

const SEL: &[u8] = b"tanana-consultation-distance-v1";
/// Texte connu, chiffré à l'inscription : permet de vérifier une clé secrète
/// avant de télécharger quoi que ce soit.
pub const TEXTE_TEMOIN: &[u8] = b"Tanana : cle secrete correcte";
/// Taille maximale d'un lot une fois décompressé (protection contre les bombes de décompression).
const DECOMPRESSE_MAX: u64 = 64 * 1024 * 1024;

/// La clé secrète de consultation à distance.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret([u8; 16]);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl Secret {
    pub fn generer() -> Resultat<Self> {
        let mut octets = [0u8; 16];
        getrandom::fill(&mut octets).map_err(|e| Erreur::Base(format!("hasard indisponible : {e}")))?;
        Ok(Self(octets))
    }

    /// « K7QF-29XM-… » : 26 caractères en 7 groupes, sans 0/1/8/9 (pas de
    /// confusion entre O et 0, I et 1, B et 8).
    pub fn afficher(&self) -> String {
        let brut = BASE32_NOPAD.encode(&self.0);
        brut.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-")
    }

    /// Lit une clé recopiée à la main : majuscules ou non, avec ou sans
    /// tirets ni espaces ; 0, 1 et 8 tapés par erreur sont compris comme O, I et B.
    pub fn lire(texte: &str) -> Resultat<Self> {
        let propre: String = texte
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .map(|c| match c.to_ascii_uppercase() {
                '0' => 'O',
                '1' => 'I',
                '8' => 'B',
                c => c,
            })
            .collect();
        let octets =
            BASE32_NOPAD.decode(propre.as_bytes()).ok().and_then(|v| <[u8; 16]>::try_from(v).ok()).ok_or_else(
                || Erreur::Validation("clé secrète mal recopiée : vérifiez chaque groupe de 4 caractères".into()),
            )?;
        Ok(Self(octets))
    }

    fn deriver(&self, usage: &[u8]) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(SEL), &self.0);
        let mut sortie = [0u8; 32];
        hk.expand(usage, &mut sortie).expect("32 octets : longueur valide pour HKDF");
        sortie
    }

    /// Jeton présenté au relais pour lire les lots (ne permet pas de déchiffrer).
    pub fn jeton_lecture(&self) -> String {
        hex::encode(self.deriver(b"jeton-lecture"))
    }

    /// Jeton des actions d'administrateur à distance (modifier, reprendre l'hôtel).
    pub fn jeton_admin(&self) -> String {
        hex::encode(self.deriver(b"jeton-admin"))
    }

    fn chiffreur(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&self.deriver(b"chiffrement").into())
    }

    /// Compresse puis chiffre ; renvoie (nonce, texte chiffré) en base64.
    pub fn chiffrer(&self, clair: &[u8], associe: &str) -> Resultat<(String, String)> {
        let mut compresseur = DeflateEncoder::new(Vec::new(), Compression::default());
        compresseur.write_all(clair).map_err(|e| Erreur::Base(format!("compression : {e}")))?;
        let compresse = compresseur.finish().map_err(|e| Erreur::Base(format!("compression : {e}")))?;
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|e| Erreur::Base(format!("hasard indisponible : {e}")))?;
        let chiffre = self
            .chiffreur()
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: &compresse, aad: associe.as_bytes() })
            .map_err(|_| Erreur::Base("chiffrement impossible".into()))?;
        Ok((BASE64.encode(&nonce), BASE64.encode(&chiffre)))
    }

    /// Déchiffre puis décompresse. Échoue si la clé est mauvaise ou si le
    /// paquet a été modifié, déplacé ou attribué à un autre hôtel.
    pub fn dechiffrer(&self, nonce: &str, chiffre: &str, associe: &str) -> Resultat<Vec<u8>> {
        let illisible = || Erreur::CleDistanceIncorrecte;
        let nonce = BASE64.decode(nonce.as_bytes()).map_err(|_| illisible())?;
        if nonce.len() != 24 {
            return Err(illisible());
        }
        let chiffre = BASE64.decode(chiffre.as_bytes()).map_err(|_| illisible())?;
        let compresse = self
            .chiffreur()
            .decrypt(XNonce::from_slice(&nonce), Payload { msg: &chiffre, aad: associe.as_bytes() })
            .map_err(|_| illisible())?;
        let mut clair = Vec::new();
        DeflateDecoder::new(&compresse[..]).take(DECOMPRESSE_MAX).read_to_end(&mut clair).map_err(|_| illisible())?;
        Ok(clair)
    }
}

// ───────────────────── Coffre d'accès (identifiant + mot de passe) ─────────────────────

/// Clé de recherche d'un compte sur le relais : il ne voit pas l'identifiant.
pub fn recherche_compte(identifiant: &str) -> String {
    use sha2::Digest;
    hex::encode(Sha256::digest(format!("tanana-acces-v1|{}", identifiant.trim().to_lowercase()).as_bytes()))
}

/// Deux clés tirées du mot de passe (Argon2id, lent exprès) :
/// la « preuve » montrée au relais, et la clé qui ouvre le coffre.
pub struct ClesMotDePasse {
    pub preuve: String,
    coffre: [u8; 32],
}

impl ClesMotDePasse {
    pub fn deriver(mot_de_passe: &str, sel: &[u8]) -> Resultat<Self> {
        let mut sortie = [0u8; 64];
        argon2::Argon2::default()
            .hash_password_into(mot_de_passe.as_bytes(), sel, &mut sortie)
            .map_err(|e| Erreur::Base(format!("argon2 : {e}")))?;
        let mut coffre = [0u8; 32];
        coffre.copy_from_slice(&sortie[32..]);
        Ok(Self { preuve: hex::encode(&sortie[..32]), coffre })
    }

    /// Clé du coffre, à garder localement (hex).
    pub fn cle_coffre_hex(&self) -> String {
        hex::encode(self.coffre)
    }

    /// Reprend des clés gardées localement (preuve et clé du coffre, en hex).
    pub fn depuis_hex(preuve: &str, cle_coffre: &str) -> Resultat<Self> {
        let octets = hex::decode(cle_coffre).ok().and_then(|v| <[u8; 32]>::try_from(v).ok());
        let coffre = octets.ok_or_else(|| Erreur::Base("clé de coffre illisible".into()))?;
        Ok(Self { preuve: preuve.to_string(), coffre })
    }

    /// Empreinte de la preuve, gardée par le relais.
    pub fn verif(&self) -> String {
        use sha2::Digest;
        hex::encode(Sha256::digest(self.preuve.as_bytes()))
    }

    fn chiffreur(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&self.coffre.into())
    }

    pub fn fermer(&self, clair: &[u8], associe: &str) -> Resultat<(String, String)> {
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|e| Erreur::Base(format!("hasard indisponible : {e}")))?;
        let chiffre = self
            .chiffreur()
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: clair, aad: associe.as_bytes() })
            .map_err(|_| Erreur::Base("chiffrement impossible".into()))?;
        Ok((BASE64.encode(&nonce), BASE64.encode(&chiffre)))
    }

    pub fn ouvrir(&self, nonce: &str, chiffre: &str, associe: &str) -> Resultat<Vec<u8>> {
        let illisible = || Erreur::IdentifiantsInvalides;
        let nonce = BASE64.decode(nonce.as_bytes()).map_err(|_| illisible())?;
        if nonce.len() != 24 {
            return Err(illisible());
        }
        let chiffre = BASE64.decode(chiffre.as_bytes()).map_err(|_| illisible())?;
        self.chiffreur()
            .decrypt(XNonce::from_slice(&nonce), Payload { msg: &chiffre, aad: associe.as_bytes() })
            .map_err(|_| illisible())
    }
}

pub fn associe_coffre(recherche: &str) -> String {
    format!("tanana-coffre-v1|{recherche}")
}

pub fn associe_instantane(hotel_id: &str, apres_numero: i64) -> String {
    format!("tanana-instantane-v1|{hotel_id}|{apres_numero}")
}

pub fn associe_commande(hotel_id: &str, id: &str) -> String {
    format!("tanana-commande-v1|{hotel_id}|{id}")
}

pub fn associe_reponse(hotel_id: &str, id: &str) -> String {
    format!("tanana-reponse-v1|{hotel_id}|{id}")
}

/// Données associées d'un lot : liées au chiffrement, vérifiées au déchiffrement.
pub fn associe_lot(hotel_id: &str, seq_debut: i64, seq_fin: i64) -> String {
    format!("tanana-lot-v1|{hotel_id}|{seq_debut}|{seq_fin}")
}

pub fn associe_temoin(hotel_id: &str) -> String {
    format!("tanana-temoin-v1|{hotel_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_cle_se_recopie_sans_piege() {
        let s = Secret::generer().unwrap();
        let affiche = s.afficher();
        assert_eq!(affiche.len(), 26 + 6, "{affiche}");
        assert!(!affiche.contains(['0', '1', '8', '9']));
        assert_eq!(Secret::lire(&affiche).unwrap(), s);
        // minuscules, espaces, sans tirets
        let recopie = affiche.to_lowercase().replace('-', " ");
        assert_eq!(Secret::lire(&recopie).unwrap(), s);
        // 0 / 1 / 8 tapés à la place de O / I / B
        let confus = affiche.replace('O', "0").replace('I', "1").replace('B', "8");
        assert_eq!(Secret::lire(&confus).unwrap(), s);
        assert!(Secret::lire("ABCD-EFGH").is_err());
        assert!(Secret::lire("").is_err());
    }

    #[test]
    fn chiffrer_puis_dechiffrer() {
        let s = Secret::generer().unwrap();
        let texte = "réservation ".repeat(1000);
        let (nonce, chiffre) = s.chiffrer(texte.as_bytes(), "a").unwrap();
        assert!(chiffre.len() < texte.len() / 4, "compressé avant chiffrement");
        assert_eq!(s.dechiffrer(&nonce, &chiffre, "a").unwrap(), texte.as_bytes());
        // Deux chiffrements du même texte diffèrent (nonce aléatoire)
        assert_ne!(s.chiffrer(texte.as_bytes(), "a").unwrap().1, chiffre);
    }

    #[test]
    fn mauvaise_cle_paquet_modifie_ou_deplace() {
        let s = Secret::generer().unwrap();
        let (nonce, chiffre) = s.chiffrer(b"donnees", "hotel-1|1|5").unwrap();
        let autre = Secret::generer().unwrap();
        assert!(matches!(autre.dechiffrer(&nonce, &chiffre, "hotel-1|1|5"), Err(Erreur::CleDistanceIncorrecte)));
        assert!(matches!(s.dechiffrer(&nonce, &chiffre, "hotel-2|1|5"), Err(Erreur::CleDistanceIncorrecte)));
        let mut abime = BASE64.decode(chiffre.as_bytes()).unwrap();
        abime[3] ^= 1;
        assert!(s.dechiffrer(&nonce, &BASE64.encode(&abime), "hotel-1|1|5").is_err());
        assert!(s.dechiffrer("pas du base64!", &chiffre, "hotel-1|1|5").is_err());
    }

    #[test]
    fn coffre_ouvert_seulement_par_le_bon_mot_de_passe() {
        let sel = [7u8; 16];
        let cles = ClesMotDePasse::deriver("Madagascar2026", &sel).unwrap();
        assert_eq!(cles.preuve.len(), 64);
        let (n, c) = cles.fermer(b"contenu", "a").unwrap();
        assert_eq!(cles.ouvrir(&n, &c, "a").unwrap(), b"contenu");
        // Même mot de passe et même sel : mêmes clés
        assert_eq!(ClesMotDePasse::deriver("Madagascar2026", &sel).unwrap().preuve, cles.preuve);
        let mauvais = ClesMotDePasse::deriver("madagascar2026", &sel).unwrap();
        assert_ne!(mauvais.preuve, cles.preuve);
        assert!(matches!(mauvais.ouvrir(&n, &c, "a"), Err(Erreur::IdentifiantsInvalides)));
        assert!(cles.ouvrir(&n, &c, "b").is_err(), "coffre d'un autre compte");
        // Un autre sel donne d'autres clés
        assert_ne!(ClesMotDePasse::deriver("Madagascar2026", &[8u8; 16]).unwrap().preuve, cles.preuve);
        // La preuve ne révèle pas la clé du coffre ; le relais n'en garde que l'empreinte
        assert_ne!(cles.verif(), cles.preuve);
    }

    #[test]
    fn recherche_insensible_a_la_casse() {
        assert_eq!(recherche_compte(" Admin "), recherche_compte("admin"));
        assert_ne!(recherche_compte("admin"), recherche_compte("admin2"));
        assert_eq!(recherche_compte("admin").len(), 64);
    }

    #[test]
    fn le_jeton_de_lecture_ne_revele_pas_la_cle() {
        let s = Secret::generer().unwrap();
        let jeton = s.jeton_lecture();
        assert_eq!(jeton.len(), 64);
        assert_ne!(jeton, hex::encode(s.deriver(b"chiffrement")));
        assert_eq!(jeton, Secret::lire(&s.afficher()).unwrap().jeton_lecture(), "stable");
        assert_ne!(s.jeton_admin(), jeton, "jeton d'administrateur distinct");
    }
}
