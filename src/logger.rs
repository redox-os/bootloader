use log::{LevelFilter, Log, Metadata, Record};

pub static LOGGER: Logger = Logger;

pub struct Logger;

impl Logger {
    pub fn init(&'static self) {
        log::set_logger(self).unwrap();
        log::set_max_level(LevelFilter::Info);
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}
