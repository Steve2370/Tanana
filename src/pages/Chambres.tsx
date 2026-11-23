import { useCallback, useEffect, useState, type FormEvent } from "react";

import type { Chambre } from "../bindings/Chambre";
import type { StatutChambre } from "../bindings/StatutChambre";
import type { TypeChambre } from "../bindings/TypeChambre";
import { Alerte, Bouton, Carte, Champ, EnTete, Pastille, Selection, useAction } from "../components/ui";
import { api } from "../lib/api";
import { versErreur } from "../lib/erreurs";
import { formatMontant, lireMontant } from "../lib/format";
import { SiPermis, useSession } from "../lib/session";

const STATUTS: { valeur: StatutChambre; libelle: string }[] = [
  { valeur: "disponible", libelle: "Peut être louée" },
  { valeur: "maintenance", libelle: "En maintenance" },
  { valeur: "hors_service", libelle: "Hors service" },
];

export function Chambres() {
  const { parametres } = useSession();
  const devise = { code: parametres?.devise ?? "XAF", decimales: parametres?.decimales_devise ?? 0 };
  const [types, setTypes] = useState<TypeChambre[]>([]);
  const [chambres, setChambres] = useState<Chambre[]>([]);
  const [erreur, setErreur] = useState<string | null>(null);

  const charger = useCallback(async () => {
    try {
      const [t, c] = await Promise.all([api.listerTypesChambre(), api.listerChambres()]);
      setTypes(t);
      setChambres(c);
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  }, []);

  useEffect(() => {
    void charger();
  }, [charger]);

  const changerStatut = async (c: Chambre, statut: StatutChambre) => {
    try {
      setErreur(null);
      await api.modifierChambre(c.id, { numero: c.numero, type_id: c.type_id, etage: c.etage, statut, notes: c.notes });
      await charger();
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  };

  return (
    <>
      <EnTete titre="Chambres" sousTitre={`${chambres.length} chambre${chambres.length > 1 ? "s" : ""}. Pour chaque chambre : son type, son prix par nuit et si elle peut être louée.`} />
      {erreur && <div className="mb-4"><Alerte type="erreur">{erreur}</Alerte></div>}
      {/* Deux colonnes seulement si la place suffit (le tableau a besoin d'environ 45 rem) ;
          sinon les formulaires passent dessous. */}
      <div className="grid gap-6 2xl:grid-cols-[minmax(0,1fr)_24rem]">
        <Carte titre="Chambres">
          {chambres.length === 0 ? (
            <p className="text-lg text-gris">Aucune chambre pour le moment. Commencez par créer un type de chambre (« Nouveau type de chambre »), puis ajoutez vos chambres.</p>
          ) : (
            // Jamais de débordement sur la colonne voisine : au pire, le tableau défile dans sa carte.
            <div className="-mx-1 overflow-x-auto px-1">
              <table className="w-full text-lg">
                <thead className="text-left text-base text-gris">
                  <tr className="border-b border-trait">
                    <th className="py-2 pr-4 font-semibold">Numéro</th>
                    <th className="py-2 pr-4 font-semibold">Type</th>
                    <th className="py-2 pr-4 font-semibold">Prix par nuit</th>
                    <th className="py-2 font-semibold">État</th>
                  </tr>
                </thead>
                <tbody>
                  {chambres.map((c) => (
                    <tr key={c.id} className="border-b border-trait/60">
                      <td className="py-3 pr-4 font-medium">{c.numero}</td>
                      <td className="py-3 pr-4">{c.type_nom}</td>
                      <td className="py-3 pr-4 whitespace-nowrap">{formatMontant(c.prix_nuit_defaut, devise)}</td>
                      <td className="py-3">
                        <SiPermis permission="chambre.gerer">
                          <select
                            aria-label={`Statut de la chambre ${c.numero}`}
                            value={c.statut}
                            onChange={(e) => void changerStatut(c, e.target.value as StatutChambre)}
                            className="min-h-11 rounded-lg border-2 border-bordure-champ/60 bg-white px-2 text-base"
                          >
                            {STATUTS.map((s) => (
                              <option key={s.valeur} value={s.valeur}>
                                {s.libelle}
                              </option>
                            ))}
                          </select>
                        </SiPermis>
                        <LectureSeule statut={c.statut} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Carte>
        <SiPermis permission="chambre.gerer">
          <div className="grid content-start gap-6">
            <NouveauType decimales={devise.decimales} devise={devise.code} onCree={charger} />
            {types.length > 0 && <NouvelleChambre types={types} onCree={charger} />}
          </div>
        </SiPermis>
      </div>
    </>
  );
}

function LectureSeule({ statut }: { statut: StatutChambre }) {
  const { peut } = useSession();
  if (peut("chambre.gerer")) return null;
  const ton = statut === "disponible" ? "succes" : statut === "maintenance" ? "alerte" : "danger";
  return <Pastille ton={ton}>{STATUTS.find((s) => s.valeur === statut)?.libelle}</Pastille>;
}

function NouveauType({ decimales, devise, onCree }: { decimales: number; devise: string; onCree: () => Promise<void> }) {
  const [nom, setNom] = useState("");
  const [prix, setPrix] = useState("");
  const [capacite, setCapacite] = useState("2");
  const { lancer, enCours, erreur, setErreur } = useAction(api.creerTypeChambre);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    const montant = lireMontant(prix, decimales);
    if (montant === null) return setErreur("Le prix n'est pas lisible. Écrivez seulement des chiffres, par exemple 25000.");
    if (await lancer({ nom, prix_nuit_defaut: montant, capacite: Number(capacite) })) {
      setNom("");
      setPrix("");
      await onCree();
    }
  };

  return (
    <Carte titre="Nouveau type de chambre">
      <form onSubmit={soumettre} className="grid gap-3">
        <Champ libelle="Nom du type" value={nom} onChange={(e) => setNom(e.target.value)} placeholder="Standard, Suite…" required />
        <div className="grid grid-cols-2 gap-3">
          <Champ libelle="Prix par nuit" value={prix} onChange={(e) => setPrix(e.target.value)} inputMode="decimal" aide={`En ${devise}`} required />
          <Champ libelle="Personnes" type="number" min={1} value={capacite} onChange={(e) => setCapacite(e.target.value)} required />
        </div>
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        <Bouton type="submit" enCours={enCours}>
          Ajouter le type
        </Bouton>
      </form>
    </Carte>
  );
}

function NouvelleChambre({ types, onCree }: { types: TypeChambre[]; onCree: () => Promise<void> }) {
  const [numero, setNumero] = useState("");
  const [etage, setEtage] = useState("");
  const [typeId, setTypeId] = useState(types[0]?.id ?? "");
  const { lancer, enCours, erreur } = useAction(api.creerChambre);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    const c = await lancer({
      numero,
      type_id: typeId,
      etage: etage === "" ? null : Number(etage),
      statut: "disponible",
      notes: null,
    });
    if (c) {
      setNumero("");
      await onCree();
    }
  };

  return (
    <Carte titre="Nouvelle chambre">
      <form onSubmit={soumettre} className="grid gap-3">
        <div className="grid grid-cols-2 gap-3">
          <Champ libelle="Numéro" value={numero} onChange={(e) => setNumero(e.target.value)} required />
          <Champ libelle="Étage" type="number" value={etage} onChange={(e) => setEtage(e.target.value)} />
        </div>
        <Selection libelle="Type" valeur={typeId} onChange={setTypeId} options={types.map((t) => ({ valeur: t.id, libelle: t.nom }))} />
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        <Bouton type="submit" enCours={enCours}>
          Ajouter la chambre
        </Bouton>
      </form>
    </Carte>
  );
}
