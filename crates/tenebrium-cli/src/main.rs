use clap::Parser;

#[derive(Parser)]
#[command(name = "tenebrium-cli")]
#[command(version = "0.1.0")]
#[command(about = "Tenebrium CLI tool")]
struct Args {}

fn main() {
    let _args = Args::parse();
    // For now, just parse and exit
}
