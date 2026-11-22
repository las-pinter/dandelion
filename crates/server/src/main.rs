//! Entry point of the `dandelion` binary.

use clap::Parser;
use dandelion_core::BuildInfo;

const BUILD: BuildInfo = BuildInfo::new("dandelion", env!("CARGO_PKG_VERSION"));

/// Monitor and control this development machine from the browser.
#[derive(Debug, Parser)]
#[command(name = "dandelion", version = BUILD.version, about)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("{BUILD}: server not implemented yet");
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
