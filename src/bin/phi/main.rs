mod approval;
mod args;
mod bridge_serve;
mod init;
mod metrics;
mod run;
mod serve;
mod tools;

use std::sync::Arc;

use anyhow::Result;
use approval::CliApprovalHandler;
use args::{CliArgs, OutputFormatArg, SubCommand};
use clap::Parser;
use phi_agent::config::resolve_llm_config;
use phi_agent::render::OutputFormat;
use phi_agent::{ApprovalMode, AutoApprovalHandler};
use phi_agent::{
    OpenAiClient, PhiAgent, SafetyConfig, TurnFactMiddleware, TurnToolLimitMiddleware, base_agent_builder,
    build_system_prompt,
};
use run::{default_node_id, init_logging, run_one_shot, run_repl};
#[cfg(feature = "shell")]
use tools::LocalShellTool;
#[cfg(feature = "browser")]
use tools::{BrowserConnectionOptions, BrowserLaunchOptions, BrowserToolset, register_browser_tools};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args = CliArgs::parse();

    // Handle subcommands (no agent needed)
    if let Some(cmd) = &args.command {
        match cmd {
            SubCommand::Init { name, lib } => return init::run(name, *lib),
            SubCommand::Metrics { cmd } => return metrics::handle_metrics(cmd, &args),
            SubCommand::Serve { http, bridge } => {
                if *bridge {
                    return bridge_serve::run().await;
                }
                return serve::run(*http).await;
            },
        }
    }

    // 1. Resolve log directory
    let log_dir = args.log_dir.replace("~", &std::env::var("HOME").unwrap_or_default());
    let log_dir_path = std::path::PathBuf::from(&log_dir);

    // 2. Clean up expired sessions
    if !args.no_log {
        match phi_agent::session::cleanup_expired_sessions(&log_dir_path, 7) {
            Ok(count) => {
                if count > 0 {
                    eprintln!("[phi] cleaned up {} expired session(s)", count);
                }
            },
            Err(e) => {
                eprintln!("[phi] warning: failed to cleanup sessions: {}", e);
            },
        }
    }

    // 3. Resolve session
    let session_ctx = phi_agent::session::resolve_session(args.session_id.as_deref(), &log_dir_path)?;
    let session_id_str = session_ctx.session_id.clone();
    let is_new_session = session_ctx.is_new_session;

    // 4. Initialize logging
    if !args.no_log {
        init_logging(&session_ctx, &args.log_level).await?;
    }

    tracing::info!(
        session_id = %session_id_str,
        is_new = is_new_session,
        format = ?args.format,
        "phi starting"
    );

    // 5. Resolve LLM config
    let llm_config = resolve_llm_config(args.model.as_deref(), args.base_url.as_deref())?;

    // 6. Create LLM client
    let llm_client = Arc::new(OpenAiClient::new(
        llm_config.api_key.clone(),
        llm_config.model.clone(),
        Some(llm_config.base_url.clone()),
    ));

    // 7. Build system prompt
    let system_prompt = build_system_prompt();

    // 8. Approval handler
    let approval_handler: Arc<dyn phi_agent::ApprovalHandler> = if args.auto_approve {
        Arc::new(AutoApprovalHandler::new(ApprovalMode::Auto))
    } else {
        Arc::new(CliApprovalHandler::new())
    };

    // 9. Safety config
    let safety_config = SafetyConfig {
        max_tool_calls_per_turn: args.max_tool_calls.unwrap_or(64),
        max_consecutive_failures: args.max_failures.unwrap_or(3),
    };

    // 9.5. Browser setup (if enabled)
    #[cfg(feature = "browser")]
    let _browser = if args.enable_browser || args.connect_ws.is_some() {
        let browser = if let Some(ws_url) = &args.connect_ws {
            let opts = BrowserConnectionOptions::new(ws_url.as_str());
            BrowserToolset::connect(opts).map_err(|e| anyhow::anyhow!("Failed to connect to browser: {}", e))?
        } else {
            let opts = BrowserLaunchOptions::new().headless(!args.headed).window_size(1280, 900);
            BrowserToolset::launch(opts).map_err(|e| anyhow::anyhow!("Failed to launch browser: {}", e))?
        };

        if args.headed {
            eprintln!("[phi] browser launched (headed mode)");
        } else {
            eprintln!("[phi] browser launched (headless)");
        }

        Some(browser)
    } else {
        None
    };

    // 10. Output format
    let output_format = match args.format {
        OutputFormatArg::Terminal => OutputFormat::Terminal {
            show_thinking: !args.no_thinking,
            show_tool_args: !args.no_tool_args,
            color: !args.no_color,
        },
        OutputFormatArg::Json => OutputFormat::Json,
        OutputFormatArg::Quiet => OutputFormat::Quiet,
    };

    // 11. PhiAgent config
    let agent_config = phi_agent::PhiAgentConfig {
        model: llm_config.model.clone(),
        enable_thinking: !args.no_thinking,
        thinking_budget: args.thinking_budget,
        thinking_effort: args.thinking_effort.clone().into(),
        safety: safety_config.clone(),
        max_turns: args.max_turns,
    };

    // 12. Assemble builder — register tools here
    #[allow(unused_mut)]
    let mut builder = base_agent_builder(llm_client)
        .system_prompt(system_prompt)
        .approval_handler(approval_handler)
        .middleware(TurnFactMiddleware::new())
        .middleware(TurnToolLimitMiddleware::from_config(&safety_config))
        .apply_if(args.thinking_budget, |b, budget| b.thinking_budget(budget))
        .apply_if(args.max_turns, |b, n| b.execution_max_turns(n));

    // Shell tool — only when feature is enabled
    #[cfg(feature = "shell")]
    {
        builder = builder.register_tool(LocalShellTool::new(args.shell_timeout_ms));
    }

    // Register browser tools if enabled
    #[cfg(feature = "browser")]
    if let Some(ref browser) = _browser {
        builder = register_browser_tools(builder, browser);
    }

    // 13. Build agent
    let agent = PhiAgent::build(builder, agent_config)?;
    let agent_session_id = agent.create_session().await;

    tracing::info!(
        agent_session_id = %agent_session_id.id,
        session_id = %session_id_str,
        model = %llm_config.model,
        "session created"
    );

    // 14. Run
    if let Some(query) = args.query {
        // Set up telemetry
        let node_id = std::env::var("PHI_NODE_ID").unwrap_or_else(|_| default_node_id());
        let metrics_enabled = std::env::var("PHI_METRICS_ENABLED")
            .map(|v| {
                let v = v.to_lowercase();
                !matches!(v.as_str(), "false" | "0" | "no" | "off" | "")
            })
            .unwrap_or(true);

        let mut telemetry = if metrics_enabled {
            Some(phi_telemetry::init_telemetry(
                agent.runtime(),
                session_id_str.clone(),
                node_id,
                llm_config.model.clone(),
            ))
        } else {
            None
        };

        let (result, run_outcome) = run_one_shot(&agent, &agent_session_id, &session_ctx, &query, &output_format).await;

        // Finalize and save metrics
        if let Some(handle) = &mut telemetry {
            handle.shutdown().await;
            let session = handle.session.read().await;
            let mut session = session.clone();
            session.finalize(phi_telemetry::types::run_outcome_to_session_outcome(&run_outcome));
            let _ = phi_telemetry::save_metrics(&session, &session_ctx.session_dir);
        }

        result?;
        Ok(())
    } else {
        run_repl(&agent, &agent_session_id, &session_ctx, &output_format).await
    }
}
