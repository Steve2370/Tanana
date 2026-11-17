/**
 * Outils du fuzzing : graine reproductible, valeurs hostiles, surveillance des erreurs.
 * Rejouer un échec : FUZZ_SEED=<graine> npm run qa:fuzz
 * Nouvelle série au hasard : FUZZ_SEED=hasard npm run qa:fuzz
 */

/**
 * Graine lue dans FUZZ_SEED ; fixe par défaut (pour que `npm test` donne toujours
 * le même résultat) ; FUZZ_SEED=hasard en tire une nouvelle. Toujours affichée
 * dans les messages d'échec, pour rejouer exactement la même série.
 */
export function graine(): number {
  const valeur = process.env.FUZZ_SEED;
  if (valeur === "hasard") return 1 + Math.floor(Math.random() * 2 ** 31);
  const lue = Number(valeur);
  return Number.isInteger(lue) && lue > 0 ? lue : 20_261_115;
}

/** Générateur pseudo-aléatoire déterministe (mulberry32). */
export function aleatoire(seed: number) {
  let a = seed >>> 0;
  const suivant = () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  return {
    suivant,
    entier: (min: number, max: number) => min + Math.floor(suivant() * (max - min + 1)),
    parmi: <T>(liste: readonly T[]): T => liste[Math.floor(suivant() * liste.length)]!,
  };
}

/** Saisies hostiles ou maladroites, typiques d'une réception d'hôtel. */
export const HOSTILES: readonly string[] = [
  "",
  "   ",
  "a".repeat(10_000),
  "Anticonstitutionnellement-Rakotoarimanana",
  "Rakotoarimananaandriamampianina",
  "👨‍👩‍👧‍👦🏨✨",
  "‮exe.ecnerefni",
  "<img src=x onerror=alert(1)>",
  "<script>alert(1)</script>",
  "'; DROP TABLE clients;--",
  "ligne 1\nligne 2",
  "١٢٣٤",
  "-5000",
  "1e9",
  "9".repeat(30),
  "0",
  "10,5",
  "10.000,50",
  "NaN",
  "  45 000  ",
  "Éléonore D'Arc-Ngô",
];

/** Signes que du texte hostile a été interprété comme du HTML. */
export function injection(): string | null {
  if (document.querySelector('img[src="x"]')) return "balise <img> injectée";
  if ([...document.querySelectorAll("script")].some((s) => s.textContent?.includes("alert(1)"))) return "balise <script> injectée";
  return null;
}

/** Capture les erreurs JavaScript et les erreurs React (console.error) pendant un test. */
export function surveiller() {
  const erreurs: string[] = [];
  const origine = console.error;
  const surErreur = (e: ErrorEvent) => erreurs.push(`exception : ${e.message}`);
  const surRejet = (e: PromiseRejectionEvent) => erreurs.push(`promesse rejetée : ${String(e.reason)}`);
  console.error = (...args: unknown[]) => {
    const texte = args.map(String).join(" ");
    // Bruit connu de jsdom, sans rapport avec l'application
    if (/Not implemented: (navigation|HTMLCanvasElement)|act\(/.test(texte)) return;
    erreurs.push(`console.error : ${texte.slice(0, 300)}`);
  };
  window.addEventListener("error", surErreur);
  window.addEventListener("unhandledrejection", surRejet);
  return {
    erreurs,
    arreter() {
      console.error = origine;
      window.removeEventListener("error", surErreur);
      window.removeEventListener("unhandledrejection", surRejet);
    },
  };
}
