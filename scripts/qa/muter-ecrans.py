#!/usr/bin/env python3
"""
Mutation des écrans de Tanana : chaque mutant est un vrai bug qu'un développeur
pourrait écrire. On l'applique, on lance les tests qui couvrent l'écran : s'ils
échouent, le mutant est « tué » (bien) ; s'ils passent, il « survit » (il manque un test).

Tout se passe dans une COPIE jetable du projet (/tmp) : le dossier réel n'est
jamais modifié, même si le script est interrompu.

  python3 scripts/qa/muter-ecrans.py scripts/qa/series/serie1.py          # toute la série
  python3 scripts/qa/muter-ecrans.py scripts/qa/series/serie1.py M04 M06  # quelques mutants
  python3 scripts/qa/muter-ecrans.py scripts/qa/series/serie1.py --prouver src/qa/mon.test.tsx M04
      → le test doit passer sans le mutant et échouer avec (preuve dans les deux sens)

Une série est un fichier Python qui définit MUTANTS, une liste de 15 au plus :
  (id, fichier, texte_avant, texte_apres, [fichiers de test], "bug simulé")
Résultats : .qa/<série>.md (tableau) et .qa/qa.log (sortie des tests), dans le vrai projet.
"""
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import time

RACINE = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
IGNORER = shutil.ignore_patterns("target", "node_modules", ".git", ".qa", "dist", "mutants.out*")


def charger(serie):
    spec = importlib.util.spec_from_file_location("serie", serie)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.MUTANTS


def copie_jetable():
    dossier = tempfile.mkdtemp(prefix="tanana-qa-")
    shutil.copytree(RACINE, dossier, ignore=IGNORER, dirs_exist_ok=True)
    os.symlink(os.path.join(RACINE, "node_modules"), os.path.join(dossier, "node_modules"))
    return dossier


def tests_qa():
    """Les tests nés de la QA couvrent tous les écrans : toujours lancés en plus (sauf le fuzzing, plus lent)."""
    dossier = os.path.join(RACINE, "src", "qa")
    return sorted(f"src/qa/{f}" for f in os.listdir(dossier) if ".test." in f) if os.path.isdir(dossier) else []


def vitest(copie, tests):
    tests = sorted(set(tests) | set(tests_qa()))
    r = subprocess.run(["npx", "vitest", "run", *tests], cwd=copie, capture_output=True, text=True, timeout=900)
    resume = "\n".join(l for l in r.stdout.splitlines() if any(k in l for k in ("×", "Tests ", "FAIL")))
    return r.returncode, resume


def appliquer(copie, fichier, avant, apres):
    chemin = os.path.join(copie, fichier)
    original = open(chemin, encoding="utf-8").read()
    if original.count(avant) != 1:
        return None
    open(chemin, "w", encoding="utf-8").write(original.replace(avant, apres, 1))
    return original


def main():
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    serie, reste = args[0], args[1:]
    prouver = None
    if "--prouver" in reste:
        i = reste.index("--prouver")
        prouver = reste[i + 1]
        reste = reste[:i] + reste[i + 2 :]
    mutants = [m for m in charger(serie) if not reste or m[0] in reste]
    if len(mutants) > 15 and not reste:
        sys.exit("15 mutants au plus par série : découpez-la.")
    nom = os.path.splitext(os.path.basename(serie))[0]
    os.makedirs(os.path.join(RACINE, ".qa"), exist_ok=True)
    journal = open(os.path.join(RACINE, ".qa", "qa.log"), "a", encoding="utf-8")
    copie = copie_jetable()
    try:
        if prouver:
            shutil.copy(os.path.join(RACINE, prouver), os.path.join(copie, prouver))
            rc, _ = vitest(copie, [prouver])
            print("sans mutant :", "passe" if rc == 0 else "ÉCHOUE (le test lui-même est faux)")
            for id_, fichier, avant, apres, _, bug in mutants:
                original = appliquer(copie, fichier, avant, apres)
                if original is None:
                    print(id_, "ancre introuvable (le code a changé)")
                    continue
                rc, _ = vitest(copie, [prouver])
                open(os.path.join(copie, fichier), "w", encoding="utf-8").write(original)
                print(id_, "tué par le test" if rc else "TOUJOURS SURVIVANT", "—", bug)
            return
        lignes = [f"# Mutants d'écran — {nom}", "", "| Mutant | Fichier | Bug simulé | Résultat |", "|---|---|---|---|"]
        tues = 0
        for id_, fichier, avant, apres, tests, bug in mutants:
            original = appliquer(copie, fichier, avant, apres)
            if original is None:
                resultat = "ancre introuvable"
            else:
                t0 = time.time()
                rc, resume = vitest(copie, tests)
                open(os.path.join(copie, fichier), "w", encoding="utf-8").write(original)
                resultat = "tué" if rc else "SURVIVANT"
                tues += rc != 0
                journal.write(f"\n===== {nom} {id_} ({resultat}, {time.time() - t0:.0f} s) : {bug}\n{resume}\n")
            lignes.append(f"| {id_} | `{fichier}` | {bug} | {resultat} |")
            print(id_, resultat, "—", bug, flush=True)
        lignes += ["", f"**{tues}/{len(mutants)} tués**"]
        open(os.path.join(RACINE, ".qa", f"{nom}.md"), "w", encoding="utf-8").write("\n".join(lignes) + "\n")
        print(f"\n{tues}/{len(mutants)} tués — détail : .qa/{nom}.md")
    finally:
        shutil.rmtree(copie, ignore_errors=True)


if __name__ == "__main__":
    main()
