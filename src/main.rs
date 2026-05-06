use anyhow::Result;
use clap::{Parser, Subcommand};
use reqwest::StatusCode;
use tokio::process::Command;
use tokio::time::Duration;

use agentsh::banner;
use agentsh::config::Config;
use agentsh::context::PermissionMode;
use agentsh::history::HistoryDb;
use agentsh::llm_setup;
use agentsh::prompt_ui;
use agentsh::repl;
use agentsh::shell_rc;

#[derive(Debug, Parser)]
#[command(name = "agentsh", version, about = "Make any terminal agentic with local LLMs")]
struct Cli {
	#[arg(long)]
	model: Option<String>,
	#[arg(long)]
	base_url: Option<String>,
	#[arg(long)]
	allow_all: bool,
	#[arg(long = "config")]
	print_config_path: bool,
	#[command(subcommand)]
	command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
	Setup,
	Deactivate,
	SelectModel,
}

#[tokio::main]
async fn main() -> Result<()> {
	std::env::set_var("AGENTSH_ACTIVE", "1");

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
		Some(Commands::Deactivate) => run_deactivate(),
		Some(Commands::SelectModel) => run_select_model(&config).await,
		None => {
			banner::print_startup_banner(
				&config.llm.model,
				runtime_name(&config.llm.base_url),
				runtime_connected(&config.llm.base_url).await,
			)?;
			let history_db = HistoryDb::open()?;
			let permission_mode = if cli.allow_all {
				PermissionMode::AutoApprove { countdown_secs: 2 }
			} else {
				PermissionMode::PerPlan
			};
			repl::run(config, history_db, permission_mode).await
		}
	}
}

fn run_deactivate() -> Result<()> {
	let results = shell_rc::deactivate_for_current_shell()?;
	let mut removed_any = false;

	for result in results {
		if result.removed {
			removed_any = true;
			prompt_ui::print_text(&format!(
				"✓ Removed auto-activation from {}\n  AgentSH will no longer start automatically.\n  You can still run it manually with: agentsh",
				shell_rc::display_path(&result.path)
			));
		} else {
			prompt_ui::print_info(&format!(
				"No AgentSH auto-activation block found in {}.",
				shell_rc::display_path(&result.path)
			));
		}
	}

	if !removed_any {
		prompt_ui::print_info("AgentSH auto-activation was already disabled.");
	}

	Ok(())
}

async fn run_select_model(config: &Config) -> Result<()> {
	let selection = llm_setup::select_model_interactively(config).await?;
	llm_setup::write_selection(&selection)?;

	prompt_ui::print_text(&format!(
		"✓ Selected: {} via {}\n  Config updated at {}\n  Change takes effect on next AgentSH start.",
		selection.model,
		selection.runtime.name,
		Config::config_path()?.display()
	));

	Ok(())
}

async fn runtime_connected(base_url: &str) -> bool {
	let client = match reqwest::Client::builder()
		.timeout(Duration::from_secs(2))
		.build()
	{
		Ok(client) => client,
		Err(_) => return false,
	};

	match client.get(format!("{base_url}/models")).send().await {
		Ok(response) => response.status() == StatusCode::OK,
		Err(_) => false,
	}
}

fn runtime_name(base_url: &str) -> &'static str {
	match base_url {
		"http://localhost:11434/v1" => "Ollama",
		"http://localhost:1234/v1" => "LM Studio",
		"http://localhost:8080/v1" => "Local runtime",
		"http://localhost:1337/v1" => "Jan",
		_ => "Runtime",
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