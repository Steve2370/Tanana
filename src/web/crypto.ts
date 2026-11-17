/**
 * Chiffrement de la consultation à distance, côté navigateur.
 * Identique à crates/tanana-core/src/distance/chiffrement.rs :
 * - clé secrète de 128 bits, recopiée en base32 (« K7QF-29XM-… ») ;
 * - HKDF-SHA256 (sel fixe) → jeton de lecture (donné au relais) et clé de chiffrement
 *   (qui ne quitte jamais ce navigateur) ;
 * - lots : deflate brut puis XChaCha20-Poly1305, avec données associées.
 */
import { xchacha20poly1305 } from "@noble/ciphers/chacha.js";

const SEL = new TextEncoder().encode("tanana-consultation-distance-v1");
export const TEXTE_TEMOIN = "Tanana : cle secrete correcte";
const BASE32 = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
/** Protection contre les « bombes de décompression » (même limite que Tanana). */
const DECOMPRESSE_MAX = 64 * 1024 * 1024;

/** Mauvaise clé, ou paquet modifié en route : on ne peut rien lire. */
export class CleIncorrecte extends Error {
  constructor() {
    super("Cette clé secrète ne correspond pas à cet hôtel. Vérifiez chaque groupe de 4 caractères.");
  }
}

/** 0, 1 et 8 tapés par erreur sont compris comme O, I et B (jamais utilisés par Tanana). */
function nettoyer(saisie: string): string {
  return saisie
    .replace(/[^0-9a-z]/gi, "")
    .toUpperCase()
    .replace(/0/g, "O")
    .replace(/1/g, "I")
    .replace(/8/g, "B");
}

/** « hk7q f29x », « HK7Q-F29X »… → « HK7Q-F29X ». */
export function normaliserIdentifiant(saisie: string): string {
  const brut = nettoyer(saisie);
  return brut.length === 8 ? `${brut.slice(0, 4)}-${brut.slice(4)}` : brut;
}

/** Clé secrète recopiée à la main → 16 octets. Lève une erreur au message clair. */
export function lireCle(saisie: string): Uint8Array {
  const propre = nettoyer(saisie);
  const erreur = new Error("Clé secrète mal recopiée : vérifiez chaque groupe de 4 caractères.");
  let bits = 0;
  let valeur = 0;
  const octets: number[] = [];
  for (const c of propre) {
    const v = BASE32.indexOf(c);
    if (v < 0) throw erreur;
    valeur = (valeur << 5) | v;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      octets.push((valeur >> bits) & 0xff);
      valeur &= (1 << bits) - 1;
    }
  }
  // Base32 strict, comme Tanana : 16 octets exactement, bits de fin à zéro.
  if (octets.length !== 16 || valeur !== 0 || bits >= 5) throw erreur;
  return new Uint8Array(octets);
}

async function deriver(cle: Uint8Array, usage: string): Promise<Uint8Array> {
  const base = await crypto.subtle.importKey("raw", cle as BufferSource, "HKDF", false, ["deriveBits"]);
  const bits = await crypto.subtle.deriveBits(
    { name: "HKDF", hash: "SHA-256", salt: SEL, info: new TextEncoder().encode(usage) },
    base,
    256,
  );
  return new Uint8Array(bits);
}

const hex = (o: Uint8Array) => Array.from(o, (b) => b.toString(16).padStart(2, "0")).join("");

export interface Cles {
  /** Présenté au relais pour lire les lots ; ne permet pas de déchiffrer. */
  jetonLecture: string;
  chiffrement: Uint8Array;
}

export async function preparerCles(cle: Uint8Array): Promise<Cles> {
  const [jeton, chiffrement] = await Promise.all([deriver(cle, "jeton-lecture"), deriver(cle, "chiffrement")]);
  return { jetonLecture: hex(jeton), chiffrement };
}

export const associeLot = (hotelId: string, debut: number, fin: number) => `tanana-lot-v1|${hotelId}|${debut}|${fin}`;
/** Le témoin est chiffré à l'inscription, avant que l'hôtel ait son identifiant. */
export const ASSOCIE_TEMOIN = "tanana-temoin-v1|inscription";

function base64(texte: string): Uint8Array {
  try {
    return Uint8Array.from(atob(texte), (c) => c.charCodeAt(0));
  } catch {
    throw new CleIncorrecte();
  }
}

async function decompresser(donnees: Uint8Array): Promise<Uint8Array> {
  const source = new ReadableStream<Uint8Array>({
    start(c) {
      c.enqueue(donnees);
      c.close();
    },
  });
  const flux = source.pipeThrough(new DecompressionStream("deflate-raw") as unknown as TransformStream<Uint8Array, Uint8Array>);
  const morceaux: Uint8Array[] = [];
  let total = 0;
  try {
    for await (const morceau of flux as unknown as AsyncIterable<Uint8Array>) {
      total += morceau.length;
      if (total > DECOMPRESSE_MAX) throw new CleIncorrecte();
      morceaux.push(morceau);
    }
  } catch {
    throw new CleIncorrecte();
  }
  const sortie = new Uint8Array(total);
  let i = 0;
  for (const m of morceaux) {
    sortie.set(m, i);
    i += m.length;
  }
  return sortie;
}

/** Déchiffre puis décompresse. `CleIncorrecte` si la clé est mauvaise ou le paquet modifié. */
export async function dechiffrer(cles: Cles, nonce: string, chiffre: string, associe: string): Promise<Uint8Array> {
  const n = base64(nonce);
  if (n.length !== 24) throw new CleIncorrecte();
  let compresse: Uint8Array;
  try {
    compresse = xchacha20poly1305(cles.chiffrement, n, new TextEncoder().encode(associe)).decrypt(base64(chiffre));
  } catch {
    throw new CleIncorrecte();
  }
  return decompresser(compresse);
}
