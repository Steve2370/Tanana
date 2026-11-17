-- Reçus et factures.
--
-- Un document émis est une pièce comptable : son numéro se suit sans trou
-- (R-2026-0001, R-2026-0002… ; F-2026-0001…), et son contenu est figé au
-- moment de l'émission (nom du client, prix, paiements). Il ne peut être ni
-- modifié ni supprimé. Une réimpression redonne le même document, marqué
-- « Duplicata », et laisse une trace dans le journal.
CREATE TABLE documents (
    id             TEXT PRIMARY KEY,
    type           TEXT NOT NULL CHECK (type IN ('recu', 'facture')),
    annee          INTEGER NOT NULL,
    sequence       INTEGER NOT NULL CHECK (sequence > 0),
    numero         TEXT NOT NULL UNIQUE,
    reservation_id TEXT NOT NULL REFERENCES reservations(id),
    paiement_id    TEXT REFERENCES paiements(id),
    -- Contenu figé (JSON) et son empreinte métier (pour savoir si une facture a changé)
    contenu        TEXT NOT NULL,
    empreinte      TEXT NOT NULL,
    -- Facture qui remplace une facture précédente de la même réservation
    remplace_id    TEXT REFERENCES documents(id),
    emis_le        TEXT NOT NULL,
    emis_par       TEXT NOT NULL REFERENCES utilisateurs(id),
    UNIQUE (type, annee, sequence),
    CHECK ((type = 'recu') = (paiement_id IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX documents_un_recu_par_paiement ON documents(paiement_id) WHERE type = 'recu';
CREATE INDEX idx_documents_reservation ON documents(reservation_id);

CREATE TRIGGER documents_pas_de_modification BEFORE UPDATE ON documents
BEGIN SELECT RAISE(ABORT, 'DOCUMENT_NON_MODIFIABLE'); END;
CREATE TRIGGER documents_pas_de_suppression BEFORE DELETE ON documents
BEGIN SELECT RAISE(ABORT, 'DOCUMENT_NON_MODIFIABLE'); END;
-- Le numéro suivant est toujours le dernier + 1 : jamais de trou ni de retour en arrière.
CREATE TRIGGER documents_numerotation_continue BEFORE INSERT ON documents
WHEN NEW.sequence <> coalesce((SELECT max(sequence) FROM documents WHERE type = NEW.type AND annee = NEW.annee), 0) + 1
BEGIN SELECT RAISE(ABORT, 'NUMEROTATION_INVALIDE'); END;
