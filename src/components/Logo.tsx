import logoComplet from "../assets/logo-complet.svg";
import logoSymbole from "../assets/logo-symbole.svg";

/** Logo complet (symbole + « Tanana »), pour les écrans d'accueil. */
export function LogoComplet({ hauteur = 140 }: { hauteur?: number }) {
  return <img src={logoComplet} alt="Tanana" style={{ height: hauteur }} className="mx-auto w-auto" draggable={false} />;
}

/** Symbole seul (les deux mains), pour le menu. */
export function LogoSymbole({ taille = 44 }: { taille?: number }) {
  return <img src={logoSymbole} alt="" aria-hidden style={{ height: taille }} className="w-auto" draggable={false} />;
}
