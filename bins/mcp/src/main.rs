//! `corex-mcp`：把 corex 的能力暴露成 MCP 工具。
//!
//! 两种传输：
//! - `--transport stdio`（默认）：本地编辑器 / agent（Cursor、Copilot、Claude Desktop…）作为
//!   子进程拉起，日志写 stderr，stdout 只放 MCP 消息。
//! - `--transport http`：Streamable HTTP，单 `/mcp` 端点，默认只绑 127.0.0.1。

mod exec;
mod handler;
mod tools;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{Parser, ValueEnum};
use corex_ipc::{config_paths, data_dir};
use rmcp::ServiceExt;

#[derive(Parser, Debug)]
#[command(
    name = "corex-mcp",
    version,
    about = "corex MCP server（Model Context Protocol）"
)]
struct Args {
    /// 传输：stdio（本地子进程）或 http（Streamable HTTP）
    #[arg(long, value_enum, default_value_t = Transport::Stdio)]
    transport: Transport,

    /// HTTP 监听地址（仅 --transport http）
    #[arg(long, default_value = "127.0.0.1")]
    bind: String,

    /// HTTP 端口（仅 --transport http）
    #[arg(long, default_value_t = 3000)]
    port: u16,

    /// 指令目录（默认 <数据目录>/directives）
    #[arg(long)]
    directives: Option<PathBuf>,

    /// 配置文件（toml）
    #[arg(long)]
    config: Option<PathBuf>,

    /// 提高日志详细程度
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Transport {
    Stdio,
    Http,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    init_tracing(args.verbose);

    let data = data_dir()?;
    // 配置文存在却解析失败是致命错误：回退到默认值会静默丢掉 `strict_permissions`。
    let paths: Vec<PathBuf> = args
        .config
        .as_deref()
        .map(|p| vec![p.to_path_buf()])
        .unwrap_or_else(config_paths);
    let resolved = corex_core::config::read(&paths)?;
    if let Some(source) = &resolved.source {
        tracing::info!(path = %source.display(), "配置已加载");
    }
    for issue in &resolved.warnings {
        tracing::warn!(key = issue.key, "{}", issue.message);
    }

    let state = Arc::new(exec::State::build(resolved.config, &data, args.directives)?);
    tracing::info!(actions = state.registry.len(), "内置动作已注册");

    match args.transport {
        Transport::Stdio => serve_stdio(state).await,
        Transport::Http => serve_http(state, &args.bind, args.port).await,
    }
}

async fn serve_stdio(state: Arc<exec::State>) -> Result<()> {
    let service = handler::CorexHandler { state }
        .serve(rmcp::transport::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

async fn serve_http(state: Arc<exec::State>, bind: &str, port: u16) -> Result<()> {
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };

    let addr: SocketAddr = format!("{bind}:{port}").parse()?;
    let service = StreamableHttpService::new(
        move || -> std::io::Result<handler::CorexHandler> {
            Ok(handler::CorexHandler {
                state: state.clone(),
            })
        },
        Arc::new(LocalSessionManager::default()),
        StreamableHttpServerConfig::default(),
    );

    let app = axum::Router::new().nest_service("/mcp", service);
    let app = require_token(app);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "corex-mcp 监听中（端点 /mcp）");
    axum::serve(listener, app).await?;
    Ok(())
}

/// 若设置了 `COREX_TOKEN`，所有请求须带 `Authorization: Bearer <token>`，否则 401。
///
/// stdio 传输不走这里（本地子进程，凭据靠环境与进程边界）；HTTP 默认只绑 127.0.0.1，
/// 但企业场景下再叠一层预共享 token，防 DNS rebinding 之外的误连。
fn require_token(app: axum::Router) -> axum::Router {
    let Some(token) = std::env::var("COREX_TOKEN").ok().filter(|t| !t.is_empty()) else {
        return app;
    };
    app.layer(axum::middleware::from_fn_with_state(token, auth_token))
}

async fn auth_token(
    axum::extract::State(token): axum::extract::State<String>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::{StatusCode, header};
    use axum::response::IntoResponse;
    let authorized = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == format!("Bearer {token}"));
    if authorized {
        next.run(req).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}

/// 日志一律写 stderr：stdio 传输下 stdout 是 MCP 的消息通道，不能混进日志。
fn init_tracing(verbose: u8) {
    use tracing_subscriber::EnvFilter;

    let default = match verbose {
        0 => "info".to_string(),
        1 => "corex_mcp=debug".to_string(),
        _ => "corex_mcp=trace".to_string(),
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}
