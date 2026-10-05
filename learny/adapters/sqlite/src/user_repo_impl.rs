use rusqlite::params;
use uuid::Uuid;
use domain_auth::User;
use ports::user_repo::UserRepository;
use ports::RepositoryError;
use crate::SharedConnection;

pub struct SqliteUserRepository {
    conn: SharedConnection,
}

impl SqliteUserRepository {
    pub fn new(conn: SharedConnection) -> Self {
        Self { conn }
    }
}

fn map_err(e: rusqlite::Error) -> RepositoryError {
    RepositoryError::Internal(e.to_string())
}

fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id:            row.get(0)?,
        username:      row.get(1)?,
        password_hash: row.get(2)?,
    })
}

impl UserRepository for SqliteUserRepository {
    fn find_by_id(&self, id: &str) -> Result<Option<User>, RepositoryError> {
        let conn = self.conn.lock()
            .map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        match conn.query_row(
            "SELECT id, username, password_hash FROM user WHERE id = ?1",
            [id],
            row_to_user,
        ) {
            Ok(u)                                       => Ok(Some(u)),
            Err(rusqlite::Error::QueryReturnedNoRows)   => Ok(None),
            Err(e)                                      => Err(map_err(e)),
        }
    }

    fn find_by_username(&self, username: &str) -> Result<Option<User>, RepositoryError> {
        let conn = self.conn.lock()
            .map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        match conn.query_row(
            "SELECT id, username, password_hash FROM user WHERE username = ?1",
            [username],
            row_to_user,
        ) {
            Ok(u)                                       => Ok(Some(u)),
            Err(rusqlite::Error::QueryReturnedNoRows)   => Ok(None),
            Err(e)                                      => Err(map_err(e)),
        }
    }

    fn create(&self, username: &str, password_hash: &str) -> Result<User, RepositoryError> {
        let conn = self.conn.lock()
            .map_err(|_| RepositoryError::Internal("lock poisoned".into()))?;

        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO user (id, username, password_hash) VALUES (?1, ?2, ?3)",
            params![id, username, password_hash],
        ).map_err(|e| match e {
            rusqlite::Error::SqliteFailure(ref err, _)
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                    RepositoryError::DuplicateName,
            other => map_err(other),
        })?;

        Ok(User { id, username: username.to_string(), password_hash: password_hash.to_string() })
    }
}
