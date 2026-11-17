# Tanana

Logiciel de gestion d'hôtel **hors ligne** pour Windows et macOS : chambres, clients, réservations, acomptes, caisse, comptes employés avec rôles et journal d'audit.

Pile : **Tauri 2** · **Rust** · **SQLite chiffré (SQLCipher)** · **React 19 + TypeScript + Tailwind 4**.

---

## Démarrer

### Prérequis

| | macOS | Windows |
|---|---|---|
| Rust (stable) | `curl https://sh.rustup.rs -sSf \| sh` | [rustup.rs](https://rustup.rs) |
| Node.js 22+ | `brew install node` | [nodejs.org](https://nodejs.org) |
| Outils de compilation | `xcode-select --install` | Visual Studio Build Tools (C++) |
| Perl (pour compiler OpenSSL/SQLCipher) | déjà présent | [Strawberry Perl](https://strawberryperl.com) |

### Lancer

```bash
npm install
npm run tauri dev        # app en développement, rechargement à chaud
```

Le premier lancement compile SQLCipher et OpenSSL : comptez quelques minutes, une seule fois.

### Tester

```bash
cargo test --workspace   # tests Rust : métier, comptes, journal, chiffrement, démarrage, reçus, restauration, distance
npm test                 # tests de l'interface (et du script de publication)
npm run typecheck
```

### QA adverse : mutations et fuzzing

Pour vérifier que les tests attraperaient vraiment un bug (avant une version importante) :

```bash
npm run qa:fuzz                          # fuzzing des formulaires, singe de 200 actions, constats (graine fixe)
FUZZ_SEED=hasard npm run qa:fuzz         # nouvelle série au hasard ; la graine s'affiche en cas d'échec
FUZZ_SEED=123 npm run qa:fuzz            # rejouer exactement une série
cargo test -p tanana-core --test qa_fuzz # fuzzing Rust (clés, identifiants, montants en lettres, taxes)
npm run qa:fenetres                      # chaque écran dans Chrome, de 1024 à 1920 px (Chrome requis)
python3 scripts/qa/muter-ecrans.py scripts/qa/series/serie1.py   # 15 mutants d'écran, dans une copie jetable
npm run qa:mutants                       # mutants Rust (cargo install cargo-mutants ; long : ~20 s par mutant)
```

Les tests nés de la QA sont dans `src/qa/` ; chaque défaut trouvé devient un test fixe dans `src/qa/constats.test.tsx`. Quand vous ajoutez une commande dans `src/lib/api.ts`, ajoutez-la aussi au faux moteur `src/qa/fuzz/fauxRust.ts` (un test le rappelle). Les résultats vont dans `.qa/` (ignoré par git).

### Construire un installeur

```bash
npm run tauri build      # .dmg/.app sur Mac, .msi/.exe sur Windows (non signé, sans mise à jour automatique)
```

Un Mac ne produit que l'installeur Mac, et Windows que celui de Windows. Pour les deux à la fois, signés et avec mise à jour automatique, passez par GitHub (ci-dessous).

### Publier une version (GitHub)

1. Mettez le numéro de version dans `package.json`, `src-tauri/tauri.conf.json` et `Cargo.toml` (`[workspace.package]`), et écrivez les nouveautés dans `NOUVEAUTES.md` (court : elles s'affichent dans Tanana quand la mise à jour est proposée).
2. `git tag v0.2.0 && git push --tags` : le workflow `publication.yml` construit les installeurs Mac (Intel + Apple Silicon) et Windows, et les attache à une release **en brouillon**.
3. Essayez les installeurs du brouillon, puis cliquez « Publish release » : c'est seulement là que les hôtels voient la mise à jour.

Tout est optionnel : sans réglages, les installeurs sont construits sans signature et sans mise à jour automatique. À régler une fois dans *Settings › Secrets and variables › Actions* du dépôt :

| Pour | Variables | Secrets |
|---|---|---|
| **Mise à jour automatique** | `TAURI_UPDATER_PUBKEY` : contenu de `~/.tauri/tanana.key.pub` | `TAURI_SIGNING_PRIVATE_KEY` : contenu de `~/.tauri/tanana.key` ; `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` : son mot de passe |
| **Signature Apple** (+ notarisation) | | `APPLE_CERTIFICATE` (certificat « Developer ID Application » exporté en .p12, en base64), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` (ex. `Developer ID Application: Brice … (TEAMID)`), `APPLE_ID`, `APPLE_PASSWORD` (mot de passe d'application, appleid.apple.com), `APPLE_TEAM_ID` |
| **Signature Windows** | | `WINDOWS_CERTIFICATE` (.pfx en base64), `WINDOWS_CERTIFICATE_PASSWORD` |
| Serveur de consultation à distance proposé par défaut | `TANANA_RELAIS` : `https://votre-domaine` | |

- Clés de mise à jour, **une seule fois**, sur votre Mac : `npm run tauri signer generate -- -w ~/.tauri/tanana.key`. Gardez la clé privée et son mot de passe en lieu sûr (gestionnaire de mots de passe) : **perdues, plus aucune mise à jour ne pourra être envoyée** aux hôtels déjà installés. Ne la mettez jamais dans le dépôt.
- Base64 d'un certificat : `base64 -i certificat.p12 | pbcopy` (Mac), puis collez dans le secret.
- Les hôtels cherchent `https://github.com/<vous>/<dépôt>/releases/latest/download/latest.json` (fichier ajouté à la release par le workflow) : le dépôt doit être **public**, ou les releases servies depuis votre propre serveur (changez `TANANA_MISE_A_JOUR_URL` dans le workflow).
- Dans Tanana, la mise à jour est proposée aux administrateurs (bandeau, et *Réglages › Mises à jour*), jamais imposée : copie de sécurité, téléchargement, redémarrage. La réception n'est pas dérangée.

### Essayer sur un vrai Mac et un vrai Windows (avant le premier hôtel)

Les tests automatiques tournent sur Linux, macOS et Windows (`tests.yml`), mais certaines choses ne se vérifient qu'à la main, sur l'installeur publié :

- [ ] **Installer** à partir du `.dmg` (Mac) et du `.msi` ou `.exe` (Windows) : pas d'alerte « développeur non identifié » une fois signé.
- [ ] **Premier lancement** : créer l'hôtel, noter le code de secours ; **fermer et rouvrir** Tanana → pas de demande de clé (le trousseau la garde).
- [ ] **Redémarrer l'ordinateur** puis rouvrir Tanana → toujours pas de demande de clé.
- [ ] **Imprimer** un reçu et une facture sur une vraie imprimante (marges, accents, montant en lettres).
- [ ] **Copie de sécurité** sur une clé USB, puis sur un **autre ordinateur** : premier lancement › « Remettre une copie de sécurité » avec la clé de 64 caractères → tout est là.
- [ ] **Petit écran** (portable 1366 × 768) et **zoom système à 125 %** (Windows) : rien de coupé, boutons atteignables.
- [ ] **Mise à jour** : publier une `v0.1.1` de test → le bandeau apparaît chez l'administrateur → installer → Tanana redémarre en 0.1.1, les données sont intactes.
- [ ] **Sans Internet** une journée avec la consultation à distance active → rien ne bloque ; Internet revenu → le propriétaire voit tout.
- [ ] **Une vraie personne à l'accueil**, une journée, avec le guide d'utilisation : noter chaque hésitation.

---

## Architecture

```
tanana/
├── crates/tanana-core/        ← TOUT le métier, sans Tauri ni interface
│   ├── migrations/            ← schéma SQL versionné (0001, 0002…)
│   ├── src/
│   │   ├── lib.rs             ← Tanana + le motif vérifier → agir → logger → commit
│   │   ├── domaine.rs         ← règles pures : chevauchement, solde, transitions
│   │   ├── auth.rs            ← comptes, Argon2, PIN, sessions, rôles, permissions
│   │   ├── audit.rs           ← journal chaîné par hash
│   │   ├── db.rs              ← ouverture, chiffrement, migrations, sauvegardes
│   │   └── services/          ← chambres, clients, réservations, argent, système
│   └── tests/                 ← tests d'intégration par thème
├── src-tauri/                 ← coquille fine : fenêtre, trousseau, commandes
└── src/                       ← interface React
    ├── bindings/              ← types TS GÉNÉRÉS depuis Rust (ne pas éditer)
    ├── lib/api.ts             ← seul point de contact avec Rust
    └── pages/
```

### Les décisions qui comptent

**1. Tout passe par `Tanana::executer`.** Chaque action vérifie la session, puis la permission, agit, écrit le journal **dans la même transaction**, puis valide. Si l'écriture du journal échoue, l'action est annulée : une action réussie est toujours tracée.

**2. La sécurité est dans Rust, jamais dans l'interface.** `<SiPermis>` masque des boutons pour le confort ; chaque commande revérifie la permission côté Rust. Un refus est lui-même journalisé (`acces.refuse`).

**3. Double réservation impossible, à deux niveaux.**
- Le service vérifie et renvoie la réservation en conflit (message clair).
- Un **trigger SQLite** refuse toute écriture qui chevaucherait, même un `INSERT` fait à la main dans la base.
- Les transactions `IMMEDIATE` sérialisent les écritures : un test lance 10 réservations simultanées de la même chambre et vérifie qu'une seule passe.

**4. L'argent ne se modifie jamais.** Les paiements sont en ajout seulement (trigger). Une erreur se corrige par un remboursement tracé. Les montants sont des **entiers** dans la plus petite unité de la devise.

**5. Journal infalsifiable sans que ça se voie.** Chaque entrée contient le hash SHA-256 de la précédente. Le tableau de bord admin vérifie la chaîne et affiche « Journal altéré » si une ligne a été modifiée ou supprimée.
*Limite connue :* supprimer les toutes dernières entrées ne casse pas la chaîne. Le chiffrement de la base et les triggers rendent l'opération très difficile.

**6. Base chiffrée, clé dans le trousseau.** La clé est générée au premier lancement et rangée dans le Trousseau macOS ou le Gestionnaire d'identification Windows. Sans elle, le fichier est illisible.
- **Nouvel ordinateur ou restauration :** l'app détecte l'absence de clé et la demande. Un admin peut l'afficher dans *Paramètres → Clé de chiffrement* (action journalisée).
- **À faire à la livraison :** faire noter la clé et le code de récupération au client, sur papier.

**7. Pas de « mot de passe oublié » par courriel.** Au premier lancement, un **code de récupération** est affiché une seule fois. Il réinitialise le mot de passe admin, puis un nouveau code est émis.

### Comptes et accès

| Fonction | Où |
|---|---|
| Rôles Administrateur, Gérant, Réceptionniste + rôles personnalisés | `auth.rs`, migration |
| Ajouter ou retirer une permission à **une** personne | `definir_permission_perso` |
| Suspendre ou désactiver (sessions fermées immédiatement) | `changer_statut_utilisateur` |
| Mot de passe temporaire à changer à la 1re connexion | `doit_changer_mdp` |
| Blocage 15 min après 5 échecs | paramètres `max_echecs_connexion`, `duree_blocage_minutes` |
| Verrouillage après inactivité (15 min par défaut) | côté Rust **et** côté interface |
| PIN de changement rapide d'utilisateur au comptoir | `connexion_pin` |
| Impossible de retirer le dernier admin actif, ni d'agir sur soi-même | `DernierAdministrateur`, `ActionSurSoiMeme` |

### Interface pensée pour des personnes âgées

Les utilisateurs visés sont souvent âgés. Chaque écran doit respecter ces règles (voir `src/components/ui.tsx`) :

- **Lisible :** texte de base à 18 px, police Lexend embarquée (fonctionne hors ligne), contrastes vérifiés (texte courant ≥ 7:1).
- **Facile à viser :** boutons et champs d'au moins 48 px de haut, libellés toujours écrits (jamais une icône seule).
- **Messages clairs :** dire ce qui se passe **et quoi faire** (`src/lib/erreurs.ts`), sans jargon. Les messages restent affichés, ils ne disparaissent pas tout seuls.
- **Pas d'erreur bête :** alerte Verr. Maj., bouton « Afficher » le mot de passe, règles du mot de passe cochées en direct, identifiant retenu (jamais le mot de passe), clavier à l'écran pour le PIN.
- **Sûr sans être pénible :** confirmation avant toute action importante (suspendre, changer un rôle, réinitialiser…), avec le focus sur « Annuler ». Avant la déconnexion automatique, la fenêtre « Êtes-vous toujours là ? » laisse une minute pour continuer.
- **Couleurs du logo :** teal `#09595A` (actions), orange `#DE7922` et or `#F2A328` (repères et alertes).

### Conventions

- **Dates de séjour :** `YYYY-MM-DD` sans fuseau. **Horodatages :** RFC 3339 en UTC. Le rapport de caisse reçoit le décalage horaire local pour couper les journées au bon moment.
- **Identifiants :** UUID v7 (triables, sans conflit si une synchro est ajoutée plus tard).
- **Migrations :** ne **jamais** modifier un fichier déjà livré. Ajoutez `0002_….sql` et déclarez-le dans `db::migrations()`.
- **Types partagés :** après avoir modifié une structure Rust, lancez `npm run types` pour régénérer `src/bindings/`. Le CI échoue si on oublie.

---

## Ajouter une fonctionnalité (checklist)

1. Règle pure ? → `domaine.rs` + tests unitaires.
2. Nouvelle table ou colonne ? → nouvelle migration.
3. Nouvelle permission ? → l'ajouter dans la migration **et** dans `Permission` (un test vérifie l'alignement), puis son libellé dans `src/lib/permissions.ts` (sinon TypeScript ne compile pas).
4. Service dans `services/` via `self.executer(token, Some(Permission::…), |tx, a| { …; audit::ecrire(…) })`.
5. Tests d'intégration dans `crates/tanana-core/tests/`.
6. Commande dans `src-tauri/src/commandes.rs`, enregistrée dans `lib.rs`, fonction dans `src/lib/api.ts`. Un test vérifie que les trois listes concordent.
7. Libellé de la nouvelle action de journal dans `src/lib/permissions.ts`. Un test le vérifie aussi.

---

## État d'avancement

**Prêt et testé (moteur Rust) :** comptes, rôles, permissions individuelles, sessions, PIN, récupération admin, journal chaîné, chambres, types, clients, réservations (création, modification, annulation, arrivée, départ), anti-double-réservation, acomptes, soldes, remboursements, dépenses, rapport de caisse, paramètres, sauvegardes automatiques et manuelles, chiffrement, reçus et factures PDF (numérotation continue R-2026-0001 / F-2026-0001, contenu figé, duplicatas tracés, montant en toutes lettres).

**Écrans faits :** premier lancement (ou rattachement à un hôtel à distance, ou restauration sur un ordinateur neuf), clé manquante, mon compte (mot de passe, code PIN), connexion (mot de passe / PIN / récupération), changement de mot de passe imposé, accueil administrateur, employés et accès, historique, chambres, réglages (coordonnées, taxes, sauvegarde, consultation à distance, clé), **réservations** (tableau chambres × jours, nouvelle réservation en 4 étapes, fiche avec acompte, solde, remboursement, arrivée, départ, modification, annulation), **caisse** (totaux par période, paiements des clients, dépenses avec annulation tracée), **clients** (recherche par nom ou par téléphone quelle que soit l'écriture, fiche, modification, alerte de doublon, historique des séjours avec reste à payer, réserver pour ce client).

**Reçus et factures :** bouton « Reçu » sur chaque paiement (fiche de réservation et caisse), « Imprimer la facture » sur la fiche. Le PDF est rangé dans *Documents › Tanana › Reçus* (ou *Factures*) puis ouvert avec le lecteur PDF de l'ordinateur, d'où l'on imprime. Coordonnées et mentions (NIU, RCCM…) dans *Réglages*. Pour regarder les modèles : `TANANA_APERCU=/tmp/apercu cargo test -p tanana-core --test documents apercu -- --ignored`.

**Taxes :** chaque hôtel règle les siennes dans *Réglages › Taxes* (TVA, taxe de séjour…), dans sa devise. Une taxe se calcule en pourcentage, en montant par nuit (ex. 2 000 Ar par nuit à Madagascar) ou en montant pour le séjour, et elle est soit **comprise** dans le prix de la chambre, soit **ajoutée** au prix (le client paie en plus, le reste à payer augmente). Les taxes de l'hôtel sont copiées sur chaque nouvelle réservation, puis modifiables sur la fiche (« Modifier les taxes ») avant la facture. Un seul calcul, en entiers et au franc près, dans `domaine::calculer_taxes` ; l'interface l'appelle pour ses aperçus. La facture montre le hors-taxes, chaque taxe et le total à payer.

**Consultation à distance :** l'hôtel reste autonome hors ligne ; quand Internet est là, ses changements partent, compressés et **chiffrés de bout en bout**, vers un relais (`crates/tanana-relais`, à installer sur un VPS : voir son README). Le propriétaire, ailleurs dans le monde, consulte une copie **en lecture seule** de l'hôtel avec les écrans habituels. Le relais ne peut rien lire. Les mots de passe, PIN, sessions et code de récupération ne quittent jamais l'hôtel.
- **À l'hôtel** : *Réglages › Consultation à distance* › « Activer » (code d'invitation si le relais en demande). Trois informations s'affichent, à noter sur papier : adresse du serveur, identifiant de l'hôtel (`AFOJ-WRFX`), clé secrète (6 groupes de 4 puis 2). Ensuite tout part seul, chaque minute ; sans Internet, rien n'est perdu et l'envoi reprend au retour de la connexion. Boutons : Envoyer maintenant, Afficher la clé (mot de passe exigé), Tout renvoyer, Arrêter (efface les données du serveur).
- **Chez le propriétaire** : au premier lancement, « Consulter un hôtel à distance », recopier les trois informations (minuscules, espaces, 0/O, 1/I, 8/B tolérés) et choisir un mot de passe pour cet ordinateur. Tanana s'ouvre alors toujours sur cet hôtel : connexion par mot de passe seul, bandeau « rien ne peut être modifié d'ici » avec la date des informations, mise à jour à l'ouverture puis toutes les 5 minutes (les nouveautés s'affichent quand la personne le demande, pour ne pas changer la page sous ses yeux). Mot de passe oublié ou clé du trousseau perdue : « Rattacher l'hôtel à nouveau » efface la copie (sans risque, tout est sur le serveur) et on recommence avec le papier.
- Adresse du serveur proposée par défaut : variable `VITE_TANANA_RELAIS` au moment de construire l'application (ex. `VITE_TANANA_RELAIS=https://relais.exemple.com npm run tauri build`). Sans elle, la personne la tape.
- Essai local : `TANANA_RELAIS_ECOUTE=127.0.0.1:8787 TANANA_RELAIS_CODES=ESSAI-2026 cargo run -p tanana-relais`, puis adresse `127.0.0.1:8787` dans l'application (`http://` n'est accepté que sur l'ordinateur lui-même).
- **Page web de consultation** (`src/web/`) : le propriétaire ouvre l'adresse du relais dans n'importe quel navigateur, ordinateur ou téléphone, tape l'identifiant et la clé, et voit son hôtel en lecture seule (aujourd'hui, réservations, caisse, historique), actualisé chaque minute. Tout est déchiffré **dans le navigateur** (mêmes HKDF et XChaCha20-Poly1305 que Tanana) ; rien n'est enregistré sur l'appareil. Le relais sert la page lui-même (`TANANA_RELAIS_PAGES`, déjà réglé dans son image Docker) avec une politique de sécurité stricte.
  - Construire : `npm run build:web` (→ `dist-web/`).
  - Démonstration sans vrai hôtel : `npm run build:web && cargo run -p tanana-relais --example demo -- --pages dist-web`, puis ouvrir `http://127.0.0.1:8787` avec l'identifiant et la clé affichés.
  - Essai dans Chrome : `node scripts/qa-consultation.mjs <identifiant> <clé>`.
  - Les tests (`src/web/*.test.ts*`) lisent un hôtel réellement envoyé par Tanana (`src/web/__fixtures__/hotel-demo.json`, régénéré par `cargo run -p tanana-relais --example demo -- --date 2026-11-17 --fixture src/web/__fixtures__/hotel-demo.json`) et vérifient qu'on retrouve exactement ses chiffres.
- Code : `crates/tanana-core/src/distance/` (capture des changements par triggers générés depuis la structure des tables, lots chiffrés XChaCha20-Poly1305 avec une clé tirée par HKDF d'une clé secrète de 128 bits, copie en lecture seule), `crates/tanana-protocole/` (messages), `crates/tanana-relais/` (serveur axum + SQLite, Docker + Caddy), `src-tauri/src/distance.rs` (envois en arrière-plan, **jamais d'appel réseau pendant que la base est verrouillée**), `src/pages/distance/` (écrans).

La police Lexend intégrée aux PDF (`crates/tanana-core/polices/`) est sous licence SIL Open Font License (fichier `OFL.txt`).

**Copies de sécurité et restauration :** une copie chiffrée chaque jour au démarrage (les 14 dernières), et à la demande vers une clé USB. *Réglages › Copie de sécurité › Remettre une copie* : Tanana montre ce que contient la copie (hôtel, date, nombre de réservations et de clients), exige le mot de passe de l'administrateur, **met l'état actuel de côté** (`tanana-avant-restauration-….db`), puis remplace et déconnecte tout le monde. Sur un **ordinateur neuf** (panne, vol), le premier écran propose « Remettre une copie de sécurité » : choisir le fichier sur la clé USB, taper la clé de 64 caractères notée à l'installation. Même sortie depuis l'écran « clé manquante » et en cas de base illisible (elle est mise de côté, jamais effacée). Une copie d'une version plus récente de Tanana, vide, abîmée ou venant d'un poste de consultation à distance est refusée avant de toucher à quoi que ce soit. Si la consultation à distance était active, elle est arrêtée (à réactiver, nouvelle clé pour le propriétaire). Code : `crates/tanana-core/src/restauration.rs`, `Etat::restaurer`, `src/components/restauration.tsx`.

**Mise à jour automatique :** `tauri-plugin-updater`, activé seulement dans les versions publiées par GitHub (voir « Publier une version »). Code : `src/lib/miseAJour.ts`, `src/components/miseAJour.tsx`, `scripts/config-publication.mjs`.

L'identifiant de l'app est `ca.tanana.gestion` (dans `src-tauri/tauri.conf.json`), définitif : le changer après une installation chez un client déplacerait le dossier de données et la clé du trousseau. (Il ne finit pas par `.app`, qui entre en conflit avec les applications macOS.)
