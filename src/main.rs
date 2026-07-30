use clap::{Parser, Subcommand};

mod commands;

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
            if let Err(e) = commands::mcp::execute(project.as_deref(), &transport).await {
                tracing::error!(error = %e, "Failed to start MCP server");
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
        Commands::Init { path } => {
            tracing::info!("Initializing Serena project at: {path}");
        }
        Commands::Setup => {
            tracing::info!("Running setup wizard...");
        }
    }
}
