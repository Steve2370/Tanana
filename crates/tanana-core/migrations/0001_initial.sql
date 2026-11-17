-- Tanana — schéma initial
-- Conventions :
--   * identifiants : UUID v7 en TEXT (triables par date, sans conflit si synchro future)
--   * dates de séjour : TEXT 'YYYY-MM-DD' (pas de fuseau : une nuit d'hôtel n'en a pas)
--   * horodatages : TEXT RFC 3339 en UTC
--   * montants : INTEGER, dans la plus petite unité de la devise (jamais de REAL)

-- ───────────────────────────── Paramètres ─────────────────────────────
CREATE TABLE parametres (
    cle    TEXT PRIMARY KEY,
    valeur TEXT NOT NULL
) STRICT;

-- ───────────────────────── Comptes et permissions ─────────────────────
CREATE TABLE roles (
    id      TEXT PRIMARY KEY,
    nom     TEXT NOT NULL UNIQUE,
    systeme INTEGER NOT NULL DEFAULT 0 CHECK (systeme IN (0, 1))
) STRICT;

CREATE TABLE permissions (
    code        TEXT PRIMARY KEY,
    description TEXT NOT NULL
) STRICT;

CREATE TABLE role_permissions (
    role_id         TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_code TEXT NOT NULL REFERENCES permissions(code),
    PRIMARY KEY (role_id, permission_code)
) STRICT;

CREATE TABLE utilisateurs (
    id                TEXT PRIMARY KEY,
    identifiant       TEXT NOT NULL UNIQUE COLLATE NOCASE,
    nom               TEXT NOT NULL,
    mot_de_passe_hash TEXT NOT NULL,
    pin_hash          TEXT,
    role_id           TEXT NOT NULL REFERENCES roles(id),
    statut            TEXT NOT NULL DEFAULT 'actif'
                      CHECK (statut IN ('actif', 'suspendu', 'desactive')),
    doit_changer_mdp  INTEGER NOT NULL DEFAULT 0 CHECK (doit_changer_mdp IN (0, 1)),
    echecs_connexion  INTEGER NOT NULL DEFAULT 0,
    bloque_jusqu_a    TEXT,
    cree_le           TEXT NOT NULL,
    derniere_connexion TEXT
) STRICT;

-- Exceptions individuelles : accorde = 1 ajoute une permission, 0 la retire.
CREATE TABLE permissions_perso (
    utilisateur_id  TEXT NOT NULL REFERENCES utilisateurs(id),
    permission_code TEXT NOT NULL REFERENCES permissions(code),
    accorde         INTEGER NOT NULL CHECK (accorde IN (0, 1)),
    PRIMARY KEY (utilisateur_id, permission_code)
) STRICT;

CREATE TABLE sessions (
    id                TEXT PRIMARY KEY,
    utilisateur_id    TEXT NOT NULL REFERENCES utilisateurs(id),
    token_hash        TEXT NOT NULL UNIQUE,   -- on ne stocke jamais le jeton en clair
    debut             TEXT NOT NULL,
    derniere_activite TEXT NOT NULL,
    fin               TEXT
) STRICT;
CREATE INDEX idx_sessions_ouvertes ON sessions(utilisateur_id) WHERE fin IS NULL;

-- ───────────────────────────── Hébergement ────────────────────────────
CREATE TABLE types_chambre (
    id               TEXT PRIMARY KEY,
    nom              TEXT NOT NULL UNIQUE COLLATE NOCASE,
    prix_nuit_defaut INTEGER NOT NULL CHECK (prix_nuit_defaut >= 0),
    capacite         INTEGER NOT NULL CHECK (capacite > 0),
    actif            INTEGER NOT NULL DEFAULT 1 CHECK (actif IN (0, 1))
) STRICT;

CREATE TABLE chambres (
    id      TEXT PRIMARY KEY,
    numero  TEXT NOT NULL UNIQUE COLLATE NOCASE,
    type_id TEXT NOT NULL REFERENCES types_chambre(id),
    etage   INTEGER,
    statut  TEXT NOT NULL DEFAULT 'disponible'
            CHECK (statut IN ('disponible', 'maintenance', 'hors_service')),
    notes   TEXT,
    cree_le TEXT NOT NULL
) STRICT;

