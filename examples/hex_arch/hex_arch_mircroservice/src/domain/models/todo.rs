use prkorm::Table;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Table, Default, FromRow)]
#[table_name("todos")]
#[primary_key("id")]
pub struct Todo {
    pub id: i32,
    pub task: String,
    pub description: Option<String>,
}
