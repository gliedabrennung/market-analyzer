use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanArray, Float64Array, Int32Array, RecordBatch, StringArray,
    TimestampMillisecondArray,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef, TimeUnit};
use arrow::ipc::writer::StreamWriter;
use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use crate::error::ApiError;

pub const ARROW_IPC_CONTENT_TYPE: &str = "application/vnd.apache.arrow.stream";

pub fn wants_arrow(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains(ARROW_IPC_CONTENT_TYPE))
}

pub trait ToRecordBatch {
    fn arrow_schema() -> SchemaRef;

    fn to_record_batch(rows: &[Self]) -> Result<RecordBatch, ApiError>
    where
        Self: Sized;
}

pub fn respond_rows<T>(headers: &HeaderMap, rows: Vec<T>) -> Result<Response, ApiError>
where
    T: Serialize + ToRecordBatch,
{
    if !wants_arrow(headers) {
        return Ok(Json(rows).into_response());
    }

    let schema = T::arrow_schema();
    let record_batch = T::to_record_batch(&rows)?;

    let mut buf = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buf, &schema)
            .map_err(|e| ApiError::Internal(format!("opening Arrow IPC writer: {e}")))?;
        writer
            .write(&record_batch)
            .map_err(|e| ApiError::Internal(format!("writing Arrow IPC batch: {e}")))?;
        writer
            .finish()
            .map_err(|e| ApiError::Internal(format!("finishing Arrow IPC stream: {e}")))?;
    }

    Ok(([(header::CONTENT_TYPE, ARROW_IPC_CONTENT_TYPE)], buf).into_response())
}

fn timestamp_field(name: &str, nullable: bool) -> Field {
    Field::new(
        name,
        DataType::Timestamp(TimeUnit::Millisecond, Some(Arc::from("UTC"))),
        nullable,
    )
}

fn timestamps_ms(values: impl Iterator<Item = DateTime<Utc>>) -> ArrayRef {
    let millis: Vec<i64> = values.map(|t| t.timestamp_millis()).collect();
    Arc::new(TimestampMillisecondArray::from(millis).with_timezone("UTC"))
}

fn decimal_strings(values: impl Iterator<Item = Decimal>) -> ArrayRef {
    Arc::new(StringArray::from_iter_values(values.map(|d| d.to_string())))
}

fn opt_decimal_strings(values: impl Iterator<Item = Option<Decimal>>) -> ArrayRef {
    Arc::new(StringArray::from_iter(
        values.map(|d| d.map(|d| d.to_string())),
    ))
}

fn record_batch(schema: SchemaRef, columns: Vec<ArrayRef>) -> Result<RecordBatch, ApiError> {
    RecordBatch::try_new(schema, columns)
        .map_err(|e| ApiError::Internal(format!("building Arrow record batch: {e}")))
}

impl ToRecordBatch for crate::queries::OhlcvRow {
    fn arrow_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            timestamp_field("open_time", false),
            timestamp_field("close_time", false),
            Field::new("open", DataType::Utf8, false),
            Field::new("high", DataType::Utf8, false),
            Field::new("low", DataType::Utf8, false),
            Field::new("close", DataType::Utf8, false),
            Field::new("volume", DataType::Utf8, false),
            Field::new("quote_volume", DataType::Utf8, false),
            Field::new("trades_count", DataType::Int32, false),
            Field::new("is_closed", DataType::Boolean, false),
        ]))
    }

    fn to_record_batch(rows: &[Self]) -> Result<RecordBatch, ApiError> {
        record_batch(
            Self::arrow_schema(),
            vec![
                timestamps_ms(rows.iter().map(|r| r.open_time)),
                timestamps_ms(rows.iter().map(|r| r.close_time)),
                decimal_strings(rows.iter().map(|r| r.open)),
                decimal_strings(rows.iter().map(|r| r.high)),
                decimal_strings(rows.iter().map(|r| r.low)),
                decimal_strings(rows.iter().map(|r| r.close)),
                decimal_strings(rows.iter().map(|r| r.volume)),
                decimal_strings(rows.iter().map(|r| r.quote_volume)),
                Arc::new(Int32Array::from_iter_values(
                    rows.iter().map(|r| r.trades_count),
                )),
                Arc::new(BooleanArray::from_iter(
                    rows.iter().map(|r| Some(r.is_closed)),
                )),
            ],
        )
    }
}

impl ToRecordBatch for ma_analytics::VwapPoint {
    fn arrow_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            timestamp_field("open_time", false),
            Field::new("close", DataType::Utf8, false),
            Field::new("vwap", DataType::Utf8, true),
        ]))
    }

    fn to_record_batch(rows: &[Self]) -> Result<RecordBatch, ApiError> {
        record_batch(
            Self::arrow_schema(),
            vec![
                timestamps_ms(rows.iter().map(|r| r.open_time)),
                decimal_strings(rows.iter().map(|r| r.close)),
                opt_decimal_strings(rows.iter().map(|r| r.vwap)),
            ],
        )
    }
}

