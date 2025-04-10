use crate::domain::error::AppError;
use crate::domain::models::todo::Todo;
use std::future::Future;

pub trait TodoRepository: Send + Sync + 'static {
    fn find_all(&self) -> impl Future<Output = Result<Vec<Todo>, AppError>> + Send;
    fn find_by_id(&self, id: i32) -> impl Future<Output = Result<Option<Todo>, AppError>> + Send;
    fn create(&self, body: Todo) -> impl Future<Output = Result<Todo, AppError>> + Send;
}
