mod code_actions;
mod code_lens;
mod completion;
mod convert;
mod diagnostics;
mod document_highlight;
mod document_link;
mod document_symbols;
mod folding;
mod formatting;
mod goto_definition;
mod hover;
mod inlay_hints;
mod references;
mod rename;
mod semantic_tokens;
mod server;
mod state;
mod type_hierarchy;
mod workspace_symbols;

use tower_lsp_server::{LspService, Server};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(server::SysmlLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
