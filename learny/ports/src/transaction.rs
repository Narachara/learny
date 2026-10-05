use crate::RepositoryError;

/// Groups many repository writes into one atomic database transaction —
/// used by bulk operations like deck import so hundreds of inserts share a
/// single commit instead of paying one disk sync each.
///
/// Repository methods that manage their own atomicity must use savepoints
/// (not top-level transactions) so they compose with an open scope.
pub trait TransactionScope: Send {
    fn begin(&self) -> Result<(), RepositoryError>;
    fn commit(&self) -> Result<(), RepositoryError>;
    fn rollback(&self) -> Result<(), RepositoryError>;
}
