fn main() {
    if let Err(err) = ccost::cli::run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
