use crate::domain::error::AppError;
use crate::domain::models::user::User;
use crate::domain::ports::user_repository::UserRepository;
use sqlx::sqlite;

#[derive(Clone)]
pub struct SqlxUserRepository {
    pool: sqlx::SqlitePool,
}

impl SqlxUserRepository {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

impl UserRepository for SqlxUserRepository {
    async fn find_all(&self) -> Result<Vec<User>, AppError> {
        let query = User::select().build();
        sqlx::query_as(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)
    }
    async fn find_by_id(&self, id: i32) -> Result<Option<User>, AppError> {
        let query = User::select().where_id(id).build();
        sqlx::query_as(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::from)
    }
    async fn create(&self, body: User) -> Result<User, AppError> {
        let query = User::insert()
            .insert_to_username(body.username.clone())
            .insert_to_email(body.email.clone())
            .build();
        let response = sqlx::query(&query)
            .execute(&self.pool)
            .await
            .map_err(AppError::from)?;
        let id = response.last_insert_rowid() as i32;
        Ok(User {
            id,
            username: body.username,
            email: body.email,
        })
    }
}
