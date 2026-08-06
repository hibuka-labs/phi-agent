//! ``phi serve`` — stdio NDJSON headless mode for SDK consumption.
//! Debug logs: ``~/.phi-agent/serve-debug.log``

use std::sync::Arc;

use phi_agent::bridge::messages::{IncomingMessage, OutgoingMessage, PROTOCOL_VERSION};
use phi_agent::bridge::server::ProtocolServer;
use phi_agent::config::resolve_llm_config;
use phi_agent::{
    ApprovalMode, AutoApprovalHandler, OpenAiClient, SafetyConfig, TurnFactMiddleware, TurnToolLimitMiddleware,
    base_agent_builder, build_system_prompt,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};

macro_rules! dbg_log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(
            std::env::var("HOME").map(|h| format!("{}/.phi-agent/serve-debug.log", h))
                .unwrap_or_else(|_| "serve-debug.log".to_string())
        ).map(|mut f| { let _ = writeln!(f, "[{}] {}", std::process::id(), format!($($arg)*)); });
    }};
}

pub async fn run() -> anyhow::Result<()> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let _log_dir = std::path::PathBuf::from(&home).join(".phi-agent");
    std::fs::create_dir_all(&_log_dir)?;

    let llm_config = resolve_llm_config(None, None)?;
    let llm_client = Arc::new(OpenAiClient::new(
        llm_config.api_key.clone(),
        llm_config.model.clone(),
        Some(llm_config.base_url.clone()),
    ));

    let builder = base_agent_builder(llm_client)
        .system_prompt(build_system_prompt())
        .approval_handler(Arc::new(AutoApprovalHandler::new(ApprovalMode::Auto)))
        .middleware(TurnFactMiddleware::new())
        .middleware(TurnToolLimitMiddleware::from_config(&SafetyConfig::default()));
    let server = ProtocolServer::from_builder(builder)?;
    let tool_count = server.list_tools().await.len();
    eprintln!("Bridge server ready. {tool_count} tools registered. Listening on stdin...");

    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();
    let mut writer = BufWriter::new(tokio::io::stdout());

    write_msg(
        &mut writer,
        &OutgoingMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            server_name: "phi-agent".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
        },
    )
    .await?;
    dbg_log!("hello");

    let mut seq: u64 = 0;

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let msg: IncomingMessage = match serde_json::from_str(&line) {
            Ok(m) => {
                dbg_log!("← {}", msg_kind(&m));
                m
            },
            Err(e) => {
                write_msg(
                    &mut writer,
                    &OutgoingMessage::Error {
                        code: "PARSE_ERROR".to_string(),
                        message: format!("invalid JSON: {e}"),
                        detail: None,
                    },
                )
                .await?;
                continue;
            },
        };

        match msg {
            IncomingMessage::RegisterTool { name, description, parameters } => {
                server.register_tool(name.clone(), description, parameters).await;
                write_msg(&mut writer, &OutgoingMessage::ToolRegistered { name, ok: true }).await?;
            },

            IncomingMessage::CreateSession { session_id } => {
                let (sid, ext) = server.create_session(session_id).await;
                write_msg(&mut writer, &OutgoingMessage::SessionCreated { session_id: ext, internal_id: sid.id })
                    .await?;
            },

            IncomingMessage::Run { session_id, query, config: _ } => {
                let sid = if session_id.is_empty() {
                    server.get_or_create_session(None).await
                } else {
                    server.get_or_create_session(Some(session_id.clone())).await
                };
                let mut event_rx = server.subscribe_events();

                let srv = server.clone();
                let q = query.clone();
                let mut handle = tokio::spawn(async move { srv.run_turn(&sid, &q, |_| Ok(())).await });

                // Pre-prepare ONE slot for the next tool call.
                // This must happen before the react loop calls ProxyTool.call().
                let mut tool_tx: Option<tokio::sync::mpsc::UnboundedSender<_>> = Some(server.prepare_tool_call().await);
                let mut n = 0u64;
                let mut turn_done = false;
                let mut handle_done = false;

                while !turn_done {
                    tokio::select! {
                        r = event_rx.recv() => {
                            match r {
                                Ok(ref e) => {
                                    seq += 1; n += 1;

                                    // Intercept ToolCallStarted: send ToolCall to SDK
                                    if let phi_agent::RuntimeEvent::ToolCallStarted { tool_name, args_json, .. } = e {
                                        dbg_log!("ToolCallStarted tool={tool_name} args_json={args_json}");
                                        let cid = format!("call-{}", n);
                                        write_msg(&mut writer, &OutgoingMessage::ToolCall {
                                            seq, call_id: cid, name: tool_name.clone(),
                                            args: serde_json::from_str(args_json).unwrap_or_default(),
                                        }).await?;
                                    }

                                    // Forward event
                                    write_msg(&mut writer, &OutgoingMessage::Event {
                                        seq, event: serde_json::to_value(e)?,
                                    }).await?;

                                    if matches!(e, phi_agent::RuntimeEvent::RunFinished { .. }
                                        | phi_agent::RuntimeEvent::RunCancelled { .. }) {
                                        dbg_log!("run finished event");
                                        turn_done = true;
                                    }
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                    dbg_log!("lagged {n}"); continue;
                                }
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                    dbg_log!("event channel closed");
                                    turn_done = true;
                                }
                            }
                        }
                        mb = lines.next_line() => {
                            if let Some(l) = mb? {
                                let l = l.trim().to_string();
                                if l.is_empty() {
                                    continue;
                                }
                                // Handle Cancel during a run
                                if let Ok(IncomingMessage::Cancel { .. }) = serde_json::from_str(&l) {
                                    dbg_log!("cancel during run");
                                    server.cancel();
                                    // Don't set turn_done — let RunCancelled event finish the loop
                                    continue;
                                }
                                // Handle ToolResult
                                if let Ok(IncomingMessage::ToolResult { call_id: _, summary, raw, .. }) =
                                    serde_json::from_str(&l)
                                {
                                    dbg_log!("tool_result");
                                    if let Some(tx) = tool_tx.take() {
                                        let _ = tx.send(Ok(phi_agent::ToolOutput {
                                            summary, raw,
                                            control_flow: phi_agent::ToolControlFlow::Break,
                                            truncation: None,
                                        }));
                                        dbg_log!("result sent via slot");
                                    } else {
                                        dbg_log!("WARN: no tool_tx available");
                                    }
                                }
                            }
                        }
                        result = &mut handle => {
                            // run_turn completed without emitting RunFinished
                            // (e.g., session not found, LLM config error, etc.)
                            // Do NOT set turn_done here — event_rx may still
                            // have pending events to forward.
                            handle_done = true;
                            match result {
                                Ok(Ok(outcome)) => {
                                    dbg_log!("turn completed via handle {:?}", outcome);
                                }
                                Ok(Err(e)) => {
                                    dbg_log!("turn error via handle: {e}");
                                    write_msg(&mut writer, &OutgoingMessage::Error {
                                        code: "TURN_ERROR".to_string(),
                                        message: e.to_string(),
                                        detail: None,
                                    }).await?;
                                }
                                Err(e) => {
                                    dbg_log!("turn panic: {e}");
                                    write_msg(&mut writer, &OutgoingMessage::Error {
                                        code: "TURN_PANIC".to_string(),
                                        message: e.to_string(),
                                        detail: None,
                                    }).await?;
                                }
                            }
                            // Wait for RunFinished from event_rx (or timeout)
                            // instead of setting turn_done immediately.
                            loop {
                                match event_rx.recv().await {
                                    Ok(ref e) => {
                                        seq += 1;
                                        write_msg(&mut writer, &OutgoingMessage::Event {
                                            seq, event: serde_json::to_value(e)?,
                                        }).await?;
                                        if matches!(e, phi_agent::RuntimeEvent::RunFinished { .. }
                                            | phi_agent::RuntimeEvent::RunCancelled { .. }) {
                                            dbg_log!("run finished after handle");
                                            turn_done = true;
                                            break;
                                        }
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                        turn_done = true;
                                        break;
                                    }
                                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                                }
                            }
                        }
                    }
                }

                // If the handle resolved in the event loop, we already
                // handled it.  Otherwise, collect the normal result here.
                let outcome = if handle_done {
                    phi_agent::RunOutcome::Failed { error: "turn aborted".into() }
                } else {
                    match handle.await {
                        Ok(Ok(o)) => {
                            dbg_log!("turn completed {:?}", o);
                            o
                        },
                        Ok(Err(e)) => {
                            dbg_log!("turn err: {e}");
                            phi_agent::RunOutcome::Failed { error: e.to_string() }
                        },
                        Err(e) => {
                            dbg_log!("turn panic: {e}");
                            phi_agent::RunOutcome::Failed { error: "turn panic".into() }
                        },
                    }
                };

                write_msg(
                    &mut writer,
                    &OutgoingMessage::Done { seq, outcome: oc_str(&outcome).to_string(), error: None, turns: None },
                )
                .await?;
                dbg_log!("done");
            },

            IncomingMessage::Cancel { .. } => {
                server.cancel();
            },
            IncomingMessage::ListTools {} => {
                let tools: Vec<phi_agent::bridge::messages::ToolMetadata> =
                    server.list_tools().await.into_iter().map(Into::into).collect();
                dbg_log!("list_tools count={}", tools.len());
                write_msg(&mut writer, &OutgoingMessage::ToolsListed { tools }).await?;
            },
            _ => {},
        }
    }

    Ok(())
}

async fn write_msg(w: &mut BufWriter<tokio::io::Stdout>, m: &OutgoingMessage) -> anyhow::Result<()> {
    let json = serde_json::to_string(m)?;
    w.write_all(json.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await?;
    Ok(())
}

fn oc_str(o: &phi_agent::RunOutcome) -> &'static str {
    match o {
        phi_agent::RunOutcome::Completed => "completed",
        phi_agent::RunOutcome::Failed { .. } => "failed",
        phi_agent::RunOutcome::MaxTurnsExceeded { .. } => "max_turns_exceeded",
        phi_agent::RunOutcome::Cancelled => "cancelled",
    }
}

fn msg_kind(m: &IncomingMessage) -> &'static str {
    match m {
        IncomingMessage::RegisterTool { .. } => "register_tool",
        IncomingMessage::CreateSession { .. } => "create_session",
        IncomingMessage::Run { .. } => "run",
        IncomingMessage::ToolResult { .. } => "tool_result",
        IncomingMessage::Cancel { .. } => "cancel",
        IncomingMessage::ListTools { .. } => "list_tools",
    }
}