CREATE TABLE clients (
    id             TEXT PRIMARY KEY,
    nom            TEXT NOT NULL,
    prenom         TEXT,
    telephone      TEXT,
    email          TEXT,
    piece_identite TEXT,
    notes          TEXT,
    cree_le        TEXT NOT NULL,
    modifie_le     TEXT NOT NULL
) STRICT;
CREATE INDEX idx_clients_nom ON clients(nom COLLATE NOCASE, prenom COLLATE NOCASE);

CREATE TABLE reservations (
    id           TEXT PRIMARY KEY,
    chambre_id   TEXT NOT NULL REFERENCES chambres(id),
    client_id    TEXT NOT NULL REFERENCES clients(id),
    date_arrivee TEXT NOT NULL CHECK (date(date_arrivee) = date_arrivee),
    date_depart  TEXT NOT NULL CHECK (date(date_depart) = date_depart),
    prix_total   INTEGER NOT NULL CHECK (prix_total >= 0),
    statut       TEXT NOT NULL DEFAULT 'confirmee'
                 CHECK (statut IN ('confirmee', 'arrivee', 'partie', 'annulee')),
    notes        TEXT,
    cree_le      TEXT NOT NULL,
    cree_par     TEXT NOT NULL REFERENCES utilisateurs(id),
    CHECK (date_depart > date_arrivee)
) STRICT;
CREATE INDEX idx_reservations_chambre_dates
    ON reservations(chambre_id, date_arrivee, date_depart) WHERE statut <> 'annulee';
CREATE INDEX idx_reservations_client ON reservations(client_id);

-- Garde-fou ultime contre la double réservation, au niveau de la base :
-- même si un bug passe dans le code, SQLite refuse l'écriture.
-- Deux séjours [a1, d1) et [a2, d2) se chevauchent si a1 < d2 ET d1 > a2
-- (le jour de départ est libre pour une nouvelle arrivée).
CREATE TRIGGER reservations_anti_chevauchement_insert
BEFORE INSERT ON reservations
WHEN NEW.statut <> 'annulee' AND EXISTS (
    SELECT 1 FROM reservations r
    WHERE r.chambre_id = NEW.chambre_id
      AND r.statut <> 'annulee'
      AND r.date_arrivee < NEW.date_depart
      AND r.date_depart > NEW.date_arrivee
)
BEGIN
    SELECT RAISE(ABORT, 'CHAMBRE_DEJA_RESERVEE');
END;

CREATE TRIGGER reservations_anti_chevauchement_update
BEFORE UPDATE OF chambre_id, date_arrivee, date_depart, statut ON reservations
WHEN NEW.statut <> 'annulee' AND EXISTS (
    SELECT 1 FROM reservations r
    WHERE r.chambre_id = NEW.chambre_id
      AND r.id <> NEW.id
      AND r.statut <> 'annulee'
      AND r.date_arrivee < NEW.date_depart
      AND r.date_depart > NEW.date_arrivee
)
BEGIN
    SELECT RAISE(ABORT, 'CHAMBRE_DEJA_RESERVEE');
END;

-- ─────────────────────────────── Argent ───────────────────────────────
-- Les paiements ne sont jamais modifiés ni supprimés : une erreur se corrige
-- par une écriture inverse (remboursement).
CREATE TABLE paiements (
    id             TEXT PRIMARY KEY,
    reservation_id TEXT NOT NULL REFERENCES reservations(id),
    montant        INTEGER NOT NULL CHECK (montant > 0),
    type           TEXT NOT NULL CHECK (type IN ('acompte', 'solde', 'remboursement')),
    mode           TEXT NOT NULL CHECK (mode IN ('especes', 'carte', 'mobile', 'virement', 'autre')),
    date           TEXT NOT NULL,
    utilisateur_id TEXT NOT NULL REFERENCES utilisateurs(id),
    note           TEXT
) STRICT;
CREATE INDEX idx_paiements_reservation ON paiements(reservation_id);
CREATE INDEX idx_paiements_date ON paiements(date);

CREATE TRIGGER paiements_pas_de_modification BEFORE UPDATE ON paiements
BEGIN SELECT RAISE(ABORT, 'PAIEMENT_NON_MODIFIABLE'); END;
CREATE TRIGGER paiements_pas_de_suppression BEFORE DELETE ON paiements
BEGIN SELECT RAISE(ABORT, 'PAIEMENT_NON_MODIFIABLE'); END;

