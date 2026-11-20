-- Réservations de groupe : un client (une entreprise, une équipe) loue
-- plusieurs chambres. Chaque chambre reste une réservation à part (dates,
-- arrivée, départ, annulation), mais l'argent se compte pour tout le groupe :
-- un seul total, un seul reste à payer, une seule facture.
CREATE TABLE groupes (
    id        TEXT PRIMARY KEY,
    client_id TEXT NOT NULL REFERENCES clients(id),
    -- Nom facultatif : « Équipe SOGEA », « Mariage Ngono »…
    nom       TEXT,
    cree_le   TEXT NOT NULL,
    cree_par  TEXT NOT NULL REFERENCES utilisateurs(id)
) STRICT;

ALTER TABLE reservations ADD COLUMN groupe_id TEXT REFERENCES groupes(id);
CREATE INDEX idx_reservations_groupe ON reservations(groupe_id) WHERE groupe_id IS NOT NULL;

-- Reçus et factures d'un groupe : ils portent sur toutes ses chambres.
ALTER TABLE documents ADD COLUMN groupe_id TEXT REFERENCES groupes(id);
CREATE INDEX idx_documents_groupe ON documents(groupe_id) WHERE groupe_id IS NOT NULL;
