use crate::parser::PerfData;

use rusqlite::{self, Connection};
use std::error::Error;

fn get_conn() -> Result<Connection, Box<dyn Error>> {
    Ok(Connection::open("slug.db")?)
}

pub fn insert(data: &PerfData) -> Result<(), Box<dyn Error>> {
    let mut conn = get_conn()?;
    insert_conn(&mut conn, data)
}

fn insert_conn(conn: &mut Connection, data: &PerfData) -> Result<(), Box<dyn Error>> {
    // Create SQL statement to create a table with dynamic columns
    let columns = data.map.keys()
                          .map(|key| format!("{} REAL", key))
                          .collect::<Vec<String>>()
                          .join(", ");
    let create_table_stmt = format!("CREATE TABLE IF NOT EXISTS {} ({})", data.name, columns);
    conn.execute(&create_table_stmt, ())?;

    // Prepare the INSERT statement with dynamic columns
    let keys = data.map.keys().map(|key| key.as_str()).collect::<Vec<&str>>().join(", ");
    let placeholders = data.map.keys().enumerate()
                            .map(|(i, _)| format!("?{}", i + 1))
                            .collect::<Vec<String>>()
                            .join(", ");

    let insert_stmt = format!("INSERT INTO perf_data ({}) VALUES ({})", keys, placeholders);

    // Prepare values for the INSERT statement
    let values: Vec<&dyn rusqlite::ToSql> = data.map.values().map(|v| v as &dyn rusqlite::ToSql).collect();

    conn.execute(&insert_stmt, values.as_slice())?;

    Ok(())
}

