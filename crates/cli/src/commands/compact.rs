use anyhow::{Context, Result};

use ma_storage::{compact_partition, discover_partitions};

use crate::config::AppConfig;

use super::CompactArgs;

/// FR-2.5: merge every partition's `part-*.parquet` files into one. With no
/// filters, sweeps every partition in both datasets.
pub fn run(args: CompactArgs, config: &AppConfig) -> Result<()> {
    let symbol_filter = args.symbol.as_deref();
    let date_filter = args.date;

    let mut partitions_touched = 0usize;
    let mut total_files_before = 0usize;
    let mut total_files_after = 0usize;
    let mut total_bytes_before = 0u64;
    let mut total_bytes_after = 0u64;

    for (dataset, order_by_column) in [("klines", "open_time"), ("trades", "ts")] {
        let dirs = discover_partitions(&config.data_dir, dataset, symbol_filter, date_filter)
            .with_context(|| format!("discovering {dataset} partitions"))?;

        for dir in dirs {
            let result = compact_partition(&dir, order_by_column)
                .with_context(|| format!("compacting {}", dir.display()))?;

            if result.files_before > 1 {
                partitions_touched += 1;
                println!(
                    "{}: {} files -> {} file, {} rows, {} -> {} bytes",
                    dir.display(),
                    result.files_before,
                    result.files_after,
                    result.rows,
                    result.bytes_before,
                    result.bytes_after
                );
            }

            total_files_before += result.files_before;
            total_files_after += result.files_after;
            total_bytes_before += result.bytes_before;
            total_bytes_after += result.bytes_after;
        }
    }

    println!(
        "compacted {partitions_touched} partition(s): {total_files_before} -> {total_files_after} files, \
         {total_bytes_before} -> {total_bytes_after} bytes total"
    );
    Ok(())
}
