
use clap::Parser;
use freeports::cli::CliArgs;

fn main() {
    let cli_args = CliArgs::parse();
    if let Err(err) = freeports::cli::execute(cli_args) {
        let message = err.to_string();
        if !message.is_empty() {
            eprintln!("{message}");
        }
        std::process::exit(1);
    }
}
