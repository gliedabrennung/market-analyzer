
CREATE TABLE IF NOT EXISTS schema_version (
    version BIGINT NOT NULL
);

INSERT INTO schema_version (version)
SELECT 1
WHERE NOT EXISTS (SELECT 1 FROM schema_version);

CREATE TABLE IF NOT EXISTS symbols (
    exchange VARCHAR NOT NULL,
    symbol VARCHAR NOT NULL,
    base_asset VARCHAR NOT NULL,
    quote_asset VARCHAR NOT NULL,
    status VARCHAR NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    PRIMARY KEY (exchange, symbol)
);

CREATE TABLE IF NOT EXISTS collector_state (
    exchange VARCHAR NOT NULL,
    symbol VARCHAR NOT NULL,
    dataset VARCHAR NOT NULL,
    interval VARCHAR,
    last_ts TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
