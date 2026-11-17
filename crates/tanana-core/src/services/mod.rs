//! Services métier. Chaque fonction publique suit le même motif via
//! `Tanana::executer` : vérifier la session et la permission, agir,
//! écrire le journal, puis valider la transaction.

pub mod argent;
pub mod chambres;
pub mod clients;
pub mod documents;
pub mod reservations;
pub mod systeme;
pub mod taxes;
