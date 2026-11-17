"""Série 2 : écrans métier (réservations, paiements, caisse, chambres, employés, saisie, fenêtres)."""
F = "src/pages/reservations/FicheReservation.tsx"
N = "src/pages/reservations/NouvelleReservation.tsx"
K = "src/pages/caisse/Caisse.tsx"
CH = "src/pages/Chambres.tsx"
U = "src/pages/Utilisateurs.tsx"
FMT = "src/lib/format.ts"
UI = "src/components/ui.tsx"
T_F = ["src/pages/reservations/FicheReservation.test.tsx"]
T_N = ["src/pages/reservations/NouvelleReservation.test.tsx"]
T_K = ["src/pages/caisse/Caisse.test.tsx"]
T_CH = ["src/pages/Chambres.test.tsx"]
T_U = ["src/pages/Administration.test.tsx"]
TOUT = ['src/components/miseAJour.test.tsx', 'src/components/restauration.test.tsx', 'src/lib/dates.test.ts', 'src/lib/distance.test.ts', 'src/lib/erreurs.test.ts', 'src/lib/format.test.ts', 'src/lib/session.test.tsx', 'src/lib/taxes.test.ts', 'src/pages/Administration.test.tsx', 'src/pages/Chambres.test.tsx', 'src/pages/Connexion.test.tsx', 'src/pages/Demarrage.test.tsx', 'src/pages/MonCompte.test.tsx', 'src/pages/Parametres.test.tsx', 'src/pages/caisse/Caisse.test.tsx', 'src/pages/clients/Clients.test.tsx', 'src/pages/distance/ConsultationDistante.test.tsx', 'src/pages/distance/ReglagesDistance.test.tsx', 'src/pages/reservations/FicheReservation.test.tsx', 'src/pages/reservations/NouvelleReservation.test.tsx', 'src/pages/reservations/Tableau.test.tsx', 'src/qa/ecrans-recents.test.tsx']  # toute la suite, sauf les tests qui lisent le code Rust (muté en parallèle)

MUTANTS = [
  ("N01", F, '    if (montant > plafond) return setErreur(messagePlafond);\n', '', T_F, "Paiement : on peut encaisser plus que le reste à payer"),
  ("N02", F, 'onClick={() => void depart()} disabled={r.solde > 0} enCours', 'onClick={() => void depart()} enCours', T_F, "Départ : bouton actif alors qu'il reste à payer"),
  ("N03", F, '    if (apercu.total_a_payer !== r.total_a_payer) {', '    if (false) {', T_F, "Taxes : le nouveau total s'enregistre sans confirmation"),
  ("N04", F, '''  const valider = () => (motif.trim() ? onValider(motif.trim()) : setErreur("Écrivez la raison de l'annulation."));''', '''  const valider = () => onValider(motif.trim());''', T_F, "Annulation de réservation sans raison"),
  ("N05", N, '    if (!client) return setErreur("Choisissez le client (étape 3).");\n', '', T_N, "Nouvelle réservation : enregistrable sans client"),
  ("N06", N, '      if (prix === null) return setErreur("Le prix n\'est pas lisible. Écrivez seulement des chiffres, par exemple 40000.");\n', '', T_N, "Nouvelle réservation : prix illisible accepté"),
  ("N07", K, '    if (montant === null || montant <= 0) return setErreur("Le montant n\'est pas lisible. Écrivez seulement des chiffres, par exemple 8000.");', '    if (montant === null || montant < 0) return setErreur("Le montant n\'est pas lisible. Écrivez seulement des chiffres, par exemple 8000.");', T_K, "Dépense de 0 acceptée"),
  ("N08", K, '''  const valider = () => (motif.trim() ? onValider(motif.trim()) : setErreur("Écrivez la raison de l'annulation."));''', '''  const valider = () => onValider(motif.trim());''', T_K, "Annulation de dépense sans raison"),
  ("N09", CH, '    if (montant === null) return setErreur("Le prix n\'est pas lisible. Écrivez seulement des chiffres, par exemple 25000.");\n', '', T_CH, "Type de chambre : prix illisible accepté"),
  ("N10", U, '    if (!motDePasseValide(mdp)) return setErreur("Le mot de passe provisoire ne respecte pas encore toutes les règles.");\n    const u', '    const u', T_U, "Nouvel employé : mot de passe provisoire faible accepté à l'écran"),
  ("N11", FMT, '  if (fraction.length > decimales) return null;\n', '', ["src/lib/format.test.ts"], "Montant : « 10,5 » accepté pour une monnaie sans centimes"),
  ("N12", FMT, '  return Number.isSafeInteger(montant) ? montant : null;', '  return montant;', ["src/lib/format.test.ts"], "Montant : nombre géant accepté (imprécis)"),
  ("N13", UI, '      if (e.key === "Escape") echap();\n', '', TOUT, "Fenêtre de confirmation : Échap ne ferme plus"),
  ("N14", UI, '      if (e.key === "Tab" && boite.current) {', '      if (false) {', TOUT, "Fenêtre de confirmation : Tab sort de la fenêtre"),
  ("N15", UI, '        aria-invalid={erreur ? true : undefined}\n', '', TOUT, "Champ en erreur non signalé aux lecteurs d'écran"),
]
