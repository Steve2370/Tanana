"""Mutants d'écran : (id, fichier, avant, après, tests qui couvrent l'écran, description)."""
R = "src/components/restauration.tsx"
M = "src/components/miseAJour.tsx"
C = "src/pages/MonCompte.tsx"
X = "src/pages/Connexion.tsx"
Q = "src/pages/Coquille.tsx"
T_R = ["src/components/restauration.test.tsx"]
T_M = ["src/components/miseAJour.test.tsx"]
T_C = ["src/pages/MonCompte.test.tsx"]
T_X = ["src/pages/Connexion.test.tsx"]
T_Q = ["src/pages/MonCompte.test.tsx", "src/pages/Administration.test.tsx", "src/components/miseAJour.test.tsx", "src/pages/distance/ConsultationDistante.test.tsx"]

MUTANTS = [
  ("M01", R, 'if (hotelInstalle && !mdp) return', 'if (false && !mdp) return', T_R, "Restauration : le mot de passe administrateur n'est plus exigé à l'écran"),
  ("M02", R, 'return err.code === "VALIDATION" && /clé/.test(err.message);', 'return false;', T_R, "Restauration : la clé d'un autre ordinateur n'est jamais demandée"),
  ("M03", R, 'if (cle.replace(/[\\s-]/g, "").length !== 64)', 'if (cle.replace(/[\\s-]/g, "").length < 3)', T_R, "Restauration : longueur de la clé plus vérifiée"),
  ("M04", R, 'icone={History} enCours={remise.enCours} grand>', 'icone={History} grand>', T_R, "Restauration : « Remettre cette copie » cliquable deux fois pendant le traitement"),
  ("M05", R, '        examen.setErreur(versErreur(e).message);\n      }', '      }', T_R, "Restauration : une copie refusée ne dit rien (état d'erreur perdu)"),
  ("M06", R, 'if (s.nom.startsWith("tanana-avant-mise-a-jour-")) return `État d\'avant une mise à jour, ${quand}`;', '', T_R, "Restauration : la copie « avant mise à jour » se présente comme une copie automatique"),
  ("M07", M, '      await api.sauvegarderAvantMiseAJour(maj.version);\n', '', T_M, "Mise à jour : plus de copie de sécurité avant d'installer"),
  ("M08", M, '<Bouton icone={Download} onClick={() => setEtape({ nom: "confirmation" })}>', '<Bouton icone={Download} onClick={() => void installer()}>', T_M, "Mise à jour : installe sans demander confirmation"),
  ("M09", M, '  if (!maj || cachee) return null;', '  if (!maj) return null;', T_M, "Mise à jour : « Plus tard » ne cache plus le bandeau"),
  ("M10", Q, '{(lectureSeule || peut("parametres.gerer")) && (', '{true && (', T_Q, "Coquille : la réception voit les mises à jour"),
  ("M11", C, 'f(chiffres ? v.replace(/\\D/g, "").slice(0, 6) : v);', 'f(v);', T_C, "Mon compte : le PIN accepte des lettres et plus de 6 caractères"),
  ("M12", C, '    f(v);\n    setOk(false);\n    setErreur(null);', '    f(v);\n    setErreur(null);', T_C, "Mon compte : « C'est enregistré » reste affiché quand on retape un mot de passe"),
  ("M13", C, '      setOk(true);\n      setActuel("");\n      setNouveau("");\n      setConfirmation("");', '      setOk(true);', T_C, "Mon compte : les mots de passe restent dans les champs après l'enregistrement"),
  ("M14", X, '        e.preventDefault(); // sinon l\'onglet encore sélectionné se réactive et efface le PIN\n', '', T_X, "Connexion : Entrée sur l'onglet PIN réactive l'onglet et efface la saisie"),
  ("M15", Q, '              aria-current={courante === "compte" ? "page" : undefined}', '', T_Q, "Coquille : « Mon compte » n'est plus annoncé comme page courante"),
]
