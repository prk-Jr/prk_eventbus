use crate::domain::error::AppError;
use crate::domain::models::todo::Todo;
use crate::domain::ports::todo_repository::TodoRepository;

#[derive(Clone)]
pub struct TodoService<R: TodoRepository> {
    repo: R,
}

impl<R: TodoRepository> TodoService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub async fn get_all(&self) -> Result<Vec<Todo>, AppError> {
        self.repo.find_all().await
    }
    pub async fn get_by_id(&self, id: i32) -> Result<Option<Todo>, AppError> {
        self.repo.find_by_id(id).await
    }
    pub async fn create(&self, body: Todo) -> Result<Todo, AppError> {
        self.repo.create(body).await
    }
    // Add other methods as needed
}