CREATE TABLE depenses (
    id             TEXT PRIMARY KEY,
    categorie      TEXT NOT NULL,
    montant        INTEGER NOT NULL CHECK (montant > 0),
    description    TEXT,
    date           TEXT NOT NULL CHECK (date(date) = date),
    utilisateur_id TEXT NOT NULL REFERENCES utilisateurs(id),
    cree_le        TEXT NOT NULL
) STRICT;
CREATE INDEX idx_depenses_date ON depenses(date);

-- ─────────────────────────── Journal d'audit ──────────────────────────
-- Chaque entrée contient le hash de la précédente : supprimer ou modifier
-- une ligne casse la chaîne, ce que l'écran admin détecte.
CREATE TABLE journal_audit (
    seq            INTEGER PRIMARY KEY AUTOINCREMENT,
    id             TEXT NOT NULL UNIQUE,
    date           TEXT NOT NULL,
    utilisateur_id TEXT REFERENCES utilisateurs(id),  -- NULL = système ou échec de connexion
    action         TEXT NOT NULL,
    entite         TEXT,
    entite_id      TEXT,
    avant_json     TEXT,
    apres_json     TEXT,
    hash_precedent TEXT NOT NULL,
    hash           TEXT NOT NULL UNIQUE
);
CREATE INDEX idx_journal_utilisateur ON journal_audit(utilisateur_id, seq);
CREATE INDEX idx_journal_action ON journal_audit(action, seq);

CREATE TRIGGER journal_pas_de_modification BEFORE UPDATE ON journal_audit
BEGIN SELECT RAISE(ABORT, 'JOURNAL_EN_AJOUT_SEULEMENT'); END;
CREATE TRIGGER journal_pas_de_suppression BEFORE DELETE ON journal_audit
BEGIN SELECT RAISE(ABORT, 'JOURNAL_EN_AJOUT_SEULEMENT'); END;

-- ───────────────────────────── Données de base ────────────────────────
INSERT INTO permissions (code, description) VALUES
    ('chambre.voir',           'Voir les chambres et le planning'),
    ('chambre.gerer',          'Créer et modifier les chambres et leurs types'),
    ('client.voir',            'Voir les fiches clients'),
    ('client.gerer',           'Créer et modifier les fiches clients'),
    ('reservation.voir',       'Voir les réservations'),
    ('reservation.creer',      'Créer une réservation'),
    ('reservation.modifier',   'Modifier les dates, la chambre ou le prix d''une réservation'),
    ('reservation.annuler',    'Annuler une réservation'),
    ('sejour.gerer',           'Faire l''arrivée (check-in) et le départ (check-out)'),
    ('paiement.encaisser',     'Encaisser un acompte ou un solde'),
    ('paiement.rembourser',    'Faire un remboursement'),
    ('depense.voir',           'Voir les dépenses'),
    ('depense.saisir',         'Enregistrer une dépense'),
    ('rapport.financier.voir', 'Voir les rapports financiers'),
    ('utilisateur.gerer',      'Gérer les comptes, rôles et accès'),
    ('journal.voir',           'Voir le journal des actions'),
    ('parametres.gerer',       'Modifier les paramètres de l''hôtel'),
    ('sauvegarde.faire',       'Faire une sauvegarde manuelle');

INSERT INTO roles (id, nom, systeme) VALUES
    ('role-admin',     'Administrateur', 1),
    ('role-gerant',    'Gérant',         0),
    ('role-reception', 'Réceptionniste', 0);

INSERT INTO role_permissions (role_id, permission_code)
    SELECT 'role-admin', code FROM permissions;

INSERT INTO role_permissions (role_id, permission_code)
    SELECT 'role-gerant', code FROM permissions
    WHERE code NOT IN ('utilisateur.gerer', 'journal.voir', 'parametres.gerer');

INSERT INTO role_permissions (role_id, permission_code) VALUES
    ('role-reception', 'chambre.voir'),
    ('role-reception', 'client.voir'),
    ('role-reception', 'client.gerer'),
    ('role-reception', 'reservation.voir'),
    ('role-reception', 'reservation.creer'),
    ('role-reception', 'reservation.modifier'),
    ('role-reception', 'sejour.gerer'),
    ('role-reception', 'paiement.encaisser');

INSERT INTO parametres (cle, valeur) VALUES
    ('devise', 'XAF'),
    ('decimales_devise', '0'),
    ('delai_inactivite_minutes', '15'),
    ('max_echecs_connexion', '5'),
    ('duree_blocage_minutes', '15');
