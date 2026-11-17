"""Série 3 : le reste de l'application (démarrage, comptes, déconnexion auto, historique, clients, taxes, distance, saisie)."""
import glob
TOUT = sorted(f for f in glob.glob("src/**/*.test.ts*", recursive=True) if not f.endswith(("coherence.test.ts", "api.test.ts")))
MUTANTS = [
  ("P01", "src/pages/ChangementMotDePasse.tsx", '    if (nouveau !== confirmation) return setErreur("Les deux nouveaux mots de passe ne sont pas identiques. Retapez-les.");\n', '', ["src/pages/Connexion.test.tsx"], "Mot de passe imposé : confirmation différente acceptée"),
  ("P02", "src/pages/PremierLancement.tsx", '    if (mdp !== confirmation) return setErreur("Les deux mots de passe ne sont pas identiques. Retapez-les.");\n', '', ["src/pages/Demarrage.test.tsx"], "Premier lancement : confirmation différente acceptée"),
  ("P03", "src/pages/PremierLancement.tsx", '<Bouton disabled={!codeNote} onClick={onTermine} grand>', '<Bouton onClick={onTermine} grand>', ["src/pages/Demarrage.test.tsx"], "Premier lancement : on continue sans avoir noté le code de secours"),
  ("P04", "src/pages/CleManquante.tsx", ' disabled={cle.trim().length === 0}', '', ["src/pages/Demarrage.test.tsx"], "Clé manquante : bouton actif avec un champ vide"),
  ("P05", "src/pages/clients/FormulaireClient.tsx", '    if (!d.nom.trim()) return setErreurLocale("Écrivez le nom du client.");\n', '', ["src/pages/clients/Clients.test.tsx", "src/pages/reservations/NouvelleReservation.test.tsx"], "Client sans nom accepté à l'écran"),
  ("P06", "src/lib/session.tsx", '      if (avertissementVisible.current) return;\n', '', TOUT, "Inactivité : un geste involontaire efface l'avertissement"),
  ("P07", "src/lib/session.tsx", '      if (reste <= 0) {', '      if (false) {', TOUT, "Inactivité : plus jamais de déconnexion automatique"),
  ("P08", "src/lib/taxes.ts", '    if (valeur > 10_000) return { erreur: "le pourcentage ne peut pas dépasser 100 %." };\n', '', ["src/lib/taxes.test.ts", "src/pages/Parametres.test.tsx", "src/pages/reservations/FicheReservation.test.tsx"], "Taxe à plus de 100 % acceptée"),
  ("P09", "src/pages/Journal.tsx", '          sensibles_seulement: sensibles,', '          sensibles_seulement: false,', TOUT, "Historique : le filtre « à vérifier » ne filtre plus"),
  ("P10", "src/pages/reservations/Tableau.tsx", 'onClick={() => onDebut(ajouterJours(debut, -nbJours))}', 'onClick={() => onDebut(ajouterJours(debut, nbJours))}', ["src/pages/reservations/Tableau.test.tsx"], "Tableau : « précédent » avance au lieu de reculer"),
  ("P11", "src/components/ui.tsx", '  const ajouter = (c: string) => valeur.length < longueurMax && onChange(valeur + c);', '  const ajouter = (c: string) => onChange(valeur + c);', TOUT, "Pavé PIN : plus de limite de longueur"),
  ("P12", "src/components/ui.tsx", 'onClick={() => onChange(valeur.slice(0, -1))}', 'onClick={() => onChange(valeur.slice(1))}', TOUT, "Pavé PIN : la touche ⌫ efface le premier chiffre"),
  ("P13", "src/pages/distance/ConsultationDistante.tsx", '.getTime() > SILENCE_INQUIETANT_MS;', '.getTime() < SILENCE_INQUIETANT_MS;', ["src/pages/distance/ConsultationDistante.test.tsx"], "Propriétaire : alerte « plus de nouvelles » inversée"),
  ("P14", "src/pages/distance/ReglagesDistance.tsx", '''      return setErreur("Écrivez l'adresse du serveur de consultation à distance.");''', '''      setErreur("Écrivez l'adresse du serveur de consultation à distance.");''', ["src/pages/distance/ReglagesDistance.test.tsx"], "Activation à distance envoyée sans adresse"),
  ("P15", "src/pages/distance/ConsultationDistante.tsx", '    if (caracteresCle(cle) !== LONGUEUR_CLE)', '    if (caracteresCle(cle) < 3)', ["src/pages/distance/ConsultationDistante.test.tsx"], "Rattachement : longueur de la clé secrète plus vérifiée"),
]
