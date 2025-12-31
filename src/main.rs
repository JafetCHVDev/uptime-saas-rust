use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
use std::{str::FromStr, sync::Arc, time::Duration};
use tokio::time::sleep;
use tracing::{error, info};
use uuid::Uuid;
use dotenvy::dotenv;
use std::env;

type Db = Pool<Sqlite>;

#[derive(Clone)]
struct AppState {
    db: Db,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct CheckRow {
    id: String,
    name: String,
    url: String,
    interval_seconds: i64,
    alert_email: Option<String>,
    is_active: i64,
    last_status: Option<String>,
    last_checked_at: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ResultRow {
    id: i64,
    check_id: String,
    checked_at: String,
    status: String,
    http_status: Option<i64>,
    latency_ms: Option<i64>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateCheckRequest {
    name: String,
    url: String,
    interval_seconds: i64,
    alert_email: Option<String>,
}

#[derive(Debug, Serialize)]
struct CreateCheckResponse {
    id: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    dotenvy::from_path(".env").ok();
    dotenvy::from_path("../.env").ok();

    // DB (SQLite)
    let opts = SqliteConnectOptions::from_str("sqlite://data/uptime.db")?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal);

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    run_migrations(&db).await?;

    let state = Arc::new(AppState { db: db.clone() });

    // Worker
    tokio::spawn(worker_loop(state.clone()));

    // API
    let app = Router::new()
        .route("/health", get(health))
        .route("/checks", post(create_check).get(list_checks))
        .route("/checks/:id/results", get(list_results))
        .with_state(state);

    let addr = "0.0.0.0:8080";
    info!("API running on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn run_migrations(db: &Db) -> anyhow::Result<()> {
    let sql = tokio::fs::read_to_string("migrations/001_init.sql").await?;
    for stmt in sql.split(';') {
        let stmt = stmt.trim();
        if !stmt.is_empty() {
            sqlx::query(stmt).execute(db).await?;
        }
    }
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}

async fn create_check(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateCheckRequest>,
) -> Result<(StatusCode, Json<CreateCheckResponse>), (StatusCode, String)> {
    if payload.interval_seconds < 10 {
        return Err((StatusCode::BAD_REQUEST, "interval_seconds mínimo: 10".into()));
    }

    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO checks (id, name, url, interval_seconds, alert_email, is_active)
        VALUES (?, ?, ?, ?, ?, 1)
        "#,
    )
    .bind(&id)
    .bind(&payload.name)
    .bind(&payload.url)
    .bind(payload.interval_seconds)
    .bind(&payload.alert_email)
    .execute(&state.db)
    .await
    .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(CreateCheckResponse { id })))
}

async fn list_checks(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<CheckRow>>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, CheckRow>("SELECT * FROM checks")
        .fetch_all(&state.db)
        .await
        .map_err(internal_error)?;

    Ok(Json(rows))
}

async fn list_results(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ResultRow>>, (StatusCode, String)> {
    let rows = sqlx::query_as::<_, ResultRow>(
        "SELECT * FROM check_results WHERE check_id = ? ORDER BY checked_at DESC",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(internal_error)?;

    Ok(Json(rows))
}

fn internal_error(e: sqlx::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

async fn worker_loop(state: Arc<AppState>) {
    let client = reqwest::Client::new();

    let tg_token = env::var("TELEGRAM_BOT_TOKEN").ok();
    let tg_chat_id = env::var("TELEGRAM_CHAT_ID").ok();

    loop {
        let checks = match sqlx::query_as::<_, CheckRow>("SELECT * FROM checks WHERE is_active = 1")
            .fetch_all(&state.db)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("Error loading checks: {e}");
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        for c in checks {
            let resp = client.get(&c.url).send().await;
            let status = if resp.is_ok() { "UP" } else { "DOWN" }.to_string();

            let previous = c.last_status.clone().unwrap_or_else(|| "UNKNOWN".into());
            let checked_at = Utc::now().to_rfc3339();

            sqlx::query(
                "INSERT INTO check_results (check_id, checked_at, status) VALUES (?, ?, ?)",
            )
            .bind(&c.id)
            .bind(&checked_at)
            .bind(&status)
            .execute(&state.db)
            .await
            .ok();

            if previous != status {
                info!("STATUS CHANGE: {} {} -> {}", c.name, previous, status);

                // 🔔 TELEGRAM ALERTA
                if let (Some(token), Some(chat)) = (&tg_token, &tg_chat_id) {
                    let msg = format!(
                        "🚨 Uptime Alert\n{}\n{} → {}\n{}",
                        c.name, previous, status, c.url
                    );

                    let url =
                        format!("https://api.telegram.org/bot{}/sendMessage", token);

                    client
                        .post(url)
                        .json(&serde_json::json!({
                            "chat_id": chat,
                            "text": msg
                        }))
                        .send()
                        .await
                        .ok();
                }
            }

            sqlx::query(
                "UPDATE checks SET last_status = ?, last_checked_at = ? WHERE id = ?",
            )
            .bind(&status)
            .bind(&checked_at)
            .bind(&c.id)
            .execute(&state.db)
            .await
            .ok();
        }

        sleep(Duration::from_secs(5)).await;
    }
}
