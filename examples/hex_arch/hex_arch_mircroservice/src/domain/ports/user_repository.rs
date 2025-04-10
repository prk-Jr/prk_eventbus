use crate::domain::error::AppError;
use crate::domain::models::user::User;
use std::future::Future;

pub trait UserRepository: Send + Sync + 'static {
    fn find_all(&self) -> impl Future<Output = Result<Vec<User>, AppError>> + Send;
    fn find_by_id(&self, id: i32) -> impl Future<Output = Result<Option<User>, AppError>> + Send;
    fn create(&self, body: User) -> impl Future<Output = Result<User, AppError>> + Send;
}
