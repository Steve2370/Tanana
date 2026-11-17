import { useEffect, useRef, useState, type ReactNode } from "react";

import { pluriel } from "../../lib/dates";
import { aujourdhui } from "../../lib/format";
import { FicheReservation } from "./FicheReservation";
import { NouvelleReservation } from "./NouvelleReservation";
import { Tableau } from "./Tableau";

type Vue =
  | { type: "tableau" }
  | { type: "nouvelle"; chambreId?: string; arrivee?: string }
  | { type: "fiche"; id: string; message?: string };

/** Écran Réservations : le tableau, puis une réservation à la fois (pas de fenêtres empilées). */
export function Reservations() {
  const [vue, setVue] = useState<Vue>({ type: "tableau" });
  // Gardé ici pour retrouver le tableau au même endroit en revenant d'une fiche.
  const [debut, setDebut] = useState(aujourdhui());
  const retour = () => setVue({ type: "tableau" });

  // En changeant de vue, on repart toujours du haut de l'écran.
  const haut = useRef<HTMLDivElement>(null);
  useEffect(() => {
    haut.current?.closest("main")?.scrollTo?.({ top: 0 });
  }, [vue]);

  let contenu: ReactNode;
  if (vue.type === "nouvelle") {
    contenu = (
      <NouvelleReservation
        chambreInitiale={vue.chambreId}
        arriveeInitiale={vue.arrivee}
        onRetour={retour}
        onCree={(r) =>
          setVue({
            type: "fiche",
            id: r.id,
            message: `La réservation de ${r.client_nom} est enregistrée (chambre ${r.chambre_numero}, ${pluriel(r.nuits, "nuit")}). Si le client verse un acompte, encaissez-le ci-dessous.`,
          })
        }
      />
    );
  } else if (vue.type === "fiche") {
    contenu = <FicheReservation key={vue.id} id={vue.id} messageInitial={vue.message} onRetour={retour} />;
  } else {
    contenu = (
      <Tableau
        debut={debut}
        onDebut={setDebut}
        onNouvelle={(chambreId, arrivee) => setVue({ type: "nouvelle", chambreId, arrivee })}
        onOuvrir={(id) => setVue({ type: "fiche", id })}
      />
    );
  }
  return (
    <>
      <div ref={haut} />
      {contenu}
    </>
  );
}
