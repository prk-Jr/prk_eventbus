mod adapters;
mod application;
mod database_connection;
mod domain;
mod infrastructure;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
            .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
            .init();
    let database_connection = database_connection::connect_to_database()
        .await
        .expect("Could not connect to database");
    let config = adapters::http::http::HttpServerConfig {
        port: "3000".into(),
    };
    let user_repo =
        infrastructure::repositories::SqlxUserRepository::new(database_connection.clone());
    let todo_repo =
        infrastructure::repositories::SqlxTodoRepository::new(database_connection.clone());
    let user_service = application::services::UserService::new(user_repo);
    let todo_service = application::services::TodoService::new(todo_repo);

    let http_server = adapters::http::http::HttpServer::new(user_service, todo_service, config)
        .await
        .expect("Failed to create HTTP server");
    http_server.run().await.expect("Failed to run HTTP server");
}
