import { useCallback, useEffect, useState, type FormEvent } from "react";
import { Ban, KeyRound, RotateCcw, UserPlus, UserX } from "lucide-react";

import type { Permission } from "../bindings/Permission";
import type { PermissionPerso } from "../bindings/PermissionPerso";
import type { Role } from "../bindings/Role";
import type { StatutUtilisateur } from "../bindings/StatutUtilisateur";
import type { Utilisateur } from "../bindings/Utilisateur";
import {
  Alerte,
  Bouton,
  Carte,
  Champ,
  ChampMotDePasse,
  EnTete,
  Pastille,
  ReglesMotDePasse,
  Selection,
  motDePasseValide,
  useAction,
  useConfirmation,
} from "../components/ui";
import { api } from "../lib/api";
import { versErreur } from "../lib/erreurs";
import { formatHorodatage } from "../lib/format";
import { GROUPES_PERMISSIONS, LIBELLES_PERMISSIONS } from "../lib/permissions";
import { useLectureSeule, useSession } from "../lib/session";

const LIBELLE_STATUT: Record<StatutUtilisateur, string> = {
  actif: "Actif",
  suspendu: "Suspendu",
  desactive: "Désactivé",
};

export function Utilisateurs() {
  const lectureSeule = useLectureSeule();
  const [utilisateurs, setUtilisateurs] = useState<Utilisateur[]>([]);
  const [roles, setRoles] = useState<Role[]>([]);
  const [selection, setSelection] = useState<string | null>(null);
  const [creation, setCreation] = useState(false);
  const [erreur, setErreur] = useState<string | null>(null);
  const [nouveau, setNouveau] = useState<Utilisateur | null>(null);

  const charger = useCallback(async () => {
    try {
      const [u, r] = await Promise.all([api.listerUtilisateurs(), api.listerRoles()]);
      setUtilisateurs(u);
      setRoles(r);
    } catch (e) {
      setErreur(versErreur(e).message);
    }
  }, []);

  useEffect(() => {
    void charger();
  }, [charger]);

  const choisi = utilisateurs.find((u) => u.id === selection) ?? null;

  return (
    <>
      <EnTete
        titre="Employés et accès"
        sousTitre={
          lectureSeule
            ? "Les comptes des employés de l'hôtel et ce que chacun peut faire."
            : "Créez un compte pour chaque employé, choisissez ce qu'il peut faire, et bloquez un accès en un clic."
        }
        actions={
          !creation &&
          !lectureSeule && (
            <Bouton
              icone={UserPlus}
              onClick={() => {
                setNouveau(null);
                setCreation(true);
              }}
              grand
            >
              Ajouter un employé
            </Bouton>
          )
        }
      />
      {erreur && (
        <div className="mb-5">
          <Alerte type="erreur">{erreur}</Alerte>
        </div>
      )}
      {nouveau && (
        <div className="mb-5">
          <Alerte type="succes" titre={`Le compte de ${nouveau.nom} est créé.`}>
            Donnez-lui son identifiant « {nouveau.identifiant} » et le mot de passe provisoire. Il choisira son propre mot
            de passe à sa première connexion.
          </Alerte>
        </div>
      )}
      {creation && (
        <NouveauCompte
          roles={roles}
          onFini={async (cree) => {
            setCreation(false);
            setNouveau(cree ?? null);
            await charger();
            if (cree) setSelection(cree.id);
          }}
        />
      )}
      <div className="grid grid-cols-[1fr_1.25fr] gap-6">
        <Carte titre={`${utilisateurs.length} compte${utilisateurs.length > 1 ? "s" : ""}`}>
          <ul className="grid gap-1.5">
            {utilisateurs.map((u) => (
              <li key={u.id}>
                <button
                  type="button"
                  onClick={() => setSelection(u.id)}
                  aria-pressed={selection === u.id}
                  className={`flex min-h-16 w-full items-center justify-between gap-3 rounded-xl border-2 px-4 py-2 text-left transition-colors ${
                    selection === u.id ? "border-marque bg-marque-clair" : "border-transparent hover:bg-sable"
                  }`}
                >
                  <span>
                    <span className="text-lg font-bold">{u.nom}</span>
                    <span className="block text-base text-gris">
                      {u.role_nom} · identifiant « {u.identifiant} »
                    </span>
                  </span>
                  <Pastille ton={u.statut === "actif" ? "succes" : u.statut === "suspendu" ? "alerte" : "neutre"}>
                    {LIBELLE_STATUT[u.statut]}
                  </Pastille>
                </button>
              </li>
            ))}
          </ul>
        </Carte>
        {choisi ? (
          <FicheCompte key={choisi.id} utilisateur={choisi} roles={roles} onChange={charger} />
        ) : (
          <Carte>
            <p className="text-lg text-gris">
              Cliquez sur un employé, à gauche, pour voir {lectureSeule ? "ses accès" : "et modifier ses accès"}.
            </p>
          </Carte>
        )}
      </div>
    </>
  );
}

