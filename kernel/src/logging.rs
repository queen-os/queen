use core::fmt::{self, Write};
use log::{Level, LevelFilter, Log};
use spin::Mutex;

static LOG_LOCK: Mutex<()> = Mutex::new(());

pub fn init() {
    static LOGGER: SimpleLogger = SimpleLogger;
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(match option_env!("LOG") {
        Some("error") => LevelFilter::Error,
        Some("warn") => LevelFilter::Warn,
        Some("info") => LevelFilter::Info,
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        _ => LevelFilter::Info,
    });
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _guard = LOG_LOCK.lock();
    crate::arch::bsp::uart::uart().write_fmt(args).unwrap();
}

/// Prints without a newline.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::logging::_print(format_args!($($arg)*)));
}

/// Prints with a newline.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ({
        $crate::logging::_print(format_args_nl!($($arg)*));
    })
}

#[allow(unused)]
macro_rules! dbg {
    () => {
        $crate::println!("[{}:{}]", core::file!(), core::line!());
    };
    ($val:expr $(,)?) => {
        // Use of `match` here is intentional because it affects the lifetimes
        // of temporaries - https://stackoverflow.com/a/48732525/1063961
        match $val {
            tmp => {
                $crate::println!("[{}:{}] {} = {:#?}",
                    core::file!(), core::line!(), core::stringify!($val), &tmp);
                tmp
            }
        }
    };
    ($($val:expr),+ $(,)?) => {
        ($($crate::dbg!($val)),+,)
    };
}

/// Add escape sequence to print with color in Linux console
macro_rules! with_color {
    ($args: ident, $color_code: ident) => {{
        format_args!("\u{1B}[{}m{}\u{1B}[0m", $color_code as u8, $args)
    }};
}

fn print_with_color(args: fmt::Arguments, color_code: u8) {
    _print(with_color!(args, color_code));
}

struct SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        print_with_color(
            format_args!(
                "[{:>5}][CPU-{}]: {}\n",
                record.level(),
                crate::arch::cpu::id(),
                record.args()
            ),
            level_to_color_code(record.level()),
        );
    }

    fn flush(&self) {}
}

fn level_to_color_code(level: Level) -> u8 {
    match level {
        Level::Error => 31, // Red
        Level::Warn => 93,  // BrightYellow
        Level::Info => 34,  // Blue
        Level::Debug => 32, // Green
        Level::Trace => 90, // BrightBlack
    }
}
