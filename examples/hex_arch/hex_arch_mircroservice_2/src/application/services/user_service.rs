use crate::domain::error::AppError;
use crate::domain::models::user::User;
use crate::domain::ports::user_repository::UserRepository;

#[derive(Clone)]
pub struct UserService<R: UserRepository> {
    repo: R,
}

impl<R: UserRepository> UserService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub async fn get_all(&self) -> Result<Vec<User>, AppError> {
        self.repo.find_all().await
    }
    pub async fn get_by_id(&self, id: i32) -> Result<Option<User>, AppError> {
        self.repo.find_by_id(id).await
    }
    pub async fn create(&self, body: User) -> Result<User, AppError> {
        self.repo.create(body).await
    }
    // Add other methods as needed
}