function NouveauCompte({ roles, onFini }: { roles: Role[]; onFini: (cree?: Utilisateur) => void }) {
  const [identifiant, setIdentifiant] = useState("");
  const [nom, setNom] = useState("");
  const [mdp, setMdp] = useState("");
  const [roleId, setRoleId] = useState("role-reception");
  const { lancer, enCours, erreur, setErreur } = useAction(api.creerUtilisateur);

  const soumettre = async (e: FormEvent) => {
    e.preventDefault();
    if (!motDePasseValide(mdp)) return setErreur("Le mot de passe provisoire ne respecte pas encore toutes les règles.");
    const u = await lancer({ identifiant, nom, mot_de_passe_temporaire: mdp, role_id: roleId });
    if (u) onFini(u);
  };

  return (
    <Carte titre="Nouvel employé" className="mb-6">
      <form onSubmit={soumettre} className="grid gap-5" noValidate>
        <div className="grid grid-cols-2 gap-5">
          <Champ libelle="Nom complet" value={nom} onChange={(e) => setNom(e.target.value)} required />
          <Champ
            libelle="Identifiant"
            value={identifiant}
            onChange={(e) => setIdentifiant(e.target.value)}
            aide="Ce qu'il tapera pour se connecter (ex. : marie)."
            autoCapitalize="off"
            spellCheck={false}
            required
          />
          <ChampMotDePasse
            libelle="Mot de passe provisoire"
            value={mdp}
            onChange={(e) => setMdp(e.target.value)}
            autoComplete="new-password"
            aide={
              <>
                <ReglesMotDePasse motDePasse={mdp} />
                <p className="mt-1">L'employé devra le changer à sa première connexion.</p>
              </>
            }
          />
          <Selection
            libelle="Rôle"
            valeur={roleId}
            onChange={setRoleId}
            options={roles.map((r) => ({ valeur: r.id, libelle: r.nom }))}
            aide="Le rôle donne un ensemble d'accès. Vous pourrez l'ajuster ensuite."
          />
        </div>
        {erreur && <Alerte type="erreur">{erreur}</Alerte>}
        <div className="flex justify-end gap-3">
          <Bouton variante="secondaire" onClick={() => onFini()}>
            Annuler
          </Bouton>
          <Bouton type="submit" enCours={enCours} grand>
            Créer le compte
          </Bouton>
        </div>
      </form>
    </Carte>
  );
}

