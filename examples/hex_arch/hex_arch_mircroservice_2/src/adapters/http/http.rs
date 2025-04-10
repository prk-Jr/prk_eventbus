
use crate::application::services;
use crate::domain::error::AppError;
use crate::domain::*;
use crate::infrastructure::*;
use actix_web::{web, App, HttpResponse};
use anyhow::Context;
use serde::*;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpServerConfig {
    pub port: String,
}

#[derive(Clone)]
pub struct AppState {
    pub user_service: Arc<services::UserService<SqlxUserRepository>>,
    pub todo_service: Arc<services::TodoService<SqlxTodoRepository>>,
}

pub async fn get_users(state: web::Data<AppState>) -> Result<web::Json<Vec<User>>, AppError> {
    let items = state.user_service.get_all().await?;
    Ok(web::Json(items))
}

pub async fn post_users(
    state: web::Data<AppState>,
    body: web::Json<User>,
) -> Result<web::Json<User>, AppError> {
    let item = state.user_service.create(body.into_inner()).await?;
    Ok(web::Json(item))
}

pub async fn get_todos(state: web::Data<AppState>) -> Result<web::Json<Vec<Todo>>, AppError> {
    let items = state.todo_service.get_all().await?;
    Ok(web::Json(items))
}

pub async fn post_todos(
    state: web::Data<AppState>,
    body: web::Json<Todo>,
) -> Result<web::Json<Todo>, AppError> {
    let item = state.todo_service.create(body.into_inner()).await?;
    Ok(web::Json(item))
}

#[derive(Deserialize)]
pub struct TodoPathParams {
    pub id: i32,
}

pub async fn get_todos_by_id(
    state: web::Data<AppState>,
    path: web::Path<TodoPathParams>,
) -> Result<web::Json<Todo>, AppError> {
    let item = state
        .todo_service
        .get_by_id(path.id)
        .await?
        .ok_or(AppError::NotFound(format!("Not found")))?;
    Ok(web::Json(item))
}

pub struct HttpServer {
    user_service: services::UserService<SqlxUserRepository>,
    todo_service: services::TodoService<SqlxTodoRepository>,
    config: HttpServerConfig,
}

impl HttpServer {
    pub async fn new(
        user_service: services::UserService<SqlxUserRepository>,
        todo_service: services::TodoService<SqlxTodoRepository>,
        config: HttpServerConfig,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            config,
            user_service: user_service,
            todo_service: todo_service,
        })
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let state = web::Data::new(AppState {
            user_service: Arc::new(self.user_service),
            todo_service: Arc::new(self.todo_service),
        });

        actix_web::HttpServer::new(move || {
            App::new()
                .wrap(tracing_actix_web::TracingLogger::default())
                .app_data(state.clone())
                .route("/health", web::get().to(health_route))
                .service(
                    web::scope("/api")
                        .route("/users", web::get().to(get_users))
                        .route("/users", web::post().to(post_users))
                        .route("/todos", web::get().to(get_todos))
                        .route("/todos", web::post().to(post_todos))
                        .route("/todos/{id}", web::get().to(get_todos_by_id)),
                )
        })
        .bind(format!(
            "0.0.0.0:{}",
            self.config.port.parse::<u16>().unwrap_or(3000)
        ))?
        .run()
        .await
        .context("received error from running server")?;
        Ok(())
    }
}

async fn health_route() -> impl actix_web::Responder {
    HttpResponse::Ok().body("OK")
}
