"""Série 4 (v0.1.2) : accès à la caisse, logo de l'hôtel, réservations de groupe, message d'activation."""
CAISSE = ["src/pages/caisse/Caisse.test.tsx", "src/pages/Administration.test.tsx"]
LOGO = ["src/pages/Logo.test.tsx"]
GROUPE = ["src/pages/reservations/Groupe.test.tsx", "src/pages/reservations/NouvelleReservation.test.tsx", "src/pages/reservations/FicheReservation.test.tsx"]
MUTANTS = [
  ("G01", "src/pages/caisse/Caisse.tsx", "        {!voitArgent && (\n", "        {false && (\n", CAISSE, "Caisse : plus de message « pas accès », écran vide"),
  ("G02", "src/pages/Coquille.tsx", 'permissions: ["rapport.financier.voir", "depense.voir", "paiement.encaisser"]', 'permissions: ["rapport.financier.voir", "depense.voir"]', CAISSE, "Caisse : onglet de nouveau caché à la réception"),
  ("G03", "src/pages/caisse/Caisse.tsx", "      {(voitArgent || voitDepenses) && (\n", "      {true && (\n", CAISSE, "Caisse : choix de période affiché sans rien à montrer"),
  ("G04", "src/pages/Parametres.tsx", "      changer(r);\n      setPropose(null);", "      setPropose(null);", LOGO, "Logo : enregistré mais le menu ne change pas"),
  ("G05", "src/pages/Parametres.tsx", "    if (!oui) return;\n    const r = await lancer(null);", "    const r = await lancer(null);", LOGO, "Logo : retiré même si on répond « Annuler »"),
  ("G06", "src/lib/logoHotel.tsx", "  const echelle = Math.min(1, max / Math.max(largeur, hauteur, 1));", "  const echelle = max / Math.max(largeur, hauteur, 1);", LOGO, "Logo : une petite image est agrandie (floue)"),
  ("G07", "src/pages/reservations/NouvelleReservation.tsx", "(prixDifferent ? (prixRetenus[i] ?? null) : null)", "(null)", GROUPE, "Groupe : les prix convenus ne sont pas envoyés"),
  ("G08", "src/pages/reservations/NouvelleReservation.tsx", "  const groupe = chambres.length > 1;", "  const groupe = chambres.length >= 1;", GROUPE, "Une seule chambre traitée comme un groupe"),
  ("G09", "src/pages/reservations/NouvelleReservation.tsx", "setDetailTaxes(cumulerTaxes(d))", "setDetailTaxes(d[0] ?? null)", GROUPE, "Groupe : taxes d'une seule chambre affichées"),
  ("G10", "src/pages/reservations/FicheReservation.tsx", "  const compte = compteDe(r, groupe);", "  const compte = compteDe(r, null);", GROUPE, "Fiche de groupe : compte de la chambre au lieu du groupe"),
  ("G11", "src/pages/reservations/FicheReservation.tsx", "  const porteur = chambreActive(r) ? r : (groupe?.reservations.find(chambreActive) ?? r);", "  const porteur = r;", GROUPE, "Groupe : paiement envoyé sur une chambre annulée"),
  ("G12", "src/pages/reservations/FicheReservation.tsx", "disabled={compte.solde > 0}", "disabled={r.solde > 0}", GROUPE, "Groupe : départ possible alors que le groupe doit encore"),
  ("G13", "src/web/vues.ts", "    if (r.groupe) groupes.set(r.groupe.id, Math.max(0, r.groupe.solde));\n    else reste", "    reste", ["src/web/consultation.test.ts"], "Page web : le reste d'un groupe compté une fois par chambre"),
  ("G14", "src/lib/erreurs.ts", '      if (e.details.includes("invitation")) {', "      if (false) {", ["src/pages/distance/ReglagesDistance.test.tsx", "src/lib/erreurs.test.ts"], "Activation : message qui parle d'un identifiant inexistant"),
  ("G15", "src/components/taxes.tsx", "      if (deja) deja.montant += l.montant;\n      else total", "      total", GROUPE, "Groupe : une ligne de taxe par chambre au lieu d'une"),
]