function FicheCompte({
  utilisateur: u,
  roles,
  onChange,
}: {
  utilisateur: Utilisateur;
  roles: Role[];
  onChange: () => Promise<void>;
}) {
  const { session } = useSession();
  const lectureSeule = useLectureSeule();
  const { confirmer, fenetre } = useConfirmation();
  const soiMeme = session?.utilisateur.id === u.id;
  /** Pas de boutons de modification : son propre compte, ou consultation à distance. */
  const fige = soiMeme || lectureSeule;
  const [perso, setPerso] = useState<PermissionPerso[]>([]);
  const [effectives, setEffectives] = useState<Permission[]>([]);
  const [mdpTemp, setMdpTemp] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const rechargerPermissions = useCallback(async () => {
    const [p, e] = await Promise.all([api.permissionsPerso(u.id), api.permissionsUtilisateur(u.id)]);
    setPerso(p);
    setEffectives(e);
  }, [u.id]);

  const action = useAction(async (f: () => Promise<unknown>, succes: string) => {
    setMessage(null);
    await f();
    setMessage(succes);
    await onChange();
    await rechargerPermissions();
  });

  useEffect(() => {
    void rechargerPermissions().catch(() => {});
  }, [rechargerPermissions]);

  const estAdmin = u.role_id === "role-admin";
  const exception = (p: Permission) => perso.find((x) => x.permission === p)?.accorde ?? null;

  const changerStatut = async (statut: StatutUtilisateur) => {
    const textes = {
      suspendu: {
        titre: `Suspendre le compte de ${u.nom} ?`,
        message: "Cette personne sera déconnectée tout de suite et ne pourra plus se connecter. Vous pourrez la réactiver plus tard.",
        bouton: "Oui, suspendre",
        succes: `Le compte de ${u.nom} est suspendu. Cette personne a été déconnectée.`,
      },
      desactive: {
        titre: `Désactiver le compte de ${u.nom} ?`,
        message: "À utiliser quand l'employé quitte l'hôtel. Son historique est conservé. Vous pourrez le réactiver si besoin.",
        bouton: "Oui, désactiver",
        succes: `Le compte de ${u.nom} est désactivé.`,
      },
      actif: {
        titre: `Réactiver le compte de ${u.nom} ?`,
        message: "Cette personne pourra de nouveau se connecter avec son mot de passe.",
        bouton: "Oui, réactiver",
        succes: `Le compte de ${u.nom} est réactivé.`,
      },
    }[statut];
    const ok = await confirmer({
      titre: textes.titre,
      message: textes.message,
      libelleConfirmer: textes.bouton,
      danger: statut !== "actif",
    });
    if (ok) await action.lancer(() => api.changerStatutUtilisateur(u.id, statut), textes.succes);
  };

  const changerRole = async (roleId: string) => {
    const role = roles.find((r) => r.id === roleId);
    const ok = await confirmer({
      titre: `Changer le rôle de ${u.nom} ?`,
      message: `${u.nom} deviendra « ${role?.nom} ». Ses accès changeront immédiatement.`,
      libelleConfirmer: "Oui, changer le rôle",
    });
    if (ok) await action.lancer(() => api.changerRole(u.id, roleId), `${u.nom} est maintenant « ${role?.nom} ».`);
  };

  const reinitialiser = async () => {
    if (!motDePasseValide(mdpTemp)) return action.setErreur("Le mot de passe provisoire ne respecte pas encore toutes les règles.");
    const ok = await confirmer({
      titre: `Donner un nouveau mot de passe à ${u.nom} ?`,
      message: "Cette personne sera déconnectée et devra choisir son propre mot de passe à sa prochaine connexion. Son code PIN sera effacé.",
      libelleConfirmer: "Oui, réinitialiser",
    });
    if (!ok) return;
    await action.lancer(
      () => api.reinitialiserMotDePasse(u.id, mdpTemp),
      `Mot de passe provisoire enregistré. Donnez-le à ${u.nom} : il devra le changer en se connectant.`,
    );
    setMdpTemp("");
  };

  return (
    <Carte titre={u.nom}>
      {fenetre}
      <div className="grid gap-6">
        <p className="text-base text-gris">
          Compte créé {formatHorodatage(u.cree_le)}. Dernière connexion :{" "}
          {u.derniere_connexion ? formatHorodatage(u.derniere_connexion) : "jamais"}.
        </p>
        {soiMeme && (
          <Alerte type="info">C'est votre propre compte. Pour le modifier, il faut passer par un autre administrateur.</Alerte>
        )}
        {message && <Alerte type="succes">{message}</Alerte>}
        {action.erreur && <Alerte type="erreur">{action.erreur}</Alerte>}

        {!fige && (
          <>
            <Selection
              libelle="Rôle"
              valeur={u.role_id}
              onChange={(r) => void changerRole(r)}
              options={roles.map((r) => ({ valeur: r.id, libelle: r.nom }))}
            />

            <div>
              <p className="mb-2 font-semibold">Accès au compte</p>
              <div className="flex flex-wrap gap-3">
                {u.statut !== "actif" && (
                  <Bouton icone={RotateCcw} onClick={() => void changerStatut("actif")}>
                    Réactiver
                  </Bouton>
                )}
                {u.statut === "actif" && (
                  <Bouton variante="danger" icone={Ban} onClick={() => void changerStatut("suspendu")}>
                    Suspendre
                  </Bouton>
                )}
                {u.statut !== "desactive" && (
                  <Bouton variante="secondaire" icone={UserX} onClick={() => void changerStatut("desactive")}>
                    Désactiver (départ de l'hôtel)
                  </Bouton>
                )}
              </div>
            </div>

            <div className="rounded-xl border-2 border-trait p-4">
              <p className="mb-3 font-semibold">Mot de passe oublié par l'employé ?</p>
              <ChampMotDePasse
                libelle="Nouveau mot de passe provisoire"
                value={mdpTemp}
                onChange={(e) => setMdpTemp(e.target.value)}
                autoComplete="new-password"
                aide={mdpTemp && <ReglesMotDePasse motDePasse={mdpTemp} />}
              />
              <Bouton className="mt-3" variante="secondaire" icone={KeyRound} disabled={!mdpTemp} onClick={() => void reinitialiser()}>
                Réinitialiser le mot de passe
              </Bouton>
            </div>
          </>
        )}

        <div>
          <p className="mb-1 text-lg font-bold">Ce que {soiMeme ? "vous pouvez" : "cette personne peut"} faire</p>
          {lectureSeule && <p className="mb-3 text-gris">Rôle : {u.role_nom}.</p>}
          {estAdmin ? (
            <p className="mb-3 text-gris">Un administrateur peut tout faire.</p>
          ) : (
            !fige && (
              <p className="mb-3 text-gris">
                « Comme son rôle » suit le rôle choisi. « Autorisé » ou « Interdit » fait une exception pour cette
                personne seulement.
              </p>
            )
          )}
          {GROUPES_PERMISSIONS.map((g) => (
            <fieldset key={g.titre} className="mb-4">
              <legend className="mb-1 text-sm font-bold tracking-wide text-gris uppercase">{g.titre}</legend>
              {g.permissions.map((p) => {
                const ok = effectives.includes(p);
                return (
                  <div key={p} className="flex min-h-12 items-center justify-between gap-3 border-b border-trait/70 py-1">
                    <span className="flex items-center gap-2.5">
                      <span
                        className={`inline-flex size-6 shrink-0 items-center justify-center rounded-full text-sm font-bold ${ok ? "bg-succes-clair text-succes" : "bg-sable text-gris"}`}
                        aria-hidden
                      >
                        {ok ? "✓" : "-"}
                      </span>
                      <span className={ok ? "" : "text-gris"}>
                        {LIBELLES_PERMISSIONS[p]}
                        <span className="sr-only">{ok ? " : oui" : " : non"}</span>
                      </span>
                    </span>
                    {!estAdmin && !fige && (
                      <select
                        aria-label={`Exception pour ${LIBELLES_PERMISSIONS[p]}`}
                        value={String(exception(p))}
                        onChange={(e) => {
                          const v = e.target.value === "null" ? null : e.target.value === "true";
                          void action.lancer(() => api.definirPermissionPerso(u.id, p, v), "Accès mis à jour.");
                        }}
                        className="min-h-11 rounded-lg border-2 border-bordure-champ/60 bg-white px-2 text-base"
                      >
                        <option value="null">Comme son rôle</option>
                        <option value="true">Autorisé</option>
                        <option value="false">Interdit</option>
                      </select>
                    )}
                  </div>
                );
              })}
            </fieldset>
          ))}
        </div>
      </div>
    </Carte>
  );
}
