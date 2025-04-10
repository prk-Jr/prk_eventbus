use crate::domain::error::AppError;
use crate::domain::models::todo::Todo;
use crate::domain::ports::todo_repository::TodoRepository;
use sqlx::sqlite;

#[derive(Clone)]
pub struct SqlxTodoRepository {
    pool: sqlx::SqlitePool,
}

impl SqlxTodoRepository {
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self { pool }
    }
}

impl TodoRepository for SqlxTodoRepository {
    async fn find_all(&self) -> Result<Vec<Todo>, AppError> {
        let query = Todo::select().build();
        sqlx::query_as(&query)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)
    }
    async fn find_by_id(&self, id: i32) -> Result<Option<Todo>, AppError> {
        let query = Todo::select().where_id(id).build();
        sqlx::query_as(&query)
            .fetch_optional(&self.pool)
            .await
            .map_err(AppError::from)
    }
    async fn create(&self, body: Todo) -> Result<Todo, AppError> {
        let mut query = Todo::insert().insert_to_task(body.task.clone());
        if body.description.is_some() {
            query = query.insert_to_description(body.description.clone().unwrap());
        }
        let query = query.build();
        let response = sqlx::query(&query)
            .execute(&self.pool)
            .await
            .map_err(AppError::from)?;
        let id = response.last_insert_rowid() as i32;
        Ok(Todo {
            id,
            task: body.task,
            description: body.description.clone(),
        })
    }
    // Implement create, update, delete similarly
}
