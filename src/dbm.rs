use crate::parser;

use rusqlite::{self, Connection};
use std::error::Error;

fn get_conn() -> Result<Connection, Box<dyn Error>> {
    Ok(Connection::open("slug.db")?)
}

fn insert(conn: &mut Connection, data: &parser::PerfData) -> Result<(), Box<dyn Error>> {
    conn.execute(
        format!(
            "CREATE TABLE IF NOT EXISTS {} (
                  min             REAL,
                  max             REAL
                  )", data.name).as_str(),
        (),
    )?;

    conn.execute(
        format!(
            "INSERT INTO {} (min, max) VALUES (?1, ?2)", data.name).as_str(),
        (&data.min, &data.max)
    )?;

    Ok(())
}

