import { Printer } from "lucide-react";

import type { CodesDistance } from "../bindings/CodesDistance";
import { Alerte, Bouton } from "./ui";

/** Les trois informations à noter sur papier pour consulter l'hôtel à distance. */
export function CodesANoter({ codes, onMasquer }: { codes: CodesDistance; onMasquer: () => void }) {
  const lignes: [string, string][] = [
    ["Adresse du serveur", codes.relais],
    ["Identifiant de l'hôtel", codes.hotel_id],
    ["Clé secrète", codes.cle_secrete],
  ];
  return (
    <div className="grid gap-4">
      <Alerte type="attention" titre="Notez ces trois informations sur papier">
        Elles permettent de consulter l'hôtel depuis un autre ordinateur, même à l'étranger. Gardez-les à l'abri :
        avec elles, on peut voir les comptes de l'hôtel (mais rien modifier).
      </Alerte>
      <dl data-testid="codes-distance" className="grid gap-3 rounded-xl border-2 border-dashed border-or bg-or-clair p-5">
        {lignes.map(([libelle, valeur]) => (
          <div key={libelle}>
            <dt className="text-base font-semibold text-gris">{libelle}</dt>
            <dd className="selectionnable font-mono text-2xl font-bold tracking-wider break-all">{valeur}</dd>
          </div>
        ))}
      </dl>
      <div className="flex flex-wrap justify-end gap-3">
        <Bouton variante="secondaire" icone={Printer} onClick={() => window.print()}>
          Imprimer
        </Bouton>
        <Bouton onClick={onMasquer}>J'ai noté, masquer</Bouton>
      </div>
    </div>
  );
}
