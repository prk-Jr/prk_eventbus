use prkorm::Table;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Table, Default, FromRow)]
#[table_name("users")]
#[primary_key("id")]
pub struct User {
    pub id: i32,
    pub username: String,
    pub email: String,
}
