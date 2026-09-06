use anyhow::Result;
use comfy_table::Table;
use serde::Serialize;
use serde_json::Value;

use crate::commands::OutputFormat;

/// Render `rows` in the requested format (FR-4.2).
pub fn print_rows<T: Serialize>(rows: &[T], format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(rows)?),
        OutputFormat::Table => print_table(&to_values(rows)?),
        OutputFormat::Csv => print_csv(&to_values(rows)?)?,
    }
    Ok(())
}

fn to_values<T: Serialize>(rows: &[T]) -> Result<Vec<Value>> {
    rows.iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

fn headers_of(values: &[Value]) -> Vec<String> {
    values
        .first()
        .and_then(Value::as_object)
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default()
}

fn cell(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn print_table(values: &[Value]) {
    let headers = headers_of(values);
    if headers.is_empty() {
        println!("(no rows)");
        return;
    }
    let mut table = Table::new();
    table.set_header(headers.clone());
    for v in values {
        if let Some(obj) = v.as_object() {
            table.add_row(headers.iter().map(|h| cell(obj.get(h))));
        }
    }
    println!("{table}");
}

fn print_csv(values: &[Value]) -> Result<()> {
    let headers = headers_of(values);
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    if headers.is_empty() {
        wtr.flush()?;
        return Ok(());
    }
    wtr.write_record(&headers)?;
    for v in values {
        if let Some(obj) = v.as_object() {
            let row: Vec<String> = headers.iter().map(|h| cell(obj.get(h))).collect();
            wtr.write_record(&row)?;
        }
    }
    wtr.flush()?;
    Ok(())
}
