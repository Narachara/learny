use crate::SharedConnection;
use ports::transaction::TransactionScope;
use ports::RepositoryError;

/// Explicit transaction control over the shared connection. `begin` takes the
/// write lock immediately so a bulk operation can't deadlock against another
/// writer mid-way through.
pub struct SqliteTransactionScope {
    conn: SharedConnection,
}

impl SqliteTransactionScope {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }

    fn exec(&self, sql: &str) -> Result<(), RepositoryError> {
        let conn = self.conn.lock()
            .map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;
        conn.execute_batch(sql)
            .map_err(|e| RepositoryError::Internal(e.to_string()))
    }
}

impl TransactionScope for SqliteTransactionScope {
    fn begin(&self) -> Result<(), RepositoryError> {
        self.exec("BEGIN IMMEDIATE;")
    }

    fn commit(&self) -> Result<(), RepositoryError> {
        self.exec("COMMIT;")
    }

    fn rollback(&self) -> Result<(), RepositoryError> {
        self.exec("ROLLBACK;")
    }
}
