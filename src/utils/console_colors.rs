use colored::Colorize;
use std::io::{self, Write};

pub struct ConsoleColors;

impl ConsoleColors {
    pub fn write_success(message: &str) {
        println!("{}", message.green());
    }

    pub fn write_error(message: &str) {
        println!("{}", message.red());
    }

    pub fn write_warning(message: &str) {
        println!("{}", message.yellow());
    }

    pub fn write_header(message: &str) {
        println!("{}", message.magenta());
    }

    pub fn write_plain(message: &str) {
        println!("{}", message);
    }

    pub fn write_inline_info(message: &str) {
        print!("{}", message.cyan());
        io::stdout().flush().unwrap();
    }
}
