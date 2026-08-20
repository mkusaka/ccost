use std::io::Write;

fn main() {
    if let Err(err) = ccost::cli::run() {
        let _ = writeln!(std::io::stderr().lock(), "{err}");
        std::process::exit(1);
    }
}
