#!/bin/sh
# `serve` needs the symbol registry to already exist (root README: "иначе
# /ohlcv, /analytics/* и /stream/* будут всегда отвечать 404
# unknown_symbol") — `symbols --refresh` is what creates `meta.duckdb` and
# its views in the first place (MetaStore::open_writable). Bootstraps that
# once, only if the file isn't there yet, so `docker compose up` on a
# fresh volume works without an undocumented manual step first. Does
# *not* run on every start: that would make every container restart
# depend on reaching Binance, which the non-Docker workflow never
# requires either.
set -e

if [ ! -f "${MA_META_DB_PATH:-/data/meta.duckdb}" ]; then
    echo "backend-entrypoint: no meta.duckdb yet, running 'symbols --refresh'..."
    market-analyzer symbols --refresh
fi

exec market-analyzer "$@"
