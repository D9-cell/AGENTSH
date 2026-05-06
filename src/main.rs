use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::process::Command;

use agentsh::config::Config;
use agentsh::history::HistoryDb;
use agentsh::prompt_ui;
use agentsh::repl;

#[derive(Debug, Parser)]
#[command(name = "agentsh", version, about = "Make any terminal agentic with local LLMs")]
struct Cli {
	#[arg(long)]
	model: Option<String>,
	#[arg(long)]
	base_url: Option<String>,
	#[arg(long = "config")]
	print_config_path: bool,
	#[command(subcommand)]
	command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
	Setup,
}

#[tokio::main]
async fn main() -> Result<()> {
	let cli = Cli::parse();
	let config_path = Config::config_path()?;
	let created_config = !config_path.exists();

	let mut config = Config::load()?;
	config.apply_overrides(cli.model, cli.base_url);

	if created_config {
		prompt_ui::print_info(&format!("Created default config at {}", config_path.display()));
	}

	if cli.print_config_path {
		println!("{}", config_path.display());
		return Ok(());
	}

	match cli.command {
		Some(Commands::Setup) => run_setup(&config).await,
		None => {
			let history_db = HistoryDb::open()?;
			repl::run(config, history_db).await
		}
	}
}

async fn run_setup(config: &Config) -> Result<()> {
	prompt_ui::print_info("Checking for Ollama...");
	let has_ollama = Command::new("ollama")
		.arg("--version")
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.status()
		.await
		.map(|status| status.success())
		.unwrap_or(false);

	if !has_ollama {
		prompt_ui::print_error("Ollama is not available in PATH.");
		return Ok(());
	}

	prompt_ui::print_info(&format!("Pulling model {}...", config.llm.model));
	let status = Command::new("ollama")
		.arg("pull")
		.arg(&config.llm.model)
		.stdin(std::process::Stdio::inherit())
		.stdout(std::process::Stdio::inherit())
		.stderr(std::process::Stdio::inherit())
		.status()
		.await?;

	if status.success() {
		prompt_ui::print_info("Setup complete.");
	} else {
		prompt_ui::print_error("Ollama model pull failed.");
	}

	Ok(())
}