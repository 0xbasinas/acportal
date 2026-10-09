//! Deterministic ACP executable. Explicit filesystem fixtures use host callbacks.
use acpd::connection::read_frame;
use acportal_protocol::{error, response};
use clap::Parser;
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    sync::{Mutex, mpsc, oneshot, watch},
};

#[derive(Parser)]
struct Options {
    #[arg(long)]
    fault: Option<String>,
    #[arg(long, default_value_t = 10)]
    delay_ms: u64,
    #[arg(long)]
    no_resume: bool,
    #[arg(long)]
    mcp_http: bool,
    #[arg(long)]
    mcp_sse: bool,
    #[arg(long)]
    prompt_media: bool,
    #[arg(long)]
    read_during_load: bool,
    #[arg(long)]
    require_auth: bool,
    #[arg(long)]
    descendant: bool,
    #[arg(long)]
    leaf: bool,
    #[arg(long)]
    terminal_fixture: bool,
    #[arg(long, default_value = "fixture output")]
    fixture_text: String,
    #[arg(long, default_value = "")]
    fixture_stderr: String,
    #[arg(long, default_value_t = 0)]
    fixture_exit_code: i32,
    #[arg(long)]
    fixture_stall: bool,
    #[arg(long)]
    terminal_during_prompt: bool,
    #[arg(long, default_value_t = 0)]
    terminal_exit_delay_ms: u64,
    #[arg(long, default_value_t = 0)]
    fixture_exit_delay_ms: u64,
}
type PermissionWaiters = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let options = Options::parse();
    if options.terminal_fixture {
        use std::io::Write;
        std::io::stderr().write_all(options.fixture_stderr.as_bytes())?;
        for bytes in options.fixture_text.as_bytes().chunks(4095) {
            std::io::stdout().write_all(bytes)?;
            std::io::stdout().flush()?;
            tokio::task::yield_now().await;
        }
        if options.fixture_stall {
            std::future::pending::<()>().await;
        }
        tokio::time::sleep(Duration::from_millis(options.fixture_exit_delay_ms)).await;
        std::process::exit(options.fixture_exit_code);
    }
    if options.leaf {
        std::future::pending::<()>().await;
    }
    if options.descendant {
        let child = std::process::Command::new(std::env::current_exe()?)
            .arg("--leaf")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        // Intentionally leave the child alive to exercise host-owned process cleanup.
        println!("{}", child.id());
        std::io::Write::flush(&mut std::io::stdout())?;
        std::future::pending::<()>().await;
    }
    let (out, mut writes) = mpsc::channel::<Value>(64);
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = writes.recv().await {
            stdout.write_all(message.to_string().as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
        Ok::<_, std::io::Error>(())
    });
    let waiters: PermissionWaiters = Arc::new(Mutex::new(HashMap::new()));
    let file_waiters: PermissionWaiters = Arc::new(Mutex::new(HashMap::new()));
    let mut client_capabilities = json!({});
    let mut authenticated = !options.require_auth;
    let mut mcp_servers = json!([]);
    let mut last_prompt = json!({});
    let mut cancellations: HashMap<String, watch::Sender<bool>> = HashMap::new();
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut session_counter = 0u64;
    let mut permission_counter = 0u64;
    while let Some(message) = read_frame(&mut stdin, 1024 * 1024).await? {
        let id = message["id"].clone();
        let params = &message["params"];
        let session = params["sessionId"].as_str().unwrap_or("").to_string();
        let Some(method) = message["method"].as_str() else {
            if let Some(waiter) = waiters.lock().await.remove(&id.to_string()) {
                let _ = waiter.send(message["result"].clone());
            } else if let Some(waiter) = file_waiters.lock().await.remove(&id.to_string()) {
                let _ = waiter.send(message);
            }
            continue;
        };
        match method {
            "initialize" => {
                client_capabilities = params["clientCapabilities"].clone();
                match options.fault.as_deref() {
                    Some("exit") => std::process::exit(23),
                    Some("malformed") => {
                        tokio::io::stdout().write_all(b"not-json\n").await?;
                        continue;
                    }
                    Some("oversized") => {
                        tokio::io::stdout()
                            .write_all(&vec![b'x'; 2 * 1024 * 1024])
                            .await?;
                        continue;
                    }
                    Some("stall") => continue,
                    _ => {}
                }
                out.send(response(id, json!({"protocolVersion":if options.fault.as_deref() == Some("v2") {2} else {1},
                    "agentCapabilities":{"loadSession":!options.no_resume,"promptCapabilities":{"image":options.prompt_media,"audio":options.prompt_media,"embeddedContext":options.prompt_media},"mcpCapabilities":{"http":options.mcp_http,"sse":options.mcp_sse}},"agentInfo":{"name":"mock-acp-agent","version":"0.1.0"},"authMethods":if options.require_auth {json!([{"id":"mock-login","name":"Mock sign-in","description":"Sign in to the test agent"}])}else{json!([])}}))).await?;
            }
            "authenticate" => {
                if params["methodId"] == "mock-login" {
                    authenticated = true;
                    out.send(response(id, json!({}))).await?;
                } else {
                    out.send(error(id, -32602, "Unknown method")).await?;
                }
            }
            "session/new" | "session/load" if !authenticated => {
                out.send(error(id, -32000, "Authentication required"))
                    .await?;
            }
            "session/new" => {
                mcp_servers = params["mcpServers"].clone();
                session_counter += 1;
                out.send(response(
                    id,
                    json!({"sessionId":format!("mock-{session_counter}"),"_meta":{"mockMcpServers":mcp_servers.as_array().unwrap().iter().map(|server| json!({"name":server["name"],"type":server.get("type").and_then(Value::as_str).unwrap_or("stdio")})).collect::<Vec<_>>()}}),
                ))
                .await?;
            }
            "session/load" if !options.no_resume => {
                mcp_servers = params["mcpServers"].clone();
                if options.read_during_load {
                    let callback_id = json!("load-file-read");
                    let (reply, result) = oneshot::channel();
                    file_waiters
                        .lock()
                        .await
                        .insert(callback_id.to_string(), reply);
                    let path =
                        std::path::Path::new(params["cwd"].as_str().unwrap()).join("sample.txt");
                    out.send(json!({"jsonrpc":"2.0","id":callback_id,"method":"fs/read_text_file","params":{"sessionId":params["sessionId"],"path":path}})).await?;
                    let out = out.clone();
                    tokio::spawn(async move {
                        if let Ok(callback) = result.await {
                            let message = if callback.get("error").is_some() {
                                json!({"jsonrpc":"2.0","id":id,"error":callback["error"]})
                            } else {
                                response(id, json!({}))
                            };
                            let _ = out.send(message).await;
                        }
                    });
                } else {
                    out.send(response(id, json!({}))).await?;
                }
            }
            "session/prompt"
                if matches!(
                    options.fault.as_deref(),
                    Some("prompt-error" | "prompt-auth" | "prompt-auth-text")
                ) =>
            {
                // Like an agent whose provider fails or rejects the credentials on the first
                // model call (Goose 1.53.0 answers an invalid key with -32000; OpenCode 1.18.35
                // with -32603 and an "Authentication Fails, Your api key ..." message).
                let (code, message) = match options.fault.as_deref() {
                    Some("prompt-auth") => {
                        (-32000, "Provider rejected key synthetic-provider-secret")
                    }
                    Some("prompt-auth-text") => (
                        -32603,
                        "Internal error: Authentication Fails, Your api key: ****synthetic-provider-secret is invalid",
                    ),
                    _ => (-32603, "Provider rejected key synthetic-provider-secret"),
                };
                out.send(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}))
                    .await?;
            }
            "session/prompt" => {
                last_prompt = params.clone();
                permission_counter += 1;
                let permission_id = format!("permission-{permission_counter}");
                let (cancel, cancellation) = watch::channel(false);
                cancellations.insert(session.clone(), cancel);
                let worker_out = out.clone();
                let worker_waiters = waiters.clone();
                let terminal_waiters = file_waiters.clone();
                let terminal_enabled =
                    options.terminal_during_prompt && client_capabilities["terminal"] == true;
                let delay = options.delay_ms;
                let terminal_delay = options.terminal_exit_delay_ms;
                tokio::spawn(async move {
                    let mut cancellation = cancellation;
                    let work = async {
                        if terminal_enabled {
                            terminal_turn(
                                &worker_out,
                                &terminal_waiters,
                                &session,
                                &permission_id,
                                terminal_delay,
                            )
                            .await?;
                        }
                        turn(
                            &worker_out,
                            &worker_waiters,
                            &session,
                            &permission_id,
                            delay,
                        )
                        .await
                    };
                    let stop_reason = tokio::select! {
                        result = work => match result { Ok(true) => "cancelled", Ok(false) => "end_turn", Err(_) => "refusal" },
                        _ = cancellation.changed() => "cancelled",
                    };
                    worker_waiters
                        .lock()
                        .await
                        .remove(&json!(permission_id).to_string());
                    let _ = worker_out
                        .send(response(id, json!({"stopReason":stop_reason})))
                        .await;
                });
            }
            "session/cancel" => {
                if let Some(cancel) = cancellations.remove(&session) {
                    let _ = cancel.send(true);
                }
            }
            "_mock/crash" => std::process::exit(24),
            "_mock/stall" => {}
            "_mock/terminal" => {
                let callback_id = json!(format!("terminal-{id}"));
                let (reply, result) = oneshot::channel();
                file_waiters
                    .lock()
                    .await
                    .insert(callback_id.to_string(), reply);
                out.send(json!({"jsonrpc":"2.0","id":callback_id,"method":params["method"],"params":params["request"]})).await?;
                let out = out.clone();
                tokio::spawn(async move {
                    if let Ok(callback) = result.await {
                        let answer = if callback.get("error").is_some() {
                            json!({"jsonrpc":"2.0","id":id,"error":callback["error"]})
                        } else {
                            response(id, callback["result"].clone())
                        };
                        let _ = out.send(answer).await;
                    }
                });
            }
            "_mock/spawn_descendant" => {
                use std::io::BufRead;
                let mut child = std::process::Command::new(std::env::current_exe()?)
                    .arg("--descendant")
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()?;
                let child_pid = child.id();
                let mut line = String::new();
                std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut line)?;
                let grandchild_pid: u32 = line.trim().parse()?;
                out.send(response(
                    id,
                    json!({"childPid":child_pid,"grandchildPid":grandchild_pid}),
                ))
                .await?;
            }
            "_mock/echo" => {
                out.send(response(id, params.clone())).await?;
            }
            "_mock/client_capabilities" => {
                out.send(response(id, client_capabilities.clone())).await?;
            }
            "_mock/last_prompt" => {
                out.send(response(id, last_prompt.clone())).await?;
            }
            "_mock/emit_update" => {
                update(&out, &session, params["update"].clone()).await?;
                out.send(response(id, json!({}))).await?;
            }
            "_mock/mcp_servers" => {
                out.send(response(id, mcp_servers.clone())).await?;
            }
            "_mock/read_file" | "_mock/write_file" => {
                let callback_id = json!(format!("file-{id}"));
                let (reply, result) = oneshot::channel();
                file_waiters
                    .lock()
                    .await
                    .insert(callback_id.to_string(), reply);
                out.send(json!({"jsonrpc":"2.0","id":callback_id,"method":if method=="_mock/write_file" {"fs/write_text_file"}else{"fs/read_text_file"},"params":params})).await?;
                let out = out.clone();
                tokio::spawn(async move {
                    if let Ok(callback) = result.await {
                        let message = if callback.get("error").is_some() {
                            json!({"jsonrpc":"2.0","id":id,"error":callback["error"]})
                        } else {
                            response(id, callback["result"].clone())
                        };
                        let _ = out.send(message).await;
                    }
                });
            }
            _ if !id.is_null() => {
                out.send(error(id, -32601, "Method not found")).await?;
            }
            _ => {}
        }
    }
    drop(out);
    writer.abort();
    Ok(())
}
async fn update(out: &mpsc::Sender<Value>, session: &str, update: Value) -> anyhow::Result<()> {
    out.send(json!({"jsonrpc":"2.0","method":"session/update","params":{"sessionId":session,"update":update}})).await?;
    Ok(())
}
async fn terminal_callback(
    out: &mpsc::Sender<Value>,
    waiters: &PermissionWaiters,
    id: Value,
    method: &str,
    params: Value,
) -> anyhow::Result<Value> {
    let (reply, result) = oneshot::channel();
    waiters.lock().await.insert(id.to_string(), reply);
    out.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
        .await?;
    let answer = result.await?;
    if answer.get("error").is_some() {
        anyhow::bail!("terminal callback denied")
    }
    Ok(answer["result"].clone())
}
async fn terminal_turn(
    out: &mpsc::Sender<Value>,
    waiters: &PermissionWaiters,
    session: &str,
    token: &str,
    delay: u64,
) -> anyhow::Result<()> {
    let created=terminal_callback(out,waiters,json!(format!("create-{token}")),"terminal/create",json!({"sessionId":session,"command":std::env::current_exe()?,"args":["--terminal-fixture","--fixture-text","ACP terminal output verified.\n","--fixture-exit-delay-ms",delay.to_string()],"outputByteLimit":4096})).await?;
    let scope = json!({"sessionId":session,"terminalId":created["terminalId"]});
    update(out,session,json!({"sessionUpdate":"tool_call","toolCallId":format!("native-terminal-{token}"),"title":"Run ACP terminal fixture","kind":"execute","status":"in_progress","content":[{"type":"terminal","terminalId":created["terminalId"]}]})).await?;
    let exit = terminal_callback(
        out,
        waiters,
        json!(format!("wait-{token}")),
        "terminal/wait_for_exit",
        scope.clone(),
    )
    .await?;
    terminal_callback(
        out,
        waiters,
        json!(format!("release-{token}")),
        "terminal/release",
        scope,
    )
    .await?;
    update(out,session,json!({"sessionUpdate":"tool_call_update","toolCallId":format!("native-terminal-{token}"),"status":if exit["exitCode"]==0 {"completed"}else{"failed"}})).await?;
    Ok(())
}
async fn turn(
    out: &mpsc::Sender<Value>,
    waiters: &PermissionWaiters,
    session: &str,
    permission_id: &str,
    delay: u64,
) -> anyhow::Result<bool> {
    for text in ["I'll inspect ", "the workspace.\n"] {
        update(
            out,
            session,
            json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":text}}),
        )
        .await?;
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
    update(out, session, json!({"sessionUpdate":"tool_call","toolCallId":"read-1","title":"Read example.rs","kind":"read","status":"completed"})).await?;
    update(out, session, json!({"sessionUpdate":"plan","entries":[{"content":"Inspect, test, and propose a change","priority":"medium","status":"in_progress"}]})).await?;
    let (decision, result) = oneshot::channel();
    waiters
        .lock()
        .await
        .insert(json!(permission_id).to_string(), decision);
    out.send(json!({"jsonrpc":"2.0","id":permission_id,"method":"session/request_permission","params":{
        "sessionId":session,"toolCall":{"toolCallId":"edit-1","title":"Propose a mock file change","kind":"edit","rawInput":{"path":"example.rs"}},
        "options":[{"optionId":"allow","name":"Allow once","kind":"allow_once"},{"optionId":"deny","name":"Deny","kind":"reject_once"}]}})).await?;
    let decision = result.await?;
    if decision["outcome"]["outcome"] == "cancelled" {
        return Ok(true);
    }
    if decision["outcome"]["optionId"] != "allow" {
        update(out, session, json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"No change was approved.\n"}})).await?;
        return Ok(false);
    }
    // Command output is tool content. A terminal handle must not be invented when
    // the client did not advertise terminal/create.
    update(out, session, json!({"sessionUpdate":"tool_call","toolCallId":"test-1","title":"Run mock tests","kind":"execute","status":"completed",
        "content":[{"type":"content","content":{"type":"text","text":"$ mock test\n2 passed\n"}}]})).await?;
    update(out, session, json!({"sessionUpdate":"tool_call","toolCallId":"edit-1","title":"Proposed example.rs change","kind":"edit","status":"completed",
        "content":[{"type":"diff","path":std::env::current_dir()?.join("example.rs"),"oldText":"let expired = true;","newText":"let expired = false;"}]})).await?;
    update(out, session, json!({"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Mock lifecycle complete. No files were changed.\n"}})).await?;
    Ok(false)
}
