use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct BatchConfig {
    pub max_rows: usize,
    pub max_age: Duration,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_rows: 50_000,
            max_age: Duration::from_secs(30),
        }
    }
}

pub struct BatchBuffer<T> {
    config: BatchConfig,
    items: Vec<T>,
    opened_at: Instant,
}

impl<T> BatchBuffer<T> {
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            items: Vec::new(),
            opened_at: Instant::now(),
        }
    }

    pub fn push(&mut self, item: T) {
        if self.items.is_empty() {
            self.opened_at = Instant::now();
        }
        self.items.push(item);
    }

    pub fn should_flush(&self) -> bool {
        !self.items.is_empty()
            && (self.items.len() >= self.config.max_rows
                || self.opened_at.elapsed() >= self.config.max_age)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn drain(&mut self) -> Vec<T> {
        self.opened_at = Instant::now();
        std::mem::take(&mut self.items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flushes_on_row_count_threshold() {
        let mut buf = BatchBuffer::new(BatchConfig {
            max_rows: 3,
            max_age: Duration::from_secs(3600),
        });
        buf.push(1);
        buf.push(2);
        assert!(!buf.should_flush());
        buf.push(3);
        assert!(buf.should_flush());
        assert_eq!(buf.drain(), vec![1, 2, 3]);
        assert!(!buf.should_flush());
        assert!(buf.is_empty());
    }

    #[test]
    fn flushes_on_age_threshold() {
        let mut buf: BatchBuffer<i32> = BatchBuffer::new(BatchConfig {
            max_rows: 50_000,
            max_age: Duration::from_millis(1),
        });
        buf.push(1);
        std::thread::sleep(Duration::from_millis(5));
        assert!(buf.should_flush());
    }

    #[test]
    fn empty_buffer_never_flushes() {
        let buf: BatchBuffer<i32> = BatchBuffer::new(BatchConfig {
            max_rows: 0,
            max_age: Duration::from_secs(0),
        });
        assert!(!buf.should_flush());
    }
}
