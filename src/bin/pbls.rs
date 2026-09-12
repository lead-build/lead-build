use clap::{ArgGroup, CommandFactory, Parser, error::ErrorKind};
use lead_build::pbls::Backend;
use std::process::ExitCode;
use tokio::net::TcpListener;
use tower_lsp::{LspService, Server};

#[derive(Parser, Debug)]
#[command(name = "pbls", version, about, long_about = None)]
#[command(group(ArgGroup::new("transport").args(["stdio", "socket"])))]
struct Args {
    /// Communicate over stdio.
    #[arg(long, group = "transport", help_heading = "Transport")]
    stdio: bool,

    /// Communicate over a TCP socket bound to this port on localhost.
    #[arg(
        long,
        group = "transport",
        value_name = "PORT",
        help_heading = "Transport"
    )]
    socket: Option<u16>,

    /// Host to bind the TCP socket to. Only valid together with --socket.
    #[arg(long, value_name = "HOST", help_heading = "Transport")]
    bind_host: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = Args::parse();
    env_logger::init();

    if args.bind_host.is_some() && args.socket.is_none() {
        Args::command()
            .error(
                ErrorKind::MissingRequiredArgument,
                "--bind-host can only be used together with --socket",
            )
            .exit();
    }

    let server_io: Option<(
        Box<dyn tokio::io::AsyncRead + Unpin>,
        Box<dyn tokio::io::AsyncWrite + Unpin>,
    )> = if args.stdio {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        Some((Box::new(stdin), Box::new(stdout)))
    } else if let Some(port) = args.socket {
        let bind_host = args.bind_host.as_deref().unwrap_or("127.0.0.1");
        let listener = match TcpListener::bind((bind_host, port)).await {
            Ok(listener) => listener,
            Err(err) => {
                eprintln!("pbls: failed to bind to {bind_host}:{port}: {err}");
                return ExitCode::FAILURE;
            }
        };
        let (stream, _) = match listener.accept().await {
            Ok(conn) => conn,
            Err(err) => {
                eprintln!("pbls: failed to accept connection: {err}");
                return ExitCode::FAILURE;
            }
        };
        let (read, write) = stream.into_split();
        Some((Box::new(read), Box::new(write)))
    } else {
        None
    };

    if let Some((read, write)) = server_io {
        let (service, socket) = LspService::new(Backend::new);
        Server::new(read, write, socket).serve(service).await;
        ExitCode::SUCCESS
    } else {
        eprintln!("pbls is the lead-build language server. It is not meant to be run directly.");
        eprintln!("Use --help for more information.");
        ExitCode::FAILURE
    }
}