impl ToRecordBatch for ma_analytics::VolatilityPoint {
    fn arrow_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            timestamp_field("open_time", false),
            Field::new("realized_volatility", DataType::Float64, true),
        ]))
    }

    fn to_record_batch(rows: &[Self]) -> Result<RecordBatch, ApiError> {
        record_batch(
            Self::arrow_schema(),
            vec![
                timestamps_ms(rows.iter().map(|r| r.open_time)),
                Arc::new(Float64Array::from_iter(
                    rows.iter().map(|r| r.realized_volatility),
                )),
            ],
        )
    }
}

impl ToRecordBatch for ma_analytics::VolumeAnomaly {
    fn arrow_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            timestamp_field("open_time", false),
            Field::new("volume", DataType::Utf8, false),
            Field::new("z_score", DataType::Float64, false),
        ]))
    }

    fn to_record_batch(rows: &[Self]) -> Result<RecordBatch, ApiError> {
        record_batch(
            Self::arrow_schema(),
            vec![
                timestamps_ms(rows.iter().map(|r| r.open_time)),
                decimal_strings(rows.iter().map(|r| r.volume)),
                Arc::new(Float64Array::from_iter_values(
                    rows.iter().map(|r| r.z_score),
                )),
            ],
        )
    }
}

impl ToRecordBatch for ma_analytics::OfiBucket {
    fn arrow_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            timestamp_field("bucket", false),
            Field::new("buy_volume", DataType::Utf8, false),
            Field::new("sell_volume", DataType::Utf8, false),
            Field::new("ofi", DataType::Float64, false),
        ]))
    }

    fn to_record_batch(rows: &[Self]) -> Result<RecordBatch, ApiError> {
        record_batch(
            Self::arrow_schema(),
            vec![
                timestamps_ms(rows.iter().map(|r| r.bucket)),
                decimal_strings(rows.iter().map(|r| r.buy_volume)),
                decimal_strings(rows.iter().map(|r| r.sell_volume)),
                Arc::new(Float64Array::from_iter_values(rows.iter().map(|r| r.ofi))),
            ],
        )
    }
}

impl ToRecordBatch for ma_analytics::CorrelationPair {
    fn arrow_schema() -> SchemaRef {
        Arc::new(Schema::new(vec![
            Field::new("symbol_a", DataType::Utf8, false),
            Field::new("symbol_b", DataType::Utf8, false),
            Field::new("correlation", DataType::Float64, true),
        ]))
    }

    fn to_record_batch(rows: &[Self]) -> Result<RecordBatch, ApiError> {
        record_batch(
            Self::arrow_schema(),
            vec![
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.symbol_a.as_str()),
                )),
                Arc::new(StringArray::from_iter_values(
                    rows.iter().map(|r| r.symbol_b.as_str()),
                )),
                Arc::new(Float64Array::from_iter(rows.iter().map(|r| r.correlation))),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::ipc::reader::StreamReader;
    use axum::http::HeaderValue;
    use std::str::FromStr;

    #[test]
    fn accept_header_arrow_is_detected() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/vnd.apache.arrow.stream"),
        );
        assert!(wants_arrow(&headers));
    }

    #[test]
    fn accept_header_json_or_absent_is_not_arrow() {
        assert!(!wants_arrow(&HeaderMap::new()));

        let mut headers = HeaderMap::new();
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
        assert!(!wants_arrow(&headers));
    }

    #[test]
    fn round_trips_ohlcv_rows_through_ipc_and_back() {
        let row = crate::queries::OhlcvRow {
            open_time: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            close_time: DateTime::from_timestamp(1_700_000_060, 0).unwrap(),
            open: Decimal::from_str("100.00000001").unwrap(),
            high: Decimal::from_str("101.5").unwrap(),
            low: Decimal::from_str("99.5").unwrap(),
            close: Decimal::from_str("100.5").unwrap(),
            volume: Decimal::from_str("42.12345678").unwrap(),
            quote_volume: Decimal::from_str("4234.5").unwrap(),
            trades_count: 7,
            is_closed: true,
        };

        let batch = <crate::queries::OhlcvRow as ToRecordBatch>::to_record_batch(
            std::slice::from_ref(&row),
        )
        .unwrap();
        let schema = <crate::queries::OhlcvRow as ToRecordBatch>::arrow_schema();

        let mut buf = Vec::new();
        {
            let mut writer = StreamWriter::try_new(&mut buf, &schema).unwrap();
            writer.write(&batch).unwrap();
            writer.finish().unwrap();
        }

        let mut reader = StreamReader::try_new(std::io::Cursor::new(buf), None).unwrap();
        let read_back = reader.next().unwrap().unwrap();
        assert_eq!(read_back.num_rows(), 1);

        let open_col = read_back
            .column(2)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();

        assert_eq!(open_col.value(0), "100.00000001");
    }
}
