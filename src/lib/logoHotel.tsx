/**
 * Logo de l'hôtel : chargé une fois, partagé entre le menu (en haut à gauche)
 * et les réglages (où l'administrateur le change).
 */
import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

import type { Logo } from "../bindings/Logo";
import type { NouveauLogo } from "../bindings/NouveauLogo";
import { api } from "./api";

interface ContexteLogo {
  logo: Logo | null;
  /** À appeler après un changement dans les réglages. */
  changer: (logo: Logo | null) => void;
}

const Contexte = createContext<ContexteLogo>({ logo: null, changer: () => {} });

export function FournisseurLogo({ children }: { children: ReactNode }) {
  const [logo, setLogo] = useState<Logo | null>(null);
  useEffect(() => {
    let annule = false;
    // Sans logo (ou en cas d'erreur), le menu garde le symbole de Tanana.
    api.logoHotel().then(
      (l) => !annule && setLogo(l),
      () => {},
    );
    return () => {
      annule = true;
    };
  }, []);
  const changer = useCallback((l: Logo | null) => setLogo(l), []);
  const valeur = useMemo(() => ({ logo, changer }), [logo, changer]);
  return <Contexte.Provider value={valeur}>{children}</Contexte.Provider>;
}

export const useLogoHotel = () => useContext(Contexte);

/** Adresse à donner à une balise <img>. */
export function urlLogo(l: Pick<Logo, "format" | "donnees">): string {
  return `data:image/${l.format};base64,${l.donnees}`;
}

/** Plus grand côté du logo enregistré, en pixels : assez net pour l'impression. */
export const COTE_LOGO = 600;
/** Au-delà, on passe en JPEG (une photo en PNG serait trop lourde). */
const OCTETS_PNG_MAX = 250 * 1024;

export class ImageRefusee extends Error {}

const TYPES_ACCEPTES = ["image/png", "image/jpeg", "image/webp"];

/** Taille réduite pour que le plus grand côté fasse au plus `max` pixels. */
export function tailleReduite(largeur: number, hauteur: number, max = COTE_LOGO): { largeur: number; hauteur: number } {
  const echelle = Math.min(1, max / Math.max(largeur, hauteur, 1));
  return { largeur: Math.max(1, Math.round(largeur * echelle)), hauteur: Math.max(1, Math.round(hauteur * echelle)) };
}

async function ouvrirImage(fichier: Blob): Promise<CanvasImageSource & { width: number; height: number }> {
  if (typeof createImageBitmap === "function") return createImageBitmap(fichier);
  const url = URL.createObjectURL(fichier);
  try {
    const img = new Image();
    img.src = url;
    await img.decode();
    return img;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function versBlob(canvas: HTMLCanvasElement, type: string, qualite?: number): Promise<Blob> {
  return new Promise((ok, ko) =>
    canvas.toBlob((b) => (b ? ok(b) : ko(new ImageRefusee("Cette image n'a pas pu être préparée."))), type, qualite),
  );
}

function versBase64(b: Blob): Promise<string> {
  return new Promise((ok, ko) => {
    const lecteur = new FileReader();
    lecteur.onload = () => ok(String(lecteur.result).replace(/^data:[^,]*,/, ""));
    lecteur.onerror = () => ko(new ImageRefusee("Cette image n'a pas pu être lue."));
    lecteur.readAsDataURL(b);
  });
}

/**
 * Prépare l'image choisie : réduite à 600 pixels au plus, en PNG (transparence
 * gardée), ou en JPEG si elle reste trop lourde.
 */
export async function preparerLogo(fichier: File): Promise<NouveauLogo> {
  if (!TYPES_ACCEPTES.includes(fichier.type)) {
    throw new ImageRefusee("Ce fichier n'est pas une image PNG ou JPEG. Choisissez une autre image.");
  }
  let image: Awaited<ReturnType<typeof ouvrirImage>>;
  try {
    image = await ouvrirImage(fichier);
  } catch {
    throw new ImageRefusee("Cette image n'a pas pu être ouverte. Elle est peut-être abîmée : choisissez-en une autre.");
  }
  const { largeur, hauteur } = tailleReduite(image.width, image.height);
  const canvas = document.createElement("canvas");
  canvas.width = largeur;
  canvas.height = hauteur;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new ImageRefusee("Cette image n'a pas pu être préparée.");
  ctx.drawImage(image, 0, 0, largeur, hauteur);
  const png = await versBlob(canvas, "image/png");
  if (png.size <= OCTETS_PNG_MAX) return { format: "png", donnees: await versBase64(png) };
  // Trop lourde en PNG : JPEG sur fond blanc (le JPEG n'a pas de transparence).
  ctx.globalCompositeOperation = "destination-over";
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, largeur, hauteur);
  return { format: "jpeg", donnees: await versBase64(await versBlob(canvas, "image/jpeg", 0.85)) };
}
