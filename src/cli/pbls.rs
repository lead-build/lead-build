use clap::{ArgGroup, Parser};

#[derive(Parser, Debug)]
#[command(
    name = "pbls",
    version,
    about = "Language server for build descriptions",
    long_about = None
)]
#[command(group(ArgGroup::new("transport").args(["stdio", "socket"])))]
pub struct Args {
    /// Communicate over stdio.
    #[arg(long, group = "transport", help_heading = "Transport")]
    pub stdio: bool,

    /// Communicate over a TCP socket bound to this port on localhost.
    #[arg(
        long,
        group = "transport",
        value_name = "PORT",
        help_heading = "Transport"
    )]
    pub socket: Option<u16>,

    /// Host to bind the TCP socket to. Only valid together with --socket.
    #[arg(long, value_name = "HOST", help_heading = "Transport")]
    pub bind_host: Option<String>,
}
