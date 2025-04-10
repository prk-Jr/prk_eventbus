
use dotenvy::dotenv;
use sqlx::{migrate::MigrateDatabase, *};
use std::env;
pub async fn connect_to_database() -> Result<SqlitePool, sqlx::Error> {
    dotenv().ok();
    let path = &env::var("DATABASE_URL").expect("Env var unavailable");
    if !Sqlite::database_exists(path).await.unwrap_or(false) {
        println!("Creating database {}", path);
        match Sqlite::create_database(path).await {
            Ok(_) => println!("Create db success"),
            Err(error) => panic!("error: {}", error),
        }
    } else {
        println!("Database already exists");
    }
    let pool =  SqlitePool::connect(path).await?;

    // Run migrations
    migrate(pool.clone()).await?;

    Ok(pool)
}


pub async fn migrate(pool: Pool<Sqlite>) -> Result<(), sqlx::Error> {
    
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS todos (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            task TEXT NOT NULL,
            description TEXT
        )"
    )
    .execute(&pool)
    .await?;

    // Create users table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL,
            email TEXT NOT NULL
        )"
    )
    .execute(&pool)
    .await?;
    Ok(())
}