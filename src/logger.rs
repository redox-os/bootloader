use core::fmt::Write;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};

use crate::VGA;

pub static LOGGER: Logger = Logger;

pub struct Logger;

impl Logger {
    pub fn init(&'static self) {
        log::set_logger(self).unwrap();
        log::set_max_level(LevelFilter::Info);
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            unsafe {
                writeln!(VGA, "{} - {}", record.level(), record.args()).unwrap();
            }
        }
    }

    fn flush(&self) {}
}
