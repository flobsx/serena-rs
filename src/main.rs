use clap::{Parser, Subcommand};

/// Serena.rs — MCP Toolkit for Coding Agents
#[derive(Parser, Debug)]
#[command(name = "serena", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the MCP server (stdio transport)
    StartMcpServer {
        /// Path to project root
        #[arg(short, long)]
        project: Option<String>,

        /// Transport protocol (stdio, sse)
        #[arg(short, long, default_value = "stdio")]
        transport: String,
    },
    /// Initialize a new Serena project
    Init {
        /// Project root path
        #[arg(default_value = ".")]
        path: String,
    },
    /// Run setup wizard
    Setup,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::StartMcpServer { project, transport } => {
            let server = serena_mcp::server::McpServer::new();

            // Register all built-in tools (symbol, file, memory, etc.)
            tracing::info!("Registering built-in tools...");
            serena_mcp::server::register_builtin_tools(&server).await;

            tracing::info!(
                "Starting MCP server (transport={transport}, tools={})",
                server.tool_count().await,
            );

            if let Some(p) = &project {
                tracing::info!("  project: {p}");
            }

            server.run_stdio().await;
        }
        Commands::Init { path } => {
            tracing::info!("Initializing Serena project at: {path}");
        }
        Commands::Setup => {
            tracing::info!("Running setup wizard...");
        }
    }
}
