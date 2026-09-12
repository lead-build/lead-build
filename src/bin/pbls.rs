use clap::Parser;
use lead_build::pbls::Backend;
use tower_lsp::{LspService, Server};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Communicate over stdio. This is the only transport pbls supports;
    /// the flag is accepted (and required by nothing else) because clients
    /// such as vscode-languageclient pass it by convention when launching a
    /// server for the stdio transport.
    #[arg(long)]
    stdio: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let _args = Args::parse();
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
