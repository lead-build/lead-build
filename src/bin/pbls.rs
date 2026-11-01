use clap::{CommandFactory, Parser, error::ErrorKind};
use lead_build::{cli::pbls::Args, pbls::Backend};
use std::process::ExitCode;
use tokio::net::TcpListener;
use tower_lsp::{LspService, Server};

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
