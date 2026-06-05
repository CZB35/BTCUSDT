use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
    env,
    net::SocketAddr,
    str::FromStr,
    sync::Arc,
    time::Duration as StdDuration,
};

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Duration, TimeZone, Timelike, Utc};
use clap::Parser;
use ethers_core::types::{Address, Signature, U256};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use rand_core::RngCore;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use tokio::sync::{broadcast, RwLock};
use tokio::task::JoinHandle;
use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use uuid::Uuid;

type HmacSha256 = Hmac<sha2::Sha256>;

const DEFAULT_SYMBOL: &str = "BTCUSDT";
const DEFAULT_PERIOD_SECONDS: i32 = 1800;
const DEFAULT_PAYOUT_RATIO: Decimal = dec!(0.80);
const PROFIT_SHARE_RATE: Decimal = dec!(0.30);
const DEPOSIT_FEE_RATE: Decimal = dec!(0.25);
const FIRST_DEPOSIT_MIN_AMOUNT: Decimal = dec!(100);
const DIRECT_REFERRAL_RATE: Decimal = dec!(0.09);
const INDIRECT_REFERRAL_RATE: Decimal = dec!(0.03);
const DEFAULT_STAKE_PERCENT: Decimal = dec!(0.05);
const MIN_EVENT_STAKE: Decimal = dec!(5);
const MAX_EVENT_STAKE: Decimal = dec!(250);
const PAGE_GATE_TTL_SECONDS: i64 = 6;
const ACCOUNT_BALANCE_TOLERANCE_USDT: Decimal = dec!(1);
const ACCOUNT_BALANCE_CACHE_TTL_SECONDS: i64 = 90;
const MIN_DEPOSIT_AMOUNT: Decimal = dec!(10);
const BINANCE_PROVIDER: &str = "binance_futures";
const BINANCE_FAPI_BASE_URL: &str = "https://fapi.binance.com";
const BINANCE_RECV_WINDOW: i64 = 5000;
const BINANCE_INCOME_LOOKBACK_MINUTES: i64 = 20;
const BINANCE_SETTLEMENT_GRACE_SECONDS: i64 = 90;
const BINANCE_BALANCE_REFRESH_SECONDS: i64 = 30;
const DEFAULT_BINANCE_BALANCE_REFRESH_LIMIT: i64 = 100;
const DEFAULT_EXCHANGE_SYNC_INTERVAL_SECONDS: u64 = 2;
const DEFAULT_BINANCE_ORDER_CONFIRM_LIMIT: i64 = 100;
const DEFAULT_BINANCE_SETTLEMENT_SYNC_LIMIT: i64 = 100;
const DEFAULT_DB_MAX_CONNECTIONS: u32 = 40;
const DEFAULT_BSC_SYNC_INTERVAL_SECONDS: u64 = 6;
const DEFAULT_BSC_DEPOSIT_SYNC_LIMIT: i64 = 100;
const DEFAULT_BSC_RPC_TIMEOUT_SECONDS: u64 = 8;
const DEFAULT_RESERVATION_RELEASE_INTERVAL_SECONDS: u64 = 2;
const DEFAULT_RESERVATION_NO_ACK_RELEASE_SECONDS: i64 = 30;
const DEFAULT_RESERVATION_ORDER_TIMEOUT_SECONDS: i64 = 90;
const DEFAULT_RESERVATION_RELEASE_LIMIT: i64 = 100;
const DEFAULT_MAX_ACTIVE_EVENT_ORDERS_PER_USER: usize = 3;
const NIGHT_PROTECTION_START_MINUTE: u32 = 30;
const NIGHT_PROTECTION_END_MINUTE: u32 = 180;
const NIGHT_CLUSTER_GAP_SECONDS: i64 = 180;
const NIGHT_CLUSTER_SKIP_FIRST: usize = 2;
const PREDICTION_LOSS_GUARD_LOOKBACK_HOURS: i64 = 12;
const PREDICTION_GLOBAL_CONSECUTIVE_LOSS_GUARD_MIN: usize = 3;
const PREDICTION_GLOBAL_LOSS_GUARD_COOLDOWN_MINUTES: i64 = 90;
const PREDICTION_RULE_CONSECUTIVE_LOSS_GUARD_MIN: usize = 2;
const PREDICTION_RULE_LOSS_GUARD_COOLDOWN_MINUTES: i64 = 120;
const PREDICTION_LOSS_GUARD_MIN_RECENT: i64 = 3;
const PREDICTION_LOSS_GUARD_MAX_WIN_RATE: f64 = 0.34;
const USER_SESSION_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;
const DEFAULT_QUERY_LIMIT: i64 = 50;
const MAX_QUERY_LIMIT: i64 = 500;
const DEFAULT_PREDICTION_INTERVAL_SECONDS: u64 = 1;
const DEFAULT_PREDICTION_CANDLE_LIMIT: usize = 720;
const DEFAULT_PREDICTION_STALE_SECONDS: i64 = 5;
const DEFAULT_PREDICTION_UP_THRESHOLD: f64 = 8.00;
const DEFAULT_PREDICTION_DOWN_THRESHOLD: f64 = -8.00;
const DEFAULT_PREDICTION_MIN_CONFIDENCE: f64 = 0.62;
const DEFAULT_PREDICTION_TECHNICAL_WEIGHT: f64 = 0.35;
const DEFAULT_PREDICTION_PROBABILITY_WEIGHT: f64 = 0.65;
const DEFAULT_PREDICTION_VOLATILITY_FLOOR: f64 = 0.00016;
const DEFAULT_PREDICTION_SIGNAL_COOLDOWN_SECONDS: i64 = 60;
const PREDICTION_MODEL_VERSION: &str = "event-edge-ml-v3-meta-guards";
const FALLBACK_PREDICTION_MODEL_VERSION: &str = "event-edge-v10";
const DEFAULT_EVENT_ML_MODEL_PATH: &str = "models/event_edge_ml_v1.json";
#[cfg(test)]
const DEFAULT_EVENT_ML_MODEL_ARTIFACT_VERSION: &str = "event-edge-ml-v2";
const DEFAULT_PREDICTION_CALIBRATION_LOOKBACK_MINUTES: i64 = 720;
const DEFAULT_PREDICTION_CALIBRATION_MIN_SAMPLES: i32 = 10;
const DEFAULT_PREDICTION_CALIBRATION_MIN_WIN_RATE: f64 = 0.60;
const DEFAULT_PREDICTION_CALIBRATION_MIN_EV_USDT: f64 = 0.01;
const DEFAULT_PREDICTION_CALIBRATION_SAMPLE_SECONDS: i64 = 60;
const PREDICTION_MEAN_REVERSION_DOWN_MIN_PROBABILITY: f64 = 0.63;
const PREDICTION_MEAN_REVERSION_DOWN_MIN_STRUCTURE: f64 = 0.82;
const PREDICTION_MEAN_REVERSION_UP_MIN_PROBABILITY: f64 = 0.63;
const PREDICTION_MEAN_REVERSION_UP_MIN_STRUCTURE: f64 = 0.84;
const PREDICTION_TREND_DOWN_MIN_PROBABILITY: f64 = 0.62;
const PREDICTION_TREND_DOWN_MIN_STRUCTURE: f64 = 0.98;
const PREDICTION_TREND_UP_MIN_PROBABILITY: f64 = 0.64;
const PREDICTION_TREND_UP_MIN_STRUCTURE: f64 = 0.94;
const PREDICTION_IMPULSE_DOWN_MIN_PROBABILITY: f64 = 0.63;
const PREDICTION_IMPULSE_DOWN_MIN_STRUCTURE: f64 = 0.96;
const PREDICTION_IMPULSE_UP_MIN_PROBABILITY: f64 = 0.64;
const PREDICTION_IMPULSE_UP_MIN_STRUCTURE: f64 = 0.94;
const PREDICTION_REVERSAL_UP_MIN_SETUP_CONFIDENCE: f64 = 0.62;
const PREDICTION_REVERSAL_UP_MIN_STRUCTURE: f64 = 0.50;
const PREDICTION_REVERSAL_DOWN_MIN_SETUP_CONFIDENCE: f64 = 0.62;
const PREDICTION_REVERSAL_DOWN_MIN_STRUCTURE: f64 = 0.55;
const PREDICTION_REVERSAL_MIN_WICK: f64 = 0.24;
const PREDICTION_TREND_PULLBACK_MIN_WICK: f64 = 0.35;
const PREDICTION_IMPULSE_CONFIRM_MIN_WICK: f64 = 0.25;
const PREDICTION_MEAN_REVERSION_DOWN_MIN_BOLL_POS: f64 = 0.40;
const PREDICTION_MEAN_REVERSION_UP_MAX_BOLL_POS: f64 = 0.60;
const PREDICTION_UP_FALLING_KNIFE_MAX_LOWER_WICK: f64 = 0.30;
const PREDICTION_UP_FALLING_KNIFE_MIN_BODY_RATIO: f64 = 0.66;
const PREDICTION_CONFIRMATION_SECONDS: i64 = 5;
const PREDICTION_CONFIRMATION_MAX_ADVERSE_RET: f64 = 0.00018;
const PREDICTION_MIN_RAW_CONFIDENCE: f64 = DEFAULT_PREDICTION_MIN_CONFIDENCE;
const EVENT_ML_WARM_SECONDS: usize = 3600;
const MARKET_SECOND_BARS_RESTORE_EXTRA_SECONDS: usize = 900;
const MARKET_SECOND_BARS_RETENTION_SECONDS: i64 = 24 * 60 * 60;
const MARKET_SECOND_BARS_CLEANUP_INTERVAL_SECONDS: u64 = 5 * 60;
const MARKET_SECOND_BARS_BACKFILL_EXTRA_SECONDS: i64 = 180;
const MARKET_SECOND_BARS_BACKFILL_MAX_PAGES: usize = 160;
const EVENT_ML_SLOW_REVERSAL_MIN_CONFIDENCE: f64 = 0.60;
const EVENT_ML_SLOW_REVERSAL_MIN_MOVE_PROBABILITY: f64 = 0.45;
const EVENT_ML_SLOW_REVERSAL_MIN_RET_1M: f64 = 0.00055;
const EVENT_ML_SLOW_REVERSAL_MIN_RET_3M: f64 = 0.00110;
const EVENT_ML_SLOW_REVERSAL_MIN_RET_5M: f64 = 0.00170;
const EVENT_ML_SLOW_REVERSAL_MIN_RET_7M: f64 = 0.00210;
const EVENT_ML_SLOW_REVERSAL_EXTREME_POS: f64 = 0.15;
const EVENT_ML_SLOW_REVERSAL_RULE: &str = "slow_trend_reversal_1_3_5_7_live";
const EVENT_ML_TWO_STAGE_RULE: &str = "two_stage_extra_trees";
const EVENT_ML_TWO_STAGE_REVERSE_RULE: &str = "two_stage_reverse_extra_trees";
const EVENT_ML_TWO_STAGE_DOWNSHIFT_RULE: &str = "two_stage_downshift_extra_trees";
const DEFAULT_EVENT_ML_TWO_STAGE_ENABLED: bool = true;
const DEFAULT_EVENT_ML_TWO_STAGE_DOWNSHIFT_ENABLED: bool = false;
const DEFAULT_EVENT_ML_TWO_STAGE_REVERSE_ENABLED: bool = false;
const EVENT_ML_SLOW_REVERSAL_EXTREME_POS_300: f64 = 0.02;
const EVENT_ML_SLOW_REVERSAL_MAX_BODY_SHARE_60: f64 = 0.95;
const EVENT_ML_SLOW_REVERSAL_MIN_WICK_SHARE_60: f64 = 0.05;
const EVENT_ML_SLOW_DOWN_MIN_POS_420: f64 = 0.95;
const EVENT_ML_SLOW_DOWN_VERTICAL_RET_1M: f64 = 0.00400;
const EVENT_ML_SLOW_DOWN_VERTICAL_RET_3M: f64 = 0.01000;
const EVENT_ML_SLOW_DOWN_VERTICAL_RET_5M: f64 = 0.00800;
const EVENT_ML_SLOW_DOWN_FAST_RET_1M: f64 = 0.00245;
const EVENT_ML_SLOW_DOWN_FAST_RET_7M: f64 = 0.00440;
const EVENT_ML_SLOW_DOWN_SOLID_BODY_SHARE_60: f64 = 0.995;
const EVENT_ML_SLOW_DOWN_SOLID_MAX_WICK_SHARE_60: f64 = 0.005;
const EVENT_ML_SLOW_UP_MIN_LOWER_WICK_SHARE_60: f64 = 0.18;
const DEFAULT_EVENT_ML_SLOW_UP_ENABLED: bool = false;
const EVENT_ML_SLOW_UP_STRONG_MIN_CONFIDENCE: f64 = 0.697;
const EVENT_ML_SLOW_UP_STRONG_MAX_RET_300: f64 = -0.00245;
const EVENT_ML_SLOW_DOWN_CONFIRM_MIN_CONFIDENCE: f64 = 0.684;
const EVENT_ML_SLOW_DOWN_CONFIRM_MAX_RET_60: f64 = 0.00093;
const EVENT_ML_SLOW_DOWN_CONFIRM_MAX_MOVE_PROBABILITY: f64 = 0.681;
const EVENT_ML_SLOW_DOWN_CONFIRM_MIN_RET_300: f64 = 0.00290;
const EVENT_ML_SLOW_DOWN_MAX_RET_600: f64 = 0.00251;
const EVENT_ML_SLOW_DOWN_MAX_RET_900: f64 = 0.00143;
const EVENT_ML_TWO_STAGE_REVERSE_DOWN_MIN_RET_420: f64 = -0.00293;
const EVENT_ML_TWO_STAGE_REVERSE_DOWN_MIN_RET_600: f64 = -0.00430;
const EVENT_ML_TWO_STAGE_DOWNSHIFT_MAX_RET_180: f64 = -0.00280;
const EVENT_ML_TWO_STAGE_DOWNSHIFT_MAX_CLOSE_POS_60: f64 = 0.30;
const EVENT_ML_TWO_STAGE_UP_HOLD_MIN_RET_300: f64 = -0.00300;
const EVENT_ML_TWO_STAGE_UP_HOLD_MAX_RET_300: f64 = -0.00150;
const DEFAULT_MARKET_DATA_PRIMARY: &str = BINANCE_FAPI_BASE_URL;
const DEFAULT_MARKET_WS_URL: &str =
    "wss://fstream.binance.com/market/stream?streams=btcusdt@aggTrade/btcusdt@markPrice@1s/btcusdt@kline_1m";
const DEFAULT_MARKET_MIN_WARM_CANDLES: usize = 240;
const DEFAULT_MARKET_WS_RECONNECT_SECONDS: u64 = 2;
const DEFAULT_EXCHANGE_WORKER_SHARD_TOTAL: i32 = 1;
const DEFAULT_EXCHANGE_WORKER_SHARD_INDEX: i32 = 0;
const DEFAULT_BSC_USDT_CONTRACT: &str = "0x55d398326f99059fF775485246999027B3197955";
const TRANSFER_TOPIC: &str = "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";
const DEPOSIT_RECEIVED_TOPIC: &str =
    "0xd5f368bbc2e92420807de92261859985839a9c191a4bcab25726232ce32263d9";
const REFUND_PAID_TOPIC: &str =
    "0x7ade7bc44016a79fbe4e7479cc3af8669b5af6b309802d1a23a4910bb84b80aa";
#[derive(Parser, Debug)]
#[command(name = "event-signal-prototype")]
struct Cli {
    #[arg(long)]
    serve: bool,
    #[arg(long)]
    once: bool,
    #[arg(long)]
    migrate_only: bool,
    #[arg(long)]
    exchange_worker: bool,
    #[arg(long)]
    print_binance_egress_ip: bool,
    #[arg(long, default_value = "127.0.0.1:8787")]
    listen: SocketAddr,
    #[arg(long, env = "DATABASE_URL")]
    database_url: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();
    if cli.once {
        let signal = SignalCore::new("DOWN".to_string(), dec!(5), dec!(1));
        println!(
            "{}",
            serde_json::to_string_pretty(&signal_api_response(&signal))?
        );
        return Ok(());
    }

    if cli.print_binance_egress_ip {
        let client = binance_http_client()
            .map_err(|err| anyhow::anyhow!("{}: {}", err.code, err.message))?;
        let ip = client
            .get("https://api.ipify.org")
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        println!("{}", ip.trim());
        return Ok(());
    }

    let db = match &cli.database_url {
        Some(database_url) => {
            let pool = PgPoolOptions::new()
                .max_connections(env_u32("DB_MAX_CONNECTIONS", DEFAULT_DB_MAX_CONNECTIONS, 5))
                .connect(database_url)
                .await?;
            sqlx::migrate!("./migrations").run(&pool).await?;
            println!("PostgreSQL connected and migrations applied");
            Some(pool)
        }
        None => None,
    };

    if cli.migrate_only {
        if db.is_none() {
            anyhow::bail!("--migrate-only requires DATABASE_URL");
        }
        return Ok(());
    }

    if cli.exchange_worker {
        let Some(pool) = db else {
            anyhow::bail!("--exchange-worker requires DATABASE_URL");
        };
        let config = ExchangeWorkerConfig::from_env();
        println!(
            "exchange worker started: shard {}/{} bind_local_ip={}",
            config.shard_index,
            config.shard_total,
            env::var("BINANCE_BIND_LOCAL_IP").unwrap_or_else(|_| "-".to_string())
        );
        run_exchange_sync_worker(pool, config, None).await;
        return Ok(());
    }

    let state = AppState::new(db);
    bootstrap_memory_admin(&state).await;
    start_exchange_sync_workers(&state)
        .await
        .map_err(|err| anyhow::anyhow!("{}: {}", err.code, err.message))?;
    start_bsc_deposit_sync_worker(&state);
    start_reservation_release_worker(&state);
    start_market_data_feed(&state);
    start_market_second_bar_retention_worker(&state);
    start_prediction_loop(&state);
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(cli.listen).await?;
    println!("event-signal service listening on http://{}", cli.listen);
    if cli.database_url.is_none() {
        println!("running with in-memory state; pass --database-url later for persistent storage");
    }
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/auth/register", post(register))
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/auth/refresh", post(refresh_token))
        .route(
            "/v1/auth/change-password-message",
            post(change_password_message),
        )
        .route("/v1/auth/change-password", post(change_password))
        .route("/v1/wallets/bind-message", post(wallet_bind_message))
        .route("/v1/wallets/verify", post(wallet_verify))
        .route("/v1/stats", get(stats))
        .route("/v1/signals/latest", get(latest_signal))
        .route("/v1/signals/history", get(signal_history))
        .route("/v1/signals/deliveries/ack", post(ack_signal_delivery))
        .route("/v1/app/runtime-summary", get(app_runtime_summary))
        .route("/v1/user-settings/trading", get(get_trading_settings))
        .route("/v1/user-settings/trading", put(save_trading_settings))
        .route("/v1/user-settings/automation", get(get_automation_settings))
        .route(
            "/v1/user-settings/automation",
            put(save_automation_settings),
        )
        .route("/v1/automation/page-state", post(save_page_state))
        .route("/v1/exchange/binance", get(exchange_status))
        .route("/v1/exchange/binance", post(save_binance_credentials))
        .route("/v1/balance", get(balance))
        .route("/v1/ledger", get(ledger))
        .route("/v1/referrals/summary", get(referral_summary))
        .route("/v1/deposits/instructions", get(deposit_instructions))
        .route("/v1/deposits/submit-tx", post(submit_deposit_tx))
        .route("/v1/deposits", get(deposits))
        .route("/v1/withdrawals", get(withdrawals).post(create_withdrawal))
        .route("/v1/event-contract-orders", post(submit_order))
        .route(
            "/v1/signal-reservations/release",
            post(release_signal_reservation),
        )
        .route("/v1/event-contract-orders", get(event_contract_orders))
        .route("/v1/execution-reports", post(submit_execution_report))
        .route("/v1/execution-reports", get(execution_reports))
        .route("/v1/settlements", get(settlements))
        .route("/v1/ws", get(ws_handler))
        .route("/v1/admin/login", post(admin_login))
        .route("/v1/admin/overview", get(admin_overview))
        .route("/v1/admin/trading-settings", get(admin_trading_settings))
        .route(
            "/v1/admin/trading-settings",
            put(admin_save_trading_settings),
        )
        .route("/v1/admin/exchange-workers", get(admin_exchange_workers))
        .route(
            "/v1/admin/exchange-workers",
            post(admin_save_exchange_worker),
        )
        .route(
            "/v1/admin/exchange-workers/toggle",
            post(admin_toggle_exchange_worker),
        )
        .route(
            "/v1/admin/exchange-workers/delete",
            post(admin_delete_exchange_worker),
        )
        .route("/v1/admin/signals", get(admin_signals))
        .route("/v1/admin/signals/emit", post(admin_emit_signal))
        .route(
            "/v1/admin/prediction-settings",
            get(admin_prediction_settings),
        )
        .route(
            "/v1/admin/prediction-settings",
            put(admin_save_prediction_settings),
        )
        .route(
            "/v1/admin/prediction-observations",
            get(admin_prediction_observations),
        )
        .route("/v1/admin/users", get(admin_users))
        .route("/v1/admin/orders", get(admin_orders))
        .route("/v1/admin/deposits", get(admin_deposits))
        .route("/v1/admin/withdrawals", get(admin_withdrawals))
        .route(
            "/v1/admin/withdrawals/confirm",
            post(admin_confirm_withdrawal),
        )
        .route(
            "/v1/admin/withdrawals/reject",
            post(admin_reject_withdrawal),
        )
        .route("/v1/admin/ledger", get(admin_ledger))
        .route("/v1/admin/execution-reports", get(admin_reports))
        .route(
            "/v1/admin/execution-reports/settle",
            post(admin_settle_execution_report),
        )
        .route("/v1/admin/settlements", get(admin_settlements))
        .route("/v1/admin/exchange/sync", post(admin_sync_exchange))
        .route("/v1/admin/page-states", get(admin_page_states))
        .nest_service("/admin", ServeDir::new("admin/dist"))
        .nest_service("/dapp", ServeDir::new("dapp"))
        .layer(cors_layer())
        .with_state(state)
}

async fn bootstrap_memory_admin(state: &SharedState) {
    if state.db.is_some() {
        return;
    }
    state.admin_users.write().await.insert(
        "2752186242".to_string(),
        AdminAccountRecord {
            id: Uuid::from_u128(0x018f0000000070008000000000000001),
            password_hash: "$argon2id$v=19$m=19456,t=2,p=1$Yg+Rws/AhevYWlikD3F7lg$2QqeBfA9pR0qOgiCrDuEYF0XKCWxZ0ViQ+yisx80XF4"
                .to_string(),
            status: "active".to_string(),
        },
    );
}

async fn start_exchange_sync_workers(state: &SharedState) -> Result<(), AppError> {
    if let Some(pool) = state.db.as_ref() {
        let slots = db_exchange_worker_slots(pool).await?;
        let enabled_slots = slots
            .into_iter()
            .filter(|slot| slot.enabled)
            .collect::<Vec<_>>();
        if !enabled_slots.is_empty() {
            for slot in enabled_slots {
                sync_exchange_worker_runtime(state, &slot).await?;
            }
            return Ok(());
        }
    }
    start_exchange_sync_worker(state);
    Ok(())
}

fn start_exchange_sync_worker(state: &SharedState) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    let config = ExchangeWorkerConfig::from_env();
    tokio::spawn(async move {
        run_exchange_sync_worker(pool, config, None).await;
    });
}

async fn run_exchange_sync_worker(
    pool: PgPool,
    config: ExchangeWorkerConfig,
    slot_id: Option<Uuid>,
) {
    let mut interval = tokio::time::interval(StdDuration::from_secs(env_u64(
        "EXCHANGE_SYNC_INTERVAL_SECONDS",
        DEFAULT_EXCHANGE_SYNC_INTERVAL_SECONDS,
        1,
    )));
    loop {
        interval.tick().await;
        if let Err(err) = sync_exchange_orders(&pool, &config).await {
            if let Some(slot_id) = slot_id {
                let _ = db_mark_exchange_worker_error(&pool, slot_id, &err.message).await;
            }
            eprintln!("exchange sync failed: {}", err.message);
        }
    }
}

fn start_bsc_deposit_sync_worker(state: &SharedState) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(StdDuration::from_secs(env_u64(
            "BSC_DEPOSIT_SYNC_INTERVAL_SECONDS",
            DEFAULT_BSC_SYNC_INTERVAL_SECONDS,
            1,
        )));
        loop {
            interval.tick().await;
            if let Err(err) = sync_confirming_deposits(&pool).await {
                eprintln!("BSC deposit sync failed: {}", err.message);
            }
        }
    });
}

fn start_reservation_release_worker(state: &SharedState) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(StdDuration::from_secs(env_u64(
            "RESERVATION_RELEASE_INTERVAL_SECONDS",
            DEFAULT_RESERVATION_RELEASE_INTERVAL_SECONDS,
            1,
        )));
        loop {
            interval.tick().await;
            if let Err(err) = db_release_stale_signal_reservations(&pool).await {
                eprintln!("reservation release failed: {}", err.message);
            }
        }
    });
}

async fn sync_confirming_deposits(pool: &PgPool) -> Result<usize, AppError> {
    let rows = db_confirming_deposits(
        pool,
        env_i64("BSC_DEPOSIT_SYNC_LIMIT", DEFAULT_BSC_DEPOSIT_SYNC_LIMIT, 1),
    )
    .await?;
    let settings = db_load_platform_trading_settings(pool).await?;
    let mut updated = 0usize;
    for deposit in rows {
        let verified = match verify_bsc_contract_deposit(
            &deposit.tx_hash,
            deposit.user_id,
            &deposit.from_address,
            Some(deposit.deposit_id.as_str()),
            Some(deposit.amount),
        )
        .await
        {
            Ok(verified) => verified,
            Err(err) => {
                eprintln!(
                    "BSC deposit {} sync skipped: {}",
                    deposit.tx_hash, err.message
                );
                continue;
            }
        };
        let platform_fee = verified.amount * settings.deposit_fee_rate;
        let refreshed = DepositRecord {
            id: deposit.id,
            user_id: deposit.user_id,
            chain: "bsc".to_string(),
            asset: "USDT".to_string(),
            token_contract: verified.token_contract,
            contract_address: verified.contract_address,
            deposit_id: verified.deposit_id,
            tx_hash: verified.tx_hash,
            from_address: verified.from_address,
            to_address: verified.to_address,
            amount: verified.amount,
            credited_amount: verified.amount - platform_fee,
            platform_fee,
            block_number: verified.block_number,
            confirmations: verified.confirmations,
            status: verified.status,
            credited_at: verified.credited_at,
            created_at: deposit.created_at,
        };
        db_credit_deposit(pool, &refreshed).await?;
        updated += 1;
    }
    Ok(updated)
}

fn start_prediction_loop(state: &SharedState) {
    let base_config = PredictionConfig::from_env();
    {
        let Ok(mut runtime) = state.prediction_runtime.try_write() else {
            return;
        };
        runtime.enabled = base_config.enabled;
        runtime.status = if base_config.enabled {
            "running"
        } else {
            "disabled"
        }
        .to_string();
        runtime.message = if base_config.enabled {
            "strategy loop running".to_string()
        } else {
            "prediction disabled by env".to_string()
        };
    }
    let state = state.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(StdDuration::from_secs(base_config.interval_seconds.max(1)));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let settings = load_prediction_settings(&state).await;
            let config = base_config.with_settings(&settings);
            if !config.enabled {
                let mut runtime = state.prediction_runtime.write().await;
                runtime.enabled = false;
                runtime.status = "disabled".to_string();
                runtime.message = "prediction disabled by admin".to_string();
                runtime.last_evaluated_at = Some(Utc::now());
                continue;
            }
            match evaluate_prediction(&state, &config).await {
                Ok(mut decision) => {
                    if let Some(pool) = state.db.as_ref() {
                        if let Err(err) =
                            apply_prediction_calibration(pool, &config, &mut decision).await
                        {
                            let calibration = PredictionCalibration::disabled(&config);
                            merge_prediction_calibration(&mut decision, &calibration, None);
                            let mut runtime = state.prediction_runtime.write().await;
                            runtime.last_error = Some(err.message.clone());
                        }
                    } else {
                        let calibration = PredictionCalibration::disabled(&config);
                        merge_prediction_calibration(&mut decision, &calibration, None);
                    }
                    update_prediction_runtime(&state, &decision).await;
                    if let Some(pool) = state.db.as_ref() {
                        let _ = db_insert_prediction_observation(pool, &decision).await;
                        if let Err(err) = db_settle_due_signal_outcomes(pool, 100).await {
                            let mut runtime = state.prediction_runtime.write().await;
                            runtime.last_error = Some(err.message);
                        }
                    }
                    if decision.direction == "UP" || decision.direction == "DOWN" {
                        match confirm_prediction_candidate(&state, &decision).await {
                            PredictionConfirmation::Waiting(message) => {
                                let mut runtime = state.prediction_runtime.write().await;
                                runtime.status = "confirming".to_string();
                                runtime.message = message;
                                continue;
                            }
                            PredictionConfirmation::Rejected(message) => {
                                let mut runtime = state.prediction_runtime.write().await;
                                runtime.status = "holding".to_string();
                                runtime.message = message;
                                continue;
                            }
                            PredictionConfirmation::Confirmed => {}
                        }
                        if let Err(err) = emit_prediction_signal(&state, &decision).await {
                            mark_prediction_error(&state, err.message).await;
                        }
                    } else {
                        clear_pending_prediction_candidate(&state).await;
                    }
                }
                Err(err) => mark_prediction_error(&state, err.message).await,
            }
        }
    });
}

async fn update_prediction_runtime(state: &SharedState, decision: &PredictionDecision) {
    let mut runtime = state.prediction_runtime.write().await;
    runtime.enabled = true;
    runtime.status = if decision.direction == "HOLD" {
        "holding"
    } else {
        "matched"
    }
    .to_string();
    runtime.last_evaluated_at = Some(Utc::now());
    runtime.last_direction = decision.direction.clone();
    runtime.last_score = Some(decision.score);
    runtime.last_confidence = Some(decision.confidence);
    runtime.market_source = Some(decision.market_source.clone());
    runtime.market_age_seconds = Some(decision.market_age_seconds);
    runtime.market_candles = decision.market_candles;
    runtime.message = format!(
        "{} | 行情 {} 延迟 {}s K线 {}",
        decision.reason,
        decision.market_source,
        decision.market_age_seconds,
        decision.market_candles
    );
    runtime.last_error = None;
}

async fn mark_prediction_error(state: &SharedState, message: String) {
    let mut runtime = state.prediction_runtime.write().await;
    runtime.enabled = true;
    runtime.status = "error".to_string();
    runtime.last_evaluated_at = Some(Utc::now());
    runtime.last_error = Some(message.chars().take(500).collect());
}

async fn emit_prediction_signal(
    state: &SharedState,
    decision: &PredictionDecision,
) -> Result<(), AppError> {
    let signal_created_at = Utc::now();
    {
        let last_emitted_at = state.prediction_last_emitted_at.read().await;
        if let Some(last) = *last_emitted_at {
            let age = (signal_created_at - last).num_seconds();
            if age >= 0 && age < decision.signal_cooldown_seconds {
                let mut runtime = state.prediction_runtime.write().await;
                runtime.status = "duplicate_skipped".to_string();
                runtime.message = format!(
                    "skip duplicate {} score {:.4}; cooldown {}s remaining",
                    decision.direction,
                    decision.score,
                    decision.signal_cooldown_seconds - age
                );
                return Ok(());
            }
        }
    }
    if let Some(message) = guard_prediction_signal_emit(state, decision, signal_created_at).await {
        let mut last_emitted_at = state.prediction_last_emitted_at.write().await;
        *last_emitted_at = Some(signal_created_at);
        drop(last_emitted_at);
        let mut runtime = state.prediction_runtime.write().await;
        runtime.status = "night_guard".to_string();
        runtime.message = message;
        return Ok(());
    }
    let platform_settings = load_platform_settings(state).await;
    let active_signal_limit = max_active_event_orders_per_user(&platform_settings);
    if let Some(message) =
        guard_active_strategy_signal_limit(state, signal_created_at, active_signal_limit).await?
    {
        let mut runtime = state.prediction_runtime.write().await;
        runtime.status = "active_limit_skipped".to_string();
        runtime.message = message;
        return Ok(());
    }
    if let Some(pool) = state.db.as_ref() {
        if let Some(message) =
            guard_open_strategy_candidate(pool, decision, signal_created_at).await?
        {
            let mut last_emitted_at = state.prediction_last_emitted_at.write().await;
            *last_emitted_at = Some(signal_created_at);
            drop(last_emitted_at);
            let mut runtime = state.prediction_runtime.write().await;
            runtime.status = "open_candidate_skipped".to_string();
            runtime.message = message;
            return Ok(());
        }
        if let Some(message) = guard_recent_strategy_losses(pool, decision).await? {
            let mut last_emitted_at = state.prediction_last_emitted_at.write().await;
            *last_emitted_at = Some(signal_created_at);
            drop(last_emitted_at);
            let mut runtime = state.prediction_runtime.write().await;
            runtime.status = "loss_guard".to_string();
            runtime.message = message;
            return Ok(());
        }
    }
    {
        let mut last_emitted_at = state.prediction_last_emitted_at.write().await;
        *last_emitted_at = Some(signal_created_at);
    }
    let signal = SignalCore::from_prediction_at(decision, signal_created_at);
    if let Some(pool) = state.db.as_ref() {
        if !db_insert_signal(pool, &signal).await? {
            return Ok(());
        }
        let _ = db_insert_prediction_audit(pool, &signal, decision).await;
    }
    state.signals.write().await.push(signal.clone());
    let _ = state.signal_tx.send(signal.clone());
    let mut runtime = state.prediction_runtime.write().await;
    runtime.status = "sent".to_string();
    runtime.last_signal_at = Some(signal.created_at);
    runtime.message = format!(
        "sent {} score {:.4} confidence {:.4}",
        signal.direction, decision.score, decision.confidence
    );
    Ok(())
}

async fn guard_open_strategy_candidate(
    pool: &PgPool,
    decision: &PredictionDecision,
    now: DateTime<Utc>,
) -> Result<Option<String>, AppError> {
    let Some(candidate) = prediction_candidate_from_snapshot(&decision.feature_snapshot) else {
        return Ok(None);
    };
    let row = sqlx::query(
        r#"
        select count(*)::bigint as count
        from signals s
        join prediction_audit_logs a on a.signal_id = s.id
        where s.symbol = $1
          and s.period_seconds = $2
          and s.model_version = $3
          and s.direction = $4
          and s.cycle_end > $5
          and a.feature_snapshot->'decision'->>'candidate_mode' = $6
          and a.feature_snapshot->'decision'->>'candidate_rule' = $7
        "#,
    )
    .bind(DEFAULT_SYMBOL)
    .bind(DEFAULT_PERIOD_SECONDS)
    .bind(PREDICTION_MODEL_VERSION)
    .bind(&candidate.direction)
    .bind(now)
    .bind(&candidate.mode)
    .bind(&candidate.rule)
    .fetch_one(pool)
    .await
    .map_err(AppError::from_sqlx)?;
    let open_count: i64 = row.get("count");
    if open_count > 0 {
        return Ok(Some(format!(
            "skip open candidate {} mode {} rule {} active {} waiting settlement",
            candidate.direction, candidate.mode, candidate.rule, open_count
        )));
    }
    Ok(None)
}

async fn guard_recent_strategy_losses(
    pool: &PgPool,
    decision: &PredictionDecision,
) -> Result<Option<String>, AppError> {
    let Some(candidate) = prediction_candidate_from_snapshot(&decision.feature_snapshot) else {
        return Ok(None);
    };
    if let Some(message) = guard_global_consecutive_losses(pool).await? {
        return Ok(Some(message));
    }
    if let Some(message) = guard_rule_consecutive_losses(pool, &candidate).await? {
        return Ok(Some(message));
    }
    let row = sqlx::query(
        r#"
        select count(*)::bigint as total,
               coalesce(count(*) filter (where o.result = 'win'), 0)::bigint as wins,
               coalesce(count(*) filter (where o.result = 'loss'), 0)::bigint as losses
        from signal_outcomes o
        join prediction_audit_logs a on a.signal_id = o.signal_id
        where o.model_version = $1
          and o.direction = $2
          and o.created_at >= now() - ($3::text || ' hours')::interval
          and a.feature_snapshot->'decision'->>'candidate_mode' = $4
          and a.feature_snapshot->'decision'->>'candidate_rule' = $5
        "#,
    )
    .bind(PREDICTION_MODEL_VERSION)
    .bind(&candidate.direction)
    .bind(PREDICTION_LOSS_GUARD_LOOKBACK_HOURS)
    .bind(&candidate.mode)
    .bind(&candidate.rule)
    .fetch_one(pool)
    .await
    .map_err(AppError::from_sqlx)?;

    let total: i64 = row.get("total");
    let wins: i64 = row.get("wins");
    let losses: i64 = row.get("losses");
    if total < PREDICTION_LOSS_GUARD_MIN_RECENT {
        return Ok(None);
    }
    let win_rate = wins as f64 / total as f64;
    if losses >= PREDICTION_LOSS_GUARD_MIN_RECENT && win_rate <= PREDICTION_LOSS_GUARD_MAX_WIN_RATE
    {
        return Ok(Some(format!(
            "loss guard skipped {} mode {} rule {} recent {}/{} win_rate {:.4}",
            candidate.direction, candidate.mode, candidate.rule, wins, total, win_rate
        )));
    }
    Ok(None)
}

async fn guard_global_consecutive_losses(pool: &PgPool) -> Result<Option<String>, AppError> {
    let rows = sqlx::query(
        r#"
        select result, direction, created_at
        from signal_outcomes
        where model_version = $1
        order by created_at desc
        limit $2
        "#,
    )
    .bind(PREDICTION_MODEL_VERSION)
    .bind(PREDICTION_GLOBAL_CONSECUTIVE_LOSS_GUARD_MIN as i64)
    .fetch_all(pool)
    .await
    .map_err(AppError::from_sqlx)?;

    if rows.len() < PREDICTION_GLOBAL_CONSECUTIVE_LOSS_GUARD_MIN {
        return Ok(None);
    }
    let losses = rows
        .iter()
        .filter(|row| row.get::<String, _>("result") == "loss")
        .count();
    if losses == rows.len() {
        let Some(latest_at) = rows
            .first()
            .map(|row| row.get::<DateTime<Utc>, _>("created_at"))
        else {
            return Ok(None);
        };
        let cooldown_until =
            latest_at + Duration::minutes(PREDICTION_GLOBAL_LOSS_GUARD_COOLDOWN_MINUTES);
        if Utc::now() >= cooldown_until {
            return Ok(None);
        }
        let last_direction = rows
            .first()
            .map(|row| row.get::<String, _>("direction"))
            .unwrap_or_default();
        return Ok(Some(format!(
            "global loss guard skipped after {} consecutive {} losses, last direction {}, cooldown until {}",
            losses, PREDICTION_MODEL_VERSION, last_direction, cooldown_until
        )));
    }
    Ok(None)
}

async fn guard_rule_consecutive_losses(
    pool: &PgPool,
    candidate: &PredictionCandidate,
) -> Result<Option<String>, AppError> {
    let rows = sqlx::query(
        r#"
        select o.result, o.created_at
        from signal_outcomes o
        join prediction_audit_logs a on a.signal_id = o.signal_id
        where o.model_version = $1
          and o.direction = $2
          and a.feature_snapshot->'decision'->>'candidate_mode' = $3
          and a.feature_snapshot->'decision'->>'candidate_rule' = $4
        order by o.created_at desc
        limit $5
        "#,
    )
    .bind(PREDICTION_MODEL_VERSION)
    .bind(&candidate.direction)
    .bind(&candidate.mode)
    .bind(&candidate.rule)
    .bind(PREDICTION_RULE_CONSECUTIVE_LOSS_GUARD_MIN as i64)
    .fetch_all(pool)
    .await
    .map_err(AppError::from_sqlx)?;

    if rows.len() < PREDICTION_RULE_CONSECUTIVE_LOSS_GUARD_MIN {
        return Ok(None);
    }
    let losses = rows
        .iter()
        .filter(|row| row.get::<String, _>("result") == "loss")
        .count();
    if losses == rows.len() {
        let Some(latest_at) = rows
            .first()
            .map(|row| row.get::<DateTime<Utc>, _>("created_at"))
        else {
            return Ok(None);
        };
        let cooldown_until =
            latest_at + Duration::minutes(PREDICTION_RULE_LOSS_GUARD_COOLDOWN_MINUTES);
        if Utc::now() >= cooldown_until {
            return Ok(None);
        }
        return Ok(Some(format!(
            "rule loss guard skipped {} mode {} rule {} after {} consecutive losses, cooldown until {}",
            candidate.direction, candidate.mode, candidate.rule, losses, cooldown_until
        )));
    }
    Ok(None)
}

async fn guard_prediction_signal_emit(
    state: &SharedState,
    decision: &PredictionDecision,
    now: DateTime<Utc>,
) -> Option<String> {
    let mut guard = state.prediction_emit_guard.write().await;
    let same_cluster = guard
        .last_at
        .map(|last| {
            guard.last_direction.as_deref() == Some(decision.direction.as_str())
                && (now - last).num_seconds() <= NIGHT_CLUSTER_GAP_SECONDS
        })
        .unwrap_or(false);
    guard.cluster_position = if same_cluster {
        guard.cluster_position.saturating_add(1)
    } else {
        1
    };
    guard.last_direction = Some(decision.direction.clone());
    guard.last_at = Some(now);

    if is_china_night_protection_window(now) && guard.cluster_position <= NIGHT_CLUSTER_SKIP_FIRST {
        return Some(format!(
            "night guard skipped {} cluster position {} score {:.4} confidence {:.4}",
            decision.direction, guard.cluster_position, decision.score, decision.confidence
        ));
    }
    None
}

async fn guard_active_strategy_signal_limit(
    state: &SharedState,
    cycle_start: DateTime<Utc>,
    active_signal_limit: usize,
) -> Result<Option<String>, AppError> {
    let active_count = active_strategy_signal_count(state, cycle_start).await?;
    if active_count >= active_signal_limit {
        return Ok(Some(format!(
            "skip active signal limit {active_count}/{active_signal_limit} for overlapping {}m window",
            DEFAULT_PERIOD_SECONDS / 60
        )));
    }
    Ok(None)
}

async fn active_strategy_signal_count(
    state: &SharedState,
    cycle_start: DateTime<Utc>,
) -> Result<usize, AppError> {
    if let Some(pool) = state.db.as_ref() {
        return db_active_strategy_signal_count(pool, cycle_start).await;
    }
    let signals = state.signals.read().await;
    Ok(signals
        .iter()
        .filter(|signal| {
            signal.symbol == DEFAULT_SYMBOL
                && signal.period_seconds == DEFAULT_PERIOD_SECONDS
                && signal.cycle_start <= cycle_start
                && signal.cycle_end > cycle_start
        })
        .count())
}

fn is_china_night_protection_window(now: DateTime<Utc>) -> bool {
    let china_hour = (now.hour() + 8) % 24;
    let minute_of_day = china_hour * 60 + now.minute();
    (NIGHT_PROTECTION_START_MINUTE..NIGHT_PROTECTION_END_MINUTE).contains(&minute_of_day)
}

async fn sync_exchange_worker_runtime(
    state: &SharedState,
    slot: &ExchangeWorkerSlot,
) -> Result<(), AppError> {
    stop_exchange_worker_runtime(state, slot.id).await;
    if !slot.enabled {
        return Ok(());
    }
    let Some(pool) = state.db.clone() else {
        return Err(AppError::bad_request(
            "DATABASE_REQUIRED",
            "exchange worker requires PostgreSQL",
        ));
    };
    let bind_local_ip = parse_optional_ip(slot.bind_local_ip.as_deref(), "bind_local_ip")?;
    let config = ExchangeWorkerConfig {
        shard_total: slot.shard_total,
        shard_index: slot.shard_index,
        bind_local_ip,
    };
    let slot_id = slot.id;
    let handle = tokio::spawn(async move {
        run_exchange_sync_worker(pool, config, Some(slot_id)).await;
    });
    state.exchange_workers.write().await.insert(slot_id, handle);
    Ok(())
}

async fn stop_exchange_worker_runtime(state: &SharedState, slot_id: Uuid) {
    if let Some(handle) = state.exchange_workers.write().await.remove(&slot_id) {
        handle.abort();
    }
}

fn cors_layer() -> CorsLayer {
    let methods = [Method::GET, Method::POST, Method::PUT, Method::OPTIONS];
    match env::var("CORS_ALLOWED_ORIGINS") {
        Ok(origins) => {
            let origins = origins
                .split(',')
                .filter_map(|origin| origin.trim().parse::<HeaderValue>().ok())
                .collect::<Vec<_>>();
            if origins.is_empty() {
                CorsLayer::new().allow_methods(methods).allow_headers(Any)
            } else {
                CorsLayer::new()
                    .allow_origin(origins)
                    .allow_methods(methods)
                    .allow_headers(Any)
            }
        }
        Err(_) => CorsLayer::new().allow_methods(methods).allow_headers(Any),
    }
}

type SharedState = Arc<AppState>;

struct AppState {
    db: Option<PgPool>,
    users: RwLock<HashMap<Uuid, UserRecord>>,
    admin_users: RwLock<HashMap<String, AdminAccountRecord>>,
    admin_sessions: RwLock<HashMap<String, AdminSessionRecord>>,
    user_by_wallet: RwLock<HashMap<String, Uuid>>,
    referral_codes: RwLock<HashMap<String, Uuid>>,
    referral_links: RwLock<HashMap<Uuid, ReferralLinkRecord>>,
    referral_rewards: RwLock<Vec<ReferralRewardRecord>>,
    wallet_nonces: RwLock<HashMap<Uuid, WalletBindChallenge>>,
    sessions: RwLock<HashMap<String, SessionRecord>>,
    app_signature_nonces: RwLock<HashMap<String, DateTime<Utc>>>,
    rate_limits: RwLock<HashMap<String, RegisterRateLimit>>,
    balances: RwLock<HashMap<Uuid, BalanceRecord>>,
    trading_settings: RwLock<HashMap<Uuid, TradingSettings>>,
    automation_settings: RwLock<HashMap<Uuid, AutomationSettings>>,
    page_states: RwLock<HashMap<Uuid, AutomationPageState>>,
    signals: RwLock<Vec<SignalCore>>,
    orders: RwLock<Vec<EventContractOrderRecord>>,
    reports: RwLock<Vec<ExecutionReportRecord>>,
    settlements: RwLock<Vec<SettlementRecord>>,
    ledger: RwLock<Vec<LedgerEntry>>,
    deposits: RwLock<Vec<DepositRecord>>,
    withdrawals: RwLock<Vec<WithdrawalRecord>>,
    reservations: RwLock<HashMap<Uuid, SignalReservation>>,
    exchange_workers: RwLock<HashMap<Uuid, JoinHandle<()>>>,
    market_data: RwLock<MarketDataState>,
    event_ml_model: Option<Arc<EventMlModel>>,
    prediction_runtime: RwLock<PredictionRuntimeStatus>,
    prediction_last_emitted_at: RwLock<Option<DateTime<Utc>>>,
    prediction_pending_candidate: RwLock<Option<PendingPredictionCandidate>>,
    prediction_emit_guard: RwLock<PredictionEmitGuard>,
    active_ws_connections: RwLock<HashMap<Uuid, Uuid>>,
    signal_tx: broadcast::Sender<SignalCore>,
    started_at: DateTime<Utc>,
}

impl AppState {
    fn new(db: Option<PgPool>) -> SharedState {
        let (signal_tx, _) = broadcast::channel(128);
        Arc::new(Self {
            db,
            users: RwLock::new(HashMap::new()),
            admin_users: RwLock::new(HashMap::new()),
            admin_sessions: RwLock::new(HashMap::new()),
            user_by_wallet: RwLock::new(HashMap::new()),
            referral_codes: RwLock::new(HashMap::new()),
            referral_links: RwLock::new(HashMap::new()),
            referral_rewards: RwLock::new(Vec::new()),
            wallet_nonces: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
            app_signature_nonces: RwLock::new(HashMap::new()),
            rate_limits: RwLock::new(HashMap::new()),
            balances: RwLock::new(HashMap::new()),
            trading_settings: RwLock::new(HashMap::new()),
            automation_settings: RwLock::new(HashMap::new()),
            page_states: RwLock::new(HashMap::new()),
            signals: RwLock::new(Vec::new()),
            orders: RwLock::new(Vec::new()),
            reports: RwLock::new(Vec::new()),
            settlements: RwLock::new(Vec::new()),
            ledger: RwLock::new(Vec::new()),
            deposits: RwLock::new(Vec::new()),
            withdrawals: RwLock::new(Vec::new()),
            reservations: RwLock::new(HashMap::new()),
            exchange_workers: RwLock::new(HashMap::new()),
            market_data: RwLock::new(MarketDataState::default()),
            event_ml_model: load_event_ml_model().map(Arc::new),
            prediction_runtime: RwLock::new(PredictionRuntimeStatus::default()),
            prediction_last_emitted_at: RwLock::new(None),
            prediction_pending_candidate: RwLock::new(None),
            prediction_emit_guard: RwLock::new(PredictionEmitGuard::default()),
            active_ws_connections: RwLock::new(HashMap::new()),
            signal_tx,
            started_at: Utc::now(),
        })
    }
}

#[derive(Clone, Serialize)]
struct UserRecord {
    id: Uuid,
    wallet_address: String,
    password_hash: String,
    status: String,
    active_device_fingerprint: Option<String>,
    session_version: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone)]
struct AdminAccountRecord {
    id: Uuid,
    password_hash: String,
    status: String,
}

#[derive(Clone)]
struct AdminSessionRecord {
    expires_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct AdminUserRecord {
    id: Uuid,
    wallet_address: String,
    status: String,
    active_device_fingerprint: Option<String>,
    balance_available: String,
    balance_frozen: String,
    balance_updated_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<UserRecord> for AdminUserRecord {
    fn from(user: UserRecord) -> Self {
        Self {
            id: user.id,
            wallet_address: user.wallet_address,
            status: user.status,
            active_device_fingerprint: user.active_device_fingerprint,
            balance_available: money(Decimal::ZERO),
            balance_frozen: money(Decimal::ZERO),
            balance_updated_at: None,
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

impl ClientType {
    fn as_str(self) -> &'static str {
        match self {
            ClientType::Android => "android",
            ClientType::Dapp => "dapp",
        }
    }
}

#[derive(Clone)]
struct WalletBindChallenge {
    purpose: WalletChallengePurpose,
    chain: String,
    address: String,
    message: String,
    expires_at: DateTime<Utc>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WalletChallengePurpose {
    BindWallet,
    ChangePassword,
}

impl WalletChallengePurpose {
    fn label(&self) -> &'static str {
        match self {
            WalletChallengePurpose::BindWallet => "wallet bind",
            WalletChallengePurpose::ChangePassword => "password change",
        }
    }
}

#[derive(Clone)]
struct SessionRecord {
    user_id: Uuid,
    device_fingerprint: String,
    refresh_token: String,
    app_session_key: String,
    client_type: ClientType,
    session_version: i32,
    expires_at: DateTime<Utc>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClientType {
    Android,
    Dapp,
}

#[derive(Clone)]
struct RegisterRateLimit {
    window_start: DateTime<Utc>,
    attempts: u32,
    blocked_until: Option<DateTime<Utc>>,
}

#[derive(Clone, Serialize)]
struct BalanceRecord {
    user_id: Uuid,
    asset: String,
    available: Decimal,
    frozen: Decimal,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize)]
struct TradingSettings {
    stake_percent: Decimal,
    min_stake_usdt: Decimal,
    max_single_stake_usdt: Option<Decimal>,
    min_confidence: Option<Decimal>,
    signal_alert_enabled: bool,
    floating_window_enabled: bool,
}

impl From<&PlatformTradingSettings> for TradingSettings {
    fn from(settings: &PlatformTradingSettings) -> Self {
        Self {
            stake_percent: settings.stake_percent,
            min_stake_usdt: settings.min_stake_usdt,
            max_single_stake_usdt: Some(settings.max_single_stake_usdt),
            min_confidence: None,
            signal_alert_enabled: true,
            floating_window_enabled: true,
        }
    }
}

impl Default for TradingSettings {
    fn default() -> Self {
        Self {
            stake_percent: DEFAULT_STAKE_PERCENT,
            min_stake_usdt: MIN_EVENT_STAKE,
            max_single_stake_usdt: None,
            min_confidence: None,
            signal_alert_enabled: true,
            floating_window_enabled: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct PlatformTradingSettings {
    stake_percent: Decimal,
    min_stake_usdt: Decimal,
    max_single_stake_usdt: Decimal,
    max_active_event_orders_per_user: i32,
    first_deposit_min_usdt: Decimal,
    deposit_fee_rate: Decimal,
    direct_referral_rate: Decimal,
    indirect_referral_rate: Decimal,
    updated_at: Option<DateTime<Utc>>,
}

impl Default for PlatformTradingSettings {
    fn default() -> Self {
        Self {
            stake_percent: DEFAULT_STAKE_PERCENT,
            min_stake_usdt: MIN_EVENT_STAKE,
            max_single_stake_usdt: MAX_EVENT_STAKE,
            max_active_event_orders_per_user: DEFAULT_MAX_ACTIVE_EVENT_ORDERS_PER_USER as i32,
            first_deposit_min_usdt: FIRST_DEPOSIT_MIN_AMOUNT,
            deposit_fee_rate: DEPOSIT_FEE_RATE,
            direct_referral_rate: DIRECT_REFERRAL_RATE,
            indirect_referral_rate: INDIRECT_REFERRAL_RATE,
            updated_at: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct AutomationSettings {
    automation_enabled: bool,
    execution_mode: String,
    target_app_profile: Option<String>,
    require_final_confirmation: bool,
    emergency_stop_enabled: bool,
}

impl Default for AutomationSettings {
    fn default() -> Self {
        Self {
            automation_enabled: true,
            execution_mode: "automatic".to_string(),
            target_app_profile: Some("binance_event_contract_v1".to_string()),
            require_final_confirmation: false,
            emergency_stop_enabled: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct AutomationPageState {
    page_ready: bool,
    screen_name: String,
    confidence: Decimal,
    package_name: String,
    amount_field_found: bool,
    up_button_found: bool,
    down_button_found: bool,
    observed_balance_usdt: Option<Decimal>,
    account_match_status: String,
    account_balance_delta_usdt: Option<Decimal>,
    observed_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct SignalCore {
    id: Uuid,
    symbol: String,
    period_seconds: i32,
    cycle_start: DateTime<Utc>,
    cycle_end: DateTime<Utc>,
    direction: String,
    confidence: Decimal,
    predicted_return: Decimal,
    entry_price: Decimal,
    suggested_stake: Decimal,
    model_version: String,
    created_at: DateTime<Utc>,
    outcome_result: Option<String>,
    outcome_expiry_price: Option<Decimal>,
    outcome_settled_at: Option<DateTime<Utc>>,
}

impl SignalCore {
    fn new(direction: String, suggested_stake: Decimal, confidence: Decimal) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::now_v7(),
            symbol: DEFAULT_SYMBOL.to_string(),
            period_seconds: DEFAULT_PERIOD_SECONDS,
            cycle_start: now,
            cycle_end: now + Duration::seconds(DEFAULT_PERIOD_SECONDS.into()),
            direction,
            confidence,
            predicted_return: dec!(0.0008),
            entry_price: dec!(0),
            suggested_stake,
            model_version: PREDICTION_MODEL_VERSION.to_string(),
            created_at: now,
            outcome_result: None,
            outcome_expiry_price: None,
            outcome_settled_at: None,
        }
    }

    fn manual(direction: String, suggested_stake: Decimal, confidence: Decimal) -> Self {
        let mut signal = Self::new(direction, suggested_stake, confidence);
        signal.model_version = "manual-signal-v1".to_string();
        signal
    }

    fn uses_explicit_stake(&self) -> bool {
        self.model_version == "manual-signal-v1"
    }

    fn from_prediction_at(decision: &PredictionDecision, created_at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::now_v7(),
            symbol: DEFAULT_SYMBOL.to_string(),
            period_seconds: DEFAULT_PERIOD_SECONDS,
            cycle_start: created_at,
            cycle_end: created_at + Duration::seconds(DEFAULT_PERIOD_SECONDS as i64),
            direction: decision.direction.clone(),
            confidence: decimal_from_f64(decision.confidence, 6),
            predicted_return: decimal_from_f64(decision.predicted_return, 10),
            entry_price: decimal_from_f64(decision.entry_price, 8),
            suggested_stake: MIN_EVENT_STAKE,
            model_version: PREDICTION_MODEL_VERSION.to_string(),
            created_at,
            outcome_result: None,
            outcome_expiry_price: None,
            outcome_settled_at: None,
        }
    }
}

#[derive(Clone)]
struct PredictionRuntimeStatus {
    enabled: bool,
    status: String,
    last_evaluated_at: Option<DateTime<Utc>>,
    last_signal_at: Option<DateTime<Utc>>,
    last_direction: String,
    last_score: Option<f64>,
    last_confidence: Option<f64>,
    market_source: Option<String>,
    market_age_seconds: Option<i64>,
    market_candles: usize,
    message: String,
    last_error: Option<String>,
}

impl Default for PredictionRuntimeStatus {
    fn default() -> Self {
        Self {
            enabled: false,
            status: "starting".to_string(),
            last_evaluated_at: None,
            last_signal_at: None,
            last_direction: "HOLD".to_string(),
            last_score: None,
            last_confidence: None,
            market_source: None,
            market_age_seconds: None,
            market_candles: 0,
            message: "prediction loop starting".to_string(),
            last_error: None,
        }
    }
}

#[derive(Clone)]
struct PredictionConfig {
    enabled: bool,
    interval_seconds: u64,
    candle_limit: usize,
    stale_seconds: i64,
    up_threshold: f64,
    down_threshold: f64,
    min_confidence: f64,
    technical_weight: f64,
    probability_weight: f64,
    volatility_floor: f64,
    market_data_base_url: String,
    market_ws_url: String,
    market_ws_reconnect_seconds: u64,
    signal_cooldown_seconds: i64,
    calibration_lookback_minutes: i64,
    calibration_min_samples: i32,
    calibration_min_win_rate: f64,
    calibration_min_ev_usdt: f64,
    calibration_sample_seconds: i64,
}

#[derive(Clone, Serialize, Deserialize)]
struct PredictionSettings {
    enabled: bool,
    interval_seconds: i32,
    up_threshold: Decimal,
    down_threshold: Decimal,
    min_confidence: Decimal,
    technical_weight: Decimal,
    probability_weight: Decimal,
    volatility_floor: Decimal,
    calibration_lookback_minutes: i32,
    calibration_min_samples: i32,
    calibration_min_win_rate: Decimal,
    calibration_min_ev_usdt: Decimal,
    updated_at: Option<DateTime<Utc>>,
}

impl Default for PredictionSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: DEFAULT_PREDICTION_INTERVAL_SECONDS as i32,
            up_threshold: decimal_from_f64(DEFAULT_PREDICTION_UP_THRESHOLD, 10),
            down_threshold: decimal_from_f64(DEFAULT_PREDICTION_DOWN_THRESHOLD, 10),
            min_confidence: decimal_from_f64(DEFAULT_PREDICTION_MIN_CONFIDENCE, 6),
            technical_weight: decimal_from_f64(DEFAULT_PREDICTION_TECHNICAL_WEIGHT, 6),
            probability_weight: decimal_from_f64(DEFAULT_PREDICTION_PROBABILITY_WEIGHT, 6),
            volatility_floor: decimal_from_f64(DEFAULT_PREDICTION_VOLATILITY_FLOOR, 8),
            calibration_lookback_minutes: DEFAULT_PREDICTION_CALIBRATION_LOOKBACK_MINUTES as i32,
            calibration_min_samples: DEFAULT_PREDICTION_CALIBRATION_MIN_SAMPLES,
            calibration_min_win_rate: decimal_from_f64(
                DEFAULT_PREDICTION_CALIBRATION_MIN_WIN_RATE,
                6,
            ),
            calibration_min_ev_usdt: decimal_from_f64(
                DEFAULT_PREDICTION_CALIBRATION_MIN_EV_USDT,
                4,
            ),
            updated_at: None,
        }
    }
}

fn normalize_prediction_settings(mut settings: PredictionSettings) -> PredictionSettings {
    settings.up_threshold = decimal_from_f64(DEFAULT_PREDICTION_UP_THRESHOLD, 10);
    settings.down_threshold = decimal_from_f64(DEFAULT_PREDICTION_DOWN_THRESHOLD, 10);
    if settings.min_confidence < decimal_from_f64(DEFAULT_PREDICTION_MIN_CONFIDENCE, 6) {
        settings.min_confidence = decimal_from_f64(DEFAULT_PREDICTION_MIN_CONFIDENCE, 6);
    }
    settings.technical_weight = decimal_from_f64(DEFAULT_PREDICTION_TECHNICAL_WEIGHT, 6);
    settings.probability_weight = decimal_from_f64(DEFAULT_PREDICTION_PROBABILITY_WEIGHT, 6);
    if settings.volatility_floor < decimal_from_f64(DEFAULT_PREDICTION_VOLATILITY_FLOOR, 8) {
        settings.volatility_floor = decimal_from_f64(DEFAULT_PREDICTION_VOLATILITY_FLOOR, 8);
    }
    if settings.calibration_lookback_minutes
        < DEFAULT_PREDICTION_CALIBRATION_LOOKBACK_MINUTES as i32
    {
        settings.calibration_lookback_minutes =
            DEFAULT_PREDICTION_CALIBRATION_LOOKBACK_MINUTES as i32;
    }
    if settings.calibration_min_samples < DEFAULT_PREDICTION_CALIBRATION_MIN_SAMPLES {
        settings.calibration_min_samples = DEFAULT_PREDICTION_CALIBRATION_MIN_SAMPLES;
    }
    if settings.calibration_min_win_rate
        < decimal_from_f64(DEFAULT_PREDICTION_CALIBRATION_MIN_WIN_RATE, 6)
    {
        settings.calibration_min_win_rate =
            decimal_from_f64(DEFAULT_PREDICTION_CALIBRATION_MIN_WIN_RATE, 6);
    }
    if settings.calibration_min_ev_usdt
        < decimal_from_f64(DEFAULT_PREDICTION_CALIBRATION_MIN_EV_USDT, 4)
    {
        settings.calibration_min_ev_usdt =
            decimal_from_f64(DEFAULT_PREDICTION_CALIBRATION_MIN_EV_USDT, 4);
    }
    settings
}

#[derive(Clone, Serialize)]
struct PredictionObservationRecord {
    id: Uuid,
    source: String,
    symbol: String,
    period_seconds: i32,
    model_version: String,
    direction: String,
    confidence: Decimal,
    predicted_return: Decimal,
    entry_price: Decimal,
    score: Option<Decimal>,
    feature_snapshot: serde_json::Value,
    reason: Option<String>,
    created_at: DateTime<Utc>,
}

struct PredictionDecision {
    direction: String,
    confidence: f64,
    predicted_return: f64,
    entry_price: f64,
    score: f64,
    signal_cooldown_seconds: i64,
    market_source: String,
    market_age_seconds: i64,
    market_candles: usize,
    reason: String,
    feature_snapshot: serde_json::Value,
}

#[derive(Default)]
struct PredictionEmitGuard {
    last_direction: Option<String>,
    last_at: Option<DateTime<Utc>>,
    cluster_position: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct EventMlModel {
    version: String,
    symbol: String,
    period_seconds: i32,
    thresholds: EventMlThresholds,
    execution_filter: EventMlExecutionFilter,
    feature_names: Vec<String>,
    models: EventMlModels,
}

#[derive(Clone, Debug, Deserialize)]
struct EventMlThresholds {
    move_probability: f64,
    direction_probability: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct EventMlExecutionFilter {
    up_falling_knife_min_body_share_60: f64,
    up_falling_knife_max_lower_wick_share_60: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct EventMlModels {
    #[serde(rename = "move")]
    move_model: EventMlForest,
    direction: EventMlForest,
}

#[derive(Clone, Debug, Deserialize)]
struct EventMlForest {
    trees: Vec<EventMlTree>,
}

#[derive(Clone, Debug, Deserialize)]
struct EventMlTree {
    children_left: Vec<i32>,
    children_right: Vec<i32>,
    feature: Vec<i32>,
    threshold: Vec<f64>,
    positive_prob: Vec<f64>,
}

#[derive(Clone, Debug)]
struct EventMlPrediction {
    direction: String,
    rule: String,
    move_probability: f64,
    up_probability: f64,
    confidence: f64,
    score: f64,
    filtered_reason: Option<String>,
    features: serde_json::Value,
}

#[derive(Clone, Copy)]
struct EventMlMetaPolicyOptions {
    two_stage_enabled: bool,
    two_stage_downshift_enabled: bool,
    two_stage_reverse_enabled: bool,
    slow_up_enabled: bool,
}

#[derive(Clone, Debug)]
struct PredictionCandidate {
    direction: String,
    mode: String,
    rule: String,
    raw_confidence: f64,
    structure_score: f64,
}

#[derive(Clone, Debug)]
struct PendingPredictionCandidate {
    direction: String,
    rule: String,
    first_seen_at: DateTime<Utc>,
    entry_price: f64,
    raw_confidence: f64,
    structure_score: f64,
}

enum PredictionConfirmation {
    Waiting(String),
    Rejected(String),
    Confirmed,
}

#[derive(Clone, Debug)]
struct PredictionCalibration {
    allowed: bool,
    sample_count: i64,
    wins: i64,
    win_rate: f64,
    ev_usdt: f64,
    min_samples: i32,
    min_win_rate: f64,
    min_ev_usdt: f64,
    lookback_minutes: i64,
    sample_seconds: i64,
    reason: String,
}

impl PredictionCalibration {
    fn disabled(config: &PredictionConfig) -> Self {
        Self {
            allowed: false,
            sample_count: 0,
            wins: 0,
            win_rate: 0.0,
            ev_usdt: -5.0,
            min_samples: config.calibration_min_samples,
            min_win_rate: config.calibration_min_win_rate,
            min_ev_usdt: config.calibration_min_ev_usdt,
            lookback_minutes: config.calibration_lookback_minutes,
            sample_seconds: config.calibration_sample_seconds,
            reason: "calibration unavailable".to_string(),
        }
    }

    fn forced_allowed(config: &PredictionConfig, reason: String, win_rate: f64) -> Self {
        Self {
            allowed: true,
            sample_count: 0,
            wins: 0,
            win_rate,
            ev_usdt: 0.0,
            min_samples: config.calibration_min_samples,
            min_win_rate: config.calibration_min_win_rate,
            min_ev_usdt: config.calibration_min_ev_usdt,
            lookback_minutes: config.calibration_lookback_minutes,
            sample_seconds: config.calibration_sample_seconds,
            reason,
        }
    }
}

#[derive(Default)]
struct MarketDataState {
    candles: BTreeMap<i64, MarketCandle>,
    price_points: BTreeMap<i64, f64>,
    second_bars: BTreeMap<i64, MarketSecondBar>,
    last_second_close: Option<f64>,
    last_second_close_from_index: bool,
    latest_price: Option<f64>,
    latest_price_time_ms: Option<i64>,
    latest_source: Option<String>,
    latest_mark_price: Option<f64>,
    latest_trade_price: Option<f64>,
    latest_futures_kline_price: Option<f64>,
    last_connected_at: Option<DateTime<Utc>>,
    last_message_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    preheated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
struct MarketSecondBar {
    second_ms: i64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    taker_buy_volume: f64,
    taker_sell_volume: f64,
    trade_count: i32,
    price_from_index: bool,
}

#[derive(Clone)]
struct MarketDataSnapshot {
    candles: Vec<MarketCandle>,
    price_points: Vec<(i64, f64)>,
    second_bars: Vec<MarketSecondBar>,
    latest_price: f64,
    latest_price_time_ms: i64,
    latest_source: String,
    latest_mark_price: Option<f64>,
    latest_trade_price: Option<f64>,
    latest_futures_kline_price: Option<f64>,
    last_message_at: Option<DateTime<Utc>>,
}

impl MarketDataState {
    fn snapshot(&self, candle_limit: usize) -> Option<MarketDataSnapshot> {
        let latest_price = self.latest_price?;
        let latest_price_time_ms = self.latest_price_time_ms?;
        let mut candles = self
            .candles
            .values()
            .rev()
            .take(candle_limit)
            .cloned()
            .collect::<Vec<_>>();
        candles.reverse();
        let mut price_points = self
            .price_points
            .iter()
            .rev()
            .take(candle_limit.saturating_mul(60).max(600))
            .map(|(time_ms, price)| (*time_ms, *price))
            .collect::<Vec<_>>();
        price_points.reverse();
        let mut second_bars = self
            .second_bars
            .values()
            .rev()
            .take(EVENT_ML_WARM_SECONDS + 700)
            .cloned()
            .collect::<Vec<_>>();
        second_bars.reverse();
        Some(MarketDataSnapshot {
            candles,
            price_points,
            second_bars,
            latest_price,
            latest_price_time_ms,
            latest_source: self
                .latest_source
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            latest_mark_price: self.latest_mark_price,
            latest_trade_price: self.latest_trade_price,
            latest_futures_kline_price: self.latest_futures_kline_price,
            last_message_at: self.last_message_at,
        })
    }
}

#[derive(Deserialize)]
struct BinanceCombinedStreamMessage {
    stream: Option<String>,
    data: serde_json::Value,
}

#[derive(Deserialize)]
struct BinanceAggTradeStream {
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "f")]
    first_trade_id: i64,
    #[serde(rename = "l")]
    last_trade_id: i64,
    #[serde(rename = "T")]
    trade_time_ms: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[derive(Deserialize)]
struct BinanceAggTradeRest {
    #[serde(rename = "a")]
    aggregate_trade_id: i64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
    #[serde(rename = "T")]
    trade_time_ms: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

#[derive(Deserialize)]
struct BinanceMarkPriceStream {
    #[serde(rename = "p")]
    mark_price: String,
    #[serde(rename = "i")]
    index_price: String,
    #[serde(rename = "E")]
    event_time_ms: i64,
}

#[derive(Deserialize)]
struct BinanceKlineStream {
    #[serde(rename = "k")]
    kline: BinanceKlinePayload,
}

#[derive(Deserialize)]
struct BinanceKlinePayload {
    #[serde(rename = "t")]
    open_time_ms: i64,
    #[serde(rename = "T")]
    close_time_ms: i64,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "x")]
    is_closed: bool,
}

#[derive(Clone)]
struct MarketCandle {
    open_time_ms: i64,
    close_time_ms: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Clone, Serialize)]
struct SignalReservation {
    id: Uuid,
    user_id: Uuid,
    signal_id: Uuid,
    stake_usdt: Decimal,
    expected_profit_usdt: Decimal,
    fee_reserve_usdt: Decimal,
    status: String,
    order_id: Option<Uuid>,
    execution_report_id: Option<Uuid>,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct EventContractOrderRecord {
    id: Uuid,
    user_id: Uuid,
    reservation_id: Option<Uuid>,
    signal_id: String,
    client_order_id: String,
    source: String,
    symbol: String,
    period_seconds: i32,
    direction: String,
    stake_usdt: Decimal,
    entry_price: Option<Decimal>,
    automation_profile: Option<String>,
    automation_confidence: Option<Decimal>,
    action_trace_id: Option<String>,
    raw_audit: Option<serde_json::Value>,
    order_status: String,
    reported_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct ExecutionReportRecord {
    id: Uuid,
    user_id: Uuid,
    signal_id: String,
    client_report_id: String,
    source: String,
    symbol: String,
    period_seconds: i32,
    direction: String,
    stake_usdt: Decimal,
    entry_price: Option<Decimal>,
    expiry_price: Option<Decimal>,
    payout_ratio: Option<Decimal>,
    gross_profit: Option<Decimal>,
    result: String,
    automation_profile: Option<String>,
    automation_confidence: Option<Decimal>,
    action_trace_id: Option<String>,
    report_status: String,
    reported_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct SettlementRecord {
    id: Uuid,
    execution_report_id: Uuid,
    user_id: Uuid,
    signal_id: Option<String>,
    symbol: Option<String>,
    direction: Option<String>,
    stake_usdt: Option<Decimal>,
    entry_price: Option<Decimal>,
    expiry_price: Option<Decimal>,
    result: Option<String>,
    gross_profit: Decimal,
    profit_share_fee: Decimal,
    loss_rebate: Decimal,
    user_net_profit: Decimal,
    status: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct LedgerEntry {
    id: Uuid,
    user_id: Uuid,
    asset: String,
    entry_type: String,
    amount: Decimal,
    balance_available_after: Decimal,
    balance_frozen_after: Decimal,
    ref_type: String,
    ref_id: Uuid,
    idempotency_key: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct ReferralLinkRecord {
    user_id: Uuid,
    parent_user_id: Option<Uuid>,
    grandparent_user_id: Option<Uuid>,
    referral_code: String,
    invited_by_code: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct ReferralRewardRecord {
    id: Uuid,
    settlement_id: Uuid,
    source_user_id: Uuid,
    recipient_user_id: Uuid,
    level: i32,
    rate: Decimal,
    base_profit: Decimal,
    amount: Decimal,
    status: String,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct DepositRecord {
    id: Uuid,
    user_id: Uuid,
    chain: String,
    asset: String,
    token_contract: String,
    contract_address: String,
    deposit_id: String,
    tx_hash: String,
    from_address: String,
    to_address: String,
    amount: Decimal,
    credited_amount: Decimal,
    platform_fee: Decimal,
    block_number: i64,
    confirmations: i32,
    status: String,
    credited_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Clone)]
struct VerifiedDeposit {
    tx_hash: String,
    token_contract: String,
    contract_address: String,
    deposit_id: String,
    from_address: String,
    to_address: String,
    amount: Decimal,
    block_number: i64,
    confirmations: i32,
    status: String,
    credited_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Serialize)]
struct WithdrawalRecord {
    id: Uuid,
    user_id: Uuid,
    chain: String,
    asset: String,
    token_contract: String,
    contract_address: String,
    tx_hash: Option<String>,
    refund_id: String,
    payout_wallet: Option<String>,
    to_address: String,
    amount: Decimal,
    block_number: Option<i64>,
    confirmations: i32,
    status: String,
    requested_at: DateTime<Utc>,
    processed_at: Option<DateTime<Utc>>,
    rejected_at: Option<DateTime<Utc>>,
    reject_reason: Option<String>,
}

#[derive(Clone, Serialize)]
struct ExchangeCredentialStatus {
    provider: String,
    api_key: Option<String>,
    connected: bool,
    can_read_account: bool,
    verification_status: String,
    balance_available_usdt: Option<String>,
    balance_wallet_usdt: Option<String>,
    account_match_status: Option<String>,
    observed_balance_usdt: Option<String>,
    account_balance_delta_usdt: Option<String>,
    balance_updated_at: Option<DateTime<Utc>>,
    balance_cache_fresh: bool,
    last_verified_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone)]
struct ExchangeCredentialRecord {
    user_id: Uuid,
    api_key: String,
    api_secret_ciphertext: String,
}

#[derive(Clone)]
struct BinanceAccountSnapshot {
    wallet_balance_usdt: Decimal,
    available_balance_usdt: Decimal,
}

#[derive(Clone)]
struct ExchangeBalanceSnapshot {
    available: Decimal,
    updated_at: Option<DateTime<Utc>>,
}

struct AccountMatch {
    status: String,
    delta: Option<Decimal>,
}

#[derive(Clone)]
struct ExchangeWorkerConfig {
    shard_total: i32,
    shard_index: i32,
    bind_local_ip: Option<std::net::IpAddr>,
}

impl ExchangeWorkerConfig {
    fn from_env() -> Self {
        let shard_total = env_i32("WORKER_SHARD_TOTAL", DEFAULT_EXCHANGE_WORKER_SHARD_TOTAL, 1);
        let shard_index = env_i32("WORKER_SHARD_INDEX", DEFAULT_EXCHANGE_WORKER_SHARD_INDEX, 0);
        Self {
            shard_total,
            shard_index: shard_index.rem_euclid(shard_total),
            bind_local_ip: binance_bind_local_ip().unwrap_or(None),
        }
    }
}

impl PredictionConfig {
    fn from_env() -> Self {
        Self {
            enabled: env_bool("PREDICTION_ENABLED", true),
            interval_seconds: env_u64(
                "PREDICTION_INTERVAL_SECONDS",
                DEFAULT_PREDICTION_INTERVAL_SECONDS,
                1,
            ),
            candle_limit: env_usize(
                "PREDICTION_CANDLE_LIMIT",
                DEFAULT_PREDICTION_CANDLE_LIMIT,
                60,
            ),
            stale_seconds: env_i64(
                "PREDICTION_STALE_SECONDS",
                DEFAULT_PREDICTION_STALE_SECONDS,
                30,
            ),
            up_threshold: env_f64(
                "PREDICTION_UP_THRESHOLD",
                DEFAULT_PREDICTION_UP_THRESHOLD,
                0.1,
            ),
            down_threshold: env_f64(
                "PREDICTION_DOWN_THRESHOLD",
                DEFAULT_PREDICTION_DOWN_THRESHOLD,
                -100.0,
            ),
            min_confidence: env_f64(
                "PREDICTION_MIN_CONFIDENCE",
                DEFAULT_PREDICTION_MIN_CONFIDENCE,
                0.0,
            ),
            technical_weight: env_f64(
                "PREDICTION_TECHNICAL_WEIGHT",
                DEFAULT_PREDICTION_TECHNICAL_WEIGHT,
                0.0,
            ),
            probability_weight: env_f64(
                "PREDICTION_PROBABILITY_WEIGHT",
                DEFAULT_PREDICTION_PROBABILITY_WEIGHT,
                0.0,
            ),
            volatility_floor: env_f64(
                "PREDICTION_VOLATILITY_FLOOR",
                DEFAULT_PREDICTION_VOLATILITY_FLOOR,
                0.0,
            ),
            market_data_base_url: env::var("MARKET_DATA_PRIMARY")
                .unwrap_or_else(|_| DEFAULT_MARKET_DATA_PRIMARY.to_string())
                .trim_end_matches('/')
                .to_string(),
            market_ws_url: env::var("MARKET_WS_URL")
                .unwrap_or_else(|_| DEFAULT_MARKET_WS_URL.to_string()),
            market_ws_reconnect_seconds: env_u64(
                "MARKET_WS_RECONNECT_SECONDS",
                DEFAULT_MARKET_WS_RECONNECT_SECONDS,
                1,
            ),
            signal_cooldown_seconds: env_i64(
                "PREDICTION_SIGNAL_COOLDOWN_SECONDS",
                DEFAULT_PREDICTION_SIGNAL_COOLDOWN_SECONDS,
                0,
            ),
            calibration_lookback_minutes: env_i64(
                "PREDICTION_CALIBRATION_LOOKBACK_MINUTES",
                DEFAULT_PREDICTION_CALIBRATION_LOOKBACK_MINUTES,
                10,
            ),
            calibration_min_samples: env_i32(
                "PREDICTION_CALIBRATION_MIN_SAMPLES",
                DEFAULT_PREDICTION_CALIBRATION_MIN_SAMPLES,
                1,
            ),
            calibration_min_win_rate: env_f64(
                "PREDICTION_CALIBRATION_MIN_WIN_RATE",
                DEFAULT_PREDICTION_CALIBRATION_MIN_WIN_RATE,
                0.0,
            ),
            calibration_min_ev_usdt: env_f64(
                "PREDICTION_CALIBRATION_MIN_EV_USDT",
                DEFAULT_PREDICTION_CALIBRATION_MIN_EV_USDT,
                -5.0,
            ),
            calibration_sample_seconds: env_i64(
                "PREDICTION_CALIBRATION_SAMPLE_SECONDS",
                DEFAULT_PREDICTION_CALIBRATION_SAMPLE_SECONDS,
                10,
            ),
        }
    }

    fn with_settings(&self, settings: &PredictionSettings) -> Self {
        let mut next = self.clone();
        next.enabled = settings.enabled;
        next.interval_seconds = settings.interval_seconds.max(1) as u64;
        next.up_threshold = settings
            .up_threshold
            .to_string()
            .parse::<f64>()
            .unwrap_or(self.up_threshold);
        next.down_threshold = settings
            .down_threshold
            .to_string()
            .parse::<f64>()
            .unwrap_or(self.down_threshold);
        next.min_confidence = settings
            .min_confidence
            .to_string()
            .parse::<f64>()
            .unwrap_or(self.min_confidence)
            .max(DEFAULT_PREDICTION_MIN_CONFIDENCE);
        next.technical_weight = settings
            .technical_weight
            .to_string()
            .parse::<f64>()
            .unwrap_or(self.technical_weight);
        next.probability_weight = settings
            .probability_weight
            .to_string()
            .parse::<f64>()
            .unwrap_or(self.probability_weight);
        next.volatility_floor = settings
            .volatility_floor
            .to_string()
            .parse::<f64>()
            .unwrap_or(self.volatility_floor);
        next.calibration_lookback_minutes =
            i64::from(settings.calibration_lookback_minutes).max(10);
        next.calibration_min_samples = settings
            .calibration_min_samples
            .max(DEFAULT_PREDICTION_CALIBRATION_MIN_SAMPLES);
        next.calibration_min_win_rate = settings
            .calibration_min_win_rate
            .to_string()
            .parse::<f64>()
            .unwrap_or(self.calibration_min_win_rate)
            .max(DEFAULT_PREDICTION_CALIBRATION_MIN_WIN_RATE);
        next.calibration_min_ev_usdt = settings
            .calibration_min_ev_usdt
            .to_string()
            .parse::<f64>()
            .unwrap_or(self.calibration_min_ev_usdt);
        next.signal_cooldown_seconds = self.signal_cooldown_seconds.max(0);
        next
    }
}

#[derive(Clone, Serialize)]
struct ExchangeWorkerSlot {
    id: Uuid,
    name: String,
    shard_total: i32,
    shard_index: i32,
    bind_local_ip: Option<String>,
    enabled: bool,
    status: String,
    last_started_at: Option<DateTime<Utc>>,
    last_stopped_at: Option<DateTime<Utc>>,
    last_error: Option<String>,
    updated_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct SaveExchangeWorkerSlotRequest {
    id: Option<Uuid>,
    name: Option<String>,
    shard_total: i32,
    shard_index: i32,
    bind_local_ip: Option<String>,
    enabled: bool,
}

#[derive(Deserialize)]
struct ToggleExchangeWorkerSlotRequest {
    id: Uuid,
    enabled: bool,
}

#[derive(Deserialize)]
struct DeleteExchangeWorkerSlotRequest {
    id: Uuid,
}

#[derive(Deserialize, Clone)]
struct ListQuery {
    page: Option<i64>,
    limit: Option<i64>,
    search: Option<String>,
    status: Option<String>,
}

#[derive(Clone, Serialize)]
struct BinanceIncomeEventRecord {
    id: Uuid,
    user_id: Uuid,
    provider: String,
    external_ref: String,
    income_type: String,
    income: Decimal,
    asset: String,
    event_time: DateTime<Utc>,
    tran_id: Option<String>,
    trade_id: Option<String>,
    raw: serde_json::Value,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize)]
struct SignalDeliveryRecord {
    id: Uuid,
    signal_id: Uuid,
    user_id: Uuid,
    reservation_id: Option<Uuid>,
    ws_connection_id: Uuid,
    delivery_status: String,
    authorization_status: String,
    trade_authorized: bool,
    stake_usdt: Decimal,
    fee_reserve_usdt: Decimal,
    sent_at: DateTime<Utc>,
    received_at: Option<DateTime<Utc>>,
    client_received_at: Option<DateTime<Utc>>,
    page_state: Option<serde_json::Value>,
    app_version: Option<String>,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone)]
struct VerifiedRefund {
    tx_hash: String,
    token_contract: String,
    contract_address: String,
    payout_wallet: String,
    to_address: String,
    amount: Decimal,
    block_number: i64,
    confirmations: i32,
    status: String,
    processed_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    message: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcTransactionReceipt {
    block_number: Option<String>,
    status: Option<String>,
    logs: Vec<RpcLog>,
}

#[derive(Deserialize)]
struct RpcLog {
    address: String,
    topics: Vec<String>,
    data: String,
}

#[derive(Deserialize)]
struct BinanceTimeResponse {
    #[serde(rename = "serverTime")]
    server_time: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceAccountResponse {
    total_wallet_balance: String,
    #[serde(default)]
    assets: Vec<BinanceAccountAsset>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinanceAccountAsset {
    asset: String,
    wallet_balance: String,
    available_balance: String,
}

#[derive(Clone, Deserialize, Serialize)]
struct BinanceIncomeResponse {
    symbol: Option<String>,
    #[serde(rename = "incomeType")]
    income_type: String,
    income: String,
    asset: String,
    time: i64,
    info: Option<String>,
    #[serde(rename = "tranId")]
    tran_id: Option<serde_json::Value>,
    #[serde(rename = "tradeId")]
    trade_id: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct AuthRequest {
    wallet_address: String,
    password: String,
    device_fingerprint: String,
    client_type: Option<String>,
    referral_code: Option<String>,
}

#[derive(Serialize)]
struct AuthResponse {
    access_token: String,
    refresh_token: String,
    app_session_key: String,
    expires_in: i64,
}

#[derive(Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Deserialize)]
struct WalletBindMessageRequest {
    chain: Option<String>,
    address: String,
}

#[derive(Serialize)]
struct WalletBindMessageResponse {
    message: String,
    nonce: String,
    address: String,
    chain: String,
    expires_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct WalletVerifyRequest {
    chain: Option<String>,
    address: String,
    message: String,
    signature: String,
}

#[derive(Deserialize)]
struct ChangePasswordRequest {
    chain: Option<String>,
    address: String,
    message: String,
    signature: String,
    new_password: String,
}

#[derive(Deserialize)]
struct PageStateRequest {
    page_ready: bool,
    screen_name: String,
    confidence: String,
    package_name: String,
    amount_field_found: bool,
    up_button_found: bool,
    down_button_found: bool,
    observed_balance_usdt: Option<String>,
    observed_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct SignalDeliveryAckRequest {
    delivery_id: Uuid,
    signal_id: Uuid,
    reservation_id: Option<Uuid>,
    received_at: DateTime<Utc>,
    app_version: Option<String>,
    page_state: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ExchangeCredentialRequest {
    api_key: String,
    api_secret: Option<String>,
}

#[derive(Serialize)]
struct ExchangeCredentialResponse {
    provider: String,
    connected: bool,
    can_read_account: bool,
    verification_status: String,
    balance_available_usdt: String,
    balance_wallet_usdt: String,
    last_verified_at: Option<DateTime<Utc>>,
    message: String,
}

#[derive(Serialize)]
struct ExchangeSyncResponse {
    refreshed_balances: usize,
    checked_orders: usize,
    confirmed_orders: usize,
    settled_reports: usize,
    pending_settlements: usize,
}

#[derive(Deserialize)]
struct EventContractOrderRequest {
    client_order_id: String,
    source: String,
    reservation_id: Option<Uuid>,
    signal_id: String,
    symbol: String,
    period_seconds: i32,
    direction: String,
    stake_usdt: String,
    entry_price: Option<String>,
    automation_profile: Option<String>,
    automation_confidence: Option<String>,
    action_trace_id: Option<String>,
    raw_audit: Option<serde_json::Value>,
    reported_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct SubmitOrderResponse {
    order_id: Uuid,
    status: String,
}

#[derive(Deserialize)]
struct ReleaseReservationRequest {
    reservation_id: Uuid,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct ExecutionReportRequest {
    client_report_id: String,
    source: String,
    signal_id: String,
    symbol: String,
    period_seconds: i32,
    direction: String,
    stake_usdt: String,
    entry_price: Option<String>,
    expiry_price: Option<String>,
    payout_ratio: Option<String>,
    gross_profit: Option<String>,
    result: String,
    automation_profile: Option<String>,
    automation_confidence: Option<String>,
    action_trace_id: Option<String>,
    reported_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct ExecutionReportResponse {
    report_id: Uuid,
    status: String,
}

#[derive(Deserialize)]
struct AdminSettleExecutionReportRequest {
    report_id: Uuid,
}

#[derive(Deserialize)]
struct AdminLoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct AdminLoginResponse {
    access_token: String,
    expires_in: i64,
}

#[derive(Deserialize)]
struct DepositTxRequest {
    tx_hash: String,
    amount: Option<String>,
    deposit_id: Option<String>,
}

#[derive(Deserialize)]
struct CreateWithdrawalRequest {
    amount: String,
    to_address: Option<String>,
}

#[derive(Deserialize)]
struct AdminConfirmWithdrawalRequest {
    withdrawal_id: Uuid,
    tx_hash: String,
}

#[derive(Deserialize)]
struct AdminRejectWithdrawalRequest {
    withdrawal_id: Uuid,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct EmitSignalRequest {
    direction: String,
    suggested_stake: Option<String>,
    confidence: Option<String>,
}

#[derive(Deserialize)]
struct WsQuery {
    access_token: Option<String>,
}

#[derive(Serialize)]
struct SignalApiResponse {
    id: String,
    signal_id: String,
    symbol: String,
    period_seconds: i32,
    cycle_start: DateTime<Utc>,
    cycle_end: DateTime<Utc>,
    direction: String,
    confidence: String,
    predicted_return: String,
    entry_price: String,
    suggested_stake: String,
    stake_percent: String,
    suggested_stake_rule: String,
    trade_authorized: bool,
    reservation_id: Option<String>,
    fee_reserve_usdt: String,
    authorization_status: String,
    account_match_status: String,
    observed_balance_usdt: Option<String>,
    account_balance_delta_usdt: Option<String>,
    balance_available_after_reserve: String,
    model_version: String,
    created_at: DateTime<Utc>,
    outcome_result: Option<String>,
    outcome_expiry_price: Option<String>,
    outcome_settled_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct SignalDeliveryAckResponse {
    delivery_id: String,
    status: String,
}

#[derive(Serialize)]
struct StatsResponse {
    users_total: usize,
    orders_total: usize,
    reports_total: usize,
    settlements_total: usize,
    signals_total: usize,
    wins: usize,
    losses: usize,
    hit_rate: String,
    uptime_seconds: i64,
    latest_signal: Option<SignalApiResponse>,
    prediction: PredictionStatus,
}

#[derive(Serialize)]
struct AppRuntimeSummary {
    orders_total: usize,
    wins: usize,
    losses: usize,
    win_rate: String,
    pnl_usdt: String,
    pending_settlements: usize,
}

#[derive(Serialize)]
struct PredictionStatus {
    enabled: bool,
    mode: String,
    status: String,
    last_evaluated_at: Option<DateTime<Utc>>,
    last_signal_at: Option<DateTime<Utc>>,
    last_signal_age_seconds: Option<i64>,
    last_direction: String,
    last_score: Option<String>,
    last_confidence: Option<String>,
    market_source: Option<String>,
    market_age_seconds: Option<i64>,
    market_candles: usize,
    message: String,
    last_error: Option<String>,
}

async fn health() -> &'static str {
    "OK"
}

async fn register(
    State(state): State<SharedState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    validate_auth_request(&req)?;
    let client_type = client_type_from_auth(&req)?;
    let account = account_from_auth(&req)?;
    enforce_register_rate_limit(&state, &account).await?;

    if let Some(pool) = state.db.as_ref() {
        if db_user_id_by_wallet(pool, &account).await?.is_some() {
            return Err(AppError::conflict(
                "AUTH_ACCOUNT_EXISTS",
                "wallet address already registered",
            ));
        }
    }
    if state.user_by_wallet.read().await.contains_key(&account) {
        return Err(AppError::conflict(
            "AUTH_ACCOUNT_EXISTS",
            "wallet address already registered",
        ));
    }

    let now = Utc::now();
    let user_id = Uuid::now_v7();
    let referral_code = referral_code_for_user(user_id);
    let invited_by_code = req
        .referral_code
        .as_deref()
        .map(normalize_referral_code)
        .transpose()?
        .filter(|code| !code.is_empty());
    let user = UserRecord {
        id: user_id,
        wallet_address: account.clone(),
        password_hash: password_hash(&req.password)?,
        status: "active".to_string(),
        active_device_fingerprint: active_device_for_client(client_type, &req.device_fingerprint),
        session_version: 0,
        created_at: now,
        updated_at: now,
    };

    if let Some(pool) = state.db.as_ref() {
        let (parent_user_id, grandparent_user_id) =
            db_referral_parents_for_code(pool, invited_by_code.as_deref()).await?;
        if parent_user_id == Some(user_id) {
            return Err(AppError::bad_request(
                "REFERRAL_SELF_INVALID",
                "referral code cannot belong to the registering user",
            ));
        }
        let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            insert into users (id, wallet_address, password_hash, status, active_device_fingerprint, session_version, created_at, updated_at)
            values ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(user.id)
        .bind(&user.wallet_address)
        .bind(&user.password_hash)
        .bind(&user.status)
        .bind(&user.active_device_fingerprint)
        .bind(user.session_version)
        .bind(user.created_at)
        .bind(user.updated_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            insert into user_automation_settings
              (user_id, automation_enabled, execution_mode, target_app_profile, require_final_confirmation, emergency_stop_enabled)
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(user_id)
        .bind(true)
        .bind("automatic")
        .bind("binance_event_contract_v1")
        .bind(false)
        .bind(true)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            insert into user_trading_settings
              (user_id, stake_percent, min_stake_usdt, max_single_stake_usdt, min_confidence, signal_alert_enabled, floating_window_enabled)
            values ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(user_id)
        .bind(DEFAULT_STAKE_PERCENT)
        .bind(MIN_EVENT_STAKE)
        .bind(Option::<Decimal>::None)
        .bind(Option::<Decimal>::None)
        .bind(true)
        .bind(true)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            insert into balances (user_id, asset, available, frozen, updated_at)
            values ($1, 'USDT', 0, 0, $2)
            "#,
        )
        .bind(user_id)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            insert into referral_links
              (user_id, parent_user_id, grandparent_user_id, referral_code, invited_by_code, created_at)
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(user_id)
        .bind(parent_user_id)
        .bind(grandparent_user_id)
        .bind(&referral_code)
        .bind(&invited_by_code)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        tx.commit().await.map_err(AppError::from_sqlx)?;
    }

    state.users.write().await.insert(user_id, user);
    state.user_by_wallet.write().await.insert(account, user_id);
    let (parent_user_id, grandparent_user_id) =
        memory_referral_parents_for_code(&state, invited_by_code.as_deref()).await;
    state
        .referral_codes
        .write()
        .await
        .insert(referral_code.clone(), user_id);
    state.referral_links.write().await.insert(
        user_id,
        ReferralLinkRecord {
            user_id,
            parent_user_id,
            grandparent_user_id,
            referral_code,
            invited_by_code,
            created_at: now,
        },
    );
    state
        .automation_settings
        .write()
        .await
        .insert(user_id, AutomationSettings::default());
    state
        .trading_settings
        .write()
        .await
        .insert(user_id, TradingSettings::default());
    state.balances.write().await.insert(
        user_id,
        BalanceRecord {
            user_id,
            asset: "USDT".to_string(),
            available: Decimal::ZERO,
            frozen: Decimal::ZERO,
            updated_at: now,
        },
    );

    Ok(Json(
        create_session(&state, user_id, &req.device_fingerprint, client_type).await?,
    ))
}

async fn login(
    State(state): State<SharedState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    validate_auth_request(&req)?;
    let client_type = client_type_from_auth(&req)?;
    let account = account_from_auth(&req)?;
    let user_id = if let Some(pool) = state.db.as_ref() {
        let user = db_user_by_wallet(pool, &account).await?.ok_or_else(|| {
            AppError::unauthorized("AUTH_INVALID_CREDENTIALS", "invalid credentials")
        })?;
        if !verify_password(&user.password_hash, &req.password) {
            return Err(AppError::unauthorized(
                "AUTH_INVALID_CREDENTIALS",
                "invalid credentials",
            ));
        }
        if client_type == ClientType::Android {
            replace_android_device_session(&state, user.id, &req.device_fingerprint).await?;
        }
        user.id
    } else {
        let user_id = state
            .user_by_wallet
            .read()
            .await
            .get(&account)
            .copied()
            .ok_or_else(|| {
                AppError::unauthorized("AUTH_INVALID_CREDENTIALS", "invalid credentials")
            })?;

        let mut users = state.users.write().await;
        let user = users.get_mut(&user_id).ok_or_else(|| {
            AppError::unauthorized("AUTH_INVALID_CREDENTIALS", "invalid credentials")
        })?;
        if !verify_password(&user.password_hash, &req.password) {
            return Err(AppError::unauthorized(
                "AUTH_INVALID_CREDENTIALS",
                "invalid credentials",
            ));
        }
        if client_type == ClientType::Android {
            user.session_version += 1;
            user.active_device_fingerprint = Some(req.device_fingerprint.clone());
            user.updated_at = Utc::now();
            state
                .sessions
                .write()
                .await
                .retain(|_, session| session.user_id != user_id);
        }
        user_id
    };

    Ok(Json(
        create_session(&state, user_id, &req.device_fingerprint, client_type).await?,
    ))
}

async fn logout(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let token = bearer_token(&headers)
        .ok_or_else(|| AppError::unauthorized("AUTH_MISSING_TOKEN", "missing bearer token"))?;
    let session = state.sessions.write().await.remove(&token);
    if let Some(session) = session {
        if let Some(pool) = state.db.as_ref() {
            let now = Utc::now();
            sqlx::query("delete from auth_sessions where access_token = $1")
                .bind(&token)
                .execute(pool)
                .await
                .map_err(AppError::from_sqlx)?;
            if session.client_type == ClientType::Android {
                sqlx::query(
                    r#"
                    update users
                    set active_device_fingerprint = null, updated_at = $3
                    where id = $1 and active_device_fingerprint = $2
                    "#,
                )
                .bind(session.user_id)
                .bind(&session.device_fingerprint)
                .bind(now)
                .execute(pool)
                .await
                .map_err(AppError::from_sqlx)?;
            }
        }
        if session.client_type == ClientType::Android {
            if let Some(user) = state.users.write().await.get_mut(&session.user_id) {
                if user.active_device_fingerprint.as_deref() != Some(&session.device_fingerprint) {
                    return Ok(Json(json!({ "ok": true })));
                }
                user.active_device_fingerprint = None;
                user.updated_at = Utc::now();
            }
        }
    }
    Ok(Json(json!({ "ok": true })))
}

async fn wallet_bind_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<WalletBindMessageRequest>,
) -> Result<Json<WalletBindMessageResponse>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    let chain = normalize_chain(req.chain.as_deref());
    let address = normalize_wallet_address(&req.address)?;
    let account_wallet = load_user_wallet_address(&state, user_id).await?;
    if address != account_wallet {
        return Err(AppError::forbidden(
            "WALLET_ACCOUNT_MISMATCH",
            "wallet address must match the logged-in account",
        ));
    }
    let nonce = new_token("wallet-nonce");
    let expires_at = Utc::now() + Duration::minutes(10);
    let message = format!(
        "Bind wallet to BTCUSDT Event Signal Platform\nUser ID: {user_id}\nChain: {chain}\nAddress: {address}\nNonce: {nonce}"
    );
    state.wallet_nonces.write().await.insert(
        user_id,
        WalletBindChallenge {
            purpose: WalletChallengePurpose::BindWallet,
            chain: chain.clone(),
            address: address.clone(),
            message: message.clone(),
            expires_at,
        },
    );
    Ok(Json(WalletBindMessageResponse {
        message,
        nonce,
        address,
        chain,
        expires_at,
    }))
}

async fn wallet_verify(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<WalletVerifyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    let chain = normalize_chain(req.chain.as_deref());
    let address = normalize_wallet_address(&req.address)?;
    let account_wallet = load_user_wallet_address(&state, user_id).await?;
    if address != account_wallet {
        return Err(AppError::forbidden(
            "WALLET_ACCOUNT_MISMATCH",
            "wallet address must match the logged-in account",
        ));
    }
    let challenge = state
        .wallet_nonces
        .write()
        .await
        .remove(&user_id)
        .ok_or_else(|| {
            AppError::bad_request("WALLET_NONCE_MISSING", "request a bind message first")
        })?;
    if challenge.purpose != WalletChallengePurpose::BindWallet {
        return Err(AppError::bad_request(
            "WALLET_CHALLENGE_PURPOSE_INVALID",
            format!("expected {}", WalletChallengePurpose::BindWallet.label()),
        ));
    }
    if challenge.expires_at < Utc::now() {
        return Err(AppError::bad_request(
            "WALLET_NONCE_EXPIRED",
            "wallet bind message expired",
        ));
    }
    if challenge.chain != chain || challenge.address != address || challenge.message != req.message
    {
        return Err(AppError::bad_request(
            "WALLET_MESSAGE_MISMATCH",
            "wallet bind message does not match current challenge",
        ));
    }
    let recovered = recover_wallet_address(&req.message, &req.signature)?;
    if recovered != address {
        return Err(AppError::unauthorized(
            "WALLET_SIGNATURE_INVALID",
            "signature does not match wallet address",
        ));
    }

    if let Some(pool) = state.db.as_ref() {
        upsert_wallet_binding(pool, user_id, &chain, &address, true).await?;
    }
    Ok(Json(json!({
        "ok": true,
        "chain": chain,
        "address": address,
        "verified": true
    })))
}

async fn change_password_message(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<WalletBindMessageRequest>,
) -> Result<Json<WalletBindMessageResponse>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    let chain = normalize_chain(req.chain.as_deref());
    let address = normalize_wallet_address(&req.address)?;
    let account_wallet = load_user_wallet_address(&state, user_id).await?;
    if address != account_wallet {
        return Err(AppError::forbidden(
            "WALLET_ACCOUNT_MISMATCH",
            "wallet address must match the logged-in account",
        ));
    }
    let nonce = new_token("password-nonce");
    let expires_at = Utc::now() + Duration::minutes(10);
    let message = format!(
        "Change password for BTCUSDT Event Signal Platform\nUser ID: {user_id}\nChain: {chain}\nAddress: {address}\nNonce: {nonce}"
    );
    state.wallet_nonces.write().await.insert(
        user_id,
        WalletBindChallenge {
            purpose: WalletChallengePurpose::ChangePassword,
            chain: chain.clone(),
            address: address.clone(),
            message: message.clone(),
            expires_at,
        },
    );
    Ok(Json(WalletBindMessageResponse {
        message,
        nonce,
        address,
        chain,
        expires_at,
    }))
}

async fn change_password(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    validate_password(&req.new_password)?;
    let chain = normalize_chain(req.chain.as_deref());
    let address = normalize_wallet_address(&req.address)?;
    let account_wallet = load_user_wallet_address(&state, user_id).await?;
    if address != account_wallet {
        return Err(AppError::forbidden(
            "WALLET_ACCOUNT_MISMATCH",
            "wallet address must match the logged-in account",
        ));
    }
    let challenge = state
        .wallet_nonces
        .write()
        .await
        .remove(&user_id)
        .ok_or_else(|| {
            AppError::bad_request("WALLET_NONCE_MISSING", "request a password message first")
        })?;
    if challenge.purpose != WalletChallengePurpose::ChangePassword {
        return Err(AppError::bad_request(
            "WALLET_CHALLENGE_PURPOSE_INVALID",
            format!(
                "expected {}",
                WalletChallengePurpose::ChangePassword.label()
            ),
        ));
    }
    if challenge.expires_at < Utc::now() {
        return Err(AppError::bad_request(
            "WALLET_NONCE_EXPIRED",
            "password change message expired",
        ));
    }
    if challenge.chain != chain || challenge.address != address || challenge.message != req.message
    {
        return Err(AppError::bad_request(
            "WALLET_MESSAGE_MISMATCH",
            "password change message does not match current challenge",
        ));
    }
    let recovered = recover_wallet_address(&req.message, &req.signature)?;
    if recovered != address {
        return Err(AppError::unauthorized(
            "WALLET_SIGNATURE_INVALID",
            "signature does not match wallet address",
        ));
    }

    let password_hash = password_hash(&req.new_password)?;
    if let Some(pool) = state.db.as_ref() {
        db_change_user_password(pool, user_id, &password_hash).await?;
    }
    if let Some(user) = state.users.write().await.get_mut(&user_id) {
        user.password_hash = password_hash;
        user.session_version += 1;
        user.active_device_fingerprint = None;
        user.updated_at = Utc::now();
    }
    state
        .sessions
        .write()
        .await
        .retain(|_, session| session.user_id != user_id);

    Ok(Json(json!({
        "ok": true,
        "message": "password changed; please log in again"
    })))
}

async fn refresh_token(
    State(state): State<SharedState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    let session = state
        .sessions
        .read()
        .await
        .values()
        .find(|session| session.refresh_token == req.refresh_token)
        .cloned();
    let session = match session {
        Some(session) => session,
        None => {
            if let Some(pool) = state.db.as_ref() {
                db_session_by_refresh_token(pool, &req.refresh_token)
                    .await?
                    .ok_or_else(|| {
                        AppError::unauthorized(
                            "AUTH_INVALID_REFRESH_TOKEN",
                            "invalid refresh token",
                        )
                    })?
            } else {
                return Err(AppError::unauthorized(
                    "AUTH_INVALID_REFRESH_TOKEN",
                    "invalid refresh token",
                ));
            }
        }
    };
    if !session_device_is_current(&state, &session).await {
        return Err(AppError::unauthorized(
            "AUTH_SESSION_REVOKED",
            "session is no longer active",
        ));
    }
    let response = create_session(
        &state,
        session.user_id,
        &session.device_fingerprint,
        session.client_type,
    )
    .await?;
    state
        .sessions
        .write()
        .await
        .retain(|_, session| session.refresh_token != req.refresh_token);
    if let Some(pool) = state.db.as_ref() {
        sqlx::query("delete from auth_sessions where refresh_token = $1")
            .bind(&req.refresh_token)
            .execute(pool)
            .await
            .map_err(AppError::from_sqlx)?;
    }
    Ok(Json(response))
}

async fn stats(State(state): State<SharedState>) -> Json<StatsResponse> {
    if let Some(pool) = state.db.as_ref() {
        let prediction = state.prediction_runtime.read().await.clone();
        if let Ok(stats) = db_build_stats(pool, state.started_at, prediction).await {
            return Json(stats);
        }
    }
    Json(build_stats(&state).await)
}

async fn app_runtime_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<AppRuntimeSummary>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_app_runtime_summary(pool, user_id).await?));
    }
    let orders = state.orders.read().await;
    let reports = state.reports.read().await;
    let settlements = state.settlements.read().await;
    let orders_total = orders
        .iter()
        .filter(|order| order.user_id == user_id)
        .count();
    let user_reports = reports
        .iter()
        .filter(|report| report.user_id == user_id)
        .collect::<Vec<_>>();
    let wins = user_reports
        .iter()
        .filter(|report| report.result.eq_ignore_ascii_case("WIN"))
        .count();
    let losses = user_reports
        .iter()
        .filter(|report| report.result.eq_ignore_ascii_case("LOSS"))
        .count();
    let settled = wins + losses;
    let pnl = settlements
        .iter()
        .filter(|settlement| settlement.user_id == user_id)
        .fold(Decimal::ZERO, |acc, settlement| {
            acc + settlement.gross_profit
        });
    Ok(Json(AppRuntimeSummary {
        orders_total,
        wins,
        losses,
        win_rate: hit_rate_string(wins, losses),
        pnl_usdt: money(pnl),
        pending_settlements: orders_total.saturating_sub(settled),
    }))
}

async fn latest_signal(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<SignalApiResponse>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    let latest = match state.db.as_ref() {
        Some(pool) => db_latest_signal(pool).await?,
        None => None,
    };
    let latest = match latest {
        Some(signal) => signal,
        None => state
            .signals
            .read()
            .await
            .last()
            .cloned()
            .unwrap_or_else(|| SignalCore::new("HOLD".to_string(), Decimal::ZERO, Decimal::ZERO)),
    };
    Ok(Json(
        display_signal_for_user(&state, user_id, &latest).await,
    ))
}

async fn signal_history(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SignalApiResponse>>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    let signals = match state.db.as_ref() {
        Some(pool) => db_signal_history(pool, 50).await?,
        None => state
            .signals
            .read()
            .await
            .iter()
            .rev()
            .take(50)
            .cloned()
            .collect(),
    };
    let mut out = Vec::with_capacity(signals.len());
    for signal in signals.iter().take(50) {
        out.push(display_signal_for_user(&state, user_id, signal).await);
    }
    Ok(Json(out))
}

async fn get_trading_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<TradingSettings>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    Ok(Json(load_trading_settings(&state, user_id).await))
}

async fn save_trading_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(_req): Json<TradingSettings>,
) -> Result<Json<TradingSettings>, AppError> {
    require_android_user(&state, &headers).await?;
    Err(AppError::forbidden(
        "TRADING_SETTINGS_ADMIN_ONLY",
        "trading stake settings are controlled by the backend administrator",
    ))
}

async fn admin_trading_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<PlatformTradingSettings>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_load_platform_trading_settings(pool).await?));
    }
    Ok(Json(PlatformTradingSettings::default()))
}

async fn admin_save_trading_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<PlatformTradingSettings>,
) -> Result<Json<PlatformTradingSettings>, AppError> {
    ensure_admin(&state, &headers).await?;
    validate_platform_trading_settings(&req)?;
    if let Some(pool) = state.db.as_ref() {
        db_save_platform_trading_settings(pool, &req).await?;
        return Ok(Json(db_load_platform_trading_settings(pool).await?));
    }
    Ok(Json(req))
}

async fn admin_exchange_workers(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ExchangeWorkerSlot>>, AppError> {
    ensure_admin(&state, &headers).await?;
    let Some(pool) = state.db.as_ref() else {
        return Ok(Json(Vec::new()));
    };
    Ok(Json(db_exchange_worker_slots(pool).await?))
}

async fn admin_save_exchange_worker(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<SaveExchangeWorkerSlotRequest>,
) -> Result<Json<Vec<ExchangeWorkerSlot>>, AppError> {
    ensure_admin(&state, &headers).await?;
    validate_exchange_worker_request(&req)?;
    let Some(pool) = state.db.as_ref() else {
        return Err(AppError::bad_request(
            "DATABASE_REQUIRED",
            "exchange worker configuration requires PostgreSQL",
        ));
    };
    let slot = db_save_exchange_worker_slot(pool, &req).await?;
    sync_exchange_worker_runtime(&state, &slot).await?;
    Ok(Json(db_exchange_worker_slots(pool).await?))
}

async fn admin_toggle_exchange_worker(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ToggleExchangeWorkerSlotRequest>,
) -> Result<Json<Vec<ExchangeWorkerSlot>>, AppError> {
    ensure_admin(&state, &headers).await?;
    let Some(pool) = state.db.as_ref() else {
        return Err(AppError::bad_request(
            "DATABASE_REQUIRED",
            "exchange worker configuration requires PostgreSQL",
        ));
    };
    let slot = db_set_exchange_worker_enabled(pool, req.id, req.enabled).await?;
    sync_exchange_worker_runtime(&state, &slot).await?;
    Ok(Json(db_exchange_worker_slots(pool).await?))
}

async fn admin_delete_exchange_worker(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<DeleteExchangeWorkerSlotRequest>,
) -> Result<Json<Vec<ExchangeWorkerSlot>>, AppError> {
    ensure_admin(&state, &headers).await?;
    let Some(pool) = state.db.as_ref() else {
        return Err(AppError::bad_request(
            "DATABASE_REQUIRED",
            "exchange worker configuration requires PostgreSQL",
        ));
    };
    stop_exchange_worker_runtime(&state, req.id).await;
    db_delete_exchange_worker_slot(pool, req.id).await?;
    Ok(Json(db_exchange_worker_slots(pool).await?))
}

async fn get_automation_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<AutomationSettings>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    Ok(Json(load_automation_settings(&state, user_id).await))
}

async fn save_automation_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<AutomationSettings>,
) -> Result<Json<AutomationSettings>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    validate_automation_settings(&req)?;
    if let Some(pool) = state.db.as_ref() {
        db_save_automation_settings(pool, user_id, &req).await?;
    }
    state
        .automation_settings
        .write()
        .await
        .insert(user_id, req.clone());
    Ok(Json(req))
}

async fn save_page_state(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<PageStateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    let confidence = parse_decimal(&req.confidence, "confidence")?;
    let observed_balance = req
        .observed_balance_usdt
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(|value| parse_decimal(value, "observed_balance_usdt"))
        .transpose()?;
    let exchange_snapshot = load_exchange_balance_snapshot(&state, user_id).await?;
    let account_match = match exchange_snapshot {
        Some(snapshot) if balance_cache_fresh(snapshot.updated_at) => {
            account_match_from_balances(observed_balance, Some(snapshot.available))
        }
        Some(_) => AccountMatch {
            status: "exchange_balance_stale".to_string(),
            delta: None,
        },
        None => account_match_from_balances(observed_balance, None),
    };
    let page_state = AutomationPageState {
        page_ready: req.page_ready,
        screen_name: req.screen_name,
        confidence,
        package_name: req.package_name,
        amount_field_found: req.amount_field_found,
        up_button_found: req.up_button_found,
        down_button_found: req.down_button_found,
        observed_balance_usdt: observed_balance,
        account_match_status: account_match.status,
        account_balance_delta_usdt: account_match.delta,
        observed_at: req.observed_at,
        updated_at: Utc::now(),
    };
    if let Some(pool) = state.db.as_ref() {
        db_save_page_state(pool, user_id, &page_state).await?;
    }
    state.page_states.write().await.insert(user_id, page_state);
    Ok(Json(json!({ "ok": true })))
}

async fn ack_signal_delivery(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<SignalDeliveryAckRequest>,
) -> Result<Json<SignalDeliveryAckResponse>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    if let Some(reservation_id) = req.reservation_id {
        if reservation_id.is_nil() {
            return Err(AppError::bad_request(
                "RESERVATION_ID_INVALID",
                "reservation_id cannot be nil",
            ));
        }
    }
    if let Some(pool) = state.db.as_ref() {
        db_ack_signal_delivery(pool, user_id, &req).await?;
    }
    Ok(Json(SignalDeliveryAckResponse {
        delivery_id: req.delivery_id.to_string(),
        status: "received".to_string(),
    }))
}

async fn exchange_status(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<ExchangeCredentialStatus>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        let status = db_exchange_credential_status(pool, user_id).await?;
        return Ok(Json(status.unwrap_or_else(default_exchange_status)));
    }
    Ok(Json(default_exchange_status()))
}

async fn save_binance_credentials(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ExchangeCredentialRequest>,
) -> Result<Json<ExchangeCredentialResponse>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    let api_key = normalize_api_key(&req.api_key)?;
    let api_secret = match req
        .api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => normalize_api_secret(value)?,
        None => {
            let pool = state.db.as_ref().ok_or_else(|| {
                AppError::bad_request(
                    "DATABASE_REQUIRED",
                    "exchange credential storage requires PostgreSQL",
                )
            })?;
            let credentials = db_exchange_credentials(pool, user_id)
                .await?
                .ok_or_else(|| {
                    AppError::bad_request(
                        "BINANCE_API_SECRET_REQUIRED",
                        "api_secret is required before credentials are saved",
                    )
                })?;
            if credentials.api_key != api_key {
                return Err(AppError::bad_request(
                    "BINANCE_API_KEY_MISMATCH",
                    "api_key does not match saved credentials",
                ));
            }
            decrypt_secret(&credentials.api_secret_ciphertext)?
        }
    };
    let snapshot = binance_account_access(&api_key, &api_secret).await?;
    let now = Utc::now();
    let ciphertext = encrypt_secret(&api_secret)?;
    if let Some(pool) = state.db.as_ref() {
        db_upsert_exchange_credentials(pool, user_id, &api_key, &ciphertext, &snapshot, now)
            .await?;
    } else {
        return Err(AppError::bad_request(
            "DATABASE_REQUIRED",
            "exchange credential storage requires PostgreSQL",
        ));
    }
    Ok(Json(ExchangeCredentialResponse {
        provider: BINANCE_PROVIDER.to_string(),
        connected: true,
        can_read_account: true,
        verification_status: "verified".to_string(),
        balance_available_usdt: money(snapshot.available_balance_usdt),
        balance_wallet_usdt: money(snapshot.wallet_balance_usdt),
        last_verified_at: Some(now),
        message: "binance futures read access verified".to_string(),
    }))
}

async fn balance(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    let balance = load_balance(&state, user_id).await;
    Ok(Json(json!({
        "asset": balance.asset,
        "available": money(balance.available),
        "frozen": money(balance.frozen),
        "activation_status": if balance.available + balance.frozen > Decimal::ZERO { "active" } else { "inactive" },
    })))
}

async fn ledger(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<LedgerEntry>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_ledger_for_user(pool, user_id, &query).await?));
    }
    let rows = filter_page_ledger(
        state
            .ledger
            .read()
            .await
            .iter()
            .filter(|entry| entry.user_id == user_id)
            .cloned()
            .collect(),
        &query,
    );
    Ok(Json(rows))
}

async fn deposit_instructions(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    let bound_wallet = if let Some(pool) = state.db.as_ref() {
        db_bound_wallet(pool, user_id, "bsc").await?
    } else {
        state
            .users
            .read()
            .await
            .get(&user_id)
            .map(|user| user.wallet_address.clone())
    };
    let platform_settings = load_platform_settings(&state).await;
    Ok(Json(json!({
        "chain": "bsc",
        "asset": "USDT",
        "token_contract": bsc_usdt_contract(),
        "vault_contract": bsc_vault_contract()?,
        "settlement_wallet": bsc_settlement_wallet()?,
        "account_id": account_id_topic(user_id),
        "deposit_id": new_bytes32_id(),
        "bound_wallet": bound_wallet,
        "min_amount": money(MIN_DEPOSIT_AMOUNT),
        "first_deposit_min_amount": money(platform_settings.first_deposit_min_usdt),
        "deposit_fee_rate": decimal(platform_settings.deposit_fee_rate),
        "credited_rate": decimal(Decimal::ONE - platform_settings.deposit_fee_rate)
    })))
}

async fn submit_deposit_tx(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<DepositTxRequest>,
) -> Result<Json<DepositRecord>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    let platform_settings = load_platform_settings(&state).await;
    let has_credited_deposit = user_has_credited_deposit(&state, user_id).await?;
    let expected_amount = req
        .amount
        .as_deref()
        .map(|value| parse_decimal(value, "amount"))
        .transpose()?;
    let tx_hash = normalize_tx_hash(&req.tx_hash)?;
    let expected_deposit_id = req
        .deposit_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_topic)
        .transpose()?;
    let bound_wallet = bound_wallet_for_user(&state, user_id).await?;
    let verified = verify_bsc_contract_deposit(
        &tx_hash,
        user_id,
        &bound_wallet,
        expected_deposit_id.as_deref(),
        expected_amount,
    )
    .await?;
    let now = Utc::now();
    if !has_credited_deposit && verified.amount < platform_settings.first_deposit_min_usdt {
        return Err(AppError::bad_request(
            "FIRST_DEPOSIT_TOO_SMALL",
            format!(
                "first deposit must be at least {} USDT to activate the account",
                money(platform_settings.first_deposit_min_usdt)
            ),
        ));
    }
    let platform_fee = verified.amount * platform_settings.deposit_fee_rate;
    let credited_amount = verified.amount - platform_fee;
    let deposit = DepositRecord {
        id: Uuid::now_v7(),
        user_id,
        chain: "bsc".to_string(),
        asset: "USDT".to_string(),
        token_contract: verified.token_contract,
        contract_address: verified.contract_address,
        deposit_id: verified.deposit_id,
        tx_hash: verified.tx_hash,
        from_address: verified.from_address,
        to_address: verified.to_address,
        amount: verified.amount,
        credited_amount,
        platform_fee,
        block_number: verified.block_number,
        confirmations: verified.confirmations,
        status: verified.status,
        credited_at: verified.credited_at,
        created_at: now,
    };
    if let Some(pool) = state.db.as_ref() {
        db_credit_deposit(pool, &deposit).await?;
    } else {
        credit_deposit_memory(&state, &deposit).await?;
    }
    Ok(Json(deposit))
}

async fn user_has_credited_deposit(state: &SharedState, user_id: Uuid) -> Result<bool, AppError> {
    if let Some(pool) = state.db.as_ref() {
        return sqlx::query(
            "select 1 from deposits where user_id = $1 and status = 'credited' limit 1",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map(|row| row.is_some())
        .map_err(AppError::from_sqlx);
    }
    Ok(state
        .deposits
        .read()
        .await
        .iter()
        .any(|deposit| deposit.user_id == user_id && deposit.status == "credited"))
}

async fn deposits(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<DepositRecord>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_deposits_for_user(pool, user_id, &query).await?));
    }
    let rows = filter_page_deposits(
        state
            .deposits
            .read()
            .await
            .iter()
            .filter(|deposit| deposit.user_id == user_id)
            .cloned()
            .collect(),
        &query,
    );
    Ok(Json(rows))
}

async fn referral_summary(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_referral_summary(pool, user_id).await?));
    }
    let link = state.referral_links.read().await.get(&user_id).cloned();
    let rewards = state
        .referral_rewards
        .read()
        .await
        .iter()
        .filter(|reward| reward.recipient_user_id == user_id)
        .cloned()
        .collect::<Vec<_>>();
    let direct_count = state
        .referral_links
        .read()
        .await
        .values()
        .filter(|row| row.parent_user_id == Some(user_id))
        .count();
    let indirect_count = state
        .referral_links
        .read()
        .await
        .values()
        .filter(|row| row.grandparent_user_id == Some(user_id))
        .count();
    let total_reward = rewards
        .iter()
        .fold(Decimal::ZERO, |total, reward| total + reward.amount);
    Ok(Json(json!({
        "referral_code": link.as_ref().map(|row| row.referral_code.clone()).unwrap_or_else(|| referral_code_for_user(user_id)),
        "invited_by_code": link.as_ref().and_then(|row| row.invited_by_code.clone()),
        "direct_count": direct_count,
        "indirect_count": indirect_count,
        "direct_rate": decimal(DIRECT_REFERRAL_RATE),
        "indirect_rate": decimal(INDIRECT_REFERRAL_RATE),
        "total_reward": money(total_reward),
        "rewards": rewards,
    })))
}

async fn event_contract_orders(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<EventContractOrderRecord>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_orders_for_user(pool, user_id, &query).await?));
    }
    let rows = filter_page_orders(
        state
            .orders
            .read()
            .await
            .iter()
            .filter(|order| order.user_id == user_id)
            .cloned()
            .collect(),
        &query,
    );
    Ok(Json(rows))
}

async fn execution_reports(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<ExecutionReportRecord>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_reports_for_user(pool, user_id, &query).await?));
    }
    let rows = filter_page_reports(
        state
            .reports
            .read()
            .await
            .iter()
            .filter(|report| report.user_id == user_id)
            .cloned()
            .collect(),
        &query,
    );
    Ok(Json(rows))
}

async fn create_withdrawal(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<CreateWithdrawalRequest>,
) -> Result<Json<WithdrawalRecord>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    let amount = parse_decimal(&req.amount, "amount")?;
    if amount <= Decimal::ZERO {
        return Err(AppError::bad_request(
            "WITHDRAWAL_AMOUNT_INVALID",
            "withdrawal amount must be positive",
        ));
    }
    let to_address = match req.to_address.as_deref() {
        Some(value) if !value.trim().is_empty() => normalize_wallet_address(value)?,
        _ => bound_wallet_for_user(&state, user_id).await?,
    };
    let now = Utc::now();
    let refund_id = bytes32_from_uuid(Uuid::now_v7());
    let withdrawal = WithdrawalRecord {
        id: Uuid::now_v7(),
        user_id,
        chain: "bsc".to_string(),
        asset: "USDT".to_string(),
        token_contract: bsc_usdt_contract(),
        contract_address: bsc_vault_contract()?,
        tx_hash: None,
        refund_id,
        payout_wallet: None,
        to_address,
        amount,
        block_number: None,
        confirmations: 0,
        status: "pending_review".to_string(),
        requested_at: now,
        processed_at: None,
        rejected_at: None,
        reject_reason: None,
    };
    if let Some(pool) = state.db.as_ref() {
        db_create_withdrawal(pool, &withdrawal).await?;
    } else {
        create_withdrawal_memory(&state, &withdrawal).await?;
    }
    Ok(Json(withdrawal))
}

async fn withdrawals(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<WithdrawalRecord>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_withdrawals_for_user(pool, user_id, &query).await?));
    }
    let rows = filter_page_withdrawals(
        state
            .withdrawals
            .read()
            .await
            .iter()
            .filter(|withdrawal| withdrawal.user_id == user_id)
            .cloned()
            .collect(),
        &query,
    );
    Ok(Json(rows))
}

async fn bound_wallet_for_user(state: &SharedState, user_id: Uuid) -> Result<String, AppError> {
    if let Some(pool) = state.db.as_ref() {
        return db_bound_wallet(pool, user_id, "bsc").await?.ok_or_else(|| {
            AppError::bad_request("WALLET_NOT_BOUND", "bind and verify a BSC wallet first")
        });
    }
    state
        .users
        .read()
        .await
        .get(&user_id)
        .map(|user| user.wallet_address.clone())
        .ok_or_else(|| AppError::unauthorized("AUTH_INVALID_USER", "user account not found"))
}

fn filter_page_deposits(mut rows: Vec<DepositRecord>, query: &ListQuery) -> Vec<DepositRecord> {
    let status = status_filter(query);
    let search = search_value(query);
    rows.retain(|row| {
        status_matches(&row.status, status.as_deref())
            && search_matches(
                search.as_deref(),
                [
                    row.tx_hash.as_str(),
                    row.deposit_id.as_str(),
                    row.from_address.as_str(),
                    row.status.as_str(),
                ],
            )
    });
    rows.sort_by_key(|row| Reverse(row.created_at));
    page_vec(rows, query)
}

fn filter_page_ledger(mut rows: Vec<LedgerEntry>, query: &ListQuery) -> Vec<LedgerEntry> {
    let search = search_value(query);
    rows.retain(|row| {
        search_matches(
            search.as_deref(),
            [
                row.entry_type.as_str(),
                row.ref_type.as_str(),
                row.idempotency_key.as_str(),
            ],
        )
    });
    rows.sort_by_key(|row| Reverse(row.created_at));
    page_vec(rows, query)
}

fn filter_page_orders(
    mut rows: Vec<EventContractOrderRecord>,
    query: &ListQuery,
) -> Vec<EventContractOrderRecord> {
    let status = status_filter(query);
    let search = search_value(query);
    rows.retain(|row| {
        status_matches(&row.order_status, status.as_deref())
            && search_matches(
                search.as_deref(),
                [
                    row.client_order_id.as_str(),
                    row.signal_id.as_str(),
                    row.symbol.as_str(),
                    row.direction.as_str(),
                ],
            )
    });
    rows.sort_by_key(|row| Reverse(row.created_at));
    page_vec(rows, query)
}

fn filter_page_reports(
    mut rows: Vec<ExecutionReportRecord>,
    query: &ListQuery,
) -> Vec<ExecutionReportRecord> {
    let status = status_filter(query);
    let search = search_value(query);
    rows.retain(|row| {
        status_matches(&row.report_status, status.as_deref())
            && search_matches(
                search.as_deref(),
                [
                    row.client_report_id.as_str(),
                    row.signal_id.as_str(),
                    row.symbol.as_str(),
                    row.result.as_str(),
                ],
            )
    });
    rows.sort_by_key(|row| Reverse(row.created_at));
    page_vec(rows, query)
}

fn filter_page_withdrawals(
    mut rows: Vec<WithdrawalRecord>,
    query: &ListQuery,
) -> Vec<WithdrawalRecord> {
    let status = status_filter(query);
    let search = search_value(query);
    rows.retain(|row| {
        status_matches(&row.status, status.as_deref())
            && search_matches(
                search.as_deref(),
                [
                    row.refund_id.as_str(),
                    row.to_address.as_str(),
                    row.tx_hash.as_deref().unwrap_or_default(),
                    row.status.as_str(),
                ],
            )
    });
    rows.sort_by_key(|row| Reverse(row.requested_at));
    page_vec(rows, query)
}

fn filter_page_settlements(
    mut rows: Vec<SettlementRecord>,
    query: &ListQuery,
) -> Vec<SettlementRecord> {
    let status = status_filter(query);
    rows.retain(|row| status_matches(&row.status, status.as_deref()));
    rows.sort_by_key(|row| Reverse(row.created_at));
    page_vec(rows, query)
}

fn page_vec<T>(rows: Vec<T>, query: &ListQuery) -> Vec<T> {
    let (_, limit, offset) = page_params(query);
    rows.into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect()
}

fn search_value(query: &ListQuery) -> Option<String> {
    query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

fn status_matches(actual: &str, expected: Option<&str>) -> bool {
    expected
        .map(|status| actual.eq_ignore_ascii_case(status))
        .unwrap_or(true)
}

fn search_matches<'a>(search: Option<&str>, values: impl IntoIterator<Item = &'a str>) -> bool {
    let Some(search) = search else {
        return true;
    };
    values
        .into_iter()
        .any(|value| value.to_ascii_lowercase().contains(search))
}

async fn credit_deposit_memory(
    state: &SharedState,
    deposit: &DepositRecord,
) -> Result<(), AppError> {
    if state
        .deposits
        .read()
        .await
        .iter()
        .any(|row| row.tx_hash.eq_ignore_ascii_case(&deposit.tx_hash))
    {
        return Err(AppError::conflict(
            "DEPOSIT_ALREADY_SUBMITTED",
            "deposit transaction was already submitted",
        ));
    }
    let mut balances = state.balances.write().await;
    let balance = balances
        .get_mut(&deposit.user_id)
        .ok_or_else(|| AppError::bad_request("BALANCE_NOT_FOUND", "balance not found"))?;
    if deposit.status == "credited" {
        balance.available += deposit.credited_amount;
        balance.updated_at = Utc::now();
    }
    let balance_after = balance.clone();
    drop(balances);

    state.deposits.write().await.push(deposit.clone());
    if deposit.status == "credited" {
        state.ledger.write().await.push(LedgerEntry {
            id: Uuid::now_v7(),
            user_id: deposit.user_id,
            asset: "USDT".to_string(),
            entry_type: "deposit_credit".to_string(),
            amount: deposit.credited_amount,
            balance_available_after: balance_after.available,
            balance_frozen_after: balance_after.frozen,
            ref_type: "deposit".to_string(),
            ref_id: deposit.id,
            idempotency_key: format!("deposit:{}", deposit.tx_hash),
            created_at: Utc::now(),
        });
    }
    Ok(())
}

async fn create_withdrawal_memory(
    state: &SharedState,
    withdrawal: &WithdrawalRecord,
) -> Result<(), AppError> {
    let mut balances = state.balances.write().await;
    let balance = balances
        .get_mut(&withdrawal.user_id)
        .ok_or_else(|| AppError::bad_request("BALANCE_NOT_FOUND", "balance not found"))?;
    if balance.available < withdrawal.amount {
        return Err(AppError::bad_request(
            "BALANCE_INSUFFICIENT",
            "available balance is below withdrawal amount",
        ));
    }
    balance.available -= withdrawal.amount;
    balance.frozen += withdrawal.amount;
    balance.updated_at = Utc::now();
    let balance_after = balance.clone();
    drop(balances);

    state.withdrawals.write().await.push(withdrawal.clone());
    state.ledger.write().await.push(LedgerEntry {
        id: Uuid::now_v7(),
        user_id: withdrawal.user_id,
        asset: "USDT".to_string(),
        entry_type: "withdrawal_reserve".to_string(),
        amount: -withdrawal.amount,
        balance_available_after: balance_after.available,
        balance_frozen_after: balance_after.frozen,
        ref_type: "withdrawal".to_string(),
        ref_id: withdrawal.id,
        idempotency_key: format!("withdrawal_reserve:{}", withdrawal.id),
        created_at: Utc::now(),
    });
    Ok(())
}

async fn confirm_withdrawal_memory(
    state: &SharedState,
    withdrawal_id: Uuid,
    verified: VerifiedRefund,
) -> Result<(), AppError> {
    let mut withdrawals = state.withdrawals.write().await;
    let withdrawal = withdrawals
        .iter_mut()
        .find(|row| row.id == withdrawal_id)
        .ok_or_else(|| AppError::bad_request("WITHDRAWAL_NOT_FOUND", "withdrawal not found"))?;
    if withdrawal.status != "pending_review" && withdrawal.status != "confirming" {
        return Err(AppError::conflict(
            "WITHDRAWAL_STATUS_INVALID",
            "only pending or confirming withdrawals can be confirmed",
        ));
    }
    withdrawal.tx_hash = Some(verified.tx_hash);
    withdrawal.token_contract = verified.token_contract;
    withdrawal.contract_address = verified.contract_address;
    withdrawal.payout_wallet = Some(verified.payout_wallet);
    withdrawal.to_address = verified.to_address;
    withdrawal.amount = verified.amount;
    withdrawal.block_number = Some(verified.block_number);
    withdrawal.confirmations = verified.confirmations;
    withdrawal.status = verified.status;
    withdrawal.processed_at = verified.processed_at;
    let snapshot = withdrawal.clone();
    drop(withdrawals);

    if snapshot.status == "paid" {
        let mut balances = state.balances.write().await;
        let balance = balances
            .get_mut(&snapshot.user_id)
            .ok_or_else(|| AppError::bad_request("BALANCE_NOT_FOUND", "balance not found"))?;
        let release = snapshot.amount.min(balance.frozen);
        balance.frozen -= release;
        balance.updated_at = Utc::now();
        let balance_after = balance.clone();
        drop(balances);
        state.ledger.write().await.push(LedgerEntry {
            id: Uuid::now_v7(),
            user_id: snapshot.user_id,
            asset: "USDT".to_string(),
            entry_type: "withdrawal_paid".to_string(),
            amount: -snapshot.amount,
            balance_available_after: balance_after.available,
            balance_frozen_after: balance_after.frozen,
            ref_type: "withdrawal".to_string(),
            ref_id: snapshot.id,
            idempotency_key: format!("withdrawal_paid:{}", snapshot.id),
            created_at: Utc::now(),
        });
    }
    Ok(())
}

async fn reject_withdrawal_memory(
    state: &SharedState,
    withdrawal_id: Uuid,
    reason: Option<String>,
) -> Result<(), AppError> {
    let mut withdrawals = state.withdrawals.write().await;
    let withdrawal = withdrawals
        .iter_mut()
        .find(|row| row.id == withdrawal_id)
        .ok_or_else(|| AppError::bad_request("WITHDRAWAL_NOT_FOUND", "withdrawal not found"))?;
    if withdrawal.status != "pending_review" && withdrawal.status != "confirming" {
        return Err(AppError::conflict(
            "WITHDRAWAL_STATUS_INVALID",
            "only pending or confirming withdrawals can be rejected",
        ));
    }
    withdrawal.status = "rejected".to_string();
    withdrawal.rejected_at = Some(Utc::now());
    withdrawal.reject_reason = reason;
    let snapshot = withdrawal.clone();
    drop(withdrawals);

    let mut balances = state.balances.write().await;
    let balance = balances
        .get_mut(&snapshot.user_id)
        .ok_or_else(|| AppError::bad_request("BALANCE_NOT_FOUND", "balance not found"))?;
    let release = snapshot.amount.min(balance.frozen);
    balance.frozen -= release;
    balance.available += release;
    balance.updated_at = Utc::now();
    let balance_after = balance.clone();
    drop(balances);

    state.ledger.write().await.push(LedgerEntry {
        id: Uuid::now_v7(),
        user_id: snapshot.user_id,
        asset: "USDT".to_string(),
        entry_type: "withdrawal_rejected".to_string(),
        amount: snapshot.amount,
        balance_available_after: balance_after.available,
        balance_frozen_after: balance_after.frozen,
        ref_type: "withdrawal".to_string(),
        ref_id: snapshot.id,
        idempotency_key: format!("withdrawal_rejected:{}", snapshot.id),
        created_at: Utc::now(),
    });
    Ok(())
}

struct TransferLog {
    from_address: String,
    to_address: String,
    amount: Decimal,
}

struct DepositReceivedLog {
    account_id: String,
    deposit_id: String,
    payer: String,
    receiver: String,
    amount: Decimal,
}

struct RefundPaidLog {
    refund_id: String,
    payer: String,
    recipient: String,
    amount: Decimal,
}

async fn verify_bsc_contract_deposit(
    tx_hash: &str,
    user_id: Uuid,
    bound_wallet: &str,
    expected_deposit_id: Option<&str>,
    expected_amount: Option<Decimal>,
) -> Result<VerifiedDeposit, AppError> {
    let rpc_url = bsc_rpc_url()?;
    let token_contract = normalize_wallet_address(&bsc_usdt_contract())?;
    let vault_contract = bsc_vault_contract()?;
    let settlement_wallet = bsc_settlement_wallet()?;
    let bound_wallet = normalize_wallet_address(bound_wallet)?;
    let expected_deposit_id = expected_deposit_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_topic)
        .transpose()?;
    let expected_account_id = account_id_topic(user_id);

    let receipt: Option<RpcTransactionReceipt> =
        rpc_call(&rpc_url, "eth_getTransactionReceipt", json!([tx_hash])).await?;
    let Some(receipt) = receipt else {
        return Ok(VerifiedDeposit {
            tx_hash: tx_hash.to_string(),
            token_contract,
            contract_address: vault_contract.clone(),
            deposit_id: expected_deposit_id.clone().unwrap_or_default(),
            from_address: bound_wallet,
            to_address: settlement_wallet,
            amount: expected_amount.unwrap_or(Decimal::ZERO),
            block_number: 0,
            confirmations: 0,
            status: "confirming".to_string(),
            credited_at: None,
        });
    };
    let block_number = receipt
        .block_number
        .as_deref()
        .map(hex_to_i64)
        .transpose()?
        .ok_or_else(|| AppError::bad_request("DEPOSIT_UNCONFIRMED", "transaction is not mined"))?;
    if receipt.status.as_deref() != Some("0x1") {
        return Err(AppError::bad_request(
            "DEPOSIT_TX_FAILED",
            "transaction receipt is not successful",
        ));
    }
    let current_block_hex: String = rpc_call(&rpc_url, "eth_blockNumber", json!([]))
        .await?
        .ok_or_else(|| {
            AppError::bad_request("BSC_BLOCK_UNAVAILABLE", "BSC block number unavailable")
        })?;
    let current_block = hex_to_i64(&current_block_hex)?;
    let confirmations = (current_block - block_number + 1).max(0) as i32;
    let min_confirmations = deposit_min_confirmations();

    let deposit = receipt
        .logs
        .iter()
        .find_map(|log| parse_deposit_received_log(log, &vault_contract).transpose())
        .transpose()?
        .ok_or_else(|| {
            AppError::bad_request("DEPOSIT_EVENT_NOT_FOUND", "DepositReceived event not found")
        })?;

    if deposit.account_id != expected_account_id {
        return Err(AppError::bad_request(
            "DEPOSIT_ACCOUNT_MISMATCH",
            "deposit event account_id does not match user",
        ));
    }
    if let Some(expected_deposit_id) = expected_deposit_id.as_ref() {
        if deposit.deposit_id.as_str() != expected_deposit_id.as_str() {
            return Err(AppError::bad_request(
                "DEPOSIT_ID_MISMATCH",
                "deposit event deposit_id does not match request",
            ));
        }
    }
    if deposit.payer != bound_wallet {
        return Err(AppError::bad_request(
            "DEPOSIT_FROM_MISMATCH",
            "deposit event payer does not match bound wallet",
        ));
    }
    if deposit.receiver != settlement_wallet {
        return Err(AppError::bad_request(
            "DEPOSIT_RECEIVER_MISMATCH",
            "deposit event receiver does not match settlement wallet",
        ));
    }
    if deposit.amount < MIN_DEPOSIT_AMOUNT {
        return Err(AppError::bad_request(
            "DEPOSIT_AMOUNT_TOO_SMALL",
            format!(
                "deposit amount must be at least {}",
                money(MIN_DEPOSIT_AMOUNT)
            ),
        ));
    }
    if let Some(expected) = expected_amount {
        if deposit.amount < expected {
            return Err(AppError::bad_request(
                "DEPOSIT_AMOUNT_MISMATCH",
                "chain deposit amount is below submitted amount",
            ));
        }
    }

    let mut matching_transfer = false;
    for log in &receipt.logs {
        if let Some(transfer) = parse_usdt_transfer_log(log, &token_contract)? {
            if transfer.from_address == bound_wallet
                && transfer.to_address == settlement_wallet
                && transfer.amount == deposit.amount
            {
                matching_transfer = true;
                break;
            }
        }
    }
    if !matching_transfer {
        return Err(AppError::bad_request(
            "DEPOSIT_TRANSFER_NOT_FOUND",
            "matching USDT transfer from wallet to settlement wallet not found",
        ));
    }

    Ok(VerifiedDeposit {
        tx_hash: tx_hash.to_string(),
        token_contract,
        contract_address: vault_contract,
        deposit_id: deposit.deposit_id,
        from_address: deposit.payer,
        to_address: deposit.receiver,
        amount: deposit.amount,
        block_number,
        confirmations,
        status: if confirmations >= min_confirmations {
            "credited".to_string()
        } else {
            "confirming".to_string()
        },
        credited_at: if confirmations >= min_confirmations {
            Some(Utc::now())
        } else {
            None
        },
    })
}

fn parse_usdt_transfer_log(
    log: &RpcLog,
    token_contract: &str,
) -> Result<Option<TransferLog>, AppError> {
    let address = normalize_wallet_address(&log.address)?;
    if address != token_contract {
        return Ok(None);
    }
    if log.topics.len() < 3 || !log.topics[0].eq_ignore_ascii_case(TRANSFER_TOPIC) {
        return Ok(None);
    }
    let from_address = topic_to_address(&log.topics[1])?;
    let to_address = topic_to_address(&log.topics[2])?;
    let units = U256::from_str_radix(log.data.trim_start_matches("0x"), 16)
        .map_err(|_| AppError::bad_request("DEPOSIT_LOG_INVALID", "invalid transfer amount"))?;
    let amount = u256_to_decimal(units, 18)?;
    Ok(Some(TransferLog {
        from_address,
        to_address,
        amount,
    }))
}

fn parse_deposit_received_log(
    log: &RpcLog,
    vault_contract: &str,
) -> Result<Option<DepositReceivedLog>, AppError> {
    let address = normalize_wallet_address(&log.address)?;
    if address != vault_contract {
        return Ok(None);
    }
    if log.topics.len() < 4 || !log.topics[0].eq_ignore_ascii_case(DEPOSIT_RECEIVED_TOPIC) {
        return Ok(None);
    }
    let words = abi_data_words(&log.data)?;
    if words.len() < 2 {
        return Err(AppError::bad_request(
            "DEPOSIT_EVENT_INVALID",
            "DepositReceived event data is incomplete",
        ));
    }
    Ok(Some(DepositReceivedLog {
        account_id: normalize_topic(&log.topics[1])?,
        deposit_id: normalize_topic(&log.topics[2])?,
        payer: topic_to_address(&log.topics[3])?,
        receiver: word_to_address(&words[0])?,
        amount: u256_to_decimal(
            U256::from_str_radix(&words[1], 16).map_err(|_| {
                AppError::bad_request("DEPOSIT_EVENT_INVALID", "invalid deposit amount")
            })?,
            18,
        )?,
    }))
}

fn parse_refund_paid_log(
    log: &RpcLog,
    vault_contract: &str,
) -> Result<Option<RefundPaidLog>, AppError> {
    let address = normalize_wallet_address(&log.address)?;
    if address != vault_contract {
        return Ok(None);
    }
    if log.topics.len() < 4 || !log.topics[0].eq_ignore_ascii_case(REFUND_PAID_TOPIC) {
        return Ok(None);
    }
    let words = abi_data_words(&log.data)?;
    if words.is_empty() {
        return Err(AppError::bad_request(
            "REFUND_EVENT_INVALID",
            "RefundPaid event data is incomplete",
        ));
    }
    Ok(Some(RefundPaidLog {
        refund_id: normalize_topic(&log.topics[1])?,
        payer: topic_to_address(&log.topics[2])?,
        recipient: topic_to_address(&log.topics[3])?,
        amount: u256_to_decimal(
            U256::from_str_radix(&words[0], 16).map_err(|_| {
                AppError::bad_request("REFUND_EVENT_INVALID", "invalid refund amount")
            })?,
            18,
        )?,
    }))
}

async fn verify_bsc_refund(
    tx_hash: &str,
    withdrawal: &WithdrawalRecord,
) -> Result<VerifiedRefund, AppError> {
    let rpc_url = bsc_rpc_url()?;
    let token_contract = normalize_wallet_address(&bsc_usdt_contract())?;
    let vault_contract = bsc_vault_contract()?;
    let expected_refund_id = normalize_topic(&withdrawal.refund_id)?;
    let expected_recipient = normalize_wallet_address(&withdrawal.to_address)?;

    let receipt: Option<RpcTransactionReceipt> =
        rpc_call(&rpc_url, "eth_getTransactionReceipt", json!([tx_hash])).await?;
    let Some(receipt) = receipt else {
        return Ok(VerifiedRefund {
            tx_hash: tx_hash.to_string(),
            token_contract,
            contract_address: vault_contract,
            payout_wallet: withdrawal.payout_wallet.clone().unwrap_or_default(),
            to_address: expected_recipient,
            amount: withdrawal.amount,
            block_number: 0,
            confirmations: 0,
            status: "confirming".to_string(),
            processed_at: None,
        });
    };
    let block_number = receipt
        .block_number
        .as_deref()
        .map(hex_to_i64)
        .transpose()?
        .ok_or_else(|| AppError::bad_request("REFUND_UNCONFIRMED", "transaction is not mined"))?;
    if receipt.status.as_deref() != Some("0x1") {
        return Err(AppError::bad_request(
            "REFUND_TX_FAILED",
            "transaction receipt is not successful",
        ));
    }
    let current_block_hex: String = rpc_call(&rpc_url, "eth_blockNumber", json!([]))
        .await?
        .ok_or_else(|| {
            AppError::bad_request("BSC_BLOCK_UNAVAILABLE", "BSC block number unavailable")
        })?;
    let current_block = hex_to_i64(&current_block_hex)?;
    let confirmations = (current_block - block_number + 1).max(0) as i32;
    let min_confirmations = deposit_min_confirmations();

    let refund = receipt
        .logs
        .iter()
        .find_map(|log| parse_refund_paid_log(log, &vault_contract).transpose())
        .transpose()?
        .ok_or_else(|| {
            AppError::bad_request("REFUND_EVENT_NOT_FOUND", "RefundPaid event not found")
        })?;
    if refund.refund_id != expected_refund_id {
        return Err(AppError::bad_request(
            "REFUND_ID_MISMATCH",
            "refund event id does not match withdrawal",
        ));
    }
    if refund.recipient != expected_recipient {
        return Err(AppError::bad_request(
            "REFUND_RECIPIENT_MISMATCH",
            "refund event recipient does not match withdrawal",
        ));
    }
    if refund.amount != withdrawal.amount {
        return Err(AppError::bad_request(
            "REFUND_AMOUNT_MISMATCH",
            "refund event amount does not match withdrawal",
        ));
    }

    let mut matching_transfer = false;
    for log in &receipt.logs {
        if let Some(transfer) = parse_usdt_transfer_log(log, &token_contract)? {
            if transfer.from_address == refund.payer
                && transfer.to_address == refund.recipient
                && transfer.amount == refund.amount
            {
                matching_transfer = true;
                break;
            }
        }
    }
    if !matching_transfer {
        return Err(AppError::bad_request(
            "REFUND_TRANSFER_NOT_FOUND",
            "matching USDT transfer from payout wallet to recipient not found",
        ));
    }

    Ok(VerifiedRefund {
        tx_hash: tx_hash.to_string(),
        token_contract,
        contract_address: vault_contract,
        payout_wallet: refund.payer,
        to_address: refund.recipient,
        amount: refund.amount,
        block_number,
        confirmations,
        status: if confirmations >= min_confirmations {
            "paid".to_string()
        } else {
            "confirming".to_string()
        },
        processed_at: if confirmations >= min_confirmations {
            Some(Utc::now())
        } else {
            None
        },
    })
}

fn default_exchange_status() -> ExchangeCredentialStatus {
    ExchangeCredentialStatus {
        provider: BINANCE_PROVIDER.to_string(),
        api_key: None,
        connected: false,
        can_read_account: false,
        verification_status: "missing".to_string(),
        balance_available_usdt: None,
        balance_wallet_usdt: None,
        account_match_status: None,
        observed_balance_usdt: None,
        account_balance_delta_usdt: None,
        balance_updated_at: None,
        balance_cache_fresh: false,
        last_verified_at: None,
        last_error: None,
        updated_at: None,
    }
}

fn binance_base_url() -> String {
    env::var("BINANCE_FAPI_BASE_URL")
        .unwrap_or_else(|_| BINANCE_FAPI_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn env_i32(name: &str, default: i32, min: i32) -> i32 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<i32>().ok())
        .filter(|value| *value >= min)
        .unwrap_or(default)
}

fn env_i64(name: &str, default: i64, min: i64) -> i64 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value >= min)
        .unwrap_or(default)
}

fn env_u32(name: &str, default: u32, min: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .filter(|value| *value >= min)
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64, min: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value >= min)
        .unwrap_or(default)
}

fn env_usize(name: &str, default: usize, min: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value >= min)
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64, min: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= min)
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(default)
}

fn page_params(query: &ListQuery) -> (i64, i64, i64) {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_QUERY_LIMIT)
        .clamp(1, MAX_QUERY_LIMIT);
    let offset = (page - 1) * limit;
    (page, limit, offset)
}

fn search_pattern(query: &ListQuery) -> Option<String> {
    query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            format!(
                "%{}%",
                value
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
        })
}

fn status_filter(query: &ListQuery) -> Option<String> {
    query
        .status
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn binance_bind_local_ip() -> Result<Option<std::net::IpAddr>, AppError> {
    let Ok(value) = env::var("BINANCE_BIND_LOCAL_IP") else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value.parse::<std::net::IpAddr>().map(Some).map_err(|_| {
        AppError::bad_request(
            "BINANCE_BIND_LOCAL_IP_INVALID",
            "BINANCE_BIND_LOCAL_IP must be a local IPv4 or IPv6 address",
        )
    })
}

fn parse_optional_ip(
    value: Option<&str>,
    field: &str,
) -> Result<Option<std::net::IpAddr>, AppError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    value.parse::<std::net::IpAddr>().map(Some).map_err(|_| {
        AppError::bad_request(
            "IP_ADDRESS_INVALID",
            format!("{field} must be a local IPv4 or IPv6 address"),
        )
    })
}

fn binance_http_client() -> Result<reqwest::Client, AppError> {
    binance_http_client_for(binance_bind_local_ip()?)
}

fn binance_http_client_for(
    bind_local_ip: Option<std::net::IpAddr>,
) -> Result<reqwest::Client, AppError> {
    let mut builder = reqwest::Client::builder().timeout(StdDuration::from_secs(8));
    if let Some(local_ip) = bind_local_ip {
        builder = builder.local_address(local_ip);
    }
    builder
        .build()
        .map_err(|err| AppError::internal("HTTP_CLIENT_FAILED", err.to_string()))
}

fn normalize_api_key(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.len() < 20 || value.len() > 128 || !value.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        return Err(AppError::bad_request(
            "BINANCE_API_KEY_INVALID",
            "api_key format is invalid",
        ));
    }
    Ok(value.to_string())
}

fn normalize_api_secret(value: &str) -> Result<String, AppError> {
    let value = value.trim();
    if value.len() < 20 || value.len() > 128 || !value.chars().all(|ch| ch.is_ascii_alphanumeric())
    {
        return Err(AppError::bad_request(
            "BINANCE_API_SECRET_INVALID",
            "api_secret format is invalid",
        ));
    }
    Ok(value.to_string())
}

fn encryption_key() -> Result<[u8; 32], AppError> {
    let raw = env::var("APP_ENCRYPTION_KEY").map_err(|_| {
        AppError::bad_request(
            "APP_ENCRYPTION_KEY_MISSING",
            "APP_ENCRYPTION_KEY must be set before storing exchange credentials",
        )
    })?;
    let trimmed = raw.trim();
    let decoded = general_purpose::STANDARD
        .decode(trimmed)
        .or_else(|_| hex::decode(trimmed))
        .map_err(|_| {
            AppError::bad_request(
                "APP_ENCRYPTION_KEY_INVALID",
                "APP_ENCRYPTION_KEY must be base64 or hex",
            )
        })?;
    if decoded.len() != 32 {
        return Err(AppError::bad_request(
            "APP_ENCRYPTION_KEY_INVALID",
            "APP_ENCRYPTION_KEY must decode to 32 bytes",
        ));
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&decoded);
    Ok(key)
}

fn encrypt_secret(secret: &str) -> Result<String, AppError> {
    let key = encryption_key()?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| {
        AppError::internal("ENCRYPTION_INIT_FAILED", "failed to initialize encryption")
    })?;
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce), secret.as_bytes())
        .map_err(|_| AppError::internal("ENCRYPTION_FAILED", "failed to encrypt secret"))?;
    Ok(format!(
        "v1:{}:{}",
        general_purpose::STANDARD.encode(nonce),
        general_purpose::STANDARD.encode(encrypted)
    ))
}

fn decrypt_secret(ciphertext: &str) -> Result<String, AppError> {
    let key = encryption_key()?;
    let parts = ciphertext.split(':').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "v1" {
        return Err(AppError::bad_request(
            "SECRET_CIPHERTEXT_INVALID",
            "stored secret has invalid format",
        ));
    }
    let nonce = general_purpose::STANDARD
        .decode(parts[1])
        .map_err(|_| AppError::bad_request("SECRET_CIPHERTEXT_INVALID", "invalid secret nonce"))?;
    let encrypted = general_purpose::STANDARD
        .decode(parts[2])
        .map_err(|_| AppError::bad_request("SECRET_CIPHERTEXT_INVALID", "invalid secret body"))?;
    if nonce.len() != 12 {
        return Err(AppError::bad_request(
            "SECRET_CIPHERTEXT_INVALID",
            "invalid secret nonce length",
        ));
    }
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| {
        AppError::internal("ENCRYPTION_INIT_FAILED", "failed to initialize encryption")
    })?;
    let decrypted = cipher
        .decrypt(Nonce::from_slice(&nonce), encrypted.as_ref())
        .map_err(|_| {
            AppError::bad_request("SECRET_DECRYPT_FAILED", "failed to decrypt exchange secret")
        })?;
    String::from_utf8(decrypted)
        .map_err(|_| AppError::bad_request("SECRET_DECRYPT_FAILED", "exchange secret is not utf8"))
}

fn binance_signature(query: &str, secret: &str) -> Result<String, AppError> {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()).map_err(|_| {
        AppError::bad_request("BINANCE_SECRET_INVALID", "invalid Binance API secret")
    })?;
    mac.update(query.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

async fn binance_server_time(client: &reqwest::Client, base_url: &str) -> Result<i64, AppError> {
    let response = client
        .get(format!("{base_url}/fapi/v1/time"))
        .send()
        .await
        .map_err(|err| AppError::bad_request("BINANCE_REQUEST_FAILED", err.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::bad_request(
            "BINANCE_REQUEST_FAILED",
            format!("server time failed: {status} {text}"),
        ));
    }
    response
        .json::<BinanceTimeResponse>()
        .await
        .map(|body| body.server_time)
        .map_err(|err| AppError::bad_request("BINANCE_RESPONSE_INVALID", err.to_string()))
}

fn append_query_pair(query: &mut String, name: &str, value: impl ToString) {
    if !query.is_empty() {
        query.push('&');
    }
    query.push_str(name);
    query.push('=');
    query.push_str(&value.to_string());
}

async fn binance_signed_get<T: DeserializeOwned>(
    client: &reqwest::Client,
    api_key: &str,
    api_secret: &str,
    path: &str,
    mut pairs: Vec<(&str, String)>,
) -> Result<T, AppError> {
    let base_url = binance_base_url();
    let server_time = binance_server_time(client, &base_url).await?;
    pairs.push(("recvWindow", BINANCE_RECV_WINDOW.to_string()));
    pairs.push(("timestamp", server_time.to_string()));
    let mut query = String::new();
    for (name, value) in pairs {
        append_query_pair(&mut query, name, value);
    }
    let signature = binance_signature(&query, api_secret)?;
    let response = client
        .get(format!("{base_url}{path}?{query}&signature={signature}"))
        .header("X-MBX-APIKEY", api_key)
        .send()
        .await
        .map_err(|err| AppError::bad_request("BINANCE_REQUEST_FAILED", err.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::bad_request(
            "BINANCE_REQUEST_FAILED",
            format!("{path} failed: {status} {text}"),
        ));
    }
    response
        .json::<T>()
        .await
        .map_err(|err| AppError::bad_request("BINANCE_RESPONSE_INVALID", err.to_string()))
}

fn binance_account_snapshot(
    account: BinanceAccountResponse,
) -> Result<BinanceAccountSnapshot, AppError> {
    if let Some(asset) = account
        .assets
        .into_iter()
        .find(|asset| asset.asset.eq_ignore_ascii_case("USDT"))
    {
        return Ok(BinanceAccountSnapshot {
            wallet_balance_usdt: parse_decimal(&asset.wallet_balance, "walletBalance")?,
            available_balance_usdt: parse_decimal(&asset.available_balance, "availableBalance")?,
        });
    }
    Ok(BinanceAccountSnapshot {
        wallet_balance_usdt: parse_decimal(&account.total_wallet_balance, "totalWalletBalance")?,
        available_balance_usdt: parse_decimal(&account.total_wallet_balance, "totalWalletBalance")?,
    })
}

async fn binance_account_access(
    api_key: &str,
    api_secret: &str,
) -> Result<BinanceAccountSnapshot, AppError> {
    binance_account_access_with_bind(api_key, api_secret, binance_bind_local_ip()?).await
}

async fn binance_account_access_with_bind(
    api_key: &str,
    api_secret: &str,
    bind_local_ip: Option<std::net::IpAddr>,
) -> Result<BinanceAccountSnapshot, AppError> {
    let client = binance_http_client_for(bind_local_ip)?;
    let account: BinanceAccountResponse =
        binance_signed_get(&client, api_key, api_secret, "/fapi/v2/account", Vec::new()).await?;
    binance_account_snapshot(account)
}

fn start_market_data_feed(state: &SharedState) {
    let config = PredictionConfig::from_env();
    let state = state.clone();
    tokio::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(StdDuration::from_secs(6))
            .build()
        {
            Ok(client) => client,
            Err(err) => {
                record_market_error(&state, err.to_string()).await;
                return;
            }
        };
        loop {
            match preheat_market_data(&state, &client, &config).await {
                Ok(()) => break,
                Err(err) => {
                    record_market_error(&state, err.message).await;
                    tokio::time::sleep(StdDuration::from_secs(
                        config.market_ws_reconnect_seconds.max(1),
                    ))
                    .await;
                }
            }
        }
        loop {
            if let Err(err) = run_market_ws(&state, &config).await {
                record_market_error(&state, err.message).await;
            }
            tokio::time::sleep(StdDuration::from_secs(
                config.market_ws_reconnect_seconds.max(1),
            ))
            .await;
        }
    });
}

fn start_market_second_bar_retention_worker(state: &SharedState) {
    let Some(pool) = state.db.clone() else {
        return;
    };
    tokio::spawn(async move {
        loop {
            if let Err(err) =
                db_delete_old_market_second_bars(&pool, MARKET_SECOND_BARS_RETENTION_SECONDS).await
            {
                eprintln!("market second bar retention failed: {}", err.message);
            }
            tokio::time::sleep(StdDuration::from_secs(
                MARKET_SECOND_BARS_CLEANUP_INTERVAL_SECONDS,
            ))
            .await;
        }
    });
}

async fn preheat_market_data(
    state: &SharedState,
    client: &reqwest::Client,
    config: &PredictionConfig,
) -> Result<(), AppError> {
    let candles = fetch_prediction_candles(client, config).await?;
    let latest = candles.last().cloned();
    let restore_limit = EVENT_ML_WARM_SECONDS + MARKET_SECOND_BARS_RESTORE_EXTRA_SECONDS;
    let mut restored_second_bars = if let Some(pool) = state.db.as_ref() {
        db_load_market_second_bars(pool, DEFAULT_SYMBOL, restore_limit).await?
    } else {
        Vec::new()
    };
    let target_now_ms = Utc::now().timestamp_millis();
    let backfilled_second_bars =
        match fetch_recent_agg_trade_second_bars(client, config, DEFAULT_SYMBOL, target_now_ms)
            .await
        {
            Ok(bars) => bars,
            Err(err) if restored_second_bars.len() >= EVENT_ML_WARM_SECONDS => {
                eprintln!("market second bar REST backfill skipped: {}", err.message);
                Vec::new()
            }
            Err(err) => return Err(err),
        };
    restored_second_bars.extend(backfilled_second_bars);
    restored_second_bars = merge_market_second_bars(restored_second_bars);
    let mut market = state.market_data.write().await;
    for candle in candles {
        market.candles.insert(candle.open_time_ms, candle);
    }
    trim_market_candles(&mut market, config.candle_limit * 2);
    if let Some(latest) = latest {
        market.latest_price = Some(latest.close);
        market.latest_price_time_ms = Some(latest.close_time_ms);
        market.latest_source = Some("preheat_index_kline_1m".to_string());
        let price_points = market
            .candles
            .values()
            .map(|candle| (candle.close_time_ms, candle.close))
            .collect::<Vec<_>>();
        for (time_ms, price) in price_points {
            market.price_points.insert(time_ms, price);
        }
        trim_market_price_points(&mut market, config.candle_limit * 60 * 2);
    }
    for bar in restored_second_bars {
        fill_missing_second_bars(&mut market, bar.second_ms);
        market.last_second_close = Some(bar.close);
        market.last_second_close_from_index = bar.price_from_index;
        market.price_points.insert(bar.second_ms, bar.close);
        market.second_bars.insert(bar.second_ms, bar);
    }
    keep_contiguous_second_bar_tail(&mut market);
    trim_market_second_bars(&mut market, EVENT_ML_WARM_SECONDS + 1200);
    trim_market_price_points(&mut market, config.candle_limit * 60 * 2);
    if let Some(latest_bar) = market.second_bars.values().next_back().cloned() {
        market.latest_price = Some(latest_bar.close);
        market.latest_price_time_ms = Some(latest_bar.second_ms);
        market.latest_source = Some(if latest_bar.price_from_index {
            "preheat_persisted_indexPrice_1s".to_string()
        } else {
            "preheat_aggTrade_1s".to_string()
        });
        market.last_second_close = Some(latest_bar.close);
        market.last_second_close_from_index = latest_bar.price_from_index;
    }
    market.preheated_at = Some(Utc::now());
    market.last_error = None;
    Ok(())
}

async fn fetch_prediction_candles(
    client: &reqwest::Client,
    config: &PredictionConfig,
) -> Result<Vec<MarketCandle>, AppError> {
    let url = format!(
        "{}/fapi/v1/indexPriceKlines?pair={}&interval=1m&limit={}",
        config.market_data_base_url, DEFAULT_SYMBOL, config.candle_limit
    );
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| AppError::bad_request("PREDICTION_DATA_FAILED", err.to_string()))?;
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::bad_request(
            "PREDICTION_DATA_FAILED",
            format!("market data failed: {status} {text}"),
        ));
    }
    let raw = response
        .json::<Vec<serde_json::Value>>()
        .await
        .map_err(|err| AppError::bad_request("PREDICTION_DATA_INVALID", err.to_string()))?;
    let now_ms = Utc::now().timestamp_millis();
    let mut candles = Vec::with_capacity(raw.len());
    for item in raw {
        let Some(values) = item.as_array() else {
            continue;
        };
        if values.len() < 7 {
            continue;
        }
        let open_time_ms = json_i64(&values[0])?;
        let close_time_ms = json_i64(&values[6])?;
        if close_time_ms > now_ms - 500 {
            continue;
        }
        candles.push(MarketCandle {
            open_time_ms,
            open: json_f64(&values[1])?,
            high: json_f64(&values[2])?,
            low: json_f64(&values[3])?,
            close: json_f64(&values[4])?,
            volume: json_f64(&values[5])?,
            close_time_ms,
        });
    }
    candles.sort_by_key(|candle| candle.open_time_ms);
    candles.dedup_by_key(|candle| candle.open_time_ms);
    if candles.len() < DEFAULT_MARKET_MIN_WARM_CANDLES.min(config.candle_limit) {
        return Err(AppError::bad_request(
            "PREDICTION_DATA_INSUFFICIENT",
            "not enough candles",
        ));
    }
    Ok(candles)
}

async fn fetch_recent_agg_trade_second_bars(
    client: &reqwest::Client,
    config: &PredictionConfig,
    symbol: &str,
    now_ms: i64,
) -> Result<Vec<MarketSecondBar>, AppError> {
    let start_ms = now_ms
        - ((EVENT_ML_WARM_SECONDS as i64 + MARKET_SECOND_BARS_BACKFILL_EXTRA_SECONDS) * 1_000);
    let mut from_id: Option<i64> = None;
    let mut page_start_ms = start_ms;
    let mut bars = BTreeMap::<i64, MarketSecondBar>::new();
    for _ in 0..MARKET_SECOND_BARS_BACKFILL_MAX_PAGES {
        let url = if let Some(from_id) = from_id {
            format!(
                "{}/fapi/v1/aggTrades?symbol={symbol}&fromId={from_id}&limit=1000",
                config.market_data_base_url
            )
        } else {
            format!(
                "{}/fapi/v1/aggTrades?symbol={symbol}&startTime={page_start_ms}&endTime={now_ms}&limit=1000",
                config.market_data_base_url
            )
        };
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|err| AppError::bad_request("PREDICTION_DATA_FAILED", err.to_string()))?;
        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::bad_request(
                "PREDICTION_DATA_FAILED",
                format!("aggTrade backfill failed: {status} {text}"),
            ));
        }
        let raw = response
            .json::<Vec<BinanceAggTradeRest>>()
            .await
            .map_err(|err| AppError::bad_request("PREDICTION_DATA_INVALID", err.to_string()))?;
        if raw.is_empty() {
            break;
        }
        let mut latest_trade_ms = page_start_ms;
        let mut latest_trade_id = from_id.unwrap_or(0);
        for trade in raw {
            latest_trade_ms = latest_trade_ms.max(trade.trade_time_ms);
            latest_trade_id = latest_trade_id.max(trade.aggregate_trade_id);
            if trade.trade_time_ms < start_ms || trade.trade_time_ms > now_ms - 500 {
                continue;
            }
            let price = parse_market_f64(&trade.price)?;
            let quantity = parse_market_f64(&trade.quantity)?;
            let second_ms = trade.trade_time_ms - trade.trade_time_ms.rem_euclid(1_000);
            let bar = bars.entry(second_ms).or_insert_with(|| MarketSecondBar {
                second_ms,
                high: price,
                low: price,
                close: price,
                volume: 0.0,
                taker_buy_volume: 0.0,
                taker_sell_volume: 0.0,
                trade_count: 0,
                price_from_index: false,
            });
            bar.high = bar.high.max(price);
            bar.low = bar.low.min(price);
            bar.close = price;
            bar.volume += quantity;
            if trade.is_buyer_maker {
                bar.taker_sell_volume += quantity;
            } else {
                bar.taker_buy_volume += quantity;
            }
            bar.trade_count += 1;
        }
        if latest_trade_ms >= now_ms - 1_000 {
            break;
        }
        page_start_ms = latest_trade_ms + 1;
        from_id = latest_trade_id.checked_add(1);
    }
    Ok(bars.into_values().collect())
}

async fn run_market_ws(state: &SharedState, config: &PredictionConfig) -> Result<(), AppError> {
    let (ws_stream, _) = connect_async(&config.market_ws_url)
        .await
        .map_err(|err| AppError::bad_request("MARKET_WS_CONNECT_FAILED", err.to_string()))?;
    {
        let mut market = state.market_data.write().await;
        market.last_connected_at = Some(Utc::now());
        market.last_error = None;
    }
    let (mut write, mut read) = ws_stream.split();
    while let Some(message) = read.next().await {
        let message = message
            .map_err(|err| AppError::bad_request("MARKET_WS_READ_FAILED", err.to_string()))?;
        if let TungsteniteMessage::Ping(payload) = message {
            write
                .send(TungsteniteMessage::Pong(payload))
                .await
                .map_err(|err| AppError::bad_request("MARKET_WS_WRITE_FAILED", err.to_string()))?;
            continue;
        }
        if !message.is_text() {
            continue;
        }
        let text = message
            .to_text()
            .map_err(|err| AppError::bad_request("MARKET_WS_MESSAGE_INVALID", err.to_string()))?;
        apply_market_ws_message(state, config, text).await?;
    }
    Err(AppError::bad_request(
        "MARKET_WS_CLOSED",
        "market websocket closed",
    ))
}

async fn apply_market_ws_message(
    state: &SharedState,
    config: &PredictionConfig,
    text: &str,
) -> Result<(), AppError> {
    let combined: BinanceCombinedStreamMessage = serde_json::from_str(text)
        .map_err(|err| AppError::bad_request("MARKET_WS_MESSAGE_INVALID", err.to_string()))?;
    let stream = combined.stream.unwrap_or_default();
    if stream.contains("@aggTrade") {
        let trade: BinanceAggTradeStream = serde_json::from_value(combined.data)
            .map_err(|err| AppError::bad_request("MARKET_WS_MESSAGE_INVALID", err.to_string()))?;
        update_market_agg_trade(state, trade).await?;
    } else if stream.contains("@markPrice") {
        let mark: BinanceMarkPriceStream = serde_json::from_value(combined.data)
            .map_err(|err| AppError::bad_request("MARKET_WS_MESSAGE_INVALID", err.to_string()))?;
        update_market_index_price(
            state,
            mark.index_price,
            Some(mark.mark_price),
            mark.event_time_ms,
        )
        .await?;
    } else if stream.contains("@kline_1m") {
        let kline: BinanceKlineStream = serde_json::from_value(combined.data)
            .map_err(|err| AppError::bad_request("MARKET_WS_MESSAGE_INVALID", err.to_string()))?;
        let live_close = kline.kline.close.clone();
        let live_time = kline.kline.close_time_ms;
        if kline.kline.is_closed {
            let candle = MarketCandle {
                open_time_ms: kline.kline.open_time_ms,
                close_time_ms: kline.kline.close_time_ms,
                open: parse_market_f64(&kline.kline.open)?,
                high: parse_market_f64(&kline.kline.high)?,
                low: parse_market_f64(&kline.kline.low)?,
                close: parse_market_f64(&kline.kline.close)?,
                volume: parse_market_f64(&kline.kline.volume)?,
            };
            let mut market = state.market_data.write().await;
            market.candles.insert(candle.open_time_ms, candle.clone());
            market.latest_futures_kline_price = Some(candle.close);
            market.last_message_at = Some(Utc::now());
            market.last_error = None;
            trim_market_candles(&mut market, config.candle_limit * 2);
        } else {
            update_market_aux_price(state, live_close, Some(live_time), "kline_1m_live").await?;
        }
    }
    Ok(())
}

async fn update_market_index_price(
    state: &SharedState,
    index_price: String,
    mark_price: Option<String>,
    event_time_ms: i64,
) -> Result<(), AppError> {
    let price = parse_market_f64(&index_price)?;
    let mark_price = mark_price.as_deref().map(parse_market_f64).transpose()?;
    let second_ms = event_time_ms - event_time_ms.rem_euclid(1_000);
    let bars_to_store = {
        let mut market = state.market_data.write().await;
        market.latest_price = Some(price);
        market.latest_price_time_ms = Some(event_time_ms);
        market.latest_source = Some("indexPrice".to_string());
        market.price_points.insert(event_time_ms, price);
        trim_market_price_points(&mut market, DEFAULT_PREDICTION_CANDLE_LIMIT * 60 * 2);
        let mut bars_to_store = fill_missing_second_bars(&mut market, second_ms);
        if let Some(previous_bar) = market
            .second_bars
            .get(&(second_ms - 1_000))
            .filter(|bar| bar.price_from_index)
            .cloned()
        {
            bars_to_store.push(previous_bar);
        }
        let updated_bar = {
            let bar = market
                .second_bars
                .entry(second_ms)
                .or_insert_with(|| MarketSecondBar {
                    second_ms,
                    high: price,
                    low: price,
                    close: price,
                    volume: 0.0,
                    taker_buy_volume: 0.0,
                    taker_sell_volume: 0.0,
                    trade_count: 0,
                    price_from_index: true,
                });
            if bar.price_from_index {
                bar.high = bar.high.max(price);
                bar.low = bar.low.min(price);
                bar.close = price;
            } else {
                bar.high = price;
                bar.low = price;
                bar.close = price;
                bar.price_from_index = true;
            }
            bar.clone()
        };
        bars_to_store.push(updated_bar);
        market.last_second_close = Some(price);
        market.last_second_close_from_index = true;
        keep_contiguous_second_bar_tail(&mut market);
        trim_market_second_bars(&mut market, EVENT_ML_WARM_SECONDS + 1200);
        if let Some(mark_price) = mark_price {
            market.latest_mark_price = Some(mark_price);
        }
        market.last_message_at = Some(Utc::now());
        market.last_error = None;
        bars_to_store
    };
    if let Some(pool) = state.db.as_ref() {
        db_upsert_market_second_bars(pool, DEFAULT_SYMBOL, &bars_to_store).await?;
    }
    Ok(())
}

async fn update_market_agg_trade(
    state: &SharedState,
    trade: BinanceAggTradeStream,
) -> Result<(), AppError> {
    let price = parse_market_f64(&trade.price)?;
    let quantity = parse_market_f64(&trade.quantity)?;
    let second_ms = trade.trade_time_ms - trade.trade_time_ms.rem_euclid(1_000);
    let trade_count = (trade.last_trade_id - trade.first_trade_id + 1).max(1) as i32;
    let mut market = state.market_data.write().await;
    fill_missing_second_bars(&mut market, second_ms);
    let mut trade_updated_bar_price = false;
    let bar = market
        .second_bars
        .entry(second_ms)
        .or_insert_with(|| MarketSecondBar {
            second_ms,
            high: price,
            low: price,
            close: price,
            volume: 0.0,
            taker_buy_volume: 0.0,
            taker_sell_volume: 0.0,
            trade_count: 0,
            price_from_index: false,
        });
    if !bar.price_from_index {
        bar.high = bar.high.max(price);
        bar.low = bar.low.min(price);
        bar.close = price;
        trade_updated_bar_price = true;
    }
    bar.volume += quantity;
    if trade.is_buyer_maker {
        bar.taker_sell_volume += quantity;
    } else {
        bar.taker_buy_volume += quantity;
    }
    bar.trade_count += trade_count;
    if trade_updated_bar_price {
        market.last_second_close = Some(price);
        market.last_second_close_from_index = false;
    }
    market.latest_trade_price = Some(price);
    if market.latest_price.is_none() {
        market.latest_price = Some(price);
        market.latest_price_time_ms = Some(trade.trade_time_ms);
        market.latest_source = Some("aggTrade".to_string());
    }
    market.last_message_at = Some(Utc::now());
    market.last_error = None;
    trim_market_second_bars(&mut market, EVENT_ML_WARM_SECONDS + 1200);
    Ok(())
}

async fn update_market_aux_price(
    state: &SharedState,
    price: String,
    event_time_ms: Option<i64>,
    source: &str,
) -> Result<(), AppError> {
    let price = parse_market_f64(&price)?;
    let mut market = state.market_data.write().await;
    match source {
        "aggTrade" => market.latest_trade_price = Some(price),
        "kline_1m_live" => market.latest_futures_kline_price = Some(price),
        _ => {}
    }
    if market.latest_price.is_none() {
        market.latest_price = Some(price);
        market.latest_price_time_ms = event_time_ms;
        market.latest_source = Some(source.to_string());
    }
    market.last_message_at = Some(Utc::now());
    market.last_error = None;
    Ok(())
}

async fn record_market_error(state: &SharedState, message: String) {
    let mut market = state.market_data.write().await;
    market.last_error = Some(message.chars().take(500).collect());
}

fn trim_market_candles(market: &mut MarketDataState, max_len: usize) {
    while market.candles.len() > max_len.max(DEFAULT_PREDICTION_CANDLE_LIMIT) {
        let Some(first) = market.candles.keys().next().copied() else {
            break;
        };
        market.candles.remove(&first);
    }
}

fn trim_market_price_points(market: &mut MarketDataState, max_len: usize) {
    while market.price_points.len() > max_len.max(600) {
        let Some(first) = market.price_points.keys().next().copied() else {
            break;
        };
        market.price_points.remove(&first);
    }
}

fn trim_market_second_bars(market: &mut MarketDataState, max_len: usize) {
    while market.second_bars.len() > max_len.max(EVENT_ML_WARM_SECONDS) {
        let Some(first) = market.second_bars.keys().next().copied() else {
            break;
        };
        market.second_bars.remove(&first);
    }
}

fn fill_missing_second_bars(
    market: &mut MarketDataState,
    current_second_ms: i64,
) -> Vec<MarketSecondBar> {
    let Some(last_second_ms) = market.second_bars.keys().next_back().copied() else {
        return Vec::new();
    };
    if current_second_ms <= last_second_ms + 1_000 {
        return Vec::new();
    }
    let Some(mut price) = market.last_second_close else {
        return Vec::new();
    };
    let max_fill = 300;
    let mut second_ms = last_second_ms + 1_000;
    let mut filled = 0;
    let mut price_from_index = market.last_second_close_from_index;
    let mut inserted = Vec::new();
    while second_ms < current_second_ms && filled < max_fill {
        if let Some(last) = market.second_bars.get(&(second_ms - 1_000)) {
            price = last.close;
            price_from_index = last.price_from_index;
        }
        let bar = MarketSecondBar {
            second_ms,
            high: price,
            low: price,
            close: price,
            volume: 0.0,
            taker_buy_volume: 0.0,
            taker_sell_volume: 0.0,
            trade_count: 0,
            price_from_index,
        };
        market.second_bars.insert(second_ms, bar.clone());
        inserted.push(bar);
        second_ms += 1_000;
        filled += 1;
    }
    if second_ms < current_second_ms {
        market.second_bars.retain(|key, _| *key >= second_ms);
    }
    inserted
}

fn keep_contiguous_second_bar_tail(market: &mut MarketDataState) {
    let keys = market.second_bars.keys().copied().collect::<Vec<_>>();
    let Some(mut start_ms) = keys.last().copied() else {
        return;
    };
    for pair in keys.windows(2).rev() {
        if pair[1] == pair[0] + 1_000 {
            start_ms = pair[0];
        } else {
            break;
        }
    }
    market.second_bars.retain(|key, _| *key >= start_ms);
}

fn merge_market_second_bars(bars: Vec<MarketSecondBar>) -> Vec<MarketSecondBar> {
    let mut merged = BTreeMap::<i64, MarketSecondBar>::new();
    for bar in bars {
        merged
            .entry(bar.second_ms)
            .and_modify(|existing| merge_market_second_bar(existing, &bar))
            .or_insert(bar);
    }
    merged.into_values().collect()
}

fn merge_market_second_bar(existing: &mut MarketSecondBar, incoming: &MarketSecondBar) {
    if incoming.price_from_index && !existing.price_from_index {
        existing.high = incoming.high;
        existing.low = incoming.low;
        existing.close = incoming.close;
        existing.price_from_index = true;
    } else if existing.price_from_index == incoming.price_from_index {
        existing.high = existing.high.max(incoming.high);
        existing.low = existing.low.min(incoming.low);
        existing.close = incoming.close;
    }
    existing.volume = existing.volume.max(incoming.volume);
    existing.taker_buy_volume = existing.taker_buy_volume.max(incoming.taker_buy_volume);
    existing.taker_sell_volume = existing.taker_sell_volume.max(incoming.taker_sell_volume);
    existing.trade_count = existing.trade_count.max(incoming.trade_count);
}

fn parse_market_f64(value: &str) -> Result<f64, AppError> {
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| AppError::bad_request("PREDICTION_DATA_INVALID", "invalid market price"))
}

async fn evaluate_prediction(
    state: &SharedState,
    config: &PredictionConfig,
) -> Result<PredictionDecision, AppError> {
    let snapshot = {
        let market = state.market_data.read().await;
        if let Some(error) = &market.last_error {
            return Err(AppError::bad_request(
                "PREDICTION_MARKET_FEED_ERROR",
                error.clone(),
            ));
        }
        market.snapshot(config.candle_limit)
    }
    .ok_or_else(|| {
        AppError::bad_request("PREDICTION_DATA_INSUFFICIENT", "market feed warming up")
    })?;
    if let Some(model) = state.event_ml_model.as_ref() {
        return Ok(build_event_ml_prediction_decision(&snapshot, config, model)
            .unwrap_or_else(|| build_event_ml_warmup_decision(&snapshot, config, model)));
    }
    build_prediction_decision(&snapshot, config)
}

fn build_event_ml_warmup_decision(
    snapshot: &MarketDataSnapshot,
    config: &PredictionConfig,
    model: &EventMlModel,
) -> PredictionDecision {
    let entry_age_seconds = millis_to_utc(snapshot.latest_price_time_ms)
        .map(|price_time| (Utc::now() - price_time).num_seconds())
        .unwrap_or(0);
    PredictionDecision {
        direction: "HOLD".to_string(),
        confidence: 0.0,
        predicted_return: 0.0,
        entry_price: snapshot.latest_price,
        score: 0.0,
        signal_cooldown_seconds: config.signal_cooldown_seconds,
        market_source: snapshot.latest_source.clone(),
        market_age_seconds: entry_age_seconds,
        market_candles: snapshot.second_bars.len(),
        reason: format!(
            "hold ml warming up second_bars {}/{}",
            snapshot.second_bars.len(),
            EVENT_ML_WARM_SECONDS + 1
        ),
        feature_snapshot: json!({
            "market": {
                "source": snapshot.latest_source.clone(),
                "feature_source": "indexPrice_1s",
                "latest_price_time_ms": snapshot.latest_price_time_ms,
                "age_seconds": entry_age_seconds,
                "second_bars": snapshot.second_bars.len()
            },
            "config": {
                "model_version": PREDICTION_MODEL_VERSION,
                "fallback_model_version": FALLBACK_PREDICTION_MODEL_VERSION,
                "interval_seconds": config.interval_seconds,
                "signal_cooldown_seconds": config.signal_cooldown_seconds
            },
            "ml": {
                "model_version": model.version,
                "symbol": model.symbol,
                "period_seconds": model.period_seconds,
                "signal_period_seconds": DEFAULT_PERIOD_SECONDS,
                "feature_count": model.feature_names.len(),
                "warm_seconds": EVENT_ML_WARM_SECONDS
            },
            "scores": {
                "model_score": 0.0,
                "raw_confidence": 0.0
            },
            "decision": {
                "model_version": PREDICTION_MODEL_VERSION,
                "candidate_direction": null,
                "candidate_mode": "ml_extra_trees",
                "candidate_rule": EVENT_ML_TWO_STAGE_RULE,
                "pre_filter_passed": false,
                "pre_calibration_direction": "HOLD",
                "calibration_required": false
            }
        }),
    }
}

fn build_event_ml_prediction_decision(
    snapshot: &MarketDataSnapshot,
    config: &PredictionConfig,
    model: &EventMlModel,
) -> Option<PredictionDecision> {
    if model.period_seconds != DEFAULT_PERIOD_SECONDS {
        let latest = snapshot.second_bars.last()?;
        let entry_price_time = millis_to_utc(snapshot.latest_price_time_ms).ok()?;
        let feature_price_time = millis_to_utc(latest.second_ms).ok()?;
        let entry_age_seconds = (Utc::now() - entry_price_time).num_seconds();
        let feature_age_seconds = (Utc::now() - feature_price_time).num_seconds();
        return Some(PredictionDecision {
            direction: "HOLD".to_string(),
            confidence: 0.0,
            predicted_return: 0.0,
            entry_price: snapshot.latest_price,
            score: 0.0,
            signal_cooldown_seconds: config.signal_cooldown_seconds,
            market_source: snapshot.latest_source.clone(),
            market_age_seconds: entry_age_seconds,
            market_candles: snapshot.second_bars.len(),
            reason: format!(
                "hold ml period mismatch model {}s signal {}s",
                model.period_seconds, DEFAULT_PERIOD_SECONDS
            ),
            feature_snapshot: json!({
                "market": {
                    "source": snapshot.latest_source.clone(),
                    "feature_source": "indexPrice_1s",
                    "latest_price_time_ms": snapshot.latest_price_time_ms,
                    "feature_price_time_ms": latest.second_ms,
                    "age_seconds": entry_age_seconds,
                    "feature_age_seconds": feature_age_seconds,
                    "entry_price": snapshot.latest_price,
                    "feature_price": latest.close,
                    "feature_price_from_index": latest.price_from_index,
                    "second_bars": snapshot.second_bars.len()
                },
                "config": {
                    "model_version": PREDICTION_MODEL_VERSION,
                    "fallback_model_version": FALLBACK_PREDICTION_MODEL_VERSION,
                    "interval_seconds": config.interval_seconds,
                    "signal_cooldown_seconds": config.signal_cooldown_seconds
                },
                "ml": {
                    "model_version": model.version,
                    "symbol": model.symbol,
                    "period_seconds": model.period_seconds,
                    "signal_period_seconds": DEFAULT_PERIOD_SECONDS,
                    "feature_count": model.feature_names.len()
                },
                "scores": {
                    "model_score": 0.0,
                    "raw_confidence": 0.0
                },
                "decision": {
                    "model_version": PREDICTION_MODEL_VERSION,
                    "candidate_direction": null,
                    "candidate_mode": "ml_extra_trees",
                    "candidate_rule": "period_mismatch",
                    "pre_filter_passed": false,
                    "pre_calibration_direction": "HOLD",
                    "calibration_required": false
                }
            }),
        });
    }
    let mut prediction = event_ml_predict(model, &snapshot.second_bars)?;
    apply_event_ml_meta_policy(&mut prediction);
    let latest = snapshot.second_bars.last()?;
    let entry_price_time = millis_to_utc(snapshot.latest_price_time_ms).ok()?;
    let feature_price_time = millis_to_utc(latest.second_ms).ok()?;
    let entry_age_seconds = (Utc::now() - entry_price_time).num_seconds();
    let feature_age_seconds = (Utc::now() - feature_price_time).num_seconds();
    let mut direction = prediction.direction.clone();
    let mut reason = if let Some(reason) = prediction.filtered_reason.clone() {
        reason
    } else if direction == "UP" || direction == "DOWN" {
        format!(
            "ml send {} move_prob {:.4} up_prob {:.4} confidence {:.4}",
            direction,
            prediction.move_probability,
            prediction.up_probability,
            prediction.confidence
        )
    } else {
        format!(
            "ml hold rule {} move_prob {:.4} < {:.4} or dir up_prob {:.4} threshold {:.4}",
            prediction.rule,
            prediction.move_probability,
            model.thresholds.move_probability,
            prediction.up_probability,
            model.thresholds.direction_probability
        )
    };
    if feature_age_seconds > config.stale_seconds {
        direction = "HOLD".to_string();
        reason = format!("hold ml stale feature data age {feature_age_seconds}s");
    } else if entry_age_seconds > config.stale_seconds {
        direction = "HOLD".to_string();
        reason = format!("hold ml stale index data age {entry_age_seconds}s");
    } else if !latest.price_from_index {
        direction = "HOLD".to_string();
        reason = "hold ml waiting for live indexPrice_1s after REST backfill".to_string();
    }
    let confidence = prediction.confidence;
    let predicted_return = clamp((prediction.up_probability - 0.5) / 500.0, -0.003, 0.003);
    Some(PredictionDecision {
        direction: direction.clone(),
        confidence,
        predicted_return,
        entry_price: snapshot.latest_price,
        score: prediction.score,
        signal_cooldown_seconds: config.signal_cooldown_seconds,
        market_source: snapshot.latest_source.clone(),
        market_age_seconds: entry_age_seconds,
        market_candles: snapshot.second_bars.len(),
        reason,
        feature_snapshot: json!({
            "market": {
                "source": snapshot.latest_source.clone(),
                "feature_source": "indexPrice_1s",
                "latest_price_time_ms": snapshot.latest_price_time_ms,
                "feature_price_time_ms": latest.second_ms,
                "age_seconds": entry_age_seconds,
                "feature_age_seconds": feature_age_seconds,
                "entry_price": snapshot.latest_price,
                "feature_price": latest.close,
                "feature_price_from_index": latest.price_from_index,
                "second_bars": snapshot.second_bars.len()
            },
            "config": {
                "model_version": PREDICTION_MODEL_VERSION,
                "fallback_model_version": FALLBACK_PREDICTION_MODEL_VERSION,
                "interval_seconds": config.interval_seconds,
                "stale_seconds": config.stale_seconds,
                "signal_cooldown_seconds": config.signal_cooldown_seconds
            },
            "ml": {
                "model_version": model.version,
                "symbol": model.symbol,
                "period_seconds": model.period_seconds,
                "signal_period_seconds": DEFAULT_PERIOD_SECONDS,
                "move_probability": prediction.move_probability,
                "up_probability": prediction.up_probability,
                "down_probability": 1.0 - prediction.up_probability,
                "move_threshold": model.thresholds.move_probability,
                "direction_threshold": model.thresholds.direction_probability,
                "feature_count": model.feature_names.len(),
                "filtered_reason": prediction.filtered_reason
            },
            "scores": {
                "model_score": prediction.score,
                "raw_confidence": confidence,
                "directional_probability": confidence,
                "move_probability": prediction.move_probability,
                "up_probability": prediction.up_probability
            },
            "structure": {
                "body_ratio": prediction.features.get("candle_body_share_60").and_then(|value| value.as_f64()).unwrap_or(0.0),
                "lower_wick_ratio": prediction.features.get("candle_lower_wick_share_60").and_then(|value| value.as_f64()).unwrap_or(0.0),
                "upper_wick_ratio": prediction.features.get("candle_upper_wick_share_60").and_then(|value| value.as_f64()).unwrap_or(0.0)
            },
            "decision": {
                "model_version": PREDICTION_MODEL_VERSION,
                "candidate_direction": if direction == "UP" || direction == "DOWN" { Some(direction.as_str()) } else { None },
                "candidate_mode": "ml_extra_trees",
                "candidate_rule": prediction.rule,
                "pre_filter_passed": direction == "UP" || direction == "DOWN",
                "pre_calibration_direction": direction,
                "calibration_required": true
            },
            "features": prediction.features
        }),
    })
}

fn build_prediction_decision(
    snapshot: &MarketDataSnapshot,
    config: &PredictionConfig,
) -> Result<PredictionDecision, AppError> {
    let candles = &snapshot.candles;
    let latest = candles
        .last()
        .ok_or_else(|| AppError::bad_request("PREDICTION_DATA_INSUFFICIENT", "missing candle"))?;
    if candles.len() < DEFAULT_MARKET_MIN_WARM_CANDLES.min(config.candle_limit) {
        return Err(AppError::bad_request(
            "PREDICTION_DATA_INSUFFICIENT",
            "not enough warm candles",
        ));
    }
    let closed_closes = candles
        .iter()
        .map(|candle| candle.close)
        .collect::<Vec<_>>();
    let closes = live_close_series(
        &closed_closes,
        snapshot.latest_price,
        snapshot.latest_price_time_ms,
        latest.close_time_ms,
    );
    let volumes = candles
        .iter()
        .map(|candle| candle.volume)
        .collect::<Vec<_>>();
    let closed_ret_1 = return_n(&closed_closes, 1);
    let closed_ret_3 = return_n(&closed_closes, 3);
    let ret_1 = rolling_price_return(&snapshot.price_points, snapshot.latest_price_time_ms, 60)
        .unwrap_or_else(|| return_n(&closes, 1));
    let ret_3 = rolling_price_return(&snapshot.price_points, snapshot.latest_price_time_ms, 180)
        .unwrap_or_else(|| return_n(&closes, 3));
    let ret_5 = rolling_price_return(&snapshot.price_points, snapshot.latest_price_time_ms, 300)
        .unwrap_or_else(|| return_n(&closes, 5));
    let ret_10 = rolling_price_return(&snapshot.price_points, snapshot.latest_price_time_ms, 600)
        .unwrap_or_else(|| return_n(&closes, 10));
    let ret_30 = rolling_price_return(&snapshot.price_points, snapshot.latest_price_time_ms, 1800)
        .unwrap_or_else(|| return_n(&closes, 30));
    let ema5 = ema_last(&closes, 5);
    let ema10 = ema_last(&closes, 10);
    let ema20 = ema_last(&closes, 20);
    let ema5_prev = ema_last(&closes[..closes.len() - 1], 5);
    let ema10_prev = ema_last(&closes[..closes.len() - 1], 10);
    let (boll_pos, boll_width) = bollinger_position_width(&closes, 20);
    let volume_z = z_score_last(&volumes, 30);
    let volatility = realized_volatility(&closes, 30);
    let body_ratio = candle_body_ratio(latest);
    let upper_wick_ratio = candle_upper_wick_ratio(latest);
    let lower_wick_ratio = candle_lower_wick_ratio(latest);
    let close_position = candle_close_position(latest);
    let price_time = millis_to_utc(snapshot.latest_price_time_ms)?;
    let age_seconds = (Utc::now() - price_time).num_seconds();
    let cycle = event_cycle_context(
        candles,
        snapshot.latest_price,
        snapshot.latest_price_time_ms,
    );

    let live_ret_1 =
        rolling_price_return(&snapshot.price_points, snapshot.latest_price_time_ms, 30)
            .unwrap_or_else(|| ratio_delta(snapshot.latest_price, latest.close));
    let live_ret_3 =
        rolling_price_return(&snapshot.price_points, snapshot.latest_price_time_ms, 90)
            .unwrap_or(ret_3);
    // Event contracts settle against the strike at contract commencement.
    // Model the probability that price is above or below the entry strike at
    // the selected 10-minute expiry, then use technical features as drift.
    let upper_extreme = clamp((boll_pos - 0.82) / 0.18, 0.0, 1.0);
    let lower_extreme = clamp((0.18 - boll_pos) / 0.18, 0.0, 1.0);
    let up_move_intensity = clamp(
        (live_ret_1.max(ret_1).max(cycle.return_from_open) - 0.00035) / 0.00150,
        0.0,
        1.0,
    );
    let down_move_intensity = clamp(
        ((-live_ret_1).max(-ret_1).max(-cycle.return_from_open) - 0.00035) / 0.00150,
        0.0,
        1.0,
    );
    let live_reclaiming = snapshot.latest_price >= latest.close || latest.close >= latest.open;
    let live_rejecting = snapshot.latest_price <= latest.close || latest.close <= latest.open;
    let up_structure = structure_score_up(
        latest,
        close_position,
        upper_wick_ratio,
        lower_wick_ratio,
        live_ret_1,
        live_ret_3,
        ret_1,
        ret_3,
        cycle.return_from_open,
    );
    let down_structure = structure_score_down(
        latest,
        close_position,
        upper_wick_ratio,
        lower_wick_ratio,
        live_ret_1,
        live_ret_3,
        ret_1,
        ret_3,
        cycle.return_from_open,
    );
    let bottom_reversal_setup = lower_extreme > 0.55
        && (cycle.return_from_open < -0.00120 || ret_3 < -0.00100 || ret_5 < -0.00120)
        && live_ret_1 > 0.00018
        && live_ret_3 > 0.00005
        && ret_1 > 0.00005
        && live_reclaiming;
    let top_reversal_setup = upper_extreme > 0.55
        && (cycle.return_from_open > 0.00120 || ret_3 > 0.00100 || ret_5 > 0.00120)
        && live_ret_1 < -0.00018
        && live_ret_3 < -0.00005
        && ret_1 < -0.00005
        && live_rejecting;
    let momentum_score = live_ret_1 * 1450.0
        + live_ret_3 * 900.0
        + ret_1 * 950.0
        + ret_3 * 700.0
        + ret_5 * 520.0
        + ret_10 * 320.0
        + ret_30 * 120.0;
    let trend_score = ratio_delta(ema5, ema5_prev) * 1200.0
        + ratio_delta(ema10, ema10_prev) * 900.0
        + ratio_delta(ema5, ema20) * 220.0
        + if ema5 > ema10 && ema10 > ema20 {
            0.45
        } else if ema5 < ema10 && ema10 < ema20 {
            -0.45
        } else {
            0.0
        };
    let volume_score = clamp(volume_z, -2.5, 2.5) * sign(ret_1 + ret_3) * 0.28;
    let mean_reversion_score = lower_extreme * (0.75 + down_move_intensity * 0.85)
        - upper_extreme * (0.75 + up_move_intensity * 0.85);
    let candle_score = body_ratio * sign(latest.close - latest.open) * 0.22;
    let live_score = ratio_delta(snapshot.latest_price, ema5) * 260.0;
    let mark_index_basis = snapshot
        .latest_mark_price
        .map(|price| ratio_delta(price, snapshot.latest_price))
        .unwrap_or(0.0);
    let trade_index_basis = snapshot
        .latest_trade_price
        .map(|price| ratio_delta(price, snapshot.latest_price))
        .unwrap_or(0.0);
    let futures_kline_index_basis = snapshot
        .latest_futures_kline_price
        .map(|price| ratio_delta(price, snapshot.latest_price))
        .unwrap_or(0.0);
    let cycle_score = cycle.return_from_open * 520.0;
    let basis_score = clamp(
        mark_index_basis * 800.0 + trade_index_basis * 500.0,
        -0.8,
        0.8,
    );
    let continuation_score = momentum_score
        + trend_score
        + volume_score
        + mean_reversion_score
        + candle_score
        + live_score
        + cycle_score
        + basis_score;

    let up_continuation_alive =
        cycle.return_from_open > 0.00120 && ret_3 > 0.00025 && ret_5 > 0.00055;
    let down_continuation_alive =
        cycle.return_from_open < -0.00120 && ret_3 < -0.00025 && ret_5 < -0.00055;
    let top_trend_damage = continuation_score < -1.25
        && momentum_score < -0.40
        && (ret_1 < -0.00020 || live_ret_3 < -0.00045);
    let bottom_trend_damage = continuation_score > 1.25
        && momentum_score > 0.40
        && (ret_1 > 0.00020 || live_ret_3 > 0.00045);
    let top_reversal_confirmed = top_reversal_setup && top_trend_damage && !up_continuation_alive;
    let bottom_reversal_confirmed =
        bottom_reversal_setup && bottom_trend_damage && !down_continuation_alive;
    let bottom_reversal_bias = if bottom_reversal_confirmed {
        5.4 + lower_extreme * 0.95
            + clamp(live_ret_1 * 4500.0, 0.0, 1.6)
            + clamp(
                (-cycle.return_from_open).max(-ret_3).max(-ret_5) * 750.0,
                0.0,
                1.2,
            )
    } else {
        0.0
    };
    let top_reversal_bias = if top_reversal_confirmed {
        -5.4 - upper_extreme * 0.95
            - clamp((-live_ret_1) * 4500.0, 0.0, 1.6)
            - clamp(
                cycle.return_from_open.max(ret_3).max(ret_5) * 750.0,
                0.0,
                1.2,
            )
    } else {
        0.0
    };
    let bottom_reversal_signal_score = continuation_score + bottom_reversal_bias;
    let top_reversal_signal_score = continuation_score + top_reversal_bias;
    let (score, reversal_mode) =
        if bottom_reversal_confirmed && bottom_reversal_signal_score >= config.up_threshold {
            (bottom_reversal_signal_score, "bottom_reversal")
        } else if top_reversal_confirmed && top_reversal_signal_score <= config.down_threshold {
            (top_reversal_signal_score, "top_reversal")
        } else {
            (continuation_score, "none")
        };

    let period_seconds = f64::from(DEFAULT_PERIOD_SECONDS);
    let elapsed_seconds = cycle.progress * period_seconds;
    let remaining_seconds = (period_seconds - elapsed_seconds).clamp(1.0, period_seconds);
    let seconds_to_expiry = period_seconds;
    let drift_return = clamp(score / 10_000.0, -0.0020, 0.0020);
    let expected_expiry_return = drift_return;
    let minute_volatility = volatility.max(config.volatility_floor);
    let live_volatility =
        (live_ret_1.abs() * 0.85 + live_ret_3.abs() * 0.55 + ret_1.abs() * 0.35).max(0.00006);
    let expiry_volatility =
        (minute_volatility * (seconds_to_expiry / 60.0).sqrt()).max(live_volatility * 1.8);
    let z_to_reference = expected_expiry_return / expiry_volatility.max(0.00005);
    let up_probability = normal_cdf(z_to_reference);
    let down_probability = 1.0 - up_probability;
    let edge_probability = up_probability.max(down_probability);
    let probability_score = (edge_probability - 0.5)
        * 20.0
        * if up_probability >= down_probability {
            1.0
        } else {
            -1.0
        };
    let weight_total = (config.technical_weight + config.probability_weight).max(f64::EPSILON);
    let technical_weight = config.technical_weight / weight_total;
    let probability_weight = config.probability_weight / weight_total;
    let model_score = clamp(
        score * technical_weight + probability_score * probability_weight,
        -12.0,
        12.0,
    );
    let directional_probability = if model_score >= 0.0 {
        up_probability
    } else {
        down_probability
    };
    let base_confidence = clamp(directional_probability, 0.0, 0.99);
    let predicted_return = clamp(expected_expiry_return, -0.003, 0.003);
    let market_regime = classify_prediction_regime(
        cycle.return_from_open,
        ret_3,
        ret_5,
        live_ret_1,
        live_ret_3,
        boll_width,
        volatility,
        mark_index_basis,
        trade_index_basis,
    );
    let candidate = prediction_candidate(
        model_score,
        &market_regime,
        upper_extreme,
        lower_extreme,
        up_move_intensity,
        down_move_intensity,
        live_ret_1,
        live_ret_3,
        ret_1,
        ret_3,
        ret_5,
        cycle.return_from_open,
        up_probability,
        down_probability,
        up_structure,
        down_structure,
        upper_wick_ratio,
        lower_wick_ratio,
        boll_pos,
    );
    let effective_min_confidence = candidate
        .as_ref()
        .map(|candidate| {
            config
                .min_confidence
                .max(prediction_candidate_min_confidence(candidate))
        })
        .unwrap_or(config.min_confidence);
    let mut direction = candidate
        .as_ref()
        .map(|candidate| candidate.direction.clone())
        .unwrap_or_else(|| "HOLD".to_string());
    let confidence = candidate
        .as_ref()
        .map(|candidate| candidate.raw_confidence)
        .unwrap_or(base_confidence);
    let mut reason = match candidate.as_ref() {
        Some(candidate) => format!(
            "candidate {} mode {} rule {} score {:.4} raw_confidence {:.4}",
            candidate.direction, candidate.mode, candidate.rule, model_score, candidate.raw_confidence
        ),
        None => format!(
            "hold no candidate mode {market_regime} score {model_score:.4} raw_confidence {base_confidence:.4}"
        ),
    };
    if age_seconds > config.stale_seconds {
        direction = "HOLD".to_string();
        reason = format!("hold stale data age {age_seconds}s");
    } else if confidence < effective_min_confidence {
        direction = "HOLD".to_string();
        reason = format!("hold low raw confidence {confidence:.4} < {effective_min_confidence:.4}");
    } else if volatility < 0.00012 {
        direction = "HOLD".to_string();
        reason = format!("hold low volatility {volatility:.6}");
    } else if mark_index_basis.abs() > 0.0008 || trade_index_basis.abs() > 0.0012 {
        direction = "HOLD".to_string();
        reason = format!(
            "hold basis abnormal mark_index {mark_index_basis:.6} trade_index {trade_index_basis:.6}"
        );
    }
    let pre_filter_passed = matches!(direction.as_str(), "UP" | "DOWN");
    let pre_calibration_direction = direction.clone();
    Ok(PredictionDecision {
        direction,
        confidence,
        predicted_return,
        entry_price: snapshot.latest_price,
        score: model_score,
        signal_cooldown_seconds: config.signal_cooldown_seconds,
        market_source: snapshot.latest_source.clone(),
        market_age_seconds: age_seconds,
        market_candles: candles.len(),
        reason,
        feature_snapshot: json!({
            "market": {
                "source": snapshot.latest_source,
                "latest_mark_price": snapshot.latest_mark_price,
                "latest_trade_price": snapshot.latest_trade_price,
                "latest_futures_kline_price": snapshot.latest_futures_kline_price,
                "last_message_at": snapshot.last_message_at,
                "latest_price_time_ms": snapshot.latest_price_time_ms,
                "latest_candle_close_time_ms": latest.close_time_ms,
                "age_seconds": age_seconds
            },
            "basis": {
                "mark_index_basis": mark_index_basis,
                "trade_index_basis": trade_index_basis,
                "futures_kline_index_basis": futures_kline_index_basis,
                "basis_score": basis_score
            },
            "config": {
                "model_version": PREDICTION_MODEL_VERSION,
                "interval_seconds": config.interval_seconds,
                "candle_limit": config.candle_limit,
                "stale_seconds": config.stale_seconds,
                "up_threshold": config.up_threshold,
                "down_threshold": config.down_threshold,
                "min_confidence": config.min_confidence,
                "effective_min_confidence": effective_min_confidence,
                "technical_weight": config.technical_weight,
                "probability_weight": config.probability_weight,
                "volatility_floor": config.volatility_floor,
                "signal_cooldown_seconds": config.signal_cooldown_seconds,
                "calibration_lookback_minutes": config.calibration_lookback_minutes,
                "calibration_min_samples": config.calibration_min_samples,
                "calibration_min_win_rate": config.calibration_min_win_rate,
                "calibration_min_ev_usdt": config.calibration_min_ev_usdt,
                "calibration_sample_seconds": config.calibration_sample_seconds
            },
            "rule_params": {
                "hard_filters": {
                    "low_volatility": 0.00012,
                    "max_mark_index_basis_abs": 0.0008,
                    "max_trade_index_basis_abs": 0.0012
                },
                "regime": {
                    "quiet_volatility": 0.00012,
                    "quiet_boll_width": 0.00045,
                    "impulse_cycle_ret": 0.00120,
                    "impulse_ret_3": 0.00035,
                    "impulse_live_ret_3": 0.00010,
                    "trend_ret_5": 0.00070,
                    "trend_ret_3": 0.00025,
                    "trend_live_ret_1": 0.00008
                },
                "candidate": {
                    "mean_reversion_down_min_probability": PREDICTION_MEAN_REVERSION_DOWN_MIN_PROBABILITY,
                    "mean_reversion_down_min_structure": PREDICTION_MEAN_REVERSION_DOWN_MIN_STRUCTURE,
                    "mean_reversion_up_min_probability": PREDICTION_MEAN_REVERSION_UP_MIN_PROBABILITY,
                    "mean_reversion_up_min_structure": PREDICTION_MEAN_REVERSION_UP_MIN_STRUCTURE,
                    "trend_down_min_probability": PREDICTION_TREND_DOWN_MIN_PROBABILITY,
                    "trend_down_min_structure": PREDICTION_TREND_DOWN_MIN_STRUCTURE,
                    "trend_up_min_probability": PREDICTION_TREND_UP_MIN_PROBABILITY,
                    "trend_up_min_structure": PREDICTION_TREND_UP_MIN_STRUCTURE,
                    "impulse_down_min_probability": PREDICTION_IMPULSE_DOWN_MIN_PROBABILITY,
                    "impulse_down_min_structure": PREDICTION_IMPULSE_DOWN_MIN_STRUCTURE,
                    "impulse_up_min_probability": PREDICTION_IMPULSE_UP_MIN_PROBABILITY,
                    "impulse_up_min_structure": PREDICTION_IMPULSE_UP_MIN_STRUCTURE,
                    "reversal_up_min_setup_confidence": PREDICTION_REVERSAL_UP_MIN_SETUP_CONFIDENCE,
                    "reversal_up_min_structure": PREDICTION_REVERSAL_UP_MIN_STRUCTURE,
                    "reversal_down_min_setup_confidence": PREDICTION_REVERSAL_DOWN_MIN_SETUP_CONFIDENCE,
                    "reversal_down_min_structure": PREDICTION_REVERSAL_DOWN_MIN_STRUCTURE,
                    "reversal_min_wick": PREDICTION_REVERSAL_MIN_WICK,
                    "trend_pullback_min_wick": PREDICTION_TREND_PULLBACK_MIN_WICK,
                    "impulse_confirm_min_wick": PREDICTION_IMPULSE_CONFIRM_MIN_WICK,
                    "mean_reversion_down_min_boll_pos": PREDICTION_MEAN_REVERSION_DOWN_MIN_BOLL_POS,
                    "mean_reversion_up_max_boll_pos": PREDICTION_MEAN_REVERSION_UP_MAX_BOLL_POS,
                    "up_falling_knife_min_body_ratio": PREDICTION_UP_FALLING_KNIFE_MIN_BODY_RATIO,
                    "up_falling_knife_max_lower_wick": PREDICTION_UP_FALLING_KNIFE_MAX_LOWER_WICK,
                    "confirmation_seconds": PREDICTION_CONFIRMATION_SECONDS,
                    "confirmation_max_adverse_ret": PREDICTION_CONFIRMATION_MAX_ADVERSE_RET
                },
                "payout": {
                    "stake_usdt": MIN_EVENT_STAKE,
                    "payout_ratio": DEFAULT_PAYOUT_RATIO,
                    "profit_share_rate": PROFIT_SHARE_RATE,
                    "loss_rebate_rate": PROFIT_SHARE_RATE,
                    "win_net_usdt": MIN_EVENT_STAKE * DEFAULT_PAYOUT_RATIO * (Decimal::ONE - PROFIT_SHARE_RATE),
                    "loss_net_usdt": MIN_EVENT_STAKE * (Decimal::ONE - PROFIT_SHARE_RATE)
                }
            },
            "scores": {
                "model_score": model_score,
                "raw_confidence": base_confidence,
                "directional_probability": directional_probability,
                "technical_score": score,
                "probability_score": probability_score,
                "technical_weight": technical_weight,
                "probability_weight": probability_weight,
                "continuation_score": continuation_score,
                "momentum_score": momentum_score,
                "trend_score": trend_score,
                "volume_score": volume_score,
                "mean_reversion_score": mean_reversion_score,
                "candle_score": candle_score,
                "live_score": live_score,
                "cycle_score": cycle_score
            },
            "structure": {
                "up_score": up_structure,
                "down_score": down_structure,
                "body_ratio": body_ratio,
                "close_position": close_position,
                "upper_wick_ratio": upper_wick_ratio,
                "lower_wick_ratio": lower_wick_ratio
            },
            "decision": {
                "model_version": PREDICTION_MODEL_VERSION,
                "regime": market_regime,
                "candidate_direction": candidate.as_ref().map(|candidate| candidate.direction.as_str()),
                "candidate_mode": candidate.as_ref().map(|candidate| candidate.mode.as_str()),
                "candidate_rule": candidate.as_ref().map(|candidate| candidate.rule.as_str()),
                "pre_filter_passed": pre_filter_passed,
                "pre_calibration_direction": pre_calibration_direction,
                "calibration_required": true
            },
            "reversal": {
                "mode": reversal_mode,
                "upper_extreme": upper_extreme,
                "lower_extreme": lower_extreme,
                "up_move_intensity": up_move_intensity,
                "down_move_intensity": down_move_intensity,
                "bottom_setup": bottom_reversal_setup,
                "top_setup": top_reversal_setup,
                "bottom_trend_damage": bottom_trend_damage,
                "top_trend_damage": top_trend_damage,
                "up_continuation_alive": up_continuation_alive,
                "down_continuation_alive": down_continuation_alive,
                "bottom_confirmed": bottom_reversal_confirmed,
                "top_confirmed": top_reversal_confirmed,
                "bottom_signal_score": bottom_reversal_signal_score,
                "top_signal_score": top_reversal_signal_score,
                "bottom_bias": bottom_reversal_bias,
                "top_bias": top_reversal_bias
            },
            "cycle": {
                "start_ms": cycle.start_ms,
                "progress": cycle.progress,
                "elapsed_seconds": elapsed_seconds,
                "remaining_seconds": remaining_seconds,
                "open_price": cycle.open_price,
                "return_from_open": cycle.return_from_open
            },
            "expiry_model": {
                "strike_reference": "entry_price",
                "up_probability": up_probability,
                "down_probability": down_probability,
                "edge_probability": edge_probability,
                "z_to_reference": z_to_reference,
                "drift_return": drift_return,
                "expected_expiry_return": expected_expiry_return,
                "minute_volatility": minute_volatility,
                "live_volatility": live_volatility,
                "expiry_volatility": expiry_volatility,
                "volatility_floor": config.volatility_floor
            },
            "returns": {
                "live_ret_1": live_ret_1,
                "live_ret_3": live_ret_3,
                "closed_ret_1": closed_ret_1,
                "closed_ret_3": closed_ret_3,
                "ret_1": ret_1,
                "ret_3": ret_3,
                "ret_5": ret_5,
                "ret_10": ret_10,
                "ret_30": ret_30
            },
            "shape": {
                "boll_pos": boll_pos,
                "boll_width": boll_width,
                "volume_z": volume_z,
                "volatility": volatility,
                "body_ratio": body_ratio
            }
        }),
    })
}

#[allow(clippy::too_many_arguments)]
fn classify_prediction_regime(
    cycle_ret: f64,
    ret_3: f64,
    ret_5: f64,
    live_ret_1: f64,
    live_ret_3: f64,
    boll_width: f64,
    volatility: f64,
    mark_index_basis: f64,
    trade_index_basis: f64,
) -> String {
    if mark_index_basis.abs() > 0.0008 || trade_index_basis.abs() > 0.0012 {
        return "chaos".to_string();
    }
    if volatility < 0.00012 || boll_width < 0.00045 {
        return "quiet".to_string();
    }
    let impulse_up = cycle_ret > 0.00120 && ret_3 > 0.00035 && live_ret_3 > 0.00010;
    let impulse_down = cycle_ret < -0.00120 && ret_3 < -0.00035 && live_ret_3 < -0.00010;
    if impulse_up || impulse_down {
        return "impulse".to_string();
    }
    let trend_up = ret_5 > 0.00070 && ret_3 > 0.00025 && live_ret_1 > -0.00008;
    let trend_down = ret_5 < -0.00070 && ret_3 < -0.00025 && live_ret_1 < 0.00008;
    if trend_up {
        "trend_up".to_string()
    } else if trend_down {
        "trend_down".to_string()
    } else {
        "mean_reversion".to_string()
    }
}

fn make_prediction_candidate(
    direction: &str,
    mode: &str,
    rule: &str,
    raw_confidence: f64,
    structure_score: f64,
) -> PredictionCandidate {
    PredictionCandidate {
        direction: direction.to_string(),
        mode: mode.to_string(),
        rule: rule.to_string(),
        raw_confidence,
        structure_score,
    }
}

#[allow(clippy::too_many_arguments)]
fn prediction_candidate(
    _model_score: f64,
    market_regime: &str,
    upper_extreme: f64,
    lower_extreme: f64,
    _up_move_intensity: f64,
    _down_move_intensity: f64,
    live_ret_1: f64,
    live_ret_3: f64,
    _ret_1: f64,
    ret_3: f64,
    _ret_5: f64,
    cycle_ret: f64,
    up_probability: f64,
    down_probability: f64,
    up_structure: f64,
    down_structure: f64,
    upper_wick_ratio: f64,
    lower_wick_ratio: f64,
    boll_pos: f64,
) -> Option<PredictionCandidate> {
    if market_regime == "impulse"
        && lower_extreme >= 0.70
        && cycle_ret <= -0.0015
        && live_ret_3 <= -0.0008
        && live_ret_1 >= -0.00010
        && lower_wick_ratio >= PREDICTION_REVERSAL_MIN_WICK
        && up_structure >= PREDICTION_REVERSAL_UP_MIN_STRUCTURE
    {
        let setup_confidence = up_probability.max(0.62);
        return Some(make_prediction_candidate(
            "UP",
            "reversal_edge",
            "reversal_up_from_impulse_down",
            setup_confidence,
            up_structure,
        ));
    }

    if market_regime == "impulse"
        && upper_extreme >= 0.70
        && cycle_ret >= 0.0015
        && live_ret_3 >= 0.0008
        && live_ret_1 <= 0.00010
        && upper_wick_ratio >= PREDICTION_REVERSAL_MIN_WICK
        && down_structure >= PREDICTION_REVERSAL_DOWN_MIN_STRUCTURE
    {
        let setup_confidence = down_probability.max(0.62);
        return Some(make_prediction_candidate(
            "DOWN",
            "reversal_edge",
            "reversal_down_from_impulse_up",
            setup_confidence,
            down_structure,
        ));
    }

    match market_regime {
        "mean_reversion"
            if down_probability >= PREDICTION_MEAN_REVERSION_DOWN_MIN_PROBABILITY
                && down_structure >= PREDICTION_MEAN_REVERSION_DOWN_MIN_STRUCTURE
                && live_ret_1 <= 0.0 =>
        {
            if boll_pos >= PREDICTION_MEAN_REVERSION_DOWN_MIN_BOLL_POS
                || upper_wick_ratio >= PREDICTION_TREND_PULLBACK_MIN_WICK
            {
                Some(make_prediction_candidate(
                    "DOWN",
                    "structure_edge",
                    "mean_reversion_down",
                    down_probability,
                    down_structure,
                ))
            } else {
                None
            }
        }
        "mean_reversion"
            if up_probability >= PREDICTION_MEAN_REVERSION_UP_MIN_PROBABILITY
                && up_structure >= PREDICTION_MEAN_REVERSION_UP_MIN_STRUCTURE
                && live_ret_1 >= 0.0 =>
        {
            if boll_pos <= PREDICTION_MEAN_REVERSION_UP_MAX_BOLL_POS
                || lower_wick_ratio >= PREDICTION_TREND_PULLBACK_MIN_WICK
            {
                Some(make_prediction_candidate(
                    "UP",
                    "structure_edge",
                    "mean_reversion_up",
                    up_probability,
                    up_structure,
                ))
            } else {
                None
            }
        }
        "trend_down"
            if down_probability >= PREDICTION_TREND_DOWN_MIN_PROBABILITY
                && down_structure >= PREDICTION_TREND_DOWN_MIN_STRUCTURE
                && live_ret_3 <= 0.0
                && lower_extreme < 0.75
                && upper_wick_ratio >= PREDICTION_TREND_PULLBACK_MIN_WICK =>
        {
            Some(make_prediction_candidate(
                "DOWN",
                "structure_edge",
                "trend_down_down",
                down_probability,
                down_structure,
            ))
        }
        "trend_up"
            if up_probability >= PREDICTION_TREND_UP_MIN_PROBABILITY
                && up_structure >= PREDICTION_TREND_UP_MIN_STRUCTURE
                && live_ret_3 >= 0.0
                && upper_extreme < 0.75
                && lower_wick_ratio >= PREDICTION_TREND_PULLBACK_MIN_WICK =>
        {
            Some(make_prediction_candidate(
                "UP",
                "structure_edge",
                "trend_up_up",
                up_probability,
                up_structure,
            ))
        }
        "impulse"
            if down_probability >= PREDICTION_IMPULSE_DOWN_MIN_PROBABILITY
                && down_structure >= PREDICTION_IMPULSE_DOWN_MIN_STRUCTURE
                && lower_extreme < 0.35
                && boll_pos >= 0.18
                && live_ret_1 <= -0.00010
                && live_ret_3 <= -0.00025
                && ret_3 <= -0.00025
                && upper_wick_ratio >= PREDICTION_IMPULSE_CONFIRM_MIN_WICK =>
        {
            Some(make_prediction_candidate(
                "DOWN",
                "structure_edge",
                "impulse_down",
                down_probability,
                down_structure,
            ))
        }
        "impulse"
            if up_probability >= PREDICTION_IMPULSE_UP_MIN_PROBABILITY
                && up_structure >= PREDICTION_IMPULSE_UP_MIN_STRUCTURE
                && upper_extreme < 0.35
                && boll_pos <= 0.82
                && live_ret_1 >= 0.00010
                && live_ret_3 >= 0.00025
                && ret_3 >= 0.00025
                && lower_wick_ratio >= PREDICTION_IMPULSE_CONFIRM_MIN_WICK =>
        {
            Some(make_prediction_candidate(
                "UP",
                "structure_edge",
                "impulse_up",
                up_probability,
                up_structure,
            ))
        }
        _ => None,
    }
}

fn prediction_candidate_min_confidence(candidate: &PredictionCandidate) -> f64 {
    match candidate.rule.as_str() {
        EVENT_ML_TWO_STAGE_RULE
        | EVENT_ML_TWO_STAGE_REVERSE_RULE
        | EVENT_ML_TWO_STAGE_DOWNSHIFT_RULE => 0.58,
        EVENT_ML_SLOW_REVERSAL_RULE => EVENT_ML_SLOW_REVERSAL_MIN_CONFIDENCE,
        "reversal_up_from_impulse_down" => PREDICTION_REVERSAL_UP_MIN_SETUP_CONFIDENCE,
        "reversal_down_from_impulse_up" => PREDICTION_REVERSAL_DOWN_MIN_SETUP_CONFIDENCE,
        "mean_reversion_down" => PREDICTION_MEAN_REVERSION_DOWN_MIN_PROBABILITY,
        "mean_reversion_up" => PREDICTION_MEAN_REVERSION_UP_MIN_PROBABILITY,
        "trend_down_down" => PREDICTION_TREND_DOWN_MIN_PROBABILITY,
        "trend_up_up" => PREDICTION_TREND_UP_MIN_PROBABILITY,
        "impulse_down" => PREDICTION_IMPULSE_DOWN_MIN_PROBABILITY,
        "impulse_up" => PREDICTION_IMPULSE_UP_MIN_PROBABILITY,
        _ => PREDICTION_MIN_RAW_CONFIDENCE,
    }
}

async fn apply_prediction_calibration(
    pool: &PgPool,
    config: &PredictionConfig,
    decision: &mut PredictionDecision,
) -> Result<(), AppError> {
    if !matches!(decision.direction.as_str(), "UP" | "DOWN") {
        let calibration = PredictionCalibration::disabled(config);
        merge_prediction_calibration(decision, &calibration, None);
        return Ok(());
    }
    let candidate = prediction_candidate_from_snapshot(&decision.feature_snapshot);
    let Some(candidate) = candidate else {
        let calibration = PredictionCalibration::disabled(config);
        merge_prediction_calibration(decision, &calibration, None);
        return Ok(());
    };
    let min_raw_confidence = prediction_candidate_min_confidence(&candidate);
    if candidate.raw_confidence < min_raw_confidence {
        let mut calibration = PredictionCalibration::disabled(config);
        calibration.reason = format!(
            "hold candidate direction confidence {:.4} < {:.4} mode {} rule {} direction {} structure {:.4}",
            candidate.raw_confidence,
            min_raw_confidence,
            candidate.mode,
            candidate.rule,
            candidate.direction,
            candidate.structure_score
        );
        merge_prediction_calibration(decision, &calibration, Some(&candidate));
        return Ok(());
    }
    if prediction_decision_uses_ml(decision) {
        let calibration = PredictionCalibration::forced_allowed(
            config,
            format!(
                "forced event-ml-v3 meta policy mode {} rule {} direction {} raw {:.4}",
                candidate.mode, candidate.rule, candidate.direction, candidate.raw_confidence
            ),
            candidate.raw_confidence,
        );
        merge_prediction_calibration(decision, &calibration, Some(&candidate));
        return Ok(());
    }
    let calibration = db_prediction_calibration(pool, config, &candidate).await?;
    merge_prediction_calibration(decision, &calibration, Some(&candidate));
    Ok(())
}

fn prediction_candidate_from_snapshot(snapshot: &serde_json::Value) -> Option<PredictionCandidate> {
    let decision = snapshot.get("decision")?;
    let direction = decision.get("candidate_direction")?.as_str()?.to_string();
    let mode = decision.get("candidate_mode")?.as_str()?.to_string();
    let rule = decision.get("candidate_rule")?.as_str()?.to_string();
    let raw_confidence = snapshot
        .get("scores")
        .and_then(|scores| scores.get("raw_confidence"))
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0);
    let structure_score = match direction.as_str() {
        "UP" => snapshot
            .get("structure")
            .and_then(|structure| structure.get("up_score"))
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0),
        "DOWN" => snapshot
            .get("structure")
            .and_then(|structure| structure.get("down_score"))
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0),
        _ => 0.0,
    };
    Some(PredictionCandidate {
        direction,
        mode,
        rule,
        raw_confidence,
        structure_score,
    })
}

async fn clear_pending_prediction_candidate(state: &SharedState) {
    let mut pending = state.prediction_pending_candidate.write().await;
    *pending = None;
}

async fn confirm_prediction_candidate(
    state: &SharedState,
    decision: &PredictionDecision,
) -> PredictionConfirmation {
    let Some(candidate) = prediction_candidate_from_snapshot(&decision.feature_snapshot) else {
        clear_pending_prediction_candidate(state).await;
        return PredictionConfirmation::Rejected("hold no active prediction candidate".to_string());
    };
    if candidate.direction != decision.direction {
        clear_pending_prediction_candidate(state).await;
        return PredictionConfirmation::Rejected(format!(
            "hold candidate direction mismatch {} != {}",
            candidate.direction, decision.direction
        ));
    }
    let min_raw_confidence = prediction_candidate_min_confidence(&candidate);
    if candidate.raw_confidence < min_raw_confidence {
        clear_pending_prediction_candidate(state).await;
        return PredictionConfirmation::Rejected(format!(
            "hold candidate raw confidence {:.4} < {:.4}",
            candidate.raw_confidence, min_raw_confidence
        ));
    }
    if !prediction_decision_uses_ml(decision) {
        if let Some(reason) =
            prediction_candidate_hard_filter_reason(&decision.feature_snapshot, &candidate)
        {
            clear_pending_prediction_candidate(state).await;
            return PredictionConfirmation::Rejected(reason);
        }
    }

    let now = Utc::now();
    let mut pending = state.prediction_pending_candidate.write().await;
    let changed = pending
        .as_ref()
        .map(|pending| pending.direction != candidate.direction || pending.rule != candidate.rule)
        .unwrap_or(true);
    if changed {
        *pending = Some(PendingPredictionCandidate {
            direction: candidate.direction.clone(),
            rule: candidate.rule.clone(),
            first_seen_at: now,
            entry_price: decision.entry_price,
            raw_confidence: candidate.raw_confidence,
            structure_score: candidate.structure_score,
        });
        return PredictionConfirmation::Waiting(format!(
            "confirming new candidate {} rule {} 0/{}s raw_confidence {:.4} structure {:.4}",
            candidate.direction,
            candidate.rule,
            PREDICTION_CONFIRMATION_SECONDS,
            candidate.raw_confidence,
            candidate.structure_score
        ));
    }

    let Some(pending_candidate) = pending.as_mut() else {
        return PredictionConfirmation::Waiting("confirming candidate".to_string());
    };
    pending_candidate.raw_confidence = candidate.raw_confidence;
    pending_candidate.structure_score = candidate.structure_score;

    let adverse_return = prediction_confirmation_adverse_return(
        pending_candidate.entry_price,
        decision.entry_price,
        candidate.direction.as_str(),
    );
    if adverse_return > PREDICTION_CONFIRMATION_MAX_ADVERSE_RET {
        let message = format!(
            "hold confirmation adverse {:.6} > {:.6} direction {} rule {}",
            adverse_return,
            PREDICTION_CONFIRMATION_MAX_ADVERSE_RET,
            candidate.direction,
            candidate.rule
        );
        *pending = None;
        return PredictionConfirmation::Rejected(message);
    }

    let elapsed_seconds = (now - pending_candidate.first_seen_at).num_seconds();
    if elapsed_seconds < PREDICTION_CONFIRMATION_SECONDS {
        return PredictionConfirmation::Waiting(format!(
            "confirming candidate {} rule {} {}/{}s adverse {:.6} raw_confidence {:.4} structure {:.4}",
            candidate.direction,
            candidate.rule,
            elapsed_seconds.max(0),
            PREDICTION_CONFIRMATION_SECONDS,
            adverse_return,
            candidate.raw_confidence,
            candidate.structure_score
        ));
    }

    *pending = None;
    PredictionConfirmation::Confirmed
}

fn prediction_confirmation_adverse_return(
    initial_price: f64,
    current_price: f64,
    direction: &str,
) -> f64 {
    if initial_price <= f64::EPSILON || current_price <= f64::EPSILON {
        return 0.0;
    }
    match direction {
        "UP" => ((initial_price - current_price) / initial_price).max(0.0),
        "DOWN" => ((current_price - initial_price) / initial_price).max(0.0),
        _ => 0.0,
    }
}

fn prediction_candidate_hard_filter_reason(
    snapshot: &serde_json::Value,
    candidate: &PredictionCandidate,
) -> Option<String> {
    let boll_pos = prediction_snapshot_f64(snapshot, "shape", "boll_pos");
    let upper_wick_ratio = prediction_snapshot_f64(snapshot, "structure", "upper_wick_ratio");
    let lower_wick_ratio = prediction_snapshot_f64(snapshot, "structure", "lower_wick_ratio");
    let body_ratio = prediction_snapshot_f64(snapshot, "structure", "body_ratio");
    let live_ret_1 = prediction_snapshot_f64(snapshot, "returns", "live_ret_1");
    let live_ret_3 = prediction_snapshot_f64(snapshot, "returns", "live_ret_3");
    let ret_3 = prediction_snapshot_f64(snapshot, "returns", "ret_3");
    let cycle_ret = prediction_snapshot_f64(snapshot, "cycle", "return_from_open");
    let noisy_wick = upper_wick_ratio >= 0.20 || lower_wick_ratio >= 0.20;
    if candidate.rule == EVENT_ML_SLOW_REVERSAL_RULE {
        if let Some(reason) = slow_reversal_soft_filter_reason(snapshot, candidate) {
            return Some(reason);
        }
    }

    match candidate.direction.as_str() {
        "UP" => {
            if body_ratio > PREDICTION_UP_FALLING_KNIFE_MIN_BODY_RATIO
                && lower_wick_ratio < PREDICTION_UP_FALLING_KNIFE_MAX_LOWER_WICK
            {
                return Some(format!(
                    "hold up falling knife body {:.3} lower_wick {:.3} live_ret_1 {:.6}",
                    body_ratio, lower_wick_ratio, live_ret_1
                ));
            }
            let high_chase = boll_pos >= 0.90
                && (candidate.rule.contains("impulse") || cycle_ret >= 0.0010 || ret_3 >= 0.0010);
            let weak_continuation = live_ret_1 < 0.00010 || live_ret_3 < 0.00015;
            if high_chase && weak_continuation && noisy_wick {
                return Some(format!(
                    "hold high chase weak continuation boll_pos {:.4} live_ret_1 {:.6} live_ret_3 {:.6} wick u {:.3} l {:.3}",
                    boll_pos,
                    live_ret_1,
                    live_ret_3,
                    upper_wick_ratio,
                    lower_wick_ratio
                ));
            }
            if high_chase && live_ret_1 <= 0.0 {
                return Some(format!(
                    "hold high chase no live continuation boll_pos {:.4} live_ret_1 {:.6}",
                    boll_pos, live_ret_1
                ));
            }
            if boll_pos <= 0.10 && live_ret_1 < 0.00005 && lower_wick_ratio < 0.18 {
                return Some(format!(
                    "hold bottom reversal not confirmed boll_pos {:.4} live_ret_1 {:.6} lower_wick {:.3}",
                    boll_pos, live_ret_1, lower_wick_ratio
                ));
            }
        }
        "DOWN" => {
            let low_chase = boll_pos <= 0.10
                && (candidate.rule.contains("impulse") || cycle_ret <= -0.0010 || ret_3 <= -0.0010);
            let weak_continuation = live_ret_1 > -0.00010 || live_ret_3 > -0.00015;
            if low_chase && weak_continuation && noisy_wick {
                return Some(format!(
                    "hold low chase weak continuation boll_pos {:.4} live_ret_1 {:.6} live_ret_3 {:.6} wick u {:.3} l {:.3}",
                    boll_pos,
                    live_ret_1,
                    live_ret_3,
                    upper_wick_ratio,
                    lower_wick_ratio
                ));
            }
            if low_chase && live_ret_1 >= 0.0 {
                return Some(format!(
                    "hold low chase no live continuation boll_pos {:.4} live_ret_1 {:.6}",
                    boll_pos, live_ret_1
                ));
            }
            if boll_pos >= 0.90 && live_ret_1 > -0.00005 && upper_wick_ratio < 0.18 {
                return Some(format!(
                    "hold top reversal not confirmed boll_pos {:.4} live_ret_1 {:.6} upper_wick {:.3}",
                    boll_pos, live_ret_1, upper_wick_ratio
                ));
            }
        }
        _ => {}
    }
    None
}

fn slow_reversal_soft_filter_reason(
    snapshot: &serde_json::Value,
    candidate: &PredictionCandidate,
) -> Option<String> {
    let pos_300 = prediction_snapshot_optional_f64(snapshot, "features", "pos_300")?;
    let body_share =
        prediction_snapshot_optional_f64(snapshot, "features", "candle_body_share_60")?;
    let upper_wick =
        prediction_snapshot_optional_f64(snapshot, "features", "candle_upper_wick_share_60")
            .unwrap_or(0.0);
    let lower_wick =
        prediction_snapshot_optional_f64(snapshot, "features", "candle_lower_wick_share_60")
            .unwrap_or(0.0);
    let wick_share = upper_wick + lower_wick;

    let solid_unfinished_move = body_share >= EVENT_ML_SLOW_REVERSAL_MAX_BODY_SHARE_60
        || wick_share <= EVENT_ML_SLOW_REVERSAL_MIN_WICK_SHARE_60;

    let extreme_against_direction = match candidate.direction.as_str() {
        "UP" => pos_300 <= EVENT_ML_SLOW_REVERSAL_EXTREME_POS_300,
        "DOWN" => pos_300 >= 1.0 - EVENT_ML_SLOW_REVERSAL_EXTREME_POS_300,
        _ => false,
    };

    if extreme_against_direction && solid_unfinished_move {
        return Some(format!(
            "hold slow reversal unfinished extreme pos300 {:.4} body60 {:.3} wick60 {:.3} direction {}",
            pos_300, body_share, wick_share, candidate.direction
        ));
    }

    None
}

fn prediction_snapshot_f64(snapshot: &serde_json::Value, section: &str, key: &str) -> f64 {
    snapshot
        .get(section)
        .and_then(|section| section.get(key))
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
}

fn prediction_snapshot_optional_f64(
    snapshot: &serde_json::Value,
    section: &str,
    key: &str,
) -> Option<f64> {
    snapshot
        .get(section)
        .and_then(|section| section.get(key))
        .and_then(|value| value.as_f64())
}

fn load_event_ml_model() -> Option<EventMlModel> {
    let path =
        env::var("EVENT_ML_MODEL_PATH").unwrap_or_else(|_| DEFAULT_EVENT_ML_MODEL_PATH.to_string());
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<EventMlModel>(&text).ok()
}

fn event_ml_predict(model: &EventMlModel, bars: &[MarketSecondBar]) -> Option<EventMlPrediction> {
    let (features, feature_values) = event_ml_features(bars, &model.feature_names)?;
    let move_probability = model.models.move_model.predict_proba(&feature_values);
    let up_probability = model.models.direction.predict_proba(&feature_values);
    let mut direction = "HOLD".to_string();
    let mut rule = EVENT_ML_TWO_STAGE_RULE.to_string();
    if move_probability >= model.thresholds.move_probability {
        if up_probability >= model.thresholds.direction_probability {
            direction = "UP".to_string();
        } else if up_probability <= 1.0 - model.thresholds.direction_probability {
            direction = "DOWN".to_string();
        }
    }
    if direction == "HOLD" {
        if let Some((reversal_direction, reversal_confidence)) =
            event_ml_slow_reversal_candidate(&features, move_probability)
        {
            direction = reversal_direction;
            rule = EVENT_ML_SLOW_REVERSAL_RULE.to_string();
            let adjusted_up_probability = if direction == "UP" {
                reversal_confidence.max(up_probability)
            } else {
                (1.0 - reversal_confidence).min(up_probability)
            };
            return Some(EventMlPrediction {
                direction,
                rule,
                move_probability,
                up_probability: adjusted_up_probability,
                confidence: reversal_confidence,
                score: event_ml_score(move_probability, adjusted_up_probability),
                filtered_reason: None,
                features,
            });
        }
    }
    let mut filtered_reason = None;
    if direction == "UP" {
        let body = features
            .get("candle_body_share_60")
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0);
        let lower_wick = features
            .get("candle_lower_wick_share_60")
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0);
        if body > model.execution_filter.up_falling_knife_min_body_share_60
            && lower_wick
                < model
                    .execution_filter
                    .up_falling_knife_max_lower_wick_share_60
        {
            filtered_reason = Some(format!(
                "hold ml up falling knife body {:.3} lower_wick {:.3}",
                body, lower_wick
            ));
            direction = "HOLD".to_string();
        }
    }
    let confidence = if direction == "DOWN" {
        1.0 - up_probability
    } else {
        up_probability
    };
    let score = event_ml_score(move_probability, up_probability);
    Some(EventMlPrediction {
        direction,
        rule,
        move_probability,
        up_probability,
        confidence,
        score,
        filtered_reason,
        features,
    })
}

fn apply_event_ml_meta_policy(prediction: &mut EventMlPrediction) {
    apply_event_ml_meta_policy_with_options(
        prediction,
        EventMlMetaPolicyOptions {
            two_stage_enabled: env_bool(
                "EVENT_ML_TWO_STAGE_ENABLED",
                DEFAULT_EVENT_ML_TWO_STAGE_ENABLED,
            ),
            two_stage_downshift_enabled: env_bool(
                "EVENT_ML_TWO_STAGE_DOWNSHIFT_ENABLED",
                DEFAULT_EVENT_ML_TWO_STAGE_DOWNSHIFT_ENABLED,
            ),
            two_stage_reverse_enabled: env_bool(
                "EVENT_ML_TWO_STAGE_REVERSE_ENABLED",
                DEFAULT_EVENT_ML_TWO_STAGE_REVERSE_ENABLED,
            ),
            slow_up_enabled: env_bool("EVENT_ML_SLOW_UP_ENABLED", DEFAULT_EVENT_ML_SLOW_UP_ENABLED),
        },
    );
}

fn apply_event_ml_meta_policy_with_options(
    prediction: &mut EventMlPrediction,
    options: EventMlMetaPolicyOptions,
) {
    if prediction.rule == EVENT_ML_TWO_STAGE_RULE
        && matches!(prediction.direction.as_str(), "UP" | "DOWN")
    {
        let original_direction = prediction.direction.clone();
        if options.two_stage_downshift_enabled
            && original_direction == "UP"
            && event_ml_two_stage_downshift_allowed(prediction)
        {
            prediction.direction = "DOWN".to_string();
            prediction.rule = EVENT_ML_TWO_STAGE_DOWNSHIFT_RULE.to_string();
            prediction.up_probability = 1.0 - prediction.up_probability;
            prediction.confidence = event_ml_direction_confidence(
                prediction.direction.as_str(),
                prediction.up_probability,
                prediction.confidence,
            );
            prediction.score =
                event_ml_score(prediction.move_probability, prediction.up_probability);
            return;
        }
        if original_direction == "UP" {
            if let Some(reason) = event_ml_two_stage_up_filter_reason(prediction) {
                prediction.direction = "HOLD".to_string();
                prediction.filtered_reason = Some(reason);
                prediction.confidence = 0.0;
                return;
            }
        }
        if options.two_stage_enabled {
            return;
        }
        if !options.two_stage_reverse_enabled {
            prediction.direction = "HOLD".to_string();
            prediction.filtered_reason = Some(format!(
                "hold two_stage disabled original_direction {} confidence {:.4}",
                original_direction, prediction.confidence
            ));
            prediction.confidence = 0.0;
            return;
        }
        prediction.direction = if prediction.direction == "UP" {
            "DOWN".to_string()
        } else {
            "UP".to_string()
        };
        prediction.rule = EVENT_ML_TWO_STAGE_REVERSE_RULE.to_string();
        prediction.up_probability = 1.0 - prediction.up_probability;
        prediction.confidence = event_ml_direction_confidence(
            prediction.direction.as_str(),
            prediction.up_probability,
            prediction.confidence,
        );
        prediction.score = event_ml_score(prediction.move_probability, prediction.up_probability);
        if let Some(reason) = event_ml_two_stage_reverse_filter_reason(prediction) {
            prediction.direction = "HOLD".to_string();
            prediction.filtered_reason = Some(reason);
            prediction.confidence = 0.0;
        }
        return;
    }

    if prediction.rule == EVENT_ML_SLOW_REVERSAL_RULE {
        if let Some(reason) =
            event_ml_slow_reversal_meta_filter_reason(prediction, options.slow_up_enabled)
        {
            prediction.direction = "HOLD".to_string();
            prediction.filtered_reason = Some(reason);
            prediction.confidence = 0.0;
        }
    }
}

fn event_ml_two_stage_downshift_allowed(prediction: &EventMlPrediction) -> bool {
    let ret_180 = feature_f64(&prediction.features, "ret_180");
    let close_pos_60 = feature_f64(&prediction.features, "candle_close_pos_60");
    ret_180 < EVENT_ML_TWO_STAGE_DOWNSHIFT_MAX_RET_180
        && close_pos_60 < EVENT_ML_TWO_STAGE_DOWNSHIFT_MAX_CLOSE_POS_60
}

fn event_ml_two_stage_up_filter_reason(prediction: &EventMlPrediction) -> Option<String> {
    let ret_300 = feature_f64(&prediction.features, "ret_300");
    if (EVENT_ML_TWO_STAGE_UP_HOLD_MIN_RET_300..EVENT_ML_TWO_STAGE_UP_HOLD_MAX_RET_300)
        .contains(&ret_300)
    {
        return Some(format!(
            "hold two_stage up medium downtrend ret300 {:.6}",
            ret_300
        ));
    }
    None
}

fn event_ml_direction_confidence(direction: &str, up_probability: f64, fallback: f64) -> f64 {
    match direction {
        "UP" => up_probability,
        "DOWN" => 1.0 - up_probability,
        _ => fallback,
    }
}

fn event_ml_two_stage_reverse_filter_reason(prediction: &EventMlPrediction) -> Option<String> {
    if prediction.direction != "DOWN" {
        return Some(format!(
            "hold two_stage reverse up not supported direction {} confidence {:.4}",
            prediction.direction, prediction.confidence
        ));
    }
    let features = &prediction.features;
    let ret_420 = feature_f64(features, "ret_420");
    let ret_600 = feature_f64(features, "ret_600");
    if ret_420 < EVENT_ML_TWO_STAGE_REVERSE_DOWN_MIN_RET_420
        || ret_600 < EVENT_ML_TWO_STAGE_REVERSE_DOWN_MIN_RET_600
    {
        return Some(format!(
            "hold two_stage reverse down chasing drop ret420 {:.6} ret600 {:.6}",
            ret_420, ret_600
        ));
    }
    None
}

fn event_ml_slow_reversal_meta_filter_reason(
    prediction: &EventMlPrediction,
    slow_up_enabled: bool,
) -> Option<String> {
    let features = &prediction.features;
    let direction = prediction.direction.as_str();
    let ret_60 = feature_f64(features, "ret_60");
    let ret_180 = feature_f64(features, "ret_180");
    let ret_300 = feature_f64(features, "ret_300");
    let ret_420 = feature_f64(features, "ret_420");
    let ret_600 = feature_f64(features, "ret_600");
    let ret_900 = feature_f64(features, "ret_900");
    let pos_420 = feature_f64(features, "pos_420");
    let body = feature_f64(features, "candle_body_share_60");
    let upper_wick = feature_f64(features, "candle_upper_wick_share_60");
    let lower_wick = feature_f64(features, "candle_lower_wick_share_60");
    let wick = upper_wick + lower_wick;

    if direction == "UP" && !slow_up_enabled {
        return Some(format!(
            "hold slow up disabled by 7d full 1s backtest confidence {:.4}",
            prediction.confidence
        ));
    }
    if direction == "UP"
        && (lower_wick < EVENT_ML_SLOW_UP_MIN_LOWER_WICK_SHARE_60 || lower_wick < upper_wick)
    {
        return Some(format!(
            "hold slow up bottom rejection weak lower_wick {:.3} upper_wick {:.3}",
            lower_wick, upper_wick
        ));
    }
    if direction == "UP"
        && (prediction.confidence < EVENT_ML_SLOW_UP_STRONG_MIN_CONFIDENCE
            || ret_300 > EVENT_ML_SLOW_UP_STRONG_MAX_RET_300)
    {
        return Some(format!(
            "hold slow up weak edge confidence {:.4} ret300 {:.6}",
            prediction.confidence, ret_300
        ));
    }

    if direction != "DOWN" {
        return None;
    }
    if pos_420 < EVENT_ML_SLOW_DOWN_MIN_POS_420 {
        return Some(format!(
            "hold slow down not extreme pos420 {:.3} < {:.3}",
            pos_420, EVENT_ML_SLOW_DOWN_MIN_POS_420
        ));
    }
    if ret_60 >= EVENT_ML_SLOW_DOWN_VERTICAL_RET_1M
        || ret_180 >= EVENT_ML_SLOW_DOWN_VERTICAL_RET_3M
        || ret_300 >= EVENT_ML_SLOW_DOWN_VERTICAL_RET_5M
    {
        return Some(format!(
            "hold slow down vertical pump ret60 {:.6} ret180 {:.6} ret300 {:.6}",
            ret_60, ret_180, ret_300
        ));
    }
    if body >= EVENT_ML_SLOW_DOWN_SOLID_BODY_SHARE_60
        && wick <= EVENT_ML_SLOW_DOWN_SOLID_MAX_WICK_SHARE_60
    {
        return Some(format!(
            "hold slow down solid no rejection body {:.3} wick {:.3}",
            body, wick
        ));
    }
    if ret_60 >= EVENT_ML_SLOW_DOWN_FAST_RET_1M && ret_420 >= EVENT_ML_SLOW_DOWN_FAST_RET_7M {
        return Some(format!(
            "hold slow down fast continuation ret60 {:.6} ret420 {:.6}",
            ret_60, ret_420
        ));
    }
    if ret_600 > EVENT_ML_SLOW_DOWN_MAX_RET_600 || ret_900 > EVENT_ML_SLOW_DOWN_MAX_RET_900 {
        return Some(format!(
            "hold slow down overextended 10m/15m ret600 {:.6} ret900 {:.6}",
            ret_600, ret_900
        ));
    }
    let confirmed_edge = (prediction.confidence >= EVENT_ML_SLOW_DOWN_CONFIRM_MIN_CONFIDENCE
        && ret_60 <= EVENT_ML_SLOW_DOWN_CONFIRM_MAX_RET_60)
        || (prediction.move_probability <= EVENT_ML_SLOW_DOWN_CONFIRM_MAX_MOVE_PROBABILITY
            && ret_300 >= EVENT_ML_SLOW_DOWN_CONFIRM_MIN_RET_300);
    if !confirmed_edge {
        return Some(format!(
            "hold slow down weak edge confidence {:.4} move_prob {:.4} ret60 {:.6} ret300 {:.6}",
            prediction.confidence, prediction.move_probability, ret_60, ret_300
        ));
    }
    None
}

fn event_ml_score(move_probability: f64, up_probability: f64) -> f64 {
    (move_probability - 0.5) * 10.0
        + if up_probability >= 0.5 {
            (up_probability - 0.5) * 10.0
        } else {
            -(0.5 - up_probability) * 10.0
        }
}

fn event_ml_slow_reversal_candidate(
    features: &serde_json::Value,
    move_probability: f64,
) -> Option<(String, f64)> {
    if move_probability < EVENT_ML_SLOW_REVERSAL_MIN_MOVE_PROBABILITY {
        return None;
    }
    let ret_60 = feature_f64(features, "ret_60");
    let ret_180 = feature_f64(features, "ret_180");
    let ret_300 = feature_f64(features, "ret_300");
    let ret_420 = feature_f64(features, "ret_420");
    let pos_420 = feature_f64(features, "pos_420");
    let overextended_down = ret_60 <= -EVENT_ML_SLOW_REVERSAL_MIN_RET_1M
        && ret_180 <= -EVENT_ML_SLOW_REVERSAL_MIN_RET_3M
        && ret_300 <= -EVENT_ML_SLOW_REVERSAL_MIN_RET_5M
        && ret_420 <= -EVENT_ML_SLOW_REVERSAL_MIN_RET_7M
        && pos_420 <= EVENT_ML_SLOW_REVERSAL_EXTREME_POS;
    if overextended_down {
        let confidence = (0.60
            + ((-ret_420 - EVENT_ML_SLOW_REVERSAL_MIN_RET_7M) * 35.0)
            + ((EVENT_ML_SLOW_REVERSAL_EXTREME_POS - pos_420).max(0.0) * 0.08)
            + ((move_probability - EVENT_ML_SLOW_REVERSAL_MIN_MOVE_PROBABILITY).max(0.0) * 0.20))
            .min(0.74);
        return Some((
            "UP".to_string(),
            confidence.max(EVENT_ML_SLOW_REVERSAL_MIN_CONFIDENCE),
        ));
    }
    let overextended_up = ret_60 >= EVENT_ML_SLOW_REVERSAL_MIN_RET_1M
        && ret_180 >= EVENT_ML_SLOW_REVERSAL_MIN_RET_3M
        && ret_300 >= EVENT_ML_SLOW_REVERSAL_MIN_RET_5M
        && ret_420 >= EVENT_ML_SLOW_REVERSAL_MIN_RET_7M
        && pos_420 >= 1.0 - EVENT_ML_SLOW_REVERSAL_EXTREME_POS;
    if overextended_up {
        let confidence = (0.60
            + ((ret_420 - EVENT_ML_SLOW_REVERSAL_MIN_RET_7M) * 35.0)
            + ((pos_420 - (1.0 - EVENT_ML_SLOW_REVERSAL_EXTREME_POS)).max(0.0) * 0.08)
            + ((move_probability - EVENT_ML_SLOW_REVERSAL_MIN_MOVE_PROBABILITY).max(0.0) * 0.20))
            .min(0.74);
        return Some((
            "DOWN".to_string(),
            confidence.max(EVENT_ML_SLOW_REVERSAL_MIN_CONFIDENCE),
        ));
    }
    None
}

fn feature_f64(features: &serde_json::Value, key: &str) -> f64 {
    features
        .get(key)
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
}

impl EventMlForest {
    fn predict_proba(&self, features: &[f64]) -> f64 {
        if self.trees.is_empty() {
            return 0.0;
        }
        self.trees
            .iter()
            .map(|tree| tree.predict_proba(features))
            .sum::<f64>()
            / self.trees.len() as f64
    }
}

impl EventMlTree {
    fn predict_proba(&self, features: &[f64]) -> f64 {
        let mut node = 0usize;
        loop {
            let left = self.children_left.get(node).copied().unwrap_or(-1);
            let right = self.children_right.get(node).copied().unwrap_or(-1);
            if left < 0 || right < 0 {
                return self.positive_prob.get(node).copied().unwrap_or(0.0);
            }
            let feature_index = self.feature.get(node).copied().unwrap_or(-2);
            if feature_index < 0 {
                return self.positive_prob.get(node).copied().unwrap_or(0.0);
            }
            let value = features
                .get(feature_index as usize)
                .copied()
                .unwrap_or(f64::NAN);
            let threshold = self.threshold.get(node).copied().unwrap_or(f64::INFINITY);
            node = if value <= threshold {
                left as usize
            } else {
                right as usize
            };
        }
    }
}

fn event_ml_features(
    bars: &[MarketSecondBar],
    feature_names: &[String],
) -> Option<(serde_json::Value, Vec<f64>)> {
    if bars.len() < EVENT_ML_WARM_SECONDS + 1 {
        return None;
    }
    let bars = &bars[bars.len() - (EVENT_ML_WARM_SECONDS + 1)..];
    if !bars
        .windows(2)
        .all(|pair| pair[1].second_ms == pair[0].second_ms + 1_000)
    {
        return None;
    }
    let latest = bars.last()?;
    let close = bars.iter().map(|bar| bar.close).collect::<Vec<_>>();
    let high = bars.iter().map(|bar| bar.high).collect::<Vec<_>>();
    let low = bars.iter().map(|bar| bar.low).collect::<Vec<_>>();
    let volume = bars.iter().map(|bar| bar.volume).collect::<Vec<_>>();
    let buy = bars
        .iter()
        .map(|bar| bar.taker_buy_volume)
        .collect::<Vec<_>>();
    let sell = bars
        .iter()
        .map(|bar| bar.taker_sell_volume)
        .collect::<Vec<_>>();
    let trades = bars
        .iter()
        .map(|bar| f64::from(bar.trade_count))
        .collect::<Vec<_>>();
    let pct = close
        .windows(2)
        .map(|pair| ratio_delta(pair[1], pair[0]))
        .collect::<Vec<_>>();
    let mut values = HashMap::<String, f64>::new();
    for sec in [
        1usize, 2, 3, 5, 10, 15, 30, 45, 60, 90, 120, 180, 300, 420, 600, 900, 1800, 3600,
    ] {
        values.insert(format!("ret_{sec}"), ratio_delta_last(&close, sec)?);
    }
    for sec in [15usize, 30, 60, 120, 300, 420, 600, 900, 1800, 3600] {
        let pct_window = last_window(&pct, sec)?;
        values.insert(
            format!("vol_{sec}"),
            sample_stddev(pct_window, average(pct_window)),
        );
        let lows = last_window(&close, sec)?;
        let rolling_low = lows.iter().copied().fold(f64::INFINITY, f64::min);
        let rolling_high = lows.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let price_range = rolling_high - rolling_low;
        let past = close.get(close.len().checked_sub(sec + 1)?)?;
        values.insert(
            format!("pos_{sec}"),
            safe_div(latest.close - rolling_low, price_range),
        );
        values.insert(format!("range_ret_{sec}"), safe_div(price_range, *past));
        values.insert(
            format!("dist_high_{sec}"),
            ratio_delta(latest.close, rolling_high),
        );
        values.insert(
            format!("dist_low_{sec}"),
            ratio_delta(latest.close, rolling_low),
        );
    }
    for sec in [60usize, 180, 300, 600] {
        let open_price = *close.get(close.len().checked_sub(sec + 1)?)?;
        let rolling_high = last_window(&high, sec)?
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max)
            .max(open_price)
            .max(latest.close);
        let rolling_low = last_window(&low, sec)?
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min)
            .min(open_price)
            .min(latest.close);
        let range = rolling_high - rolling_low;
        let upper_body = open_price.max(latest.close);
        let lower_body = open_price.min(latest.close);
        values.insert(
            format!("candle_body_bp_{sec}"),
            ratio_delta(latest.close, open_price) * 10_000.0,
        );
        values.insert(
            format!("candle_body_share_{sec}"),
            safe_div((latest.close - open_price).abs(), range),
        );
        values.insert(
            format!("candle_upper_wick_share_{sec}"),
            safe_div(rolling_high - upper_body, range),
        );
        values.insert(
            format!("candle_lower_wick_share_{sec}"),
            safe_div(lower_body - rolling_low, range),
        );
        values.insert(
            format!("candle_close_pos_{sec}"),
            safe_div(latest.close - rolling_low, range),
        );
        values.insert(
            format!("candle_range_bp_{sec}"),
            safe_div(range, open_price) * 10_000.0,
        );
    }
    for sec in [15usize, 30, 60, 120, 300, 600, 900] {
        let buy_sum = last_window(&buy, sec)?.iter().sum::<f64>();
        let sell_sum = last_window(&sell, sec)?.iter().sum::<f64>();
        let vol_sum = last_window(&volume, sec)?.iter().sum::<f64>();
        let trade_sum = last_window(&trades, sec)?.iter().sum::<f64>();
        let total = buy_sum + sell_sum;
        values.insert(format!("buy_ratio_{sec}"), safe_div(buy_sum, total));
        values.insert(
            format!("buy_imbalance_{sec}"),
            safe_div(buy_sum - sell_sum, total),
        );
        values.insert(format!("log_volume_{sec}"), vol_sum.ln_1p());
        values.insert(format!("log_trades_{sec}"), trade_sum.ln_1p());
    }
    for (short, long) in [
        (10usize, 60usize),
        (30, 120),
        (60, 300),
        (120, 600),
        (300, 1800),
    ] {
        let value = *values.get(&format!("ret_{short}"))?
            - *values.get(&format!("ret_{long}"))? * (short as f64 / long as f64);
        values.insert(format!("ret_{short}_minus_scaled_{long}"), value);
    }
    for (short, long) in [(30usize, 300usize), (60, 900), (120, 1800)] {
        let value = safe_div(
            *values.get(&format!("vol_{short}"))?,
            *values.get(&format!("vol_{long}"))?,
        );
        values.insert(format!("vol_{short}_vs_{long}"), value);
    }
    let seconds_of_day = ((latest.second_ms / 1_000).rem_euclid(86_400)) as f64;
    let event_period_seconds = f64::from(DEFAULT_PERIOD_SECONDS);
    let event_pos = (seconds_of_day % event_period_seconds) / event_period_seconds;
    values.insert("event_pos".to_string(), event_pos);
    values.insert(
        "event_sin".to_string(),
        (2.0 * std::f64::consts::PI * event_pos).sin(),
    );
    values.insert(
        "event_cos".to_string(),
        (2.0 * std::f64::consts::PI * event_pos).cos(),
    );
    values.insert(
        "tod_sin".to_string(),
        (2.0 * std::f64::consts::PI * seconds_of_day / 86_400.0).sin(),
    );
    values.insert(
        "tod_cos".to_string(),
        (2.0 * std::f64::consts::PI * seconds_of_day / 86_400.0).cos(),
    );
    let feature_values = feature_names
        .iter()
        .map(|name| values.get(name).copied().filter(|value| value.is_finite()))
        .collect::<Option<Vec<_>>>()?;
    Some((json!(values), feature_values))
}

fn last_window(values: &[f64], len: usize) -> Option<&[f64]> {
    if values.len() < len {
        None
    } else {
        Some(&values[values.len() - len..])
    }
}

fn ratio_delta_last(values: &[f64], seconds: usize) -> Option<f64> {
    if values.len() <= seconds {
        None
    } else {
        Some(ratio_delta(
            values[values.len() - 1],
            values[values.len() - 1 - seconds],
        ))
    }
}

fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator.abs() <= f64::EPSILON {
        0.0
    } else {
        numerator / denominator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_candidate(direction: &str) -> PredictionCandidate {
        make_prediction_candidate(direction, "structure_edge", "test_rule", 0.70, 1.0)
    }

    fn test_slow_reversal_candidate(direction: &str) -> PredictionCandidate {
        make_prediction_candidate(
            direction,
            "ml_extra_trees",
            EVENT_ML_SLOW_REVERSAL_RULE,
            0.70,
            1.0,
        )
    }

    fn test_ml_prediction(
        direction: &str,
        rule: &str,
        features: serde_json::Value,
    ) -> EventMlPrediction {
        EventMlPrediction {
            direction: direction.to_string(),
            rule: rule.to_string(),
            move_probability: 0.60,
            up_probability: if direction == "UP" { 0.59 } else { 0.41 },
            confidence: 0.59,
            score: 1.0,
            filtered_reason: None,
            features,
        }
    }

    fn test_meta_policy_options(
        two_stage_enabled: bool,
        two_stage_downshift_enabled: bool,
        two_stage_reverse_enabled: bool,
        slow_up_enabled: bool,
    ) -> EventMlMetaPolicyOptions {
        EventMlMetaPolicyOptions {
            two_stage_enabled,
            two_stage_downshift_enabled,
            two_stage_reverse_enabled,
            slow_up_enabled,
        }
    }

    #[test]
    fn meta_policy_allows_two_stage_by_default_after_bucket_backtest() {
        let mut prediction = test_ml_prediction(
            "UP",
            EVENT_ML_TWO_STAGE_RULE,
            json!({
                "ret_180": -0.0010,
                "ret_300": -0.0010,
                "ret_420": -0.0010,
                "ret_600": -0.0020,
                "candle_close_pos_60": 0.450
            }),
        );

        apply_event_ml_meta_policy_with_options(
            &mut prediction,
            test_meta_policy_options(true, false, false, false),
        );

        assert_eq!(prediction.direction, "UP");
        assert_eq!(prediction.rule, EVENT_ML_TWO_STAGE_RULE);
        assert!(prediction.filtered_reason.is_none());
    }

    #[test]
    fn meta_policy_holds_two_stage_up_medium_downtrend_bucket() {
        let mut prediction = test_ml_prediction(
            "UP",
            EVENT_ML_TWO_STAGE_RULE,
            json!({
                "ret_180": -0.0020,
                "ret_300": -0.0021,
                "ret_420": -0.0030,
                "ret_600": -0.0035,
                "candle_close_pos_60": 0.200
            }),
        );

        apply_event_ml_meta_policy_with_options(
            &mut prediction,
            test_meta_policy_options(true, false, false, false),
        );

        assert_eq!(prediction.direction, "HOLD");
        assert_eq!(prediction.rule, EVENT_ML_TWO_STAGE_RULE);
        assert!(prediction
            .filtered_reason
            .as_deref()
            .unwrap_or_default()
            .contains("medium downtrend"));
    }

    #[test]
    fn meta_policy_downshifts_two_stage_up_when_drop_continues() {
        let mut prediction = test_ml_prediction(
            "UP",
            EVENT_ML_TWO_STAGE_RULE,
            json!({
                "ret_180": -0.0032,
                "ret_420": -0.0040,
                "ret_600": -0.0048,
                "candle_close_pos_60": 0.120
            }),
        );
        prediction.up_probability = 0.590;
        prediction.confidence = 0.590;

        apply_event_ml_meta_policy_with_options(
            &mut prediction,
            test_meta_policy_options(true, true, false, false),
        );

        assert_eq!(prediction.direction, "DOWN");
        assert_eq!(prediction.rule, EVENT_ML_TWO_STAGE_DOWNSHIFT_RULE);
        assert!(prediction.confidence > 0.40);
    }

    #[test]
    fn meta_policy_does_not_downshift_two_stage_up_after_reclaim() {
        let mut prediction = test_ml_prediction(
            "UP",
            EVENT_ML_TWO_STAGE_RULE,
            json!({
                "ret_180": -0.0032,
                "ret_420": -0.0040,
                "ret_600": -0.0048,
                "candle_close_pos_60": 0.520
            }),
        );

        apply_event_ml_meta_policy_with_options(
            &mut prediction,
            test_meta_policy_options(true, false, false, false),
        );

        assert_eq!(prediction.direction, "UP");
        assert_eq!(prediction.rule, EVENT_ML_TWO_STAGE_RULE);
    }

    #[test]
    fn meta_policy_holds_slow_down_overextended_10m_15m() {
        let mut prediction = test_ml_prediction(
            "DOWN",
            EVENT_ML_SLOW_REVERSAL_RULE,
            json!({
                "pos_420": 0.980,
                "ret_60": 0.0007,
                "ret_180": 0.0019,
                "ret_300": 0.0032,
                "ret_420": 0.0034,
                "ret_600": 0.0031,
                "ret_900": 0.0018,
                "candle_body_share_60": 0.750,
                "candle_upper_wick_share_60": 0.180,
                "candle_lower_wick_share_60": 0.070
            }),
        );
        prediction.confidence = 0.700;

        apply_event_ml_meta_policy_with_options(
            &mut prediction,
            test_meta_policy_options(true, false, false, false),
        );

        assert_eq!(prediction.direction, "HOLD");
        assert!(prediction
            .filtered_reason
            .as_deref()
            .unwrap_or_default()
            .contains("overextended"));
    }

    #[test]
    fn meta_policy_holds_slow_down_vertical_pump() {
        let mut prediction = test_ml_prediction(
            "DOWN",
            EVENT_ML_SLOW_REVERSAL_RULE,
            json!({
                "pos_420": 0.980,
                "ret_60": 0.0047,
                "ret_180": 0.0128,
                "ret_300": 0.0081,
                "ret_420": 0.0022,
                "candle_body_share_60": 0.931,
                "candle_upper_wick_share_60": 0.069,
                "candle_lower_wick_share_60": 0.000
            }),
        );

        apply_event_ml_meta_policy_with_options(
            &mut prediction,
            test_meta_policy_options(true, false, false, false),
        );

        assert_eq!(prediction.direction, "HOLD");
        assert!(prediction
            .filtered_reason
            .as_deref()
            .unwrap_or_default()
            .contains("vertical pump"));
    }

    #[test]
    fn meta_policy_holds_slow_up_without_bottom_rejection() {
        let mut prediction = test_ml_prediction(
            "UP",
            EVENT_ML_SLOW_REVERSAL_RULE,
            json!({
                "pos_420": 0.041,
                "ret_60": -0.0013,
                "ret_180": -0.0040,
                "ret_300": -0.0036,
                "ret_420": -0.0041,
                "candle_body_share_60": 0.692,
                "candle_upper_wick_share_60": 0.197,
                "candle_lower_wick_share_60": 0.111
            }),
        );

        apply_event_ml_meta_policy_with_options(
            &mut prediction,
            test_meta_policy_options(true, false, false, false),
        );

        assert_eq!(prediction.direction, "HOLD");
        assert!(prediction
            .filtered_reason
            .as_deref()
            .unwrap_or_default()
            .contains("slow up disabled"));
    }

    #[test]
    fn meta_policy_holds_slow_up_weak_edge() {
        let mut prediction = test_ml_prediction(
            "UP",
            EVENT_ML_SLOW_REVERSAL_RULE,
            json!({
                "pos_420": 0.041,
                "ret_60": -0.0013,
                "ret_180": -0.0030,
                "ret_300": -0.0018,
                "ret_420": -0.0041,
                "candle_body_share_60": 0.450,
                "candle_upper_wick_share_60": 0.050,
                "candle_lower_wick_share_60": 0.500
            }),
        );
        prediction.confidence = 0.650;
        prediction.up_probability = 0.650;

        apply_event_ml_meta_policy_with_options(
            &mut prediction,
            test_meta_policy_options(true, false, false, false),
        );

        assert_eq!(prediction.direction, "HOLD");
        assert!(prediction
            .filtered_reason
            .as_deref()
            .unwrap_or_default()
            .contains("slow up disabled"));
    }

    #[test]
    fn up_falling_knife_filter_rejects_solid_drop_without_lower_wick() {
        let snapshot = json!({
            "shape": { "boll_pos": 0.05 },
            "structure": {
                "body_ratio": 0.72,
                "upper_wick_ratio": 0.08,
                "lower_wick_ratio": 0.12
            },
            "returns": {
                "live_ret_1": -0.00001,
                "live_ret_3": -0.00020,
                "ret_3": -0.00110
            },
            "cycle": { "return_from_open": -0.00130 }
        });

        let reason = prediction_candidate_hard_filter_reason(&snapshot, &test_candidate("UP"));

        assert!(reason
            .as_deref()
            .unwrap_or_default()
            .contains("hold up falling knife"));
    }

    #[test]
    fn up_falling_knife_filter_allows_lower_wick_confirmation() {
        let snapshot = json!({
            "shape": { "boll_pos": 0.05 },
            "structure": {
                "body_ratio": 0.72,
                "upper_wick_ratio": 0.08,
                "lower_wick_ratio": 0.30
            },
            "returns": {
                "live_ret_1": -0.00001,
                "live_ret_3": -0.00020,
                "ret_3": -0.00110
            },
            "cycle": { "return_from_open": -0.00130 }
        });

        let reason = prediction_candidate_hard_filter_reason(&snapshot, &test_candidate("UP"));

        assert!(!reason
            .as_deref()
            .unwrap_or_default()
            .contains("hold up falling knife"));
    }

    #[test]
    fn up_falling_knife_filter_rejects_even_with_small_live_bounce() {
        let snapshot = json!({
            "shape": { "boll_pos": 0.05 },
            "structure": {
                "body_ratio": 0.72,
                "upper_wick_ratio": 0.08,
                "lower_wick_ratio": 0.12
            },
            "returns": {
                "live_ret_1": 0.00002,
                "live_ret_3": -0.00020,
                "ret_3": -0.00110
            },
            "cycle": { "return_from_open": -0.00130 }
        });

        let reason = prediction_candidate_hard_filter_reason(&snapshot, &test_candidate("UP"));

        assert!(reason
            .as_deref()
            .unwrap_or_default()
            .contains("hold up falling knife"));
    }

    #[test]
    fn up_falling_knife_filter_does_not_reject_down_candidates() {
        let snapshot = json!({
            "shape": { "boll_pos": 0.05 },
            "structure": {
                "body_ratio": 0.72,
                "upper_wick_ratio": 0.08,
                "lower_wick_ratio": 0.12
            },
            "returns": {
                "live_ret_1": -0.00001,
                "live_ret_3": -0.00020,
                "ret_3": -0.00110
            },
            "cycle": { "return_from_open": -0.00130 }
        });

        let reason = prediction_candidate_hard_filter_reason(&snapshot, &test_candidate("DOWN"));

        assert!(!reason
            .as_deref()
            .unwrap_or_default()
            .contains("hold up falling knife"));
    }

    #[test]
    fn slow_reversal_filter_rejects_unfinished_extreme_bottom() {
        let snapshot = json!({
            "shape": { "boll_pos": 0.05 },
            "structure": {
                "body_ratio": 0.40,
                "upper_wick_ratio": 0.02,
                "lower_wick_ratio": 0.02
            },
            "returns": {
                "live_ret_1": -0.00001,
                "live_ret_3": -0.00020,
                "ret_3": -0.00110
            },
            "cycle": { "return_from_open": -0.00130 },
            "features": {
                "pos_300": 0.010,
                "candle_body_share_60": 0.970,
                "candle_upper_wick_share_60": 0.015,
                "candle_lower_wick_share_60": 0.015
            }
        });

        let reason =
            prediction_candidate_hard_filter_reason(&snapshot, &test_slow_reversal_candidate("UP"));

        assert!(reason
            .as_deref()
            .unwrap_or_default()
            .contains("hold slow reversal unfinished extreme"));
    }

    #[test]
    fn slow_reversal_filter_allows_extreme_with_rejection_wick() {
        let snapshot = json!({
            "shape": { "boll_pos": 0.05 },
            "structure": {
                "body_ratio": 0.40,
                "upper_wick_ratio": 0.08,
                "lower_wick_ratio": 0.25
            },
            "returns": {
                "live_ret_1": -0.00001,
                "live_ret_3": -0.00020,
                "ret_3": -0.00110
            },
            "cycle": { "return_from_open": -0.00130 },
            "features": {
                "pos_300": 0.010,
                "candle_body_share_60": 0.820,
                "candle_upper_wick_share_60": 0.040,
                "candle_lower_wick_share_60": 0.140
            }
        });

        let reason =
            prediction_candidate_hard_filter_reason(&snapshot, &test_slow_reversal_candidate("UP"));

        assert!(reason.is_none());
    }

    #[test]
    fn slow_reversal_filter_rejects_unfinished_extreme_top() {
        let snapshot = json!({
            "shape": { "boll_pos": 0.95 },
            "structure": {
                "body_ratio": 0.40,
                "upper_wick_ratio": 0.02,
                "lower_wick_ratio": 0.02
            },
            "returns": {
                "live_ret_1": 0.00001,
                "live_ret_3": 0.00020,
                "ret_3": 0.00110
            },
            "cycle": { "return_from_open": 0.00130 },
            "features": {
                "pos_300": 0.990,
                "candle_body_share_60": 0.970,
                "candle_upper_wick_share_60": 0.015,
                "candle_lower_wick_share_60": 0.015
            }
        });

        let reason = prediction_candidate_hard_filter_reason(
            &snapshot,
            &test_slow_reversal_candidate("DOWN"),
        );

        assert!(reason
            .as_deref()
            .unwrap_or_default()
            .contains("hold slow reversal unfinished extreme"));
    }

    #[test]
    fn event_ml_model_file_loads() {
        let text = std::fs::read_to_string(DEFAULT_EVENT_ML_MODEL_PATH)
            .expect("event ml model file should exist");
        let model: EventMlModel =
            serde_json::from_str(&text).expect("event ml model json should parse");

        assert_eq!(model.version, DEFAULT_EVENT_ML_MODEL_ARTIFACT_VERSION);
        assert_eq!(model.period_seconds, DEFAULT_PERIOD_SECONDS);
        assert_eq!(model.feature_names.len(), 127);
        assert!(!model.models.move_model.trees.is_empty());
        assert!(!model.models.direction.trees.is_empty());
    }

    fn synthetic_second_bars(count: usize) -> Vec<MarketSecondBar> {
        (0..count)
            .map(|index| {
                let second_ms = 1_700_000_000_000 + index as i64 * 1_000;
                let wave = (index as f64 / 17.0).sin();
                let drift = index as f64 * 0.002;
                let close = 78_000.0 + drift + wave;
                MarketSecondBar {
                    second_ms,
                    high: close + 0.45,
                    low: close - 0.45,
                    close,
                    volume: 4.0 + wave.abs(),
                    taker_buy_volume: 2.0 + wave.max(0.0),
                    taker_sell_volume: 2.0 + (-wave).max(0.0),
                    trade_count: 8,
                    price_from_index: true,
                }
            })
            .collect()
    }

    #[test]
    fn event_ml_predicts_from_continuous_second_bars() {
        let text = std::fs::read_to_string(DEFAULT_EVENT_ML_MODEL_PATH)
            .expect("event ml model file should exist");
        let model: EventMlModel =
            serde_json::from_str(&text).expect("event ml model json should parse");
        let bars = synthetic_second_bars(EVENT_ML_WARM_SECONDS + 1);
        let prediction = event_ml_predict(&model, &bars).expect("continuous bars should predict");

        assert!(prediction.move_probability.is_finite());
        assert!(prediction.up_probability.is_finite());
        assert!((0.0..=1.0).contains(&prediction.move_probability));
        assert!((0.0..=1.0).contains(&prediction.up_probability));
        assert!(prediction.features.is_object());
        let features = prediction
            .features
            .as_object()
            .expect("feature snapshot should be an object");
        for name in &model.feature_names {
            assert!(features.contains_key(name), "missing feature {name}");
        }
    }

    #[test]
    fn event_ml_rejects_non_continuous_second_bars() {
        let text = std::fs::read_to_string(DEFAULT_EVENT_ML_MODEL_PATH)
            .expect("event ml model file should exist");
        let model: EventMlModel =
            serde_json::from_str(&text).expect("event ml model json should parse");
        let mut bars = synthetic_second_bars(EVENT_ML_WARM_SECONDS + 1);
        bars[EVENT_ML_WARM_SECONDS / 2].second_ms += 1_000;

        assert!(event_ml_predict(&model, &bars).is_none());
    }

    #[test]
    fn prediction_decision_uses_ml_candidate_mode_not_artifact_version() {
        let decision = PredictionDecision {
            direction: "UP".to_string(),
            confidence: 0.70,
            predicted_return: 0.0,
            entry_price: 78_000.0,
            score: 0.0,
            signal_cooldown_seconds: DEFAULT_PREDICTION_SIGNAL_COOLDOWN_SECONDS,
            market_source: "test".to_string(),
            market_age_seconds: 0,
            market_candles: 0,
            reason: "test".to_string(),
            feature_snapshot: json!({
                "ml": {
                    "model_version": DEFAULT_EVENT_ML_MODEL_ARTIFACT_VERSION
                },
                "decision": {
                    "model_version": PREDICTION_MODEL_VERSION,
                    "candidate_mode": "ml_extra_trees",
                    "candidate_direction": "UP",
                    "candidate_rule": EVENT_ML_TWO_STAGE_RULE
                }
            }),
        };

        assert!(prediction_decision_uses_ml(&decision));
    }

    #[test]
    fn market_second_bar_tail_keeps_only_latest_contiguous_window() {
        let mut market = MarketDataState::default();
        for bar in synthetic_second_bars(5) {
            market.second_bars.insert(bar.second_ms, bar);
        }
        let mut tail = synthetic_second_bars(4);
        for bar in &mut tail {
            bar.second_ms += 60_000;
        }
        for bar in tail {
            market.second_bars.insert(bar.second_ms, bar);
        }

        keep_contiguous_second_bar_tail(&mut market);

        assert_eq!(market.second_bars.len(), 4);
        let keys = market.second_bars.keys().copied().collect::<Vec<_>>();
        assert!(keys.windows(2).all(|pair| pair[1] == pair[0] + 1_000));
        assert!(keys[0] > 1_700_000_000_000 + 5_000);
    }
}

fn merge_prediction_calibration(
    decision: &mut PredictionDecision,
    calibration: &PredictionCalibration,
    candidate: Option<&PredictionCandidate>,
) {
    let candidate_direction = candidate.map(|candidate| candidate.direction.as_str());
    if let Some(candidate) = candidate {
        if calibration.allowed {
            decision.direction = candidate.direction.clone();
            decision.confidence = if calibration.sample_count < i64::from(calibration.min_samples) {
                candidate.raw_confidence
            } else {
                calibration.win_rate
            };
            decision.reason = format!(
                "send {} mode {} rule {} confidence {:.4} calibrated win {:.4} ev {:.4} samples {}/{} dir_conf {:.4} structure {:.4}",
                candidate.direction,
                candidate.mode,
                candidate.rule,
                decision.confidence,
                calibration.win_rate,
                calibration.ev_usdt,
                calibration.sample_count,
                calibration.min_samples,
                candidate.raw_confidence,
                candidate.structure_score
            );
        } else {
            decision.direction = "HOLD".to_string();
            decision.confidence = calibration.win_rate;
            decision.reason = calibration.reason.clone();
        }
    } else {
        let existing_reason = decision.reason.clone();
        decision.direction = "HOLD".to_string();
        decision.reason = if existing_reason.starts_with("hold ") {
            existing_reason
        } else {
            format!("hold no candidate; {}", calibration.reason)
        };
    }
    let mut snapshot = decision.feature_snapshot.clone();
    if let Some(object) = snapshot.as_object_mut() {
        object.insert(
            "calibration".to_string(),
            json!({
                "model_version": PREDICTION_MODEL_VERSION,
                "allowed": calibration.allowed,
                "candidate_direction": candidate_direction,
                "sample_count": calibration.sample_count,
                "wins": calibration.wins,
                "win_rate": calibration.win_rate,
                "ev_usdt": calibration.ev_usdt,
                "min_samples": calibration.min_samples,
                "min_win_rate": calibration.min_win_rate,
                "min_ev_usdt": calibration.min_ev_usdt,
                "lookback_minutes": calibration.lookback_minutes,
                "sample_seconds": calibration.sample_seconds,
                "reason": calibration.reason
            }),
        );
    }
    decision.feature_snapshot = snapshot;
}

fn prediction_decision_uses_ml(decision: &PredictionDecision) -> bool {
    decision
        .feature_snapshot
        .get("decision")
        .and_then(|decision| decision.get("candidate_mode"))
        .and_then(|value| value.as_str())
        == Some("ml_extra_trees")
}

fn json_i64(value: &serde_json::Value) -> Result<i64, AppError> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
        .ok_or_else(|| AppError::bad_request("PREDICTION_DATA_INVALID", "invalid int"))
}

fn json_f64(value: &serde_json::Value) -> Result<f64, AppError> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse::<f64>().ok()))
        .filter(|value| value.is_finite())
        .ok_or_else(|| AppError::bad_request("PREDICTION_DATA_INVALID", "invalid number"))
}

fn millis_to_utc(value: i64) -> Result<DateTime<Utc>, AppError> {
    Utc.timestamp_millis_opt(value)
        .single()
        .ok_or_else(|| AppError::bad_request("PREDICTION_DATA_INVALID", "invalid timestamp"))
}

fn return_n(values: &[f64], n: usize) -> f64 {
    if values.len() <= n {
        0.0
    } else {
        ratio_delta(values[values.len() - 1], values[values.len() - 1 - n])
    }
}

fn live_close_series(
    closed_closes: &[f64],
    latest_price: f64,
    latest_price_time_ms: i64,
    latest_closed_candle_close_time_ms: i64,
) -> Vec<f64> {
    let mut closes = closed_closes.to_vec();
    if latest_price_time_ms > latest_closed_candle_close_time_ms.saturating_add(500) {
        closes.push(latest_price);
    } else if let Some(last) = closes.last_mut() {
        *last = latest_price;
    }
    closes
}

fn rolling_price_return(
    points: &[(i64, f64)],
    latest_time_ms: i64,
    lookback_seconds: i64,
) -> Option<f64> {
    let target_time_ms = latest_time_ms.checked_sub(lookback_seconds.checked_mul(1_000)?)?;
    let (_, past_price) = points
        .iter()
        .rev()
        .find(|(time_ms, _)| *time_ms <= target_time_ms)?;
    Some(ratio_delta(
        points.last().map(|(_, price)| *price)?,
        *past_price,
    ))
}

struct EventCycleContext {
    start_ms: i64,
    progress: f64,
    open_price: f64,
    return_from_open: f64,
}

fn event_cycle_context(
    candles: &[MarketCandle],
    latest_price: f64,
    latest_price_time_ms: i64,
) -> EventCycleContext {
    let period_ms = i64::from(DEFAULT_PERIOD_SECONDS) * 1_000;
    let start_ms = latest_price_time_ms - latest_price_time_ms.rem_euclid(period_ms);
    let open_price = candles
        .iter()
        .rev()
        .find(|candle| candle.open_time_ms <= start_ms)
        .map(|candle| candle.open)
        .or_else(|| candles.first().map(|candle| candle.open))
        .unwrap_or(latest_price);
    let elapsed_ms = (latest_price_time_ms - start_ms).clamp(0, period_ms);
    EventCycleContext {
        start_ms,
        progress: elapsed_ms as f64 / period_ms as f64,
        open_price,
        return_from_open: ratio_delta(latest_price, open_price),
    }
}

fn ratio_delta(current: f64, previous: f64) -> f64 {
    if previous.abs() < f64::EPSILON {
        0.0
    } else {
        current / previous - 1.0
    }
}

fn ema_last(values: &[f64], period: usize) -> f64 {
    let alpha = 2.0 / (period as f64 + 1.0);
    values
        .iter()
        .copied()
        .fold(values[0], |ema, value| alpha * value + (1.0 - alpha) * ema)
}

fn bollinger_position_width(values: &[f64], period: usize) -> (f64, f64) {
    let window = &values[values.len() - period..];
    let mean = average(window);
    let stdev = stddev(window, mean);
    let upper = mean + 2.0 * stdev;
    let lower = mean - 2.0 * stdev;
    let width = if mean.abs() < f64::EPSILON {
        0.0
    } else {
        (upper - lower) / mean
    };
    let position = if (upper - lower).abs() < f64::EPSILON {
        0.5
    } else {
        clamp(
            (values[values.len() - 1] - lower) / (upper - lower),
            0.0,
            1.0,
        )
    };
    (position, width)
}

fn realized_volatility(values: &[f64], period: usize) -> f64 {
    let returns = values[values.len() - period - 1..]
        .windows(2)
        .map(|pair| ratio_delta(pair[1], pair[0]))
        .collect::<Vec<_>>();
    stddev(&returns, average(&returns))
}

fn z_score_last(values: &[f64], period: usize) -> f64 {
    let window = &values[values.len() - period..];
    let mean = average(window);
    let stdev = stddev(window, mean);
    if stdev <= f64::EPSILON {
        0.0
    } else {
        (window[window.len() - 1] - mean) / stdev
    }
}

fn candle_body_ratio(candle: &MarketCandle) -> f64 {
    let range = candle.high - candle.low;
    if range <= f64::EPSILON {
        0.0
    } else {
        clamp((candle.close - candle.open).abs() / range, 0.0, 1.0)
    }
}

fn candle_upper_wick_ratio(candle: &MarketCandle) -> f64 {
    let range = candle.high - candle.low;
    if range <= f64::EPSILON {
        0.0
    } else {
        clamp(
            (candle.high - candle.open.max(candle.close)) / range,
            0.0,
            1.0,
        )
    }
}

fn candle_lower_wick_ratio(candle: &MarketCandle) -> f64 {
    let range = candle.high - candle.low;
    if range <= f64::EPSILON {
        0.0
    } else {
        clamp(
            (candle.open.min(candle.close) - candle.low) / range,
            0.0,
            1.0,
        )
    }
}

fn candle_close_position(candle: &MarketCandle) -> f64 {
    let range = candle.high - candle.low;
    if range <= f64::EPSILON {
        0.5
    } else {
        clamp((candle.close - candle.low) / range, 0.0, 1.0)
    }
}

#[allow(clippy::too_many_arguments)]
fn structure_score_down(
    candle: &MarketCandle,
    close_position: f64,
    upper_wick_ratio: f64,
    lower_wick_ratio: f64,
    live_ret_1: f64,
    live_ret_3: f64,
    ret_1: f64,
    ret_3: f64,
    cycle_ret: f64,
) -> f64 {
    let bearish_close = if candle.close < candle.open {
        0.25
    } else {
        0.0
    };
    let high_rejection = clamp(upper_wick_ratio * 1.25 - lower_wick_ratio * 0.45, 0.0, 0.45);
    let close_weakness = clamp((0.58 - close_position) / 0.58, 0.0, 1.0) * 0.25;
    let short_term_damage = clamp((-live_ret_1 * 2600.0).max(-ret_1 * 2200.0), 0.0, 0.35);
    let impulse_stalling = if cycle_ret > 0.0006 && live_ret_3 < ret_3 * 0.35 {
        0.18
    } else {
        0.0
    };
    clamp(
        bearish_close + high_rejection + close_weakness + short_term_damage + impulse_stalling,
        0.0,
        1.0,
    )
}

#[allow(clippy::too_many_arguments)]
fn structure_score_up(
    candle: &MarketCandle,
    close_position: f64,
    upper_wick_ratio: f64,
    lower_wick_ratio: f64,
    live_ret_1: f64,
    live_ret_3: f64,
    ret_1: f64,
    ret_3: f64,
    cycle_ret: f64,
) -> f64 {
    let bullish_close = if candle.close > candle.open {
        0.25
    } else {
        0.0
    };
    let low_rejection = clamp(lower_wick_ratio * 1.25 - upper_wick_ratio * 0.45, 0.0, 0.45);
    let close_strength = clamp((close_position - 0.42) / 0.58, 0.0, 1.0) * 0.25;
    let short_term_reclaim = clamp((live_ret_1 * 2600.0).max(ret_1 * 2200.0), 0.0, 0.35);
    let impulse_stalling = if cycle_ret < -0.0006 && live_ret_3 > ret_3 * 0.35 {
        0.18
    } else {
        0.0
    };
    clamp(
        bullish_close + low_rejection + close_strength + short_term_reclaim + impulse_stalling,
        0.0,
        1.0,
    )
}

fn average(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn stddev(values: &[f64], mean: f64) -> f64 {
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>()
        / values.len() as f64;
    variance.sqrt()
}

fn sample_stddev(values: &[f64], mean: f64) -> f64 {
    if values.len() <= 1 {
        return 0.0;
    }
    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>()
        / (values.len() - 1) as f64;
    variance.sqrt()
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.max(min).min(max)
}

fn sign(value: f64) -> f64 {
    if value > 0.0 {
        1.0
    } else if value < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn normal_cdf(value: f64) -> f64 {
    0.5 * (1.0 + erf(value / 2.0_f64.sqrt()))
}

fn erf(value: f64) -> f64 {
    let sign = if value < 0.0 { -1.0 } else { 1.0 };
    let x = value.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    sign * y
}

async fn binance_income_events_with_client(
    api_key: &str,
    api_secret: &str,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    limit: usize,
    bind_local_ip: Option<std::net::IpAddr>,
) -> Result<Vec<BinanceIncomeResponse>, AppError> {
    let client = binance_http_client_for(bind_local_ip)?;
    binance_signed_get(
        &client,
        api_key,
        api_secret,
        "/fapi/v1/income",
        vec![
            ("startTime", start_time.timestamp_millis().to_string()),
            ("endTime", end_time.timestamp_millis().to_string()),
            ("limit", limit.to_string()),
        ],
    )
    .await
}

async fn rpc_call<T: DeserializeOwned>(
    rpc_url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<Option<T>, AppError> {
    let client = reqwest::Client::builder()
        .timeout(StdDuration::from_secs(env_u64(
            "BSC_RPC_TIMEOUT_SECONDS",
            DEFAULT_BSC_RPC_TIMEOUT_SECONDS,
            1,
        )))
        .build()
        .map_err(|err| AppError::internal("BSC_RPC_CLIENT_FAILED", err.to_string()))?;
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let response = client
        .post(rpc_url)
        .json(&body)
        .send()
        .await
        .map_err(|err| AppError::bad_request("BSC_RPC_REQUEST_FAILED", err.to_string()))?;
    let response: RpcResponse<T> = response
        .json()
        .await
        .map_err(|err| AppError::bad_request("BSC_RPC_RESPONSE_INVALID", err.to_string()))?;
    if let Some(error) = response.error {
        return Err(AppError::bad_request("BSC_RPC_ERROR", error.message));
    }
    Ok(response.result)
}

async fn submit_order(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<EventContractOrderRequest>,
) -> Result<Json<SubmitOrderResponse>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    let stake_usdt = parse_decimal(&req.stake_usdt, "stake_usdt")?;
    validate_event_stake(stake_usdt)?;
    let direction = req.direction.to_uppercase();
    if direction != "UP" && direction != "DOWN" {
        return Err(AppError::bad_request(
            "ORDER_DIRECTION_INVALID",
            "direction must be UP or DOWN",
        ));
    }
    let reservation_id = req.reservation_id.ok_or_else(|| {
        AppError::bad_request(
            "RESERVATION_REQUIRED",
            "automatic orders must include reservation_id",
        )
    })?;
    let now = Utc::now();
    let order_id = Uuid::now_v7();
    let order = EventContractOrderRecord {
        id: order_id,
        user_id,
        reservation_id: Some(reservation_id),
        signal_id: req.signal_id.clone(),
        client_order_id: req.client_order_id,
        source: req.source,
        symbol: req.symbol,
        period_seconds: req.period_seconds,
        direction,
        stake_usdt,
        entry_price: parse_optional_decimal(req.entry_price.as_deref(), "entry_price")?,
        automation_profile: req.automation_profile,
        automation_confidence: parse_optional_decimal(
            req.automation_confidence.as_deref(),
            "automation_confidence",
        )?,
        action_trace_id: req.action_trace_id,
        raw_audit: req.raw_audit,
        order_status: "submitted".to_string(),
        reported_at: req.reported_at,
        created_at: now,
    };
    let saved_order_id = if let Some(pool) = state.db.as_ref() {
        db_insert_order_with_reservation_validation(pool, &order).await?
    } else {
        let signal_id = Uuid::parse_str(&order.signal_id)
            .map_err(|_| AppError::bad_request("SIGNAL_ID_INVALID", "signal_id must be a uuid"))?;
        let mut reservations = state.reservations.write().await;
        let reservation = reservations.get_mut(&reservation_id).ok_or_else(|| {
            AppError::bad_request("RESERVATION_NOT_FOUND", "reservation not found")
        })?;
        validate_order_against_reservation(&order, reservation, signal_id)?;
        if reservation.status != "reserved" && reservation.status != "order_submitted" {
            return Err(AppError::conflict(
                "RESERVATION_STATUS_INVALID",
                "reservation is not open for order submission",
            ));
        }
        reservation.status = "order_submitted".to_string();
        reservation.order_id = Some(order_id);
        reservation.updated_at = now;
        state.orders.write().await.push(order);
        order_id
    };
    Ok(Json(SubmitOrderResponse {
        order_id: saved_order_id,
        status: "submitted".to_string(),
    }))
}

async fn release_signal_reservation(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ReleaseReservationRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    let Some(pool) = state.db.as_ref() else {
        return Ok(Json(json!({ "ok": true, "released": false })));
    };
    let reason = reservation_release_entry_type(req.reason.as_deref().unwrap_or("client_failed"));
    let released = db_release_signal_reservation_for_user(
        pool,
        req.reservation_id,
        user_id,
        reason,
        "released",
    )
    .await?;
    Ok(Json(json!({ "ok": true, "released": released })))
}

async fn submit_execution_report(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<ExecutionReportRequest>,
) -> Result<Json<ExecutionReportResponse>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    let report_id = Uuid::now_v7();
    let stake_usdt = parse_decimal(&req.stake_usdt, "stake_usdt")?;
    let report = ExecutionReportRecord {
        id: report_id,
        user_id,
        signal_id: req.signal_id.clone(),
        client_report_id: req.client_report_id,
        source: req.source,
        symbol: req.symbol,
        period_seconds: req.period_seconds,
        direction: req.direction,
        stake_usdt,
        entry_price: parse_optional_decimal(req.entry_price.as_deref(), "entry_price")?,
        expiry_price: parse_optional_decimal(req.expiry_price.as_deref(), "expiry_price")?,
        payout_ratio: parse_optional_decimal(req.payout_ratio.as_deref(), "payout_ratio")?,
        gross_profit: parse_optional_decimal(req.gross_profit.as_deref(), "gross_profit")?,
        result: req.result,
        automation_profile: req.automation_profile,
        automation_confidence: parse_optional_decimal(
            req.automation_confidence.as_deref(),
            "automation_confidence",
        )?,
        action_trace_id: req.action_trace_id,
        report_status: "pending_review".to_string(),
        reported_at: req.reported_at,
        created_at: Utc::now(),
    };
    if let Some(pool) = state.db.as_ref() {
        db_insert_execution_report(pool, &report).await?;
    }
    state.reports.write().await.push(report);
    Ok(Json(ExecutionReportResponse {
        report_id,
        status: "pending_review".to_string(),
    }))
}

async fn admin_sync_exchange(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<ExchangeSyncResponse>, AppError> {
    ensure_admin(&state, &headers).await?;
    let Some(pool) = state.db.as_ref() else {
        return Err(AppError::bad_request(
            "DATABASE_REQUIRED",
            "exchange synchronization requires PostgreSQL",
        ));
    };
    let config = ExchangeWorkerConfig::from_env();
    let response = sync_exchange_orders(pool, &config).await?;
    Ok(Json(response))
}

async fn settlements(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<SettlementRecord>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_settlements_for_user(pool, user_id, &query).await?));
    }
    let rows = filter_page_settlements(
        state
            .settlements
            .read()
            .await
            .iter()
            .filter(|settlement| settlement.user_id == user_id)
            .cloned()
            .collect(),
        &query,
    );
    Ok(Json(rows))
}

async fn admin_login(
    State(state): State<SharedState>,
    Json(req): Json<AdminLoginRequest>,
) -> Result<Json<AdminLoginResponse>, AppError> {
    let username = normalize_admin_username(&req.username)?;
    let admin = if let Some(pool) = state.db.as_ref() {
        db_admin_account_by_username(pool, &username)
            .await?
            .ok_or_else(|| {
                AppError::unauthorized("ADMIN_INVALID_CREDENTIALS", "invalid admin credentials")
            })?
    } else {
        state
            .admin_users
            .read()
            .await
            .get(&username)
            .cloned()
            .ok_or_else(|| {
                AppError::unauthorized("ADMIN_INVALID_CREDENTIALS", "invalid admin credentials")
            })?
    };
    if admin.status != "active" || !verify_password(&admin.password_hash, &req.password) {
        return Err(AppError::unauthorized(
            "ADMIN_INVALID_CREDENTIALS",
            "invalid admin credentials",
        ));
    }
    Ok(Json(create_admin_session(&state, &admin).await?))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let token = query
        .access_token
        .or_else(|| bearer_token(&headers))
        .ok_or_else(|| AppError::unauthorized("AUTH_MISSING_TOKEN", "missing access token"))?;
    let session = authenticate_token(&state, &token).await?;
    if session.client_type != ClientType::Android {
        return Err(AppError::forbidden(
            "CLIENT_TYPE_FORBIDDEN",
            "only android sessions can connect to signal websocket",
        ));
    }
    verify_app_request_signature(&state, &headers, &session).await?;
    Ok(ws.on_upgrade(move |socket| websocket_loop(socket, state, session)))
}

async fn websocket_loop(mut socket: WebSocket, state: SharedState, session: SessionRecord) {
    let user_id = session.user_id;
    let mut rx = state.signal_tx.subscribe();
    let ws_connection_id = Uuid::now_v7();
    state
        .active_ws_connections
        .write()
        .await
        .insert(user_id, ws_connection_id);
    let _ = socket
        .send(Message::Text(
            json!({
                "type": "connection.ready",
                "data": {
                    "server_time": Utc::now(),
                    "ws_connection_id": ws_connection_id
                }
            })
            .to_string(),
        ))
        .await;
    loop {
        tokio::select! {
            socket_message = socket.recv() => {
                match socket_message {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
                continue;
            }
            signal_result = rx.recv() => {
                let Ok(signal) = signal_result else {
                    break;
                };
        let is_current_connection = state
            .active_ws_connections
            .read()
            .await
            .get(&user_id)
            .copied()
            == Some(ws_connection_id);
        if !is_current_connection {
            let _ = socket
                .send(Message::Text(
                    json!({
                        "type": "connection.closed",
                        "code": "WS_REPLACED",
                        "message": "newer websocket connection is active"
                    })
                    .to_string(),
                ))
                .await;
            break;
        }
        if !session_device_is_current(&state, &session).await {
            let _ = socket
                .send(Message::Text(
                    json!({
                        "type": "connection.closed",
                        "code": "AUTH_SESSION_REVOKED",
                        "message": "账号会话已失效，请重新登录"
                    })
                    .to_string(),
                ))
                .await;
            break;
        }
        let payload = authorize_signal_for_user(&state, user_id, &signal).await;
        let delivery_id = Uuid::now_v7();
        let sent_at = Utc::now();
        if let Some(pool) = state.db.as_ref() {
            if let Err(err) = db_insert_signal_delivery(
                pool,
                &SignalDeliveryRecord {
                    id: delivery_id,
                    signal_id: signal.id,
                    user_id,
                    reservation_id: payload
                        .reservation_id
                        .as_deref()
                        .and_then(|value| Uuid::parse_str(value).ok()),
                    ws_connection_id,
                    delivery_status: "sent".to_string(),
                    authorization_status: payload.authorization_status.clone(),
                    trade_authorized: payload.trade_authorized,
                    stake_usdt: parse_decimal(&payload.suggested_stake, "suggested_stake")
                        .unwrap_or(Decimal::ZERO),
                    fee_reserve_usdt: parse_decimal(&payload.fee_reserve_usdt, "fee_reserve_usdt")
                        .unwrap_or(Decimal::ZERO),
                    sent_at,
                    received_at: None,
                    client_received_at: None,
                    page_state: None,
                    app_version: None,
                    last_error: None,
                    created_at: sent_at,
                    updated_at: sent_at,
                },
            )
            .await
            {
                eprintln!("signal delivery insert failed: {}", err.message);
            }
        }
        let text = json!({
            "type": "signal.created",
            "delivery_id": delivery_id,
            "sent_at": sent_at,
            "ws_connection_id": ws_connection_id,
            "data": payload,
        })
        .to_string();
        if socket.send(Message::Text(text)).await.is_err() {
            if let Some(pool) = state.db.as_ref() {
                let _ = db_mark_signal_delivery_failed(
                    pool,
                    delivery_id,
                    "websocket send failed",
                    Utc::now(),
                )
                .await;
                if let Some(reservation_id) = payload
                    .reservation_id
                    .as_deref()
                    .and_then(|value| Uuid::parse_str(value).ok())
                {
                    let _ = db_release_signal_reservation(
                        pool,
                        reservation_id,
                        reservation_release_entry_type("send_failed"),
                        "released",
                    )
                    .await;
                }
            }
            break;
        }
            }
        }
    }
    let mut active = state.active_ws_connections.write().await;
    if active.get(&user_id).copied() == Some(ws_connection_id) {
        active.remove(&user_id);
    }
}

async fn admin_overview(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        let prediction = state.prediction_runtime.read().await.clone();
        let overview = db_admin_overview(pool, state.started_at, prediction).await?;
        return Ok(Json(overview));
    }
    let stats = build_stats(&state).await;
    let users = state.users.read().await.len();
    let page_states = state.page_states.read().await.clone();
    let ready_pages = page_states
        .values()
        .filter(|page| page_is_fresh_and_ready(page))
        .count();
    let orders = state.orders.read().await.clone();
    let reports = state.reports.read().await.clone();
    let settlements = state.settlements.read().await.clone();
    Ok(Json(json!({
        "stats": stats,
        "users_total": users,
        "ready_pages": ready_pages,
        "orders": orders.into_iter().rev().take(20).collect::<Vec<_>>(),
        "reports": reports.into_iter().rev().take(20).collect::<Vec<_>>(),
        "settlements": settlements.into_iter().rev().take(20).collect::<Vec<_>>()
    })))
}

async fn admin_emit_signal(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<EmitSignalRequest>,
) -> Result<Json<SignalApiResponse>, AppError> {
    ensure_admin(&state, &headers).await?;
    let direction = req.direction.to_uppercase();
    if direction != "UP" && direction != "DOWN" && direction != "HOLD" {
        return Err(AppError::bad_request(
            "SIGNAL_DIRECTION_INVALID",
            "direction must be UP, DOWN, or HOLD",
        ));
    }
    let stake = match req.suggested_stake {
        Some(value) => parse_decimal(&value, "suggested_stake")?,
        None => MIN_EVENT_STAKE,
    };
    validate_event_stake(stake)?;
    let stake = stake.floor();
    let confidence = match req.confidence {
        Some(value) => parse_decimal(&value, "confidence")?,
        None => dec!(1),
    };
    let signal = SignalCore::manual(direction, stake, confidence);
    if let Some(pool) = state.db.as_ref() {
        if !db_insert_signal(pool, &signal).await? {
            return Err(AppError::conflict(
                "SIGNAL_CYCLE_EXISTS",
                "signal already exists for this cycle",
            ));
        }
    }
    state.signals.write().await.push(signal.clone());
    let _ = state.signal_tx.send(signal.clone());
    Ok(Json(signal_api_response(&signal)))
}

async fn admin_prediction_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<PredictionSettings>, AppError> {
    ensure_admin(&state, &headers).await?;
    Ok(Json(load_prediction_settings(&state).await))
}

async fn admin_save_prediction_settings(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<PredictionSettings>,
) -> Result<Json<PredictionSettings>, AppError> {
    ensure_admin(&state, &headers).await?;
    let req = normalize_prediction_settings(req);
    validate_prediction_settings(&req)?;
    if let Some(pool) = state.db.as_ref() {
        db_save_prediction_settings(pool, &req).await?;
        return Ok(Json(db_load_prediction_settings(pool).await?));
    }
    Ok(Json(req))
}

async fn admin_prediction_observations(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PredictionObservationRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_prediction_observations(pool, 200).await?));
    }
    Ok(Json(Vec::new()))
}

async fn admin_signals(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SignalApiResponse>>, AppError> {
    ensure_admin(&state, &headers).await?;
    let signals = match state.db.as_ref() {
        Some(pool) => db_signal_history(pool, 200).await?,
        None => state
            .signals
            .read()
            .await
            .iter()
            .rev()
            .take(200)
            .cloned()
            .collect(),
    };
    Ok(Json(
        signals.iter().take(200).map(signal_api_response).collect(),
    ))
}

async fn admin_users(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<AdminUserRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_users(pool, &query).await?));
    }
    let mut rows = state
        .users
        .read()
        .await
        .values()
        .cloned()
        .map(AdminUserRecord::from)
        .collect::<Vec<_>>();
    let search = search_value(&query);
    rows.retain(|row| {
        search_matches(
            search.as_deref(),
            [
                row.wallet_address.as_str(),
                row.status.as_str(),
                row.id.to_string().as_str(),
            ],
        )
    });
    rows.sort_by_key(|row| Reverse(row.created_at));
    Ok(Json(page_vec(rows, &query)))
}

async fn admin_orders(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<EventContractOrderRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_orders(pool, &query).await?));
    }
    Ok(Json(filter_page_orders(
        state.orders.read().await.clone(),
        &query,
    )))
}

async fn admin_deposits(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<DepositRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_deposits(pool, &query).await?));
    }
    Ok(Json(filter_page_deposits(
        state.deposits.read().await.clone(),
        &query,
    )))
}

async fn admin_withdrawals(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<WithdrawalRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_withdrawals(pool, &query).await?));
    }
    Ok(Json(filter_page_withdrawals(
        state.withdrawals.read().await.clone(),
        &query,
    )))
}

async fn admin_confirm_withdrawal(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<AdminConfirmWithdrawalRequest>,
) -> Result<Json<WithdrawalRecord>, AppError> {
    ensure_admin(&state, &headers).await?;
    let tx_hash = normalize_tx_hash(&req.tx_hash)?;
    let withdrawal = if let Some(pool) = state.db.as_ref() {
        db_withdrawal_by_id(pool, req.withdrawal_id).await?
    } else {
        state
            .withdrawals
            .read()
            .await
            .iter()
            .find(|row| row.id == req.withdrawal_id)
            .cloned()
            .ok_or_else(|| AppError::bad_request("WITHDRAWAL_NOT_FOUND", "withdrawal not found"))?
    };
    let verified = verify_bsc_refund(&tx_hash, &withdrawal).await?;
    if let Some(pool) = state.db.as_ref() {
        db_confirm_withdrawal(pool, req.withdrawal_id, &verified).await?;
        return Ok(Json(db_withdrawal_by_id(pool, req.withdrawal_id).await?));
    }
    confirm_withdrawal_memory(&state, req.withdrawal_id, verified).await?;
    let row = state
        .withdrawals
        .read()
        .await
        .iter()
        .find(|row| row.id == req.withdrawal_id)
        .cloned()
        .ok_or_else(|| AppError::bad_request("WITHDRAWAL_NOT_FOUND", "withdrawal not found"))?;
    Ok(Json(row))
}

async fn admin_reject_withdrawal(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<AdminRejectWithdrawalRequest>,
) -> Result<Json<WithdrawalRecord>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        db_reject_withdrawal(pool, req.withdrawal_id, req.reason).await?;
        return Ok(Json(db_withdrawal_by_id(pool, req.withdrawal_id).await?));
    }
    reject_withdrawal_memory(&state, req.withdrawal_id, req.reason).await?;
    let row = state
        .withdrawals
        .read()
        .await
        .iter()
        .find(|row| row.id == req.withdrawal_id)
        .cloned()
        .ok_or_else(|| AppError::bad_request("WITHDRAWAL_NOT_FOUND", "withdrawal not found"))?;
    Ok(Json(row))
}

async fn admin_ledger(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<LedgerEntry>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_ledger(pool, &query).await?));
    }
    Ok(Json(filter_page_ledger(
        state.ledger.read().await.clone(),
        &query,
    )))
}

async fn admin_reports(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<ExecutionReportRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_reports(pool, &query).await?));
    }
    Ok(Json(filter_page_reports(
        state.reports.read().await.clone(),
        &query,
    )))
}

async fn admin_settle_execution_report(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<AdminSettleExecutionReportRequest>,
) -> Result<Json<ExecutionReportResponse>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        db_admin_settle_execution_report(pool, req.report_id).await?;
        return Ok(Json(ExecutionReportResponse {
            report_id: req.report_id,
            status: "settled".to_string(),
        }));
    }

    let report = state
        .reports
        .write()
        .await
        .iter_mut()
        .find(|report| report.id == req.report_id)
        .ok_or_else(|| AppError::bad_request("REPORT_NOT_FOUND", "execution report not found"))?
        .clone();
    if report.report_status != "pending_review" {
        return Err(AppError::conflict(
            "REPORT_STATUS_INVALID",
            "only pending_review reports can be settled",
        ));
    }
    settle_execution_report(&state, &report).await?;
    if let Some(report) = state
        .reports
        .write()
        .await
        .iter_mut()
        .find(|report| report.id == req.report_id)
    {
        report.report_status = "settled".to_string();
    }
    Ok(Json(ExecutionReportResponse {
        report_id: req.report_id,
        status: "settled".to_string(),
    }))
}

async fn admin_settlements(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<SettlementRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_settlements(pool, &query).await?));
    }
    Ok(Json(filter_page_settlements(
        state.settlements.read().await.clone(),
        &query,
    )))
}

async fn admin_page_states(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<HashMap<Uuid, AutomationPageState>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_page_states(pool).await?));
    }
    Ok(Json(state.page_states.read().await.clone()))
}

async fn enforce_register_rate_limit(state: &SharedState, account: &str) -> Result<(), AppError> {
    if let Some(pool) = state.db.as_ref() {
        return db_enforce_register_rate_limit(pool, account).await;
    }

    let key = normalize_account(account);
    let now = Utc::now();
    let mut limits = state.rate_limits.write().await;
    let limit = limits.entry(key).or_insert(RegisterRateLimit {
        window_start: now,
        attempts: 0,
        blocked_until: None,
    });
    if let Some(blocked_until) = limit.blocked_until {
        if blocked_until > now {
            return Err(AppError::too_many_requests(
                "AUTH_RATE_LIMITED",
                "register attempts are temporarily blocked",
            ));
        }
    }
    if now - limit.window_start > Duration::minutes(10) {
        limit.window_start = now;
        limit.attempts = 0;
        limit.blocked_until = None;
    }
    limit.attempts += 1;
    if limit.attempts > 5 {
        limit.blocked_until = Some(now + Duration::minutes(15));
        return Err(AppError::too_many_requests(
            "AUTH_RATE_LIMITED",
            "too many register attempts",
        ));
    }
    Ok(())
}

async fn create_session(
    state: &SharedState,
    user_id: Uuid,
    device: &str,
    client_type: ClientType,
) -> Result<AuthResponse, AppError> {
    let access_token = new_token("access");
    let refresh_token = new_token("refresh");
    let app_session_key = new_token("app-key");
    let expires_in = USER_SESSION_TTL_SECONDS;
    let expires_at = Utc::now() + Duration::seconds(expires_in);
    if let Some(pool) = state.db.as_ref() {
        let session_version = db_user_session_version(pool, user_id).await?;
        sqlx::query(
            r#"
            insert into auth_sessions
              (access_token, user_id, device_fingerprint, refresh_token, app_session_key, client_type, session_version, expires_at)
            values ($1, $2, $3, $4, $5, $6, $7, $8)
            "#,
        )
        .bind(&access_token)
        .bind(user_id)
        .bind(device)
        .bind(&refresh_token)
        .bind(&app_session_key)
        .bind(client_type.as_str())
        .bind(session_version)
        .bind(expires_at)
        .execute(pool)
        .await
        .map_err(AppError::from_sqlx)?;
        state.sessions.write().await.insert(
            access_token.clone(),
            SessionRecord {
                user_id,
                device_fingerprint: device.to_string(),
                refresh_token: refresh_token.clone(),
                app_session_key: app_session_key.clone(),
                client_type,
                session_version,
                expires_at,
            },
        );
        return Ok(AuthResponse {
            access_token,
            refresh_token,
            app_session_key,
            expires_in,
        });
    }
    let session_version = load_user_session_version(state, user_id).await?;
    state.sessions.write().await.insert(
        access_token.clone(),
        SessionRecord {
            user_id,
            device_fingerprint: device.to_string(),
            refresh_token: refresh_token.clone(),
            app_session_key: app_session_key.clone(),
            client_type,
            session_version,
            expires_at,
        },
    );
    Ok(AuthResponse {
        access_token,
        refresh_token,
        app_session_key,
        expires_in,
    })
}

async fn create_admin_session(
    state: &SharedState,
    admin: &AdminAccountRecord,
) -> Result<AdminLoginResponse, AppError> {
    let access_token = new_token("admin");
    let expires_in = 86_400;
    let expires_at = Utc::now() + Duration::seconds(expires_in);
    if let Some(pool) = state.db.as_ref() {
        let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            insert into admin_sessions (access_token, admin_user_id, expires_at)
            values ($1, $2, $3)
            "#,
        )
        .bind(&access_token)
        .bind(admin.id)
        .bind(expires_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            update admin_users
            set last_login_at = $2, updated_at = $2
            where id = $1
            "#,
        )
        .bind(admin.id)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            insert into admin_audit_logs (id, admin_user_id, action, target_type, target_id)
            values ($1, $2, 'admin.login', 'admin_user', $3)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(admin.id)
        .bind(admin.id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        tx.commit().await.map_err(AppError::from_sqlx)?;
    }
    state
        .admin_sessions
        .write()
        .await
        .insert(access_token.clone(), AdminSessionRecord { expires_at });
    Ok(AdminLoginResponse {
        access_token,
        expires_in,
    })
}

async fn load_user_wallet_address(state: &SharedState, user_id: Uuid) -> Result<String, AppError> {
    if let Some(user) = state.users.read().await.get(&user_id) {
        return Ok(user.wallet_address.clone());
    }
    if let Some(pool) = state.db.as_ref() {
        return db_user_wallet_address(pool, user_id).await;
    }
    Err(AppError::unauthorized(
        "AUTH_INVALID_USER",
        "user account not found",
    ))
}

async fn current_session(
    state: &SharedState,
    headers: &HeaderMap,
) -> Result<SessionRecord, AppError> {
    let token = bearer_token(headers)
        .ok_or_else(|| AppError::unauthorized("AUTH_MISSING_TOKEN", "missing bearer token"))?;
    authenticate_token(state, &token).await
}

async fn require_client(
    state: &SharedState,
    headers: &HeaderMap,
    allowed: &[ClientType],
) -> Result<Uuid, AppError> {
    let session = current_session(state, headers).await?;
    if allowed.contains(&session.client_type) {
        Ok(session.user_id)
    } else {
        Err(AppError::forbidden(
            "CLIENT_TYPE_FORBIDDEN",
            "token is not allowed to call this endpoint",
        ))
    }
}

async fn require_dapp_user(state: &SharedState, headers: &HeaderMap) -> Result<Uuid, AppError> {
    require_client(state, headers, &[ClientType::Dapp]).await
}

async fn require_android_user(state: &SharedState, headers: &HeaderMap) -> Result<Uuid, AppError> {
    let session = current_session(state, headers).await?;
    if session.client_type != ClientType::Android {
        return Err(AppError::forbidden(
            "CLIENT_TYPE_FORBIDDEN",
            "token is not allowed to call this endpoint",
        ));
    }
    verify_app_request_signature(state, headers, &session).await?;
    Ok(session.user_id)
}

async fn authenticate_token(state: &SharedState, token: &str) -> Result<SessionRecord, AppError> {
    let session = match state.sessions.read().await.get(token).cloned() {
        Some(session) => session,
        None => {
            if let Some(pool) = state.db.as_ref() {
                db_session_by_access_token(pool, token)
                    .await?
                    .ok_or_else(|| AppError::unauthorized("AUTH_INVALID_TOKEN", "invalid token"))?
            } else {
                return Err(AppError::unauthorized(
                    "AUTH_INVALID_TOKEN",
                    "invalid token",
                ));
            }
        }
    };
    if session.expires_at < Utc::now() {
        return Err(AppError::unauthorized(
            "AUTH_TOKEN_EXPIRED",
            "token expired",
        ));
    }
    if !session_device_is_current(state, &session).await {
        return Err(AppError::unauthorized(
            "AUTH_SESSION_REVOKED",
            "session is no longer active",
        ));
    }
    Ok(session)
}

async fn session_device_is_current(state: &SharedState, session: &SessionRecord) -> bool {
    if let Some(pool) = state.db.as_ref() {
        let Some(user) = db_user_by_id(pool, session.user_id).await.ok().flatten() else {
            return false;
        };
        if user.session_version != session.session_version {
            return false;
        }
        if session.client_type == ClientType::Android {
            return user.active_device_fingerprint.as_deref()
                == Some(session.device_fingerprint.as_str());
        }
        return true;
    }
    let users = state.users.read().await;
    let Some(user) = users.get(&session.user_id) else {
        return false;
    };
    if user.session_version != session.session_version {
        return false;
    }
    if session.client_type == ClientType::Android {
        return user.active_device_fingerprint.as_deref()
            == Some(session.device_fingerprint.as_str());
    }
    true
}

async fn load_user_session_version(state: &SharedState, user_id: Uuid) -> Result<i32, AppError> {
    if let Some(user) = state.users.read().await.get(&user_id) {
        return Ok(user.session_version);
    }
    if let Some(pool) = state.db.as_ref() {
        return db_user_session_version(pool, user_id).await;
    }
    Err(AppError::unauthorized(
        "AUTH_INVALID_USER",
        "user account not found",
    ))
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

async fn verify_app_request_signature(
    state: &SharedState,
    headers: &HeaderMap,
    session: &SessionRecord,
) -> Result<(), AppError> {
    let timestamp = header_str(headers, "x-es-timestamp")?;
    let nonce = header_str(headers, "x-es-nonce")?;
    let signature = header_str(headers, "x-es-signature")?;
    let ts = timestamp.parse::<i64>().map_err(|_| {
        AppError::unauthorized("APP_SIGNATURE_INVALID", "invalid app request signature")
    })?;
    let now_ms = Utc::now().timestamp_millis();
    if (now_ms - ts).abs() > 60_000 {
        return Err(AppError::unauthorized(
            "APP_SIGNATURE_EXPIRED",
            "app request signature expired",
        ));
    }
    if nonce.len() < 16 || nonce.len() > 96 {
        return Err(AppError::unauthorized(
            "APP_SIGNATURE_INVALID",
            "invalid app request signature",
        ));
    }
    let payload = format!("android:{timestamp}:{nonce}");
    let expected = hmac_sha256_hex(session.app_session_key.as_bytes(), payload.as_bytes());
    if !constant_time_eq(expected.as_bytes(), signature.as_bytes()) {
        return Err(AppError::unauthorized(
            "APP_SIGNATURE_INVALID",
            "invalid app request signature",
        ));
    }
    let nonce_key = format!("{}:{nonce}", session.user_id);
    let mut nonces = state.app_signature_nonces.write().await;
    nonces.retain(|_, expires_at| *expires_at > Utc::now());
    if nonces.contains_key(&nonce_key) {
        return Err(AppError::unauthorized(
            "APP_SIGNATURE_REPLAYED",
            "app request signature already used",
        ));
    }
    nonces.insert(nonce_key, Utc::now() + Duration::seconds(60));
    Ok(())
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, AppError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::unauthorized("APP_SIGNATURE_MISSING", "missing app request signature")
        })
}

fn hmac_sha256_hex(key: &[u8], payload: &[u8]) -> String {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(key).expect("hmac accepts any key length");
    mac.update(payload);
    hex::encode(mac.finalize().into_bytes())
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b))
        == 0
}

async fn ensure_admin(
    state: &SharedState,
    headers: &HeaderMap,
) -> Result<AdminSessionRecord, AppError> {
    let supplied = headers
        .get("x-admin-token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| bearer_token(headers))
        .ok_or_else(|| AppError::forbidden("ADMIN_FORBIDDEN", "missing admin token"))?;
    let session = match state.admin_sessions.read().await.get(&supplied).cloned() {
        Some(session) => session,
        None => {
            if let Some(pool) = state.db.as_ref() {
                db_admin_session_by_access_token(pool, &supplied)
                    .await?
                    .ok_or_else(|| AppError::forbidden("ADMIN_FORBIDDEN", "invalid admin token"))?
            } else {
                return Err(AppError::forbidden(
                    "ADMIN_FORBIDDEN",
                    "invalid admin token",
                ));
            }
        }
    };
    if session.expires_at < Utc::now() {
        return Err(AppError::unauthorized(
            "ADMIN_SESSION_EXPIRED",
            "admin session expired",
        ));
    }
    Ok(session)
}

fn validate_auth_request(req: &AuthRequest) -> Result<(), AppError> {
    let account = account_from_auth(req)?;
    normalize_wallet_address(&account)?;
    validate_password(&req.password)?;
    if req.device_fingerprint.trim().len() < 8 {
        return Err(AppError::bad_request(
            "AUTH_DEVICE_INVALID",
            "device fingerprint is required",
        ));
    }
    if let Some(code) = req.referral_code.as_deref() {
        normalize_referral_code(code)?;
    }
    Ok(())
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < 6 {
        return Err(AppError::bad_request(
            "AUTH_PASSWORD_WEAK",
            "password must be at least 6 characters",
        ));
    }
    Ok(())
}

fn normalize_admin_username(username: &str) -> Result<String, AppError> {
    let username = username.trim();
    if username.len() < 3 || username.len() > 64 {
        return Err(AppError::bad_request(
            "ADMIN_USERNAME_INVALID",
            "admin username length is invalid",
        ));
    }
    if !username
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.')
    {
        return Err(AppError::bad_request(
            "ADMIN_USERNAME_INVALID",
            "admin username contains invalid characters",
        ));
    }
    Ok(username.to_string())
}

fn validate_platform_trading_settings(settings: &PlatformTradingSettings) -> Result<(), AppError> {
    if settings.stake_percent < dec!(0.01) || settings.stake_percent > dec!(0.20) {
        return Err(AppError::bad_request(
            "PLATFORM_SETTING_OUT_OF_RANGE",
            "stake_percent must be between 0.01 and 0.20",
        ));
    }
    if !(MIN_EVENT_STAKE..=MAX_EVENT_STAKE).contains(&settings.min_stake_usdt) {
        return Err(AppError::bad_request(
            "PLATFORM_SETTING_OUT_OF_RANGE",
            "min_stake_usdt must be between 5 and 250",
        ));
    }
    if !(MIN_EVENT_STAKE..=MAX_EVENT_STAKE).contains(&settings.max_single_stake_usdt) {
        return Err(AppError::bad_request(
            "PLATFORM_SETTING_OUT_OF_RANGE",
            "max_single_stake_usdt must be between 5 and 250",
        ));
    }
    if settings.max_active_event_orders_per_user < 1
        || settings.max_active_event_orders_per_user > 20
    {
        return Err(AppError::bad_request(
            "PLATFORM_SETTING_OUT_OF_RANGE",
            "max_active_event_orders_per_user must be between 1 and 20",
        ));
    }
    if settings.first_deposit_min_usdt < MIN_DEPOSIT_AMOUNT {
        return Err(AppError::bad_request(
            "PLATFORM_SETTING_OUT_OF_RANGE",
            "first_deposit_min_usdt must be at least the deposit minimum",
        ));
    }
    for (name, value) in [
        ("deposit_fee_rate", settings.deposit_fee_rate),
        ("direct_referral_rate", settings.direct_referral_rate),
        ("indirect_referral_rate", settings.indirect_referral_rate),
    ] {
        if value < Decimal::ZERO || value > dec!(0.50) {
            return Err(AppError::bad_request(
                "PLATFORM_SETTING_OUT_OF_RANGE",
                format!("{name} must be between 0 and 0.50"),
            ));
        }
    }
    if settings.min_stake_usdt > settings.max_single_stake_usdt {
        return Err(AppError::bad_request(
            "PLATFORM_SETTING_OUT_OF_RANGE",
            "min_stake_usdt cannot exceed max_single_stake_usdt",
        ));
    }
    Ok(())
}

fn validate_prediction_settings(settings: &PredictionSettings) -> Result<(), AppError> {
    if settings.interval_seconds < 1 || settings.interval_seconds > 60 {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "interval_seconds must be between 1 and 60",
        ));
    }
    if settings.up_threshold <= Decimal::ZERO {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "up_threshold must be positive",
        ));
    }
    if settings.down_threshold >= Decimal::ZERO {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "down_threshold must be negative",
        ));
    }
    if settings.min_confidence < dec!(0.62) || settings.min_confidence > dec!(0.99) {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "min_confidence must be between 0.62 and 0.99",
        ));
    }
    if settings.technical_weight < Decimal::ZERO || settings.technical_weight > dec!(1.00) {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "technical_weight must be between 0 and 1",
        ));
    }
    if settings.probability_weight < Decimal::ZERO || settings.probability_weight > dec!(1.00) {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "probability_weight must be between 0 and 1",
        ));
    }
    if settings.technical_weight + settings.probability_weight <= Decimal::ZERO {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "at least one prediction score weight must be positive",
        ));
    }
    if settings.volatility_floor < dec!(0.00001) || settings.volatility_floor > dec!(0.00200) {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "volatility_floor must be between 0.00001 and 0.002",
        ));
    }
    if settings.calibration_lookback_minutes < 10 || settings.calibration_lookback_minutes > 1440 {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "calibration_lookback_minutes must be between 10 and 1440",
        ));
    }
    if settings.calibration_min_samples < 10 || settings.calibration_min_samples > 300 {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "calibration_min_samples must be between 10 and 300",
        ));
    }
    if settings.calibration_min_win_rate < dec!(0.60)
        || settings.calibration_min_win_rate > dec!(0.95)
    {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "calibration_min_win_rate must be between 0.60 and 0.95",
        ));
    }
    if settings.calibration_min_ev_usdt < dec!(0.01)
        || settings.calibration_min_ev_usdt > dec!(5.00)
    {
        return Err(AppError::bad_request(
            "PREDICTION_SETTING_OUT_OF_RANGE",
            "calibration_min_ev_usdt must be between 0.01 and 5",
        ));
    }
    Ok(())
}

fn validate_automation_settings(settings: &AutomationSettings) -> Result<(), AppError> {
    if settings.execution_mode != "automatic" {
        return Err(AppError::bad_request(
            "AUTOMATION_SETTING_OUT_OF_RANGE",
            "execution_mode must be automatic",
        ));
    }
    if settings.require_final_confirmation {
        return Err(AppError::bad_request(
            "AUTOMATION_SETTING_OUT_OF_RANGE",
            "automatic mode cannot require final confirmation",
        ));
    }
    Ok(())
}

fn validate_event_stake(stake: Decimal) -> Result<(), AppError> {
    if !(MIN_EVENT_STAKE..=MAX_EVENT_STAKE).contains(&stake) {
        return Err(AppError::bad_request(
            "STAKE_OUT_OF_RANGE",
            "event contract stake must be between 5 and 250 USDT",
        ));
    }
    Ok(())
}

async fn load_automation_settings(state: &SharedState, user_id: Uuid) -> AutomationSettings {
    if let Some(pool) = state.db.as_ref() {
        if let Ok(Some(settings)) = db_load_automation_settings(pool, user_id).await {
            return settings;
        }
    }
    state
        .automation_settings
        .read()
        .await
        .get(&user_id)
        .cloned()
        .unwrap_or_default()
}

async fn load_trading_settings(state: &SharedState, user_id: Uuid) -> TradingSettings {
    if let Some(pool) = state.db.as_ref() {
        if let Ok(settings) = db_load_platform_trading_settings(pool).await {
            return TradingSettings::from(&settings);
        }
    }
    state
        .trading_settings
        .read()
        .await
        .get(&user_id)
        .cloned()
        .unwrap_or_default()
}

async fn load_platform_settings(state: &SharedState) -> PlatformTradingSettings {
    if let Some(pool) = state.db.as_ref() {
        if let Ok(settings) = db_load_platform_trading_settings(pool).await {
            return settings;
        }
    }
    PlatformTradingSettings::default()
}

async fn load_prediction_settings(state: &SharedState) -> PredictionSettings {
    if let Some(pool) = state.db.as_ref() {
        if let Ok(settings) = db_load_prediction_settings(pool).await {
            return settings;
        }
    }
    PredictionSettings::default()
}

async fn load_balance(state: &SharedState, user_id: Uuid) -> BalanceRecord {
    if let Some(pool) = state.db.as_ref() {
        if let Ok(Some(balance)) = db_load_balance(pool, user_id).await {
            return balance;
        }
    }
    state
        .balances
        .read()
        .await
        .get(&user_id)
        .cloned()
        .unwrap_or(BalanceRecord {
            user_id,
            asset: "USDT".to_string(),
            available: Decimal::ZERO,
            frozen: Decimal::ZERO,
            updated_at: Utc::now(),
        })
}

async fn load_exchange_available_balance(
    state: &SharedState,
    user_id: Uuid,
) -> Result<Option<Decimal>, AppError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(None);
    };
    db_exchange_available_balance(pool, user_id).await
}

async fn load_exchange_balance_snapshot(
    state: &SharedState,
    user_id: Uuid,
) -> Result<Option<ExchangeBalanceSnapshot>, AppError> {
    let Some(pool) = state.db.as_ref() else {
        return Ok(None);
    };
    db_exchange_balance_snapshot(pool, user_id).await
}

fn balance_cache_fresh(updated_at: Option<DateTime<Utc>>) -> bool {
    updated_at
        .map(|value| Utc::now() - value <= Duration::seconds(ACCOUNT_BALANCE_CACHE_TTL_SECONDS))
        .unwrap_or(false)
}

fn account_match_from_balances(
    observed_balance: Option<Decimal>,
    exchange_available: Option<Decimal>,
) -> AccountMatch {
    match (observed_balance, exchange_available) {
        (Some(observed), Some(exchange)) => {
            let delta = (observed - exchange).abs();
            AccountMatch {
                status: if delta <= ACCOUNT_BALANCE_TOLERANCE_USDT {
                    "matched".to_string()
                } else {
                    "mismatch".to_string()
                },
                delta: Some(delta),
            }
        }
        (None, Some(_)) => AccountMatch {
            status: "observed_balance_missing".to_string(),
            delta: None,
        },
        (Some(_), None) => AccountMatch {
            status: "exchange_balance_missing".to_string(),
            delta: None,
        },
        (None, None) => AccountMatch {
            status: "unknown".to_string(),
            delta: None,
        },
    }
}

async fn display_signal_for_user(
    state: &SharedState,
    user_id: Uuid,
    signal: &SignalCore,
) -> SignalApiResponse {
    let settings = load_trading_settings(state, user_id).await;
    let balance = load_balance(state, user_id).await;
    let exchange_available = load_exchange_available_balance(state, user_id)
        .await
        .ok()
        .flatten();
    let stake_base = exchange_available.unwrap_or(balance.available);
    let explicit_stake = signal
        .uses_explicit_stake()
        .then_some(signal.suggested_stake);
    let stake = calculated_stake(stake_base, &settings, explicit_stake);
    let stake_rule = if explicit_stake.is_some() {
        "手动信号按后台填写金额下发；latest/history 不创建预扣"
    } else {
        "展示值按缓存的 U 本位合约可用余额和后台比例计算；latest/history 不创建预扣"
    };
    SignalApiResponse {
        id: signal.id.to_string(),
        signal_id: signal.id.to_string(),
        symbol: signal.symbol.clone(),
        period_seconds: signal.period_seconds,
        cycle_start: signal.cycle_start,
        cycle_end: signal.cycle_end,
        direction: signal.direction.clone(),
        confidence: decimal(signal.confidence),
        predicted_return: decimal(signal.predicted_return),
        entry_price: money(signal.entry_price),
        suggested_stake: money(stake),
        stake_percent: decimal(settings.stake_percent),
        suggested_stake_rule: stake_rule.to_string(),
        trade_authorized: false,
        reservation_id: None,
        fee_reserve_usdt: money(Decimal::ZERO),
        authorization_status: "not_reserved".to_string(),
        account_match_status: "not_checked".to_string(),
        observed_balance_usdt: None,
        account_balance_delta_usdt: None,
        balance_available_after_reserve: money(balance.available),
        model_version: signal.model_version.clone(),
        created_at: signal.created_at,
        outcome_result: signal.outcome_result.clone(),
        outcome_expiry_price: signal.outcome_expiry_price.map(money),
        outcome_settled_at: signal.outcome_settled_at,
    }
}

async fn authorize_signal_for_user(
    state: &SharedState,
    user_id: Uuid,
    signal: &SignalCore,
) -> SignalApiResponse {
    let platform_settings = load_platform_settings(state).await;
    let trading = if state.db.is_some() {
        TradingSettings::from(&platform_settings)
    } else {
        load_trading_settings(state, user_id).await
    };
    let automation = load_automation_settings(state, user_id).await;
    let balance = load_balance(state, user_id).await;
    let exchange_snapshot = match load_exchange_balance_snapshot(state, user_id).await {
        Ok(value) => value,
        Err(err) => {
            return signal_response_with_status(
                signal,
                &trading,
                MIN_EVENT_STAKE,
                Decimal::ZERO,
                balance.available,
                err.code,
            )
        }
    };
    let exchange_available = exchange_snapshot
        .as_ref()
        .map(|snapshot| snapshot.available);
    let stake_base = exchange_available.unwrap_or(Decimal::ZERO);
    let explicit_stake = signal
        .uses_explicit_stake()
        .then_some(signal.suggested_stake);
    let stake = calculated_stake(stake_base, &trading, explicit_stake);
    let stake_rule = if explicit_stake.is_some() {
        "手动信号按后台填写金额下发；预扣 = 预计盈利 * 30%"
    } else {
        "U 本位合约可用余额 * 后台比例，金额向下取整，最低 5 USDT，最高 250 USDT；预扣 = 预计盈利 * 30%"
    };
    let expected_profit = stake * DEFAULT_PAYOUT_RATIO;
    let fee_reserve = expected_profit * PROFIT_SHARE_RATE;

    let mut status = "reserved".to_string();
    if exchange_snapshot.is_none() {
        status = "exchange_balance_missing".to_string();
    }
    if status == "reserved"
        && !balance_cache_fresh(
            exchange_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.updated_at),
        )
    {
        status = "exchange_balance_stale".to_string();
    }
    if status == "reserved" && stake_base < stake {
        status = "exchange_balance_insufficient".to_string();
    }
    if !automation.automation_enabled
        || automation.execution_mode != "automatic"
        || automation.require_final_confirmation
    {
        status = "automation_gate_closed".to_string();
    }
    let page = match state.db.as_ref() {
        Some(pool) => db_load_page_state(pool, user_id).await.unwrap_or(None),
        None => None,
    };
    let page = match page {
        Some(page) => Some(page),
        None => state.page_states.read().await.get(&user_id).cloned(),
    };
    let mut account_match = AccountMatch {
        status: "unknown".to_string(),
        delta: None,
    };
    if status == "reserved" {
        match page.as_ref() {
            None => status = "page_state_missing".to_string(),
            Some(page) if !page_is_fresh(page) => status = "page_state_stale".to_string(),
            Some(page) if !page_is_fresh_and_ready(page) => status = "page_not_ready".to_string(),
            Some(page) => {
                account_match =
                    account_match_from_balances(page.observed_balance_usdt, exchange_available);
                if account_match.status != "matched" {
                    status = account_match.status.clone();
                }
            }
        }
    }
    if status == "reserved" && balance.available < fee_reserve {
        status = "fee_reserve_insufficient".to_string();
    }
    if status == "reserved" && state.db.is_none() {
        let active_orders =
            active_event_order_count_memory(state, user_id, signal.cycle_start).await;
        if active_orders >= max_active_event_orders_per_user(&platform_settings) {
            status = "active_order_limit_reached".to_string();
        }
    }

    let mut reservation_id = None;
    let mut balance_after = balance.available;
    if status == "reserved" {
        let now = Utc::now();
        let id = Uuid::now_v7();
        let reservation = SignalReservation {
            id,
            user_id,
            signal_id: signal.id,
            stake_usdt: stake,
            expected_profit_usdt: expected_profit,
            fee_reserve_usdt: fee_reserve,
            status: "reserved".to_string(),
            order_id: None,
            execution_report_id: None,
            expires_at: now + Duration::seconds(DEFAULT_RESERVATION_ORDER_TIMEOUT_SECONDS),
            created_at: now,
            updated_at: now,
        };

        if let Some(pool) = state.db.as_ref() {
            match db_reserve_or_load_signal_fee(pool, &reservation, &platform_settings).await {
                Ok(after) => {
                    balance_after = after.available;
                    reservation_id = Some(id);
                }
                Err(err) if err.code == "SIGNAL_RESERVATION_ALREADY_EXISTS" => {
                    match db_signal_reservation_for_user_signal(pool, user_id, signal.id).await {
                        Ok(Some(existing)) if existing.status == "reserved" => {
                            status = "reservation_already_open".to_string();
                            balance_after = balance.available;
                        }
                        Ok(Some(_)) => {
                            status = "reservation_already_used".to_string();
                        }
                        _ => {
                            status = "reservation_conflict".to_string();
                        }
                    }
                }
                Err(err) if err.code == "BALANCE_INSUFFICIENT" => {
                    status = "fee_reserve_insufficient".to_string();
                }
                Err(err) if err.code == "ACTIVE_ORDER_LIMIT_REACHED" => {
                    status = "active_order_limit_reached".to_string();
                }
                Err(err) => {
                    return signal_response_with_status(
                        signal,
                        &trading,
                        stake,
                        fee_reserve,
                        balance_after,
                        err.code,
                    )
                }
            }
        } else {
            let mut balances = state.balances.write().await;
            if let Some(balance) = balances.get_mut(&user_id) {
                if balance.available >= fee_reserve {
                    balance.available -= fee_reserve;
                    balance.frozen += fee_reserve;
                    balance.updated_at = now;
                    balance_after = balance.available;
                    reservation_id = Some(id);
                    state
                        .reservations
                        .write()
                        .await
                        .insert(id, reservation.clone());
                    state.ledger.write().await.push(LedgerEntry {
                        id: Uuid::now_v7(),
                        user_id,
                        asset: "USDT".to_string(),
                        entry_type: "signal_fee_reserve".to_string(),
                        amount: -fee_reserve,
                        balance_available_after: balance.available,
                        balance_frozen_after: balance.frozen,
                        ref_type: "signal_reservation".to_string(),
                        ref_id: id,
                        idempotency_key: format!("signal_fee_reserve:{id}"),
                        created_at: now,
                    });
                } else {
                    status = "fee_reserve_insufficient".to_string();
                }
            }
        }
        if status == "reserved" && state.db.is_none() && reservation_id.is_some() {
            state.reservations.write().await.insert(id, reservation);
        }
    }

    SignalApiResponse {
        id: signal.id.to_string(),
        signal_id: signal.id.to_string(),
        symbol: signal.symbol.clone(),
        period_seconds: signal.period_seconds,
        cycle_start: signal.cycle_start,
        cycle_end: signal.cycle_end,
        direction: signal.direction.clone(),
        confidence: decimal(signal.confidence),
        predicted_return: decimal(signal.predicted_return),
        entry_price: money(signal.entry_price),
        suggested_stake: money(stake),
        stake_percent: decimal(trading.stake_percent),
        suggested_stake_rule: stake_rule.to_string(),
        trade_authorized: status == "reserved",
        reservation_id: reservation_id.map(|id| id.to_string()),
        fee_reserve_usdt: money(fee_reserve),
        authorization_status: status,
        account_match_status: account_match.status,
        observed_balance_usdt: page
            .as_ref()
            .and_then(|page| page.observed_balance_usdt)
            .map(money),
        account_balance_delta_usdt: account_match.delta.map(money),
        balance_available_after_reserve: money(balance_after),
        model_version: signal.model_version.clone(),
        created_at: signal.created_at,
        outcome_result: signal.outcome_result.clone(),
        outcome_expiry_price: signal.outcome_expiry_price.map(money),
        outcome_settled_at: signal.outcome_settled_at,
    }
}

fn signal_api_response(signal: &SignalCore) -> SignalApiResponse {
    SignalApiResponse {
        id: signal.id.to_string(),
        signal_id: signal.id.to_string(),
        symbol: signal.symbol.clone(),
        period_seconds: signal.period_seconds,
        cycle_start: signal.cycle_start,
        cycle_end: signal.cycle_end,
        direction: signal.direction.clone(),
        confidence: decimal(signal.confidence),
        predicted_return: decimal(signal.predicted_return),
        entry_price: money(signal.entry_price),
        suggested_stake: money(signal.suggested_stake),
        stake_percent: decimal(DEFAULT_STAKE_PERCENT),
        suggested_stake_rule: "admin preview".to_string(),
        trade_authorized: false,
        reservation_id: None,
        fee_reserve_usdt: money(signal.suggested_stake * DEFAULT_PAYOUT_RATIO * PROFIT_SHARE_RATE),
        authorization_status: "admin_preview".to_string(),
        account_match_status: "not_checked".to_string(),
        observed_balance_usdt: None,
        account_balance_delta_usdt: None,
        balance_available_after_reserve: money(Decimal::ZERO),
        model_version: signal.model_version.clone(),
        created_at: signal.created_at,
        outcome_result: signal.outcome_result.clone(),
        outcome_expiry_price: signal.outcome_expiry_price.map(money),
        outcome_settled_at: signal.outcome_settled_at,
    }
}

fn signal_response_with_status(
    signal: &SignalCore,
    trading: &TradingSettings,
    stake: Decimal,
    fee_reserve: Decimal,
    balance_after: Decimal,
    status: &str,
) -> SignalApiResponse {
    SignalApiResponse {
        id: signal.id.to_string(),
        signal_id: signal.id.to_string(),
        symbol: signal.symbol.clone(),
        period_seconds: signal.period_seconds,
        cycle_start: signal.cycle_start,
        cycle_end: signal.cycle_end,
        direction: signal.direction.clone(),
        confidence: decimal(signal.confidence),
        predicted_return: decimal(signal.predicted_return),
        entry_price: money(signal.entry_price),
        suggested_stake: money(stake),
        stake_percent: decimal(trading.stake_percent),
        suggested_stake_rule:
            "U本位合约可用余额 * 后台比例，最低 5 USDT，最高 250 USDT；预扣 = 预计盈利 * 30%"
                .to_string(),
        trade_authorized: false,
        reservation_id: None,
        fee_reserve_usdt: money(fee_reserve),
        authorization_status: status.to_string(),
        account_match_status: "not_checked".to_string(),
        observed_balance_usdt: None,
        account_balance_delta_usdt: None,
        balance_available_after_reserve: money(balance_after),
        model_version: signal.model_version.clone(),
        created_at: signal.created_at,
        outcome_result: signal.outcome_result.clone(),
        outcome_expiry_price: signal.outcome_expiry_price.map(money),
        outcome_settled_at: signal.outcome_settled_at,
    }
}

async fn sync_exchange_orders(
    pool: &PgPool,
    config: &ExchangeWorkerConfig,
) -> Result<ExchangeSyncResponse, AppError> {
    let refreshed_balances = refresh_exchange_balances(pool, config).await?;
    let pending_orders = db_pending_exchange_orders(
        pool,
        env_i64(
            "BINANCE_ORDER_CONFIRM_LIMIT",
            DEFAULT_BINANCE_ORDER_CONFIRM_LIMIT,
            1,
        ),
        config,
    )
    .await?;
    let mut checked_orders = 0usize;
    let mut confirmed_orders = 0usize;
    for order in pending_orders {
        checked_orders += 1;
        if confirm_order_from_binance_income(pool, &order, config).await? {
            confirmed_orders += 1;
        }
    }

    let due_orders = db_confirmed_exchange_orders_due_for_settlement(
        pool,
        Utc::now(),
        env_i64(
            "BINANCE_SETTLEMENT_SYNC_LIMIT",
            DEFAULT_BINANCE_SETTLEMENT_SYNC_LIMIT,
            1,
        ),
        config,
    )
    .await?;
    let mut settled_reports = 0usize;
    let mut pending_settlements = 0usize;
    for order in due_orders {
        match settle_order_from_binance_income(pool, &order, config).await? {
            true => settled_reports += 1,
            false => pending_settlements += 1,
        }
    }

    Ok(ExchangeSyncResponse {
        refreshed_balances,
        checked_orders,
        confirmed_orders,
        settled_reports,
        pending_settlements,
    })
}

async fn refresh_exchange_balances(
    pool: &PgPool,
    config: &ExchangeWorkerConfig,
) -> Result<usize, AppError> {
    let now = Utc::now();
    let credentials = db_exchange_credentials_for_balance_refresh(
        pool,
        now,
        env_i64(
            "BINANCE_BALANCE_REFRESH_LIMIT",
            DEFAULT_BINANCE_BALANCE_REFRESH_LIMIT,
            1,
        ),
        config,
    )
    .await?;
    let mut refreshed = 0usize;
    for credential in credentials {
        let api_secret = match decrypt_secret(&credential.api_secret_ciphertext) {
            Ok(secret) => secret,
            Err(err) => {
                db_mark_exchange_error(pool, credential.user_id, &err.message, Utc::now()).await?;
                continue;
            }
        };
        match binance_account_access_with_bind(
            &credential.api_key,
            &api_secret,
            config.bind_local_ip,
        )
        .await
        {
            Ok(snapshot) => {
                db_update_exchange_balance(pool, credential.user_id, &snapshot, Utc::now()).await?;
                refreshed += 1;
            }
            Err(err) => {
                db_mark_exchange_error(pool, credential.user_id, &err.message, Utc::now()).await?;
            }
        }
    }
    Ok(refreshed)
}

async fn confirm_order_from_binance_income(
    pool: &PgPool,
    order: &EventContractOrderRecord,
    config: &ExchangeWorkerConfig,
) -> Result<bool, AppError> {
    let Some(credentials) = db_exchange_credentials(pool, order.user_id).await? else {
        return Ok(false);
    };
    let api_secret = decrypt_secret(&credentials.api_secret_ciphertext)?;
    let anchor = order.reported_at.unwrap_or(order.created_at);
    let start = anchor - Duration::minutes(BINANCE_INCOME_LOOKBACK_MINUTES);
    let end = anchor + Duration::minutes(BINANCE_INCOME_LOOKBACK_MINUTES);
    let rows = binance_income_events_with_client(
        &credentials.api_key,
        &api_secret,
        start,
        end,
        100,
        config.bind_local_ip,
    )
    .await?;
    let mut matches = Vec::new();
    for row in rows {
        if row.income_type != "EVENT_CONTRACTS_ORDER" {
            continue;
        }
        let Some(event) = binance_income_to_record(order.user_id, &row)? else {
            continue;
        };
        if event.asset != "USDT" {
            continue;
        }
        if event.income != -order.stake_usdt {
            continue;
        }
        let delta = (event.event_time - anchor).num_seconds().abs();
        matches.push((delta, event));
    }
    matches.sort_by_key(|(delta, _)| *delta);
    let Some((_, event)) = matches.into_iter().next() else {
        return Ok(false);
    };
    db_confirm_order_from_exchange(pool, order.id, &event).await
}

async fn settle_order_from_binance_income(
    pool: &PgPool,
    order: &EventContractOrderRecord,
    config: &ExchangeWorkerConfig,
) -> Result<bool, AppError> {
    let Some(exchange_ref) = db_order_exchange_ref(pool, order.id).await? else {
        return Ok(false);
    };
    let Some(credentials) = db_exchange_credentials(pool, order.user_id).await? else {
        return Ok(false);
    };
    let api_secret = decrypt_secret(&credentials.api_secret_ciphertext)?;
    let anchor = order.reported_at.unwrap_or(order.created_at);
    let start = anchor - Duration::minutes(1);
    let end =
        anchor + Duration::seconds(order.period_seconds as i64 + BINANCE_SETTLEMENT_GRACE_SECONDS);
    if Utc::now() < end {
        return Ok(false);
    }
    let rows = binance_income_events_with_client(
        &credentials.api_key,
        &api_secret,
        start,
        Utc::now(),
        100,
        config.bind_local_ip,
    )
    .await?;
    let mut order_event: Option<BinanceIncomeEventRecord> = None;
    let mut payout_event: Option<BinanceIncomeEventRecord> = None;
    for row in rows {
        let Some(event) = binance_income_to_record(order.user_id, &row)? else {
            continue;
        };
        if event.external_ref != exchange_ref || event.asset != "USDT" {
            continue;
        }
        if event.income_type == "EVENT_CONTRACTS_ORDER" && event.income == -order.stake_usdt {
            order_event = Some(event);
        } else if event.income_type == "EVENT_CONTRACTS_PAYOUT" && event.income > Decimal::ZERO {
            match payout_event.as_ref() {
                Some(existing) if existing.event_time >= event.event_time => {}
                _ => payout_event = Some(event),
            }
        }
    }
    if order_event.is_none() {
        return Ok(false);
    }
    let payout = payout_event
        .as_ref()
        .map(|event| event.income)
        .unwrap_or(Decimal::ZERO);
    let gross_profit = payout - order.stake_usdt;
    let result = if gross_profit > Decimal::ZERO {
        "WIN"
    } else {
        "LOSS"
    };
    let outcome_expiry_price = db_signal_outcome_expiry_price(pool, &order.signal_id).await?;
    let now = Utc::now();
    let report = ExecutionReportRecord {
        id: Uuid::now_v7(),
        user_id: order.user_id,
        signal_id: order.signal_id.clone(),
        client_report_id: format!("binance-income:{}:{}", order.id, exchange_ref),
        source: "binance_income".to_string(),
        symbol: order.symbol.clone(),
        period_seconds: order.period_seconds,
        direction: order.direction.clone(),
        stake_usdt: order.stake_usdt,
        entry_price: order.entry_price,
        expiry_price: outcome_expiry_price,
        payout_ratio: Some(DEFAULT_PAYOUT_RATIO),
        gross_profit: Some(gross_profit),
        result: result.to_string(),
        automation_profile: order.automation_profile.clone(),
        automation_confidence: order.automation_confidence,
        action_trace_id: order.action_trace_id.clone(),
        report_status: "pending_review".to_string(),
        reported_at: now,
        created_at: now,
    };
    db_insert_exchange_execution_report_and_settle(pool, &report, payout_event.as_ref()).await
}

async fn db_signal_outcome_expiry_price(
    pool: &PgPool,
    signal_ref: &str,
) -> Result<Option<Decimal>, AppError> {
    let Ok(signal_id) = Uuid::parse_str(signal_ref) else {
        return Ok(None);
    };
    sqlx::query(
        r#"
        select expiry_price
        from signal_outcomes
        where signal_id = $1
        "#,
    )
    .bind(signal_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.and_then(|row| row.get("expiry_price")))
    .map_err(AppError::from_sqlx)
}

fn calculated_stake(
    available: Decimal,
    settings: &TradingSettings,
    explicit: Option<Decimal>,
) -> Decimal {
    let mut stake = explicit.unwrap_or(available * settings.stake_percent);
    if stake < settings.min_stake_usdt {
        stake = settings.min_stake_usdt;
    }
    if let Some(max) = settings.max_single_stake_usdt {
        if stake > max {
            stake = max;
        }
    }
    if stake < MIN_EVENT_STAKE {
        stake = MIN_EVENT_STAKE;
    }
    if stake > MAX_EVENT_STAKE {
        stake = MAX_EVENT_STAKE;
    }
    stake.floor()
}

fn max_active_event_orders_per_user(settings: &PlatformTradingSettings) -> usize {
    usize::try_from(settings.max_active_event_orders_per_user)
        .ok()
        .filter(|value| *value >= 1)
        .unwrap_or(DEFAULT_MAX_ACTIVE_EVENT_ORDERS_PER_USER)
}

async fn active_event_order_count_memory(
    state: &SharedState,
    user_id: Uuid,
    cycle_start: DateTime<Utc>,
) -> usize {
    let signals = state.signals.read().await;
    let signal_cycles = signals
        .iter()
        .filter(|signal| {
            signal.symbol == DEFAULT_SYMBOL
                && signal.period_seconds == DEFAULT_PERIOD_SECONDS
                && signal.cycle_start <= cycle_start
                && signal.cycle_end > cycle_start
        })
        .map(|signal| signal.id)
        .collect::<std::collections::HashSet<_>>();
    if signal_cycles.is_empty() {
        return 0;
    }
    state
        .reservations
        .read()
        .await
        .values()
        .filter(|reservation| {
            reservation.user_id == user_id
                && matches!(reservation.status.as_str(), "reserved" | "order_submitted")
                && signal_cycles.contains(&reservation.signal_id)
        })
        .count()
}

fn page_is_fresh(page: &AutomationPageState) -> bool {
    Utc::now() - page.observed_at <= Duration::seconds(PAGE_GATE_TTL_SECONDS)
}

fn page_is_fresh_and_ready(page: &AutomationPageState) -> bool {
    page_is_fresh(page)
        && page.page_ready
        && page.screen_name == "event_contract"
        && page.amount_field_found
        && page.up_button_found
        && page.down_button_found
}

fn validate_order_against_reservation(
    order: &EventContractOrderRecord,
    reservation: &SignalReservation,
    signal_id: Uuid,
) -> Result<(), AppError> {
    if reservation.user_id != order.user_id {
        return Err(AppError::forbidden(
            "RESERVATION_USER_MISMATCH",
            "reservation does not belong to this user",
        ));
    }
    if reservation.signal_id != signal_id {
        return Err(AppError::bad_request(
            "RESERVATION_SIGNAL_MISMATCH",
            "reservation does not match signal_id",
        ));
    }
    if reservation.stake_usdt != order.stake_usdt {
        return Err(AppError::bad_request(
            "RESERVATION_STAKE_MISMATCH",
            "order stake does not match reservation",
        ));
    }
    if order.symbol != DEFAULT_SYMBOL || order.period_seconds != DEFAULT_PERIOD_SECONDS {
        return Err(AppError::bad_request(
            "ORDER_CONTRACT_MISMATCH",
            "order symbol or period does not match event contract settings",
        ));
    }
    Ok(())
}

fn validate_report_against_reservation(
    report: &ExecutionReportRecord,
    reservation: &SignalReservation,
    signal_id: Uuid,
) -> Result<(), AppError> {
    if reservation.user_id != report.user_id {
        return Err(AppError::forbidden(
            "RESERVATION_USER_MISMATCH",
            "reservation does not belong to this user",
        ));
    }
    if reservation.signal_id != signal_id {
        return Err(AppError::bad_request(
            "RESERVATION_SIGNAL_MISMATCH",
            "reservation does not match signal_id",
        ));
    }
    if reservation.stake_usdt != report.stake_usdt {
        return Err(AppError::bad_request(
            "RESERVATION_STAKE_MISMATCH",
            "report stake does not match reservation",
        ));
    }
    if report.symbol != DEFAULT_SYMBOL || report.period_seconds != DEFAULT_PERIOD_SECONDS {
        return Err(AppError::bad_request(
            "REPORT_CONTRACT_MISMATCH",
            "report symbol or period does not match event contract settings",
        ));
    }
    Ok(())
}

async fn settle_execution_report(
    state: &SharedState,
    report: &ExecutionReportRecord,
) -> Result<(), AppError> {
    if let Some(pool) = state.db.as_ref() {
        return db_settle_execution_report(pool, report).await;
    }

    let signal_id = Uuid::parse_str(&report.signal_id).ok();
    let reservation = match signal_id {
        Some(signal_id) => state
            .reservations
            .read()
            .await
            .values()
            .find(|reservation| {
                reservation.user_id == report.user_id && reservation.signal_id == signal_id
            })
            .cloned(),
        None => None,
    };

    let result = report.result.to_uppercase();
    let gross_profit = if result == "WIN" {
        report
            .gross_profit
            .unwrap_or(report.stake_usdt * report.payout_ratio.unwrap_or(DEFAULT_PAYOUT_RATIO))
    } else if result == "LOSS" {
        report.gross_profit.unwrap_or(-report.stake_usdt)
    } else {
        Decimal::ZERO
    };
    let profit_share_fee = if result == "WIN" {
        gross_profit.max(Decimal::ZERO) * PROFIT_SHARE_RATE
    } else {
        Decimal::ZERO
    };
    let loss_rebate = if result == "LOSS" {
        report.stake_usdt * PROFIT_SHARE_RATE
    } else {
        Decimal::ZERO
    };
    let user_net_profit = gross_profit - profit_share_fee + loss_rebate;

    let mut balances = state.balances.write().await;
    let balance = balances
        .get_mut(&report.user_id)
        .ok_or_else(|| AppError::bad_request("BALANCE_NOT_FOUND", "balance not found"))?;
    let reserved = reservation
        .as_ref()
        .map(|reservation| reservation.fee_reserve_usdt)
        .unwrap_or(Decimal::ZERO);
    if reserved > Decimal::ZERO {
        let release = reserved.min(balance.frozen);
        balance.frozen -= release;
        balance.available += release;
    }
    if profit_share_fee > Decimal::ZERO {
        if balance.available >= profit_share_fee {
            balance.available -= profit_share_fee;
        } else {
            balance.available = Decimal::ZERO;
        }
    }
    if loss_rebate > Decimal::ZERO {
        balance.available += loss_rebate;
    }
    balance.updated_at = Utc::now();
    let balance_after = balance.clone();
    drop(balances);

    let settlement = SettlementRecord {
        id: Uuid::now_v7(),
        execution_report_id: report.id,
        user_id: report.user_id,
        signal_id: Some(report.signal_id.clone()),
        symbol: Some(report.symbol.clone()),
        direction: Some(report.direction.clone()),
        stake_usdt: Some(report.stake_usdt),
        entry_price: report.entry_price,
        expiry_price: report.expiry_price,
        result: Some(report.result.clone()),
        gross_profit,
        profit_share_fee,
        loss_rebate,
        user_net_profit,
        status: "settled".to_string(),
        created_at: Utc::now(),
    };
    state.settlements.write().await.push(settlement.clone());
    if let Some(reservation) = reservation {
        if let Some(row) = state.reservations.write().await.get_mut(&reservation.id) {
            row.status = "settled".to_string();
            row.execution_report_id = Some(report.id);
            row.updated_at = Utc::now();
        }
    }
    state.ledger.write().await.push(LedgerEntry {
        id: Uuid::now_v7(),
        user_id: report.user_id,
        asset: "USDT".to_string(),
        entry_type: if profit_share_fee > Decimal::ZERO {
            "profit_share_fee".to_string()
        } else {
            "loss_rebate".to_string()
        },
        amount: if profit_share_fee > Decimal::ZERO {
            -profit_share_fee
        } else {
            loss_rebate
        },
        balance_available_after: balance_after.available,
        balance_frozen_after: balance_after.frozen,
        ref_type: "settlement".to_string(),
        ref_id: settlement.id,
        idempotency_key: format!("settlement:{}", report.id),
        created_at: Utc::now(),
    });
    if result == "WIN" && gross_profit > Decimal::ZERO {
        let settings = load_platform_settings(state).await;
        apply_referral_rewards_memory(state, &settlement, gross_profit, &settings).await?;
    }
    Ok(())
}

async fn apply_referral_rewards_memory(
    state: &SharedState,
    settlement: &SettlementRecord,
    base_profit: Decimal,
    settings: &PlatformTradingSettings,
) -> Result<(), AppError> {
    let link = state
        .referral_links
        .read()
        .await
        .get(&settlement.user_id)
        .cloned();
    let Some(link) = link else {
        return Ok(());
    };
    let recipients = [
        (link.parent_user_id, 1, settings.direct_referral_rate),
        (link.grandparent_user_id, 2, settings.indirect_referral_rate),
    ];
    for (recipient_user_id, level, rate) in recipients {
        let Some(recipient_user_id) = recipient_user_id else {
            continue;
        };
        if recipient_user_id == settlement.user_id {
            continue;
        }
        let amount = base_profit * rate;
        if amount <= Decimal::ZERO {
            continue;
        }
        let reward = ReferralRewardRecord {
            id: Uuid::now_v7(),
            settlement_id: settlement.id,
            source_user_id: settlement.user_id,
            recipient_user_id,
            level,
            rate,
            base_profit,
            amount,
            status: "credited".to_string(),
            created_at: settlement.created_at,
        };
        let mut balances = state.balances.write().await;
        let balance = balances
            .get_mut(&recipient_user_id)
            .ok_or_else(|| AppError::bad_request("BALANCE_NOT_FOUND", "balance not found"))?;
        balance.available += amount;
        balance.updated_at = settlement.created_at;
        let balance_after = balance.clone();
        drop(balances);
        state.referral_rewards.write().await.push(reward.clone());
        state.ledger.write().await.push(LedgerEntry {
            id: Uuid::now_v7(),
            user_id: recipient_user_id,
            asset: "USDT".to_string(),
            entry_type: if level == 1 {
                "direct_referral_reward".to_string()
            } else {
                "indirect_referral_reward".to_string()
            },
            amount,
            balance_available_after: balance_after.available,
            balance_frozen_after: balance_after.frozen,
            ref_type: "referral_reward".to_string(),
            ref_id: reward.id,
            idempotency_key: format!("referral:{}:{}:{}", settlement.id, recipient_user_id, level),
            created_at: settlement.created_at,
        });
    }
    Ok(())
}

async fn db_user_id_by_wallet(
    pool: &PgPool,
    wallet_address: &str,
) -> Result<Option<Uuid>, AppError> {
    sqlx::query("select id from users where wallet_address = $1")
        .bind(wallet_address)
        .fetch_optional(pool)
        .await
        .map(|row| row.map(|row| row.get("id")))
        .map_err(AppError::from_sqlx)
}

async fn db_user_by_wallet(
    pool: &PgPool,
    wallet_address: &str,
) -> Result<Option<UserRecord>, AppError> {
    sqlx::query(
        r#"
        select id, wallet_address, password_hash, status, active_device_fingerprint, session_version, created_at, updated_at
        from users
        where wallet_address = $1
        "#,
    )
    .bind(wallet_address)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(user_from_row))
    .map_err(AppError::from_sqlx)
}

async fn db_user_by_id(pool: &PgPool, user_id: Uuid) -> Result<Option<UserRecord>, AppError> {
    sqlx::query(
        r#"
        select id, wallet_address, password_hash, status, active_device_fingerprint, session_version, created_at, updated_at
        from users
        where id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(user_from_row))
    .map_err(AppError::from_sqlx)
}

fn user_from_row(row: sqlx::postgres::PgRow) -> UserRecord {
    UserRecord {
        id: row.get("id"),
        wallet_address: row.get("wallet_address"),
        password_hash: row.get("password_hash"),
        status: row.get("status"),
        active_device_fingerprint: row.get("active_device_fingerprint"),
        session_version: row.get("session_version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn db_user_session_version(pool: &PgPool, user_id: Uuid) -> Result<i32, AppError> {
    sqlx::query("select session_version from users where id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::from_sqlx)?
        .map(|row| row.get("session_version"))
        .ok_or_else(|| AppError::unauthorized("AUTH_INVALID_USER", "user account not found"))
}

async fn replace_android_device_session(
    state: &SharedState,
    user_id: Uuid,
    device_fingerprint: &str,
) -> Result<(), AppError> {
    if let Some(pool) = state.db.as_ref() {
        let now = Utc::now();
        let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            update users
            set active_device_fingerprint = $2,
                session_version = session_version + 1,
                updated_at = $3
            where id = $1
            "#,
        )
        .bind(user_id)
        .bind(device_fingerprint)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        sqlx::query("delete from auth_sessions where user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::from_sqlx)?;
        tx.commit().await.map_err(AppError::from_sqlx)?;
    } else if let Some(user) = state.users.write().await.get_mut(&user_id) {
        user.session_version += 1;
        user.active_device_fingerprint = Some(device_fingerprint.to_string());
        user.updated_at = Utc::now();
    }

    state
        .sessions
        .write()
        .await
        .retain(|_, session| session.user_id != user_id);

    Ok(())
}

async fn db_change_user_password(
    pool: &PgPool,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), AppError> {
    let now = Utc::now();
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    sqlx::query(
        r#"
        update users
        set password_hash = $2,
            session_version = session_version + 1,
            active_device_fingerprint = null,
            updated_at = $3
        where id = $1
        "#,
    )
    .bind(user_id)
    .bind(password_hash)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    sqlx::query("delete from auth_sessions where user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
    tx.commit().await.map_err(AppError::from_sqlx)
}

async fn db_user_wallet_address(pool: &PgPool, user_id: Uuid) -> Result<String, AppError> {
    sqlx::query("select wallet_address from users where id = $1")
        .bind(user_id)
        .fetch_optional(pool)
        .await
        .map_err(AppError::from_sqlx)?
        .map(|row| row.get("wallet_address"))
        .ok_or_else(|| AppError::unauthorized("AUTH_INVALID_USER", "user account not found"))
}

async fn db_admin_account_by_username(
    pool: &PgPool,
    username: &str,
) -> Result<Option<AdminAccountRecord>, AppError> {
    sqlx::query(
        r#"
        select id, password_hash, status
        from admin_users
        where username = $1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(admin_account_from_row))
    .map_err(AppError::from_sqlx)
}

async fn db_admin_session_by_access_token(
    pool: &PgPool,
    access_token: &str,
) -> Result<Option<AdminSessionRecord>, AppError> {
    sqlx::query(
        r#"
        select s.expires_at
        from admin_sessions s
        join admin_users u on u.id = s.admin_user_id
        where s.access_token = $1 and u.status = 'active'
        "#,
    )
    .bind(access_token)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|row| AdminSessionRecord {
            expires_at: row.get("expires_at"),
        })
    })
    .map_err(AppError::from_sqlx)
}

fn admin_account_from_row(row: sqlx::postgres::PgRow) -> AdminAccountRecord {
    AdminAccountRecord {
        id: row.get("id"),
        password_hash: row.get("password_hash"),
        status: row.get("status"),
    }
}

async fn db_enforce_register_rate_limit(pool: &PgPool, account: &str) -> Result<(), AppError> {
    let key = normalize_account(account);
    let now = Utc::now();
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let row = sqlx::query(
        r#"
        insert into auth_rate_limits (id, scope, key, window_start, attempts, blocked_until, updated_at)
        values ($1, 'register_wallet', $2, $3, 0, null, $3)
        on conflict (scope, key) do update set updated_at = excluded.updated_at
        returning id, window_start, attempts, blocked_until
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(&key)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;

    let id: Uuid = row.get("id");
    let mut window_start: DateTime<Utc> = row.get("window_start");
    let mut attempts: i32 = row.get("attempts");
    let mut blocked_until: Option<DateTime<Utc>> = row.get("blocked_until");

    if let Some(until) = blocked_until {
        if until > now {
            tx.commit().await.map_err(AppError::from_sqlx)?;
            return Err(AppError::too_many_requests(
                "AUTH_RATE_LIMITED",
                "register attempts are temporarily blocked",
            ));
        }
    }

    if now - window_start > Duration::minutes(10) {
        window_start = now;
        attempts = 0;
        blocked_until = None;
    }
    attempts += 1;
    if attempts > 5 {
        blocked_until = Some(now + Duration::minutes(15));
    }

    sqlx::query(
        r#"
        update auth_rate_limits
        set window_start = $2, attempts = $3, blocked_until = $4, updated_at = $5
        where id = $1
        "#,
    )
    .bind(id)
    .bind(window_start)
    .bind(attempts)
    .bind(blocked_until)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    tx.commit().await.map_err(AppError::from_sqlx)?;

    if attempts > 5 {
        return Err(AppError::too_many_requests(
            "AUTH_RATE_LIMITED",
            "too many register attempts",
        ));
    }
    Ok(())
}

async fn db_session_by_access_token(
    pool: &PgPool,
    access_token: &str,
) -> Result<Option<SessionRecord>, AppError> {
    db_session_by_token(pool, "access_token", access_token).await
}

async fn db_session_by_refresh_token(
    pool: &PgPool,
    refresh_token: &str,
) -> Result<Option<SessionRecord>, AppError> {
    db_session_by_token(pool, "refresh_token", refresh_token).await
}

async fn db_session_by_token(
    pool: &PgPool,
    column: &str,
    token: &str,
) -> Result<Option<SessionRecord>, AppError> {
    let sql = match column {
        "access_token" => {
            "select user_id, device_fingerprint, refresh_token, app_session_key, client_type, session_version, expires_at from auth_sessions where access_token = $1"
        }
        "refresh_token" => {
            "select user_id, device_fingerprint, refresh_token, app_session_key, client_type, session_version, expires_at from auth_sessions where refresh_token = $1"
        }
        _ => return Ok(None),
    };
    sqlx::query(sql)
        .bind(token)
        .fetch_optional(pool)
        .await
        .map(|row| {
            row.map(|row| SessionRecord {
                user_id: row.get("user_id"),
                device_fingerprint: row.get("device_fingerprint"),
                refresh_token: row.get("refresh_token"),
                app_session_key: row.get("app_session_key"),
                client_type: client_type_from_str(row.get::<String, _>("client_type").as_str())
                    .unwrap_or(ClientType::Android),
                session_version: row.get("session_version"),
                expires_at: row.get("expires_at"),
            })
        })
        .map_err(AppError::from_sqlx)
}

async fn db_load_platform_trading_settings(
    pool: &PgPool,
) -> Result<PlatformTradingSettings, AppError> {
    sqlx::query(
        r#"
        select stake_percent, min_stake_usdt, max_single_stake_usdt,
               max_active_event_orders_per_user,
               first_deposit_min_usdt, deposit_fee_rate, direct_referral_rate,
               indirect_referral_rate, updated_at
        from platform_trading_settings
        where id = true
        "#,
    )
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|row| PlatformTradingSettings {
            stake_percent: row.get("stake_percent"),
            min_stake_usdt: row.get("min_stake_usdt"),
            max_single_stake_usdt: row.get("max_single_stake_usdt"),
            max_active_event_orders_per_user: row.get("max_active_event_orders_per_user"),
            first_deposit_min_usdt: row.get("first_deposit_min_usdt"),
            deposit_fee_rate: row.get("deposit_fee_rate"),
            direct_referral_rate: row.get("direct_referral_rate"),
            indirect_referral_rate: row.get("indirect_referral_rate"),
            updated_at: Some(row.get("updated_at")),
        })
        .unwrap_or_default()
    })
    .map_err(AppError::from_sqlx)
}

async fn db_platform_settings_tx(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<PlatformTradingSettings, AppError> {
    sqlx::query(
        r#"
        select stake_percent, min_stake_usdt, max_single_stake_usdt,
               max_active_event_orders_per_user,
               first_deposit_min_usdt, deposit_fee_rate, direct_referral_rate,
               indirect_referral_rate, updated_at
        from platform_trading_settings
        where id = true
        "#,
    )
    .fetch_optional(&mut **tx)
    .await
    .map(|row| {
        row.map(|row| PlatformTradingSettings {
            stake_percent: row.get("stake_percent"),
            min_stake_usdt: row.get("min_stake_usdt"),
            max_single_stake_usdt: row.get("max_single_stake_usdt"),
            max_active_event_orders_per_user: row.get("max_active_event_orders_per_user"),
            first_deposit_min_usdt: row.get("first_deposit_min_usdt"),
            deposit_fee_rate: row.get("deposit_fee_rate"),
            direct_referral_rate: row.get("direct_referral_rate"),
            indirect_referral_rate: row.get("indirect_referral_rate"),
            updated_at: Some(row.get("updated_at")),
        })
        .unwrap_or_default()
    })
    .map_err(AppError::from_sqlx)
}

async fn db_save_platform_trading_settings(
    pool: &PgPool,
    settings: &PlatformTradingSettings,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into platform_trading_settings
          (id, stake_percent, min_stake_usdt, max_single_stake_usdt,
           max_active_event_orders_per_user, first_deposit_min_usdt, deposit_fee_rate,
           direct_referral_rate, indirect_referral_rate, updated_at)
        values (true, $1, $2, $3, $4, $5, $6, $7, $8, $9)
        on conflict (id) do update set
          stake_percent = excluded.stake_percent,
          min_stake_usdt = excluded.min_stake_usdt,
          max_single_stake_usdt = excluded.max_single_stake_usdt,
          max_active_event_orders_per_user = excluded.max_active_event_orders_per_user,
          first_deposit_min_usdt = excluded.first_deposit_min_usdt,
          deposit_fee_rate = excluded.deposit_fee_rate,
          direct_referral_rate = excluded.direct_referral_rate,
          indirect_referral_rate = excluded.indirect_referral_rate,
          updated_at = excluded.updated_at
        "#,
    )
    .bind(settings.stake_percent)
    .bind(settings.min_stake_usdt)
    .bind(settings.max_single_stake_usdt)
    .bind(settings.max_active_event_orders_per_user)
    .bind(settings.first_deposit_min_usdt)
    .bind(settings.deposit_fee_rate)
    .bind(settings.direct_referral_rate)
    .bind(settings.indirect_referral_rate)
    .bind(Utc::now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_load_prediction_settings(pool: &PgPool) -> Result<PredictionSettings, AppError> {
    sqlx::query(
        r#"
        select enabled, interval_seconds, up_threshold, down_threshold, min_confidence,
               technical_weight, probability_weight, volatility_floor,
               calibration_lookback_minutes, calibration_min_samples,
               calibration_min_win_rate, calibration_min_ev_usdt, updated_at
        from prediction_settings
        where id = true
        "#,
    )
    .fetch_optional(pool)
    .await
    .map(|row| {
        normalize_prediction_settings(
            row.map(|row| PredictionSettings {
                enabled: row.get("enabled"),
                interval_seconds: row.get("interval_seconds"),
                up_threshold: row.get("up_threshold"),
                down_threshold: row.get("down_threshold"),
                min_confidence: row.get("min_confidence"),
                technical_weight: row
                    .try_get("technical_weight")
                    .unwrap_or_else(|_| decimal_from_f64(DEFAULT_PREDICTION_TECHNICAL_WEIGHT, 6)),
                probability_weight: row
                    .try_get("probability_weight")
                    .unwrap_or_else(|_| decimal_from_f64(DEFAULT_PREDICTION_PROBABILITY_WEIGHT, 6)),
                volatility_floor: row
                    .try_get("volatility_floor")
                    .unwrap_or_else(|_| decimal_from_f64(DEFAULT_PREDICTION_VOLATILITY_FLOOR, 8)),
                calibration_lookback_minutes: row
                    .try_get("calibration_lookback_minutes")
                    .unwrap_or(DEFAULT_PREDICTION_CALIBRATION_LOOKBACK_MINUTES as i32),
                calibration_min_samples: row
                    .try_get("calibration_min_samples")
                    .unwrap_or(DEFAULT_PREDICTION_CALIBRATION_MIN_SAMPLES),
                calibration_min_win_rate: row.try_get("calibration_min_win_rate").unwrap_or_else(
                    |_| decimal_from_f64(DEFAULT_PREDICTION_CALIBRATION_MIN_WIN_RATE, 6),
                ),
                calibration_min_ev_usdt: row.try_get("calibration_min_ev_usdt").unwrap_or_else(
                    |_| decimal_from_f64(DEFAULT_PREDICTION_CALIBRATION_MIN_EV_USDT, 4),
                ),
                updated_at: row.get("updated_at"),
            })
            .unwrap_or_default(),
        )
    })
    .map_err(AppError::from_sqlx)
}

async fn db_save_prediction_settings(
    pool: &PgPool,
    settings: &PredictionSettings,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into prediction_settings
          (id, enabled, interval_seconds, up_threshold, down_threshold, min_confidence,
           technical_weight, probability_weight, volatility_floor,
           calibration_lookback_minutes, calibration_min_samples,
           calibration_min_win_rate, calibration_min_ev_usdt, updated_at)
        values (true, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, now())
        on conflict (id) do update set
          enabled = excluded.enabled,
          interval_seconds = excluded.interval_seconds,
          up_threshold = excluded.up_threshold,
          down_threshold = excluded.down_threshold,
          min_confidence = excluded.min_confidence,
          technical_weight = excluded.technical_weight,
          probability_weight = excluded.probability_weight,
          volatility_floor = excluded.volatility_floor,
          calibration_lookback_minutes = excluded.calibration_lookback_minutes,
          calibration_min_samples = excluded.calibration_min_samples,
          calibration_min_win_rate = excluded.calibration_min_win_rate,
          calibration_min_ev_usdt = excluded.calibration_min_ev_usdt,
          updated_at = now()
        "#,
    )
    .bind(settings.enabled)
    .bind(settings.interval_seconds)
    .bind(settings.up_threshold)
    .bind(settings.down_threshold)
    .bind(settings.min_confidence)
    .bind(settings.technical_weight)
    .bind(settings.probability_weight)
    .bind(settings.volatility_floor)
    .bind(settings.calibration_lookback_minutes)
    .bind(settings.calibration_min_samples)
    .bind(settings.calibration_min_win_rate)
    .bind(settings.calibration_min_ev_usdt)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_load_automation_settings(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<AutomationSettings>, AppError> {
    sqlx::query(
        r#"
        select automation_enabled, execution_mode, target_app_profile,
               require_final_confirmation, emergency_stop_enabled
        from user_automation_settings
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|row| AutomationSettings {
            automation_enabled: row.get("automation_enabled"),
            execution_mode: row.get("execution_mode"),
            target_app_profile: row.get("target_app_profile"),
            require_final_confirmation: row.get("require_final_confirmation"),
            emergency_stop_enabled: row.get("emergency_stop_enabled"),
        })
    })
    .map_err(AppError::from_sqlx)
}

async fn db_save_automation_settings(
    pool: &PgPool,
    user_id: Uuid,
    settings: &AutomationSettings,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into user_automation_settings
          (user_id, automation_enabled, execution_mode, target_app_profile,
           require_final_confirmation, emergency_stop_enabled, updated_at)
        values ($1, $2, $3, $4, $5, $6, $7)
        on conflict (user_id) do update set
          automation_enabled = excluded.automation_enabled,
          execution_mode = excluded.execution_mode,
          target_app_profile = excluded.target_app_profile,
          require_final_confirmation = excluded.require_final_confirmation,
          emergency_stop_enabled = excluded.emergency_stop_enabled,
          updated_at = excluded.updated_at
        "#,
    )
    .bind(user_id)
    .bind(settings.automation_enabled)
    .bind(&settings.execution_mode)
    .bind(&settings.target_app_profile)
    .bind(settings.require_final_confirmation)
    .bind(settings.emergency_stop_enabled)
    .bind(Utc::now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_load_balance(pool: &PgPool, user_id: Uuid) -> Result<Option<BalanceRecord>, AppError> {
    sqlx::query(
        "select user_id, asset, available, frozen, updated_at from balances where user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|row| BalanceRecord {
            user_id: row.get("user_id"),
            asset: row.get("asset"),
            available: row.get("available"),
            frozen: row.get("frozen"),
            updated_at: row.get("updated_at"),
        })
    })
    .map_err(AppError::from_sqlx)
}

async fn db_load_page_state(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<AutomationPageState>, AppError> {
    sqlx::query(
        r#"
        select page_ready, screen_name, confidence, package_name, amount_field_found,
               up_button_found, down_button_found, observed_balance_usdt,
               account_match_status, account_balance_delta_usdt, observed_at, updated_at
        from automation_page_states
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|row| AutomationPageState {
            page_ready: row.get("page_ready"),
            screen_name: row.get("screen_name"),
            confidence: row.get("confidence"),
            package_name: row.get("package_name"),
            amount_field_found: row.get("amount_field_found"),
            up_button_found: row.get("up_button_found"),
            down_button_found: row.get("down_button_found"),
            observed_balance_usdt: row.get("observed_balance_usdt"),
            account_match_status: row.get("account_match_status"),
            account_balance_delta_usdt: row.get("account_balance_delta_usdt"),
            observed_at: row.get("observed_at"),
            updated_at: row.get("updated_at"),
        })
    })
    .map_err(AppError::from_sqlx)
}

async fn db_save_page_state(
    pool: &PgPool,
    user_id: Uuid,
    page: &AutomationPageState,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into automation_page_states
          (user_id, page_ready, screen_name, confidence, package_name, amount_field_found,
           up_button_found, down_button_found, observed_balance_usdt, account_match_status,
           account_balance_delta_usdt, observed_at, updated_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        on conflict (user_id) do update set
          page_ready = excluded.page_ready,
          screen_name = excluded.screen_name,
          confidence = excluded.confidence,
          package_name = excluded.package_name,
          amount_field_found = excluded.amount_field_found,
          up_button_found = excluded.up_button_found,
          down_button_found = excluded.down_button_found,
          observed_balance_usdt = excluded.observed_balance_usdt,
          account_match_status = excluded.account_match_status,
          account_balance_delta_usdt = excluded.account_balance_delta_usdt,
          observed_at = excluded.observed_at,
          updated_at = excluded.updated_at
        "#,
    )
    .bind(user_id)
    .bind(page.page_ready)
    .bind(&page.screen_name)
    .bind(page.confidence)
    .bind(&page.package_name)
    .bind(page.amount_field_found)
    .bind(page.up_button_found)
    .bind(page.down_button_found)
    .bind(page.observed_balance_usdt)
    .bind(&page.account_match_status)
    .bind(page.account_balance_delta_usdt)
    .bind(page.observed_at)
    .bind(page.updated_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_insert_signal_delivery(
    pool: &PgPool,
    delivery: &SignalDeliveryRecord,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into signal_deliveries
          (id, signal_id, user_id, reservation_id, ws_connection_id,
           delivery_status, authorization_status, trade_authorized, stake_usdt,
           fee_reserve_usdt, sent_at, received_at, client_received_at, page_state,
           app_version, last_error, created_at, updated_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
        on conflict (signal_id, user_id, ws_connection_id) do nothing
        "#,
    )
    .bind(delivery.id)
    .bind(delivery.signal_id)
    .bind(delivery.user_id)
    .bind(delivery.reservation_id)
    .bind(delivery.ws_connection_id)
    .bind(&delivery.delivery_status)
    .bind(&delivery.authorization_status)
    .bind(delivery.trade_authorized)
    .bind(delivery.stake_usdt)
    .bind(delivery.fee_reserve_usdt)
    .bind(delivery.sent_at)
    .bind(delivery.received_at)
    .bind(delivery.client_received_at)
    .bind(&delivery.page_state)
    .bind(&delivery.app_version)
    .bind(&delivery.last_error)
    .bind(delivery.created_at)
    .bind(delivery.updated_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_ack_signal_delivery(
    pool: &PgPool,
    user_id: Uuid,
    req: &SignalDeliveryAckRequest,
) -> Result<(), AppError> {
    let now = Utc::now();
    let row = sqlx::query(
        r#"
        update signal_deliveries
        set delivery_status = case
              when delivery_status = 'received' then 'ack_duplicate'
              else 'received'
            end,
            received_at = coalesce(received_at, $4),
            client_received_at = coalesce(client_received_at, $5),
            page_state = coalesce($6, page_state),
            app_version = coalesce($7, app_version),
            updated_at = $4
        where id = $1
          and user_id = $2
          and signal_id = $3
        returning id
        "#,
    )
    .bind(req.delivery_id)
    .bind(user_id)
    .bind(req.signal_id)
    .bind(now)
    .bind(req.received_at)
    .bind(&req.page_state)
    .bind(
        req.app_version
            .as_deref()
            .map(|value| value.chars().take(64).collect::<String>()),
    )
    .fetch_optional(pool)
    .await
    .map_err(AppError::from_sqlx)?;
    if row.is_none() {
        return Err(AppError::bad_request(
            "SIGNAL_DELIVERY_NOT_FOUND",
            "signal delivery not found for this user",
        ));
    }
    Ok(())
}

async fn db_mark_signal_delivery_failed(
    pool: &PgPool,
    delivery_id: Uuid,
    message: &str,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        update signal_deliveries
        set delivery_status = 'failed',
            last_error = $2,
            updated_at = $3
        where id = $1
          and delivery_status = 'sent'
        "#,
    )
    .bind(delivery_id)
    .bind(message.chars().take(500).collect::<String>())
    .bind(now)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_exchange_credential_status(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<ExchangeCredentialStatus>, AppError> {
    sqlx::query(
        r#"
        select c.provider, c.api_key, c.can_read_account, c.verification_status,
               c.balance_available_usdt, c.balance_wallet_usdt,
               p.observed_balance_usdt, p.account_match_status, p.account_balance_delta_usdt,
               c.balance_updated_at,
               last_verified_at, last_error, updated_at
        from user_exchange_credentials c
        left join automation_page_states p on p.user_id = c.user_id
        where c.user_id = $1 and c.provider = $2
        "#,
    )
    .bind(user_id)
    .bind(BINANCE_PROVIDER)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|row| ExchangeCredentialStatus {
            provider: row.get("provider"),
            api_key: Some(row.get("api_key")),
            connected: true,
            can_read_account: row.get("can_read_account"),
            verification_status: row.get("verification_status"),
            balance_available_usdt: row
                .try_get::<Option<Decimal>, _>("balance_available_usdt")
                .ok()
                .flatten()
                .map(money),
            balance_wallet_usdt: row
                .try_get::<Option<Decimal>, _>("balance_wallet_usdt")
                .ok()
                .flatten()
                .map(money),
            account_match_status: row.get("account_match_status"),
            observed_balance_usdt: row
                .try_get::<Option<Decimal>, _>("observed_balance_usdt")
                .ok()
                .flatten()
                .map(money),
            account_balance_delta_usdt: row
                .try_get::<Option<Decimal>, _>("account_balance_delta_usdt")
                .ok()
                .flatten()
                .map(money),
            balance_updated_at: row.get("balance_updated_at"),
            balance_cache_fresh: balance_cache_fresh(row.get("balance_updated_at")),
            last_verified_at: row.get("last_verified_at"),
            last_error: row.get("last_error"),
            updated_at: Some(row.get("updated_at")),
        })
    })
    .map_err(AppError::from_sqlx)
}

async fn db_upsert_exchange_credentials(
    pool: &PgPool,
    user_id: Uuid,
    api_key: &str,
    api_secret_ciphertext: &str,
    snapshot: &BinanceAccountSnapshot,
    verified_at: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into user_exchange_credentials
          (user_id, provider, api_key, api_secret_ciphertext, can_read_account,
           verification_status, balance_available_usdt, balance_wallet_usdt, balance_updated_at,
           last_verified_at, last_error, created_at, updated_at)
        values ($1, $2, $3, $4, true, 'verified', $5, $6, $7, $7, null, $7, $7)
        on conflict (user_id) do update set
          api_key = excluded.api_key,
          api_secret_ciphertext = excluded.api_secret_ciphertext,
          can_read_account = true,
          verification_status = 'verified',
          balance_available_usdt = excluded.balance_available_usdt,
          balance_wallet_usdt = excluded.balance_wallet_usdt,
          balance_updated_at = excluded.balance_updated_at,
          last_verified_at = excluded.last_verified_at,
          last_error = null,
          updated_at = excluded.updated_at
        "#,
    )
    .bind(user_id)
    .bind(BINANCE_PROVIDER)
    .bind(api_key)
    .bind(api_secret_ciphertext)
    .bind(snapshot.available_balance_usdt)
    .bind(snapshot.wallet_balance_usdt)
    .bind(verified_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_exchange_credentials(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<ExchangeCredentialRecord>, AppError> {
    sqlx::query(
        r#"
        select user_id, provider, api_key, api_secret_ciphertext, can_read_account,
               verification_status, last_verified_at, last_error, updated_at
        from user_exchange_credentials
        where user_id = $1 and provider = $2 and verification_status = 'verified'
        "#,
    )
    .bind(user_id)
    .bind(BINANCE_PROVIDER)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|row| ExchangeCredentialRecord {
            user_id: row.get("user_id"),
            api_key: row.get("api_key"),
            api_secret_ciphertext: row.get("api_secret_ciphertext"),
        })
    })
    .map_err(AppError::from_sqlx)
}

async fn db_exchange_available_balance(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<Decimal>, AppError> {
    sqlx::query(
        r#"
        select balance_available_usdt
        from user_exchange_credentials
        where user_id = $1
          and provider = $2
          and verification_status = 'verified'
          and can_read_account = true
        "#,
    )
    .bind(user_id)
    .bind(BINANCE_PROVIDER)
    .fetch_optional(pool)
    .await
    .map(|row| row.and_then(|row| row.get("balance_available_usdt")))
    .map_err(AppError::from_sqlx)
}

async fn db_exchange_balance_snapshot(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<ExchangeBalanceSnapshot>, AppError> {
    sqlx::query(
        r#"
        select balance_available_usdt, balance_updated_at
        from user_exchange_credentials
        where user_id = $1
          and provider = $2
          and verification_status = 'verified'
          and can_read_account = true
        "#,
    )
    .bind(user_id)
    .bind(BINANCE_PROVIDER)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.and_then(|row| {
            row.get::<Option<Decimal>, _>("balance_available_usdt")
                .map(|available| ExchangeBalanceSnapshot {
                    available,
                    updated_at: row.get("balance_updated_at"),
                })
        })
    })
    .map_err(AppError::from_sqlx)
}

async fn db_exchange_credentials_for_balance_refresh(
    pool: &PgPool,
    now: DateTime<Utc>,
    limit: i64,
    config: &ExchangeWorkerConfig,
) -> Result<Vec<ExchangeCredentialRecord>, AppError> {
    sqlx::query(
        r#"
        select user_id, api_key, api_secret_ciphertext
        from user_exchange_credentials
        where provider = $1
          and verification_status = 'verified'
          and can_read_account = true
          and (
            balance_updated_at is null
            or balance_updated_at <= $2
          )
          and mod(abs(hashtext(user_id::text)), $3) = $4
        order by coalesce(balance_updated_at, timestamptz 'epoch') asc
        limit $5
        "#,
    )
    .bind(BINANCE_PROVIDER)
    .bind(now - Duration::seconds(BINANCE_BALANCE_REFRESH_SECONDS))
    .bind(config.shard_total)
    .bind(config.shard_index)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|row| ExchangeCredentialRecord {
                user_id: row.get("user_id"),
                api_key: row.get("api_key"),
                api_secret_ciphertext: row.get("api_secret_ciphertext"),
            })
            .collect()
    })
    .map_err(AppError::from_sqlx)
}

async fn db_update_exchange_balance(
    pool: &PgPool,
    user_id: Uuid,
    snapshot: &BinanceAccountSnapshot,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        update user_exchange_credentials
        set balance_available_usdt = $3,
            balance_wallet_usdt = $4,
            balance_updated_at = $5,
            last_verified_at = $5,
            last_error = null,
            updated_at = $5
        where user_id = $1 and provider = $2
        "#,
    )
    .bind(user_id)
    .bind(BINANCE_PROVIDER)
    .bind(snapshot.available_balance_usdt)
    .bind(snapshot.wallet_balance_usdt)
    .bind(now)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_mark_exchange_error(
    pool: &PgPool,
    user_id: Uuid,
    message: &str,
    now: DateTime<Utc>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        update user_exchange_credentials
        set last_error = $3,
            balance_updated_at = $4,
            updated_at = $4
        where user_id = $1 and provider = $2
        "#,
    )
    .bind(user_id)
    .bind(BINANCE_PROVIDER)
    .bind(message.chars().take(500).collect::<String>())
    .bind(now)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_insert_signal(pool: &PgPool, signal: &SignalCore) -> Result<bool, AppError> {
    sqlx::query(
        r#"
        insert into signals
          (id, symbol, period_seconds, cycle_start, cycle_end, direction, confidence,
           predicted_return, entry_price, suggested_stake, model_version, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        on conflict (symbol, period_seconds, cycle_start) do nothing
        "#,
    )
    .bind(signal.id)
    .bind(&signal.symbol)
    .bind(signal.period_seconds)
    .bind(signal.cycle_start)
    .bind(signal.cycle_end)
    .bind(&signal.direction)
    .bind(signal.confidence)
    .bind(signal.predicted_return)
    .bind(signal.entry_price)
    .bind(signal.suggested_stake)
    .bind(&signal.model_version)
    .bind(signal.created_at)
    .execute(pool)
    .await
    .map(|result| result.rows_affected() > 0)
    .map_err(AppError::from_sqlx)
}

async fn db_active_strategy_signal_count(
    pool: &PgPool,
    cycle_start: DateTime<Utc>,
) -> Result<usize, AppError> {
    let count: i64 = sqlx::query(
        r#"
        select count(*) as count
        from signals
        where symbol = $1
          and period_seconds = $2
          and cycle_start <= $3
          and cycle_end > $3
          and model_version = $4
        "#,
    )
    .bind(DEFAULT_SYMBOL)
    .bind(DEFAULT_PERIOD_SECONDS)
    .bind(cycle_start)
    .bind(PREDICTION_MODEL_VERSION)
    .fetch_one(pool)
    .await
    .map_err(AppError::from_sqlx)?
    .get("count");
    Ok(usize::try_from(count).unwrap_or(usize::MAX))
}

async fn db_insert_prediction_audit(
    pool: &PgPool,
    signal: &SignalCore,
    decision: &PredictionDecision,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into prediction_audit_logs
          (id, signal_id, source, symbol, period_seconds, model_version, direction,
           confidence, predicted_return, entry_price, suggested_stake, score,
           feature_snapshot, rule_hits, reason, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, '[]'::jsonb, $14, $15)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(signal.id)
    .bind("strategy_loop")
    .bind(&signal.symbol)
    .bind(signal.period_seconds)
    .bind(&signal.model_version)
    .bind(&signal.direction)
    .bind(signal.confidence)
    .bind(signal.predicted_return)
    .bind(signal.entry_price)
    .bind(signal.suggested_stake)
    .bind(decimal_from_f64(decision.score, 10))
    .bind(&decision.feature_snapshot)
    .bind(&decision.reason)
    .bind(signal.created_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_insert_prediction_observation(
    pool: &PgPool,
    decision: &PredictionDecision,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into prediction_observations
          (id, source, symbol, period_seconds, model_version, direction, confidence,
           predicted_return, entry_price, score, feature_snapshot, reason, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
    )
    .bind(Uuid::now_v7())
    .bind("strategy_loop")
    .bind(DEFAULT_SYMBOL)
    .bind(DEFAULT_PERIOD_SECONDS)
    .bind(PREDICTION_MODEL_VERSION)
    .bind(&decision.direction)
    .bind(decimal_from_f64(decision.confidence, 6))
    .bind(decimal_from_f64(decision.predicted_return, 10))
    .bind(decimal_from_f64(decision.entry_price, 8))
    .bind(decimal_from_f64(decision.score, 10))
    .bind(&decision.feature_snapshot)
    .bind(&decision.reason)
    .bind(Utc::now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_load_market_second_bars(
    pool: &PgPool,
    symbol: &str,
    limit: usize,
) -> Result<Vec<MarketSecondBar>, AppError> {
    let rows = sqlx::query(
        r#"
        select second_ms, high, low, close, volume, taker_buy_volume,
               taker_sell_volume, trade_count, price_from_index
        from (
          select second_ms, high, low, close, volume, taker_buy_volume,
                 taker_sell_volume, trade_count, price_from_index
          from market_second_bars
          where symbol = $1
          order by second_ms desc
          limit $2
        ) recent
        order by second_ms asc
        "#,
    )
    .bind(symbol)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .map_err(AppError::from_sqlx)?;
    Ok(rows
        .into_iter()
        .map(|row| MarketSecondBar {
            second_ms: row.get("second_ms"),
            high: decimal_to_f64(row.get::<Decimal, _>("high")),
            low: decimal_to_f64(row.get::<Decimal, _>("low")),
            close: decimal_to_f64(row.get::<Decimal, _>("close")),
            volume: decimal_to_f64(row.get::<Decimal, _>("volume")),
            taker_buy_volume: decimal_to_f64(row.get::<Decimal, _>("taker_buy_volume")),
            taker_sell_volume: decimal_to_f64(row.get::<Decimal, _>("taker_sell_volume")),
            trade_count: row.get("trade_count"),
            price_from_index: row.get("price_from_index"),
        })
        .collect())
}

async fn db_upsert_market_second_bars(
    pool: &PgPool,
    symbol: &str,
    bars: &[MarketSecondBar],
) -> Result<(), AppError> {
    for bar in merge_market_second_bars(bars.to_vec()) {
        sqlx::query(
            r#"
            insert into market_second_bars
              (symbol, second_ms, high, low, close, volume, taker_buy_volume,
               taker_sell_volume, trade_count, price_from_index, updated_at)
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, now())
            on conflict (symbol, second_ms) do update set
              high = greatest(market_second_bars.high, excluded.high),
              low = least(market_second_bars.low, excluded.low),
              close = excluded.close,
              volume = greatest(market_second_bars.volume, excluded.volume),
              taker_buy_volume = greatest(market_second_bars.taker_buy_volume, excluded.taker_buy_volume),
              taker_sell_volume = greatest(market_second_bars.taker_sell_volume, excluded.taker_sell_volume),
              trade_count = greatest(market_second_bars.trade_count, excluded.trade_count),
              price_from_index = market_second_bars.price_from_index or excluded.price_from_index,
              updated_at = now()
            "#,
        )
        .bind(symbol)
        .bind(bar.second_ms)
        .bind(decimal_from_f64(bar.high, 18))
        .bind(decimal_from_f64(bar.low, 18))
        .bind(decimal_from_f64(bar.close, 18))
        .bind(decimal_from_f64(bar.volume, 18))
        .bind(decimal_from_f64(bar.taker_buy_volume, 18))
        .bind(decimal_from_f64(bar.taker_sell_volume, 18))
        .bind(bar.trade_count)
        .bind(bar.price_from_index)
        .execute(pool)
        .await
        .map_err(AppError::from_sqlx)?;
    }
    Ok(())
}

async fn db_delete_old_market_second_bars(
    pool: &PgPool,
    retention_seconds: i64,
) -> Result<u64, AppError> {
    sqlx::query(
        r#"
        delete from market_second_bars
        where second_ms < ((extract(epoch from now()) * 1000)::bigint - ($1 * 1000))
        "#,
    )
    .bind(retention_seconds)
    .execute(pool)
    .await
    .map(|result| result.rows_affected())
    .map_err(AppError::from_sqlx)
}

async fn db_settle_due_signal_outcomes(pool: &PgPool, limit: i64) -> Result<u64, AppError> {
    sqlx::query(
        r#"
        with due as (
          select s.id, s.symbol, s.period_seconds, s.cycle_start, s.cycle_end,
                 s.direction, s.entry_price, s.model_version, s.created_at
          from signals s
          where s.cycle_end <= now()
            and not exists (
              select 1 from signal_outcomes o where o.signal_id = s.id
            )
          order by s.cycle_end asc
          limit $1
        ),
        priced as (
          select due.*,
                 coalesce(nullif(due.entry_price, 0), entry.entry_price) as settled_entry_price,
                 expiry.entry_price as expiry_price
          from due
          join lateral (
            select o.entry_price
            from prediction_observations o
            where o.symbol = due.symbol
              and o.created_at >= due.created_at
            order by (o.period_seconds = due.period_seconds) desc, o.created_at asc
            limit 1
          ) entry on true
          join lateral (
            select o.entry_price
            from prediction_observations o
            where o.symbol = due.symbol
              and o.created_at >= due.cycle_end
            order by (o.period_seconds = due.period_seconds) desc, o.created_at asc
            limit 1
          ) expiry on true
        )
        insert into signal_outcomes
          (signal_id, symbol, period_seconds, cycle_start, cycle_end, direction,
           entry_price, expiry_price, result, source, model_version, settled_at, created_at)
        select id, symbol, period_seconds, cycle_start, cycle_end, direction,
               settled_entry_price, expiry_price,
               case
                 when direction = 'UP' and expiry_price > settled_entry_price then 'win'
                 when direction = 'DOWN' and expiry_price < settled_entry_price then 'win'
                 else 'loss'
               end as result,
               'prediction_observation',
               model_version,
               now(),
               now()
        from priced
        on conflict (signal_id) do nothing
        "#,
    )
    .bind(limit.max(1))
    .execute(pool)
    .await
    .map(|result| result.rows_affected())
    .map_err(AppError::from_sqlx)
}

async fn db_prediction_calibration(
    pool: &PgPool,
    config: &PredictionConfig,
    candidate: &PredictionCandidate,
) -> Result<PredictionCalibration, AppError> {
    let gross_win = decimal_to_f64(DEFAULT_PAYOUT_RATIO * MIN_EVENT_STAKE);
    let fee = decimal_to_f64(DEFAULT_PAYOUT_RATIO * MIN_EVENT_STAKE * PROFIT_SHARE_RATE);
    let net_win = gross_win - fee;
    let loss = decimal_to_f64(MIN_EVENT_STAKE * (Decimal::ONE - PROFIT_SHARE_RATE));
    let row = sqlx::query(
        r#"
        with matched as (
          select distinct on (
                   floor(extract(epoch from o.created_at) / $6::double precision)
                 )
                 o.created_at,
                 o.entry_price::float8 as entry_price,
                 f.entry_price::float8 as future_price
          from prediction_observations o
          join lateral (
            select entry_price
            from prediction_observations p
            where p.created_at >= o.created_at + make_interval(secs => $8::double precision)
            order by p.created_at asc
            limit 1
          ) f on true
          where o.model_version = $1
            and o.created_at >= now() - ($2::text || ' minutes')::interval
            and o.created_at <= now() - make_interval(secs => $8::double precision)
            and o.feature_snapshot->'decision'->>'candidate_mode' = $3
            and o.feature_snapshot->'decision'->>'candidate_rule' = $4
            and o.feature_snapshot->'decision'->>'candidate_direction' = $5
            and (o.feature_snapshot->'scores'->>'raw_confidence')::float8 >= $7
          order by floor(extract(epoch from o.created_at) / $6::double precision), o.created_at desc
        ),
        scored as (
          select *,
                 case
                   when $5 = 'UP' and future_price > entry_price then 1
                   when $5 = 'DOWN' and future_price < entry_price then 1
                   else 0
                 end as win
          from matched
        )
        select count(*)::bigint as samples,
               coalesce(sum(win), 0)::bigint as wins
        from scored
        "#,
    )
    .bind(PREDICTION_MODEL_VERSION)
    .bind(config.calibration_lookback_minutes)
    .bind(&candidate.mode)
    .bind(&candidate.rule)
    .bind(&candidate.direction)
    .bind(config.calibration_sample_seconds.max(1))
    .bind(prediction_candidate_min_confidence(candidate))
    .bind(DEFAULT_PERIOD_SECONDS)
    .fetch_one(pool)
    .await
    .map_err(AppError::from_sqlx)?;

    let sample_count: i64 = row.get("samples");
    let wins: i64 = row.get("wins");
    let win_rate = if sample_count > 0 {
        wins as f64 / sample_count as f64
    } else {
        0.0
    };
    let ev_usdt = win_rate * net_win - (1.0 - win_rate) * loss;
    let effective_min_win_rate = config
        .calibration_min_win_rate
        .max(DEFAULT_PREDICTION_CALIBRATION_MIN_WIN_RATE);
    let allowed = sample_count >= i64::from(config.calibration_min_samples)
        && win_rate >= effective_min_win_rate
        && ev_usdt >= config.calibration_min_ev_usdt;
    let reason = if sample_count < i64::from(config.calibration_min_samples) {
        format!(
            "hold calibration warmup mode {} rule {} direction {} samples {}/{}",
            candidate.mode,
            candidate.rule,
            candidate.direction,
            sample_count,
            config.calibration_min_samples
        )
    } else if win_rate < effective_min_win_rate {
        format!(
            "hold calibration win_rate {:.4} < {:.4} mode {} rule {} samples {}",
            win_rate, effective_min_win_rate, candidate.mode, candidate.rule, sample_count
        )
    } else if ev_usdt < config.calibration_min_ev_usdt {
        format!(
            "hold calibration ev {:.4} < {:.4} mode {} rule {} samples {}",
            ev_usdt, config.calibration_min_ev_usdt, candidate.mode, candidate.rule, sample_count
        )
    } else {
        format!(
            "calibration passed win_rate {:.4} ev {:.4} mode {} rule {} samples {}",
            win_rate, ev_usdt, candidate.mode, candidate.rule, sample_count
        )
    };
    Ok(PredictionCalibration {
        allowed,
        sample_count,
        wins,
        win_rate,
        ev_usdt,
        min_samples: config.calibration_min_samples,
        min_win_rate: effective_min_win_rate,
        min_ev_usdt: config.calibration_min_ev_usdt,
        lookback_minutes: config.calibration_lookback_minutes,
        sample_seconds: config.calibration_sample_seconds,
        reason,
    })
}

async fn db_prediction_observations(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<PredictionObservationRecord>, AppError> {
    sqlx::query(
        r#"
        select id, source, symbol, period_seconds, model_version, direction, confidence,
               predicted_return, entry_price, score, feature_snapshot, reason, created_at
        from prediction_observations
        where model_version = $2
        order by created_at desc
        limit $1
        "#,
    )
    .bind(limit)
    .bind(PREDICTION_MODEL_VERSION)
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(prediction_observation_from_row)
            .collect()
    })
    .map_err(AppError::from_sqlx)
}

fn prediction_observation_from_row(row: sqlx::postgres::PgRow) -> PredictionObservationRecord {
    PredictionObservationRecord {
        id: row.get("id"),
        source: row.get("source"),
        symbol: row.get("symbol"),
        period_seconds: row.get("period_seconds"),
        model_version: row.get("model_version"),
        direction: row.get("direction"),
        confidence: row.get("confidence"),
        predicted_return: row.get("predicted_return"),
        entry_price: row.get("entry_price"),
        score: row.get("score"),
        feature_snapshot: row.get("feature_snapshot"),
        reason: row.get("reason"),
        created_at: row.get("created_at"),
    }
}

async fn db_latest_signal(pool: &PgPool) -> Result<Option<SignalCore>, AppError> {
    sqlx::query(
        r#"
        select id, symbol, period_seconds, cycle_start, cycle_end, direction, confidence,
               predicted_return, entry_price, suggested_stake, model_version, created_at,
               outcome_result, outcome_expiry_price, outcome_settled_at
        from signals_with_outcome
        order by created_at desc
        limit 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .map(|row| row.map(signal_from_row))
    .map_err(AppError::from_sqlx)
}

async fn db_signal_history(pool: &PgPool, limit: i64) -> Result<Vec<SignalCore>, AppError> {
    sqlx::query(
        r#"
        select id, symbol, period_seconds, cycle_start, cycle_end, direction, confidence,
               predicted_return, entry_price, suggested_stake, model_version, created_at,
               outcome_result, outcome_expiry_price, outcome_settled_at
        from signals_with_outcome
        order by created_at desc
        limit $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(signal_from_row).collect())
    .map_err(AppError::from_sqlx)
}

fn signal_from_row(row: sqlx::postgres::PgRow) -> SignalCore {
    SignalCore {
        id: row.get("id"),
        symbol: row.get("symbol"),
        period_seconds: row.get("period_seconds"),
        cycle_start: row.get("cycle_start"),
        cycle_end: row.get("cycle_end"),
        direction: row.get("direction"),
        confidence: row.get("confidence"),
        predicted_return: row.get("predicted_return"),
        entry_price: row.get("entry_price"),
        suggested_stake: row.get("suggested_stake"),
        model_version: row.get("model_version"),
        created_at: row.get("created_at"),
        outcome_result: row.try_get("outcome_result").ok(),
        outcome_expiry_price: row.try_get("outcome_expiry_price").ok(),
        outcome_settled_at: row.try_get("outcome_settled_at").ok(),
    }
}

async fn db_ledger_for_user(
    pool: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<LedgerEntry>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    sqlx::query(
        r#"
        select id, user_id, asset, entry_type, amount, balance_available_after,
               balance_frozen_after, ref_type, ref_id, idempotency_key, created_at
        from ledger_entries
        where user_id = $1
          and (
            $2::text is null
            or entry_type ilike $2 escape '\'
            or ref_type ilike $2 escape '\'
            or idempotency_key ilike $2 escape '\'
          )
        order by created_at desc
        limit $3 offset $4
        "#,
    )
    .bind(user_id)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(ledger_from_row).collect())
    .map_err(AppError::from_sqlx)
}

fn ledger_from_row(row: sqlx::postgres::PgRow) -> LedgerEntry {
    LedgerEntry {
        id: row.get("id"),
        user_id: row.get("user_id"),
        asset: row.get("asset"),
        entry_type: row.get("entry_type"),
        amount: row.get("amount"),
        balance_available_after: row.get("balance_available_after"),
        balance_frozen_after: row.get("balance_frozen_after"),
        ref_type: row.get("ref_type"),
        ref_id: row.get("ref_id"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

async fn insert_ledger_tx(
    tx: &mut Transaction<'_, Postgres>,
    entry: LedgerEntry,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into ledger_entries
          (id, user_id, asset, entry_type, amount, balance_available_after,
           balance_frozen_after, ref_type, ref_id, idempotency_key, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        on conflict (idempotency_key) do nothing
        "#,
    )
    .bind(entry.id)
    .bind(entry.user_id)
    .bind(&entry.asset)
    .bind(&entry.entry_type)
    .bind(entry.amount)
    .bind(entry.balance_available_after)
    .bind(entry.balance_frozen_after)
    .bind(&entry.ref_type)
    .bind(entry.ref_id)
    .bind(&entry.idempotency_key)
    .bind(entry.created_at)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn insert_deposit_tx(
    tx: &mut Transaction<'_, Postgres>,
    deposit: &DepositRecord,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into deposits
          (id, user_id, chain, asset, token_contract, contract_address, deposit_id, tx_hash,
           from_address, to_address, amount, credited_amount, platform_fee, block_number,
           confirmations, status, credited_at, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
        on conflict (tx_hash) do nothing
        "#,
    )
    .bind(deposit.id)
    .bind(deposit.user_id)
    .bind(&deposit.chain)
    .bind(&deposit.asset)
    .bind(&deposit.token_contract)
    .bind(&deposit.contract_address)
    .bind(&deposit.deposit_id)
    .bind(&deposit.tx_hash)
    .bind(&deposit.from_address)
    .bind(&deposit.to_address)
    .bind(deposit.amount)
    .bind(deposit.credited_amount)
    .bind(deposit.platform_fee)
    .bind(deposit.block_number)
    .bind(deposit.confirmations)
    .bind(&deposit.status)
    .bind(deposit.credited_at)
    .bind(deposit.created_at)
    .execute(&mut **tx)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_credit_deposit(pool: &PgPool, deposit: &DepositRecord) -> Result<(), AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let existing = sqlx::query(
        r#"
        select id, user_id, amount, status
        from deposits
        where tx_hash = $1
        for update
        "#,
    )
    .bind(&deposit.tx_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;

    if let Some(row) = existing {
        let existing_id: Uuid = row.get("id");
        let existing_user_id: Uuid = row.get("user_id");
        let existing_amount: Decimal = row.get("amount");
        let existing_status: String = row.get("status");
        if existing_user_id != deposit.user_id
            || (existing_amount != deposit.amount && existing_status == "credited")
        {
            return Err(AppError::conflict(
                "DEPOSIT_ALREADY_SUBMITTED",
                "deposit transaction belongs to a different deposit",
            ));
        }
        sqlx::query(
            r#"
            update deposits
            set confirmations = $2,
                status = $3,
                credited_at = coalesce(credited_at, $4),
                block_number = case when $5 > 0 then $5 else block_number end,
                credited_amount = $6,
                platform_fee = $7,
                amount = $8
            where tx_hash = $1
            "#,
        )
        .bind(&deposit.tx_hash)
        .bind(deposit.confirmations)
        .bind(&deposit.status)
        .bind(deposit.credited_at)
        .bind(deposit.block_number)
        .bind(deposit.credited_amount)
        .bind(deposit.platform_fee)
        .bind(deposit.amount)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        if existing_status != "credited" && deposit.status == "credited" {
            db_apply_deposit_credit_tx(&mut tx, deposit, existing_id).await?;
        }
        tx.commit().await.map_err(AppError::from_sqlx)?;
        return Ok(());
    }

    insert_deposit_tx(&mut tx, deposit).await?;
    if deposit.status == "credited" {
        db_apply_deposit_credit_tx(&mut tx, deposit, deposit.id).await?;
    }
    tx.commit().await.map_err(AppError::from_sqlx)
}

async fn db_apply_deposit_credit_tx(
    tx: &mut Transaction<'_, Postgres>,
    deposit: &DepositRecord,
    deposit_id: Uuid,
) -> Result<(), AppError> {
    let balance_row = sqlx::query(
        r#"
        select user_id, asset, available, frozen, updated_at
        from balances
        where user_id = $1
        for update
        "#,
    )
    .bind(deposit.user_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let available: Decimal = balance_row.get("available");
    let frozen: Decimal = balance_row.get("frozen");
    let after_available = available + deposit.credited_amount;
    let now = Utc::now();
    sqlx::query(
        r#"
        update balances
        set available = $2, frozen = $3, updated_at = $4
        where user_id = $1
        "#,
    )
    .bind(deposit.user_id)
    .bind(after_available)
    .bind(frozen)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(AppError::from_sqlx)?;
    insert_ledger_tx(
        tx,
        LedgerEntry {
            id: Uuid::now_v7(),
            user_id: deposit.user_id,
            asset: "USDT".to_string(),
            entry_type: "deposit_credit".to_string(),
            amount: deposit.credited_amount,
            balance_available_after: after_available,
            balance_frozen_after: frozen,
            ref_type: "deposit".to_string(),
            ref_id: deposit_id,
            idempotency_key: format!("deposit:{}", deposit.tx_hash),
            created_at: now,
        },
    )
    .await
}

async fn upsert_wallet_binding(
    pool: &PgPool,
    user_id: Uuid,
    chain: &str,
    address: &str,
    verified: bool,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into wallet_bindings (id, user_id, chain, address, verified, created_at, updated_at)
        values ($1, $2, $3, $4, $5, $6, $6)
        on conflict (chain, address) do update
        set user_id = excluded.user_id,
            verified = excluded.verified,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(chain)
    .bind(address)
    .bind(verified)
    .bind(Utc::now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_bound_wallet(
    pool: &PgPool,
    user_id: Uuid,
    chain: &str,
) -> Result<Option<String>, AppError> {
    sqlx::query(
        r#"
        select address
        from wallet_bindings
        where user_id = $1 and chain = $2 and verified = true
        order by updated_at desc
        limit 1
        "#,
    )
    .bind(user_id)
    .bind(chain)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(|row| row.get("address")))
    .map_err(AppError::from_sqlx)
}

async fn db_referral_parents_for_code(
    pool: &PgPool,
    invited_by_code: Option<&str>,
) -> Result<(Option<Uuid>, Option<Uuid>), AppError> {
    let Some(code) = invited_by_code else {
        return Ok((None, None));
    };
    let code = normalize_referral_code(code)?;
    if code.is_empty() {
        return Ok((None, None));
    }
    let parent = sqlx::query(
        r#"
        select user_id, parent_user_id
        from referral_links
        where referral_code = $1
        "#,
    )
    .bind(&code)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from_sqlx)?
    .ok_or_else(|| AppError::bad_request("REFERRAL_CODE_NOT_FOUND", "referral code not found"))?;
    Ok((parent.get("user_id"), parent.get("parent_user_id")))
}

async fn memory_referral_parents_for_code(
    state: &SharedState,
    invited_by_code: Option<&str>,
) -> (Option<Uuid>, Option<Uuid>) {
    let Some(code) = invited_by_code else {
        return (None, None);
    };
    let Ok(code) = normalize_referral_code(code) else {
        return (None, None);
    };
    let parent_user_id = state.referral_codes.read().await.get(&code).cloned();
    let grandparent_user_id = match parent_user_id {
        Some(parent_id) => state
            .referral_links
            .read()
            .await
            .get(&parent_id)
            .and_then(|link| link.parent_user_id),
        None => None,
    };
    (parent_user_id, grandparent_user_id)
}

async fn db_referral_summary(pool: &PgPool, user_id: Uuid) -> Result<serde_json::Value, AppError> {
    let link = sqlx::query(
        r#"
        select user_id, parent_user_id, grandparent_user_id, referral_code, invited_by_code, created_at
        from referral_links
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from_sqlx)?
    .map(referral_link_from_row);
    let direct_count: i64 =
        sqlx::query("select count(*) as count from referral_links where parent_user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map(|row| row.get("count"))
            .map_err(AppError::from_sqlx)?;
    let indirect_count: i64 =
        sqlx::query("select count(*) as count from referral_links where grandparent_user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map(|row| row.get("count"))
            .map_err(AppError::from_sqlx)?;
    let rewards = db_referral_rewards_for_user(pool, user_id).await?;
    let total_reward = rewards
        .iter()
        .fold(Decimal::ZERO, |total, reward| total + reward.amount);
    Ok(json!({
        "referral_code": link.as_ref().map(|row| row.referral_code.clone()).unwrap_or_else(|| referral_code_for_user(user_id)),
        "invited_by_code": link.as_ref().and_then(|row| row.invited_by_code.clone()),
        "direct_count": direct_count,
        "indirect_count": indirect_count,
        "direct_rate": decimal(DIRECT_REFERRAL_RATE),
        "indirect_rate": decimal(INDIRECT_REFERRAL_RATE),
        "total_reward": money(total_reward),
        "rewards": rewards,
    }))
}

async fn db_referral_rewards_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<ReferralRewardRecord>, AppError> {
    sqlx::query(
        r#"
        select id, settlement_id, source_user_id, recipient_user_id, level,
               rate, base_profit, amount, status, created_at
        from referral_rewards
        where recipient_user_id = $1
        order by created_at desc
        limit 100
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(referral_reward_from_row).collect())
    .map_err(AppError::from_sqlx)
}

fn referral_link_from_row(row: sqlx::postgres::PgRow) -> ReferralLinkRecord {
    ReferralLinkRecord {
        user_id: row.get("user_id"),
        parent_user_id: row.get("parent_user_id"),
        grandparent_user_id: row.get("grandparent_user_id"),
        referral_code: row.get("referral_code"),
        invited_by_code: row.get("invited_by_code"),
        created_at: row.get("created_at"),
    }
}

fn referral_reward_from_row(row: sqlx::postgres::PgRow) -> ReferralRewardRecord {
    ReferralRewardRecord {
        id: row.get("id"),
        settlement_id: row.get("settlement_id"),
        source_user_id: row.get("source_user_id"),
        recipient_user_id: row.get("recipient_user_id"),
        level: row.get("level"),
        rate: row.get("rate"),
        base_profit: row.get("base_profit"),
        amount: row.get("amount"),
        status: row.get("status"),
        created_at: row.get("created_at"),
    }
}

async fn db_deposits_for_user(
    pool: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<DepositRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, deposit_id, tx_hash,
               from_address, to_address, amount, credited_amount, platform_fee, block_number,
               confirmations, status, credited_at, created_at
        from deposits
        where user_id = $1
          and ($2::text is null or status = $2)
          and (
            $3::text is null
            or tx_hash ilike $3 escape '\'
            or deposit_id ilike $3 escape '\'
            or from_address ilike $3 escape '\'
            or status ilike $3 escape '\'
          )
        order by created_at desc
        limit $4 offset $5
        "#,
    )
    .bind(user_id)
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(deposit_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_confirming_deposits(pool: &PgPool, limit: i64) -> Result<Vec<DepositRecord>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, deposit_id, tx_hash,
               from_address, to_address, amount, credited_amount, platform_fee, block_number,
               confirmations, status, credited_at, created_at
        from deposits
        where status = 'confirming'
        order by created_at asc
        limit $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(deposit_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_admin_deposits(
    pool: &PgPool,
    query: &ListQuery,
) -> Result<Vec<DepositRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, deposit_id, tx_hash,
               from_address, to_address, amount, credited_amount, platform_fee, block_number,
               confirmations, status, credited_at, created_at
        from deposits
        where ($1::text is null or status = $1)
          and (
            $2::text is null
            or tx_hash ilike $2 escape '\'
            or deposit_id ilike $2 escape '\'
            or from_address ilike $2 escape '\'
            or user_id::text ilike $2 escape '\'
            or status ilike $2 escape '\'
          )
        order by created_at desc
        limit $3 offset $4
        "#,
    )
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(deposit_from_row).collect())
    .map_err(AppError::from_sqlx)
}

fn deposit_from_row(row: sqlx::postgres::PgRow) -> DepositRecord {
    DepositRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        chain: row.get("chain"),
        asset: row.get("asset"),
        token_contract: row.get("token_contract"),
        contract_address: row.get("contract_address"),
        deposit_id: row.get("deposit_id"),
        tx_hash: row.get("tx_hash"),
        from_address: row.get("from_address"),
        to_address: row.get("to_address"),
        amount: row.get("amount"),
        credited_amount: row.get("credited_amount"),
        platform_fee: row.get("platform_fee"),
        block_number: row.get("block_number"),
        confirmations: row.get("confirmations"),
        status: row.get("status"),
        credited_at: row.get("credited_at"),
        created_at: row.get("created_at"),
    }
}

async fn db_admin_ledger(pool: &PgPool, query: &ListQuery) -> Result<Vec<LedgerEntry>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    sqlx::query(
        r#"
        select id, user_id, asset, entry_type, amount, balance_available_after,
               balance_frozen_after, ref_type, ref_id, idempotency_key, created_at
        from ledger_entries
        where $1::text is null
           or entry_type ilike $1 escape '\'
           or ref_type ilike $1 escape '\'
           or idempotency_key ilike $1 escape '\'
           or user_id::text ilike $1 escape '\'
        order by created_at desc
        limit $2 offset $3
        "#,
    )
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(ledger_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_confirm_withdrawal(
    pool: &PgPool,
    withdrawal_id: Uuid,
    verified: &VerifiedRefund,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let row = sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, tx_hash, refund_id,
               payout_wallet, to_address, amount, block_number, confirmations, status,
               requested_at, processed_at, rejected_at, reject_reason
        from withdrawals
        where id = $1
        for update
        "#,
    )
    .bind(withdrawal_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?
    .ok_or_else(|| AppError::bad_request("WITHDRAWAL_NOT_FOUND", "withdrawal not found"))?;
    let mut withdrawal = withdrawal_from_row(row);
    if withdrawal.status != "pending_review" && withdrawal.status != "confirming" {
        return Err(AppError::conflict(
            "WITHDRAWAL_STATUS_INVALID",
            "only pending or confirming withdrawals can be confirmed",
        ));
    }
    withdrawal.tx_hash = Some(verified.tx_hash.clone());
    withdrawal.token_contract = verified.token_contract.clone();
    withdrawal.contract_address = verified.contract_address.clone();
    withdrawal.payout_wallet = Some(verified.payout_wallet.clone());
    withdrawal.to_address = verified.to_address.clone();
    withdrawal.amount = verified.amount;
    withdrawal.block_number = Some(verified.block_number);
    withdrawal.confirmations = verified.confirmations;
    withdrawal.status = verified.status.clone();
    withdrawal.processed_at = verified.processed_at;
    sqlx::query(
        r#"
        update withdrawals
        set tx_hash = $2, token_contract = $3, contract_address = $4, payout_wallet = $5,
            to_address = $6, amount = $7, block_number = $8, confirmations = $9,
            status = $10, processed_at = $11
        where id = $1
        "#,
    )
    .bind(withdrawal.id)
    .bind(&withdrawal.tx_hash)
    .bind(&withdrawal.token_contract)
    .bind(&withdrawal.contract_address)
    .bind(&withdrawal.payout_wallet)
    .bind(&withdrawal.to_address)
    .bind(withdrawal.amount)
    .bind(withdrawal.block_number)
    .bind(withdrawal.confirmations)
    .bind(&withdrawal.status)
    .bind(withdrawal.processed_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;

    if withdrawal.status == "paid" {
        let balance_row = sqlx::query(
            r#"
            select available, frozen
            from balances
            where user_id = $1
            for update
            "#,
        )
        .bind(withdrawal.user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        let available: Decimal = balance_row.get("available");
        let mut frozen: Decimal = balance_row.get("frozen");
        let release = withdrawal.amount.min(frozen);
        frozen -= release;
        sqlx::query(
            r#"
            update balances
            set available = $2, frozen = $3, updated_at = $4
            where user_id = $1
            "#,
        )
        .bind(withdrawal.user_id)
        .bind(available)
        .bind(frozen)
        .bind(Utc::now())
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        insert_ledger_tx(
            &mut tx,
            LedgerEntry {
                id: Uuid::now_v7(),
                user_id: withdrawal.user_id,
                asset: "USDT".to_string(),
                entry_type: "withdrawal_paid".to_string(),
                amount: -withdrawal.amount,
                balance_available_after: available,
                balance_frozen_after: frozen,
                ref_type: "withdrawal".to_string(),
                ref_id: withdrawal.id,
                idempotency_key: format!("withdrawal_paid:{}", withdrawal.id),
                created_at: Utc::now(),
            },
        )
        .await?;
    }

    tx.commit().await.map_err(AppError::from_sqlx)
}

async fn db_reject_withdrawal(
    pool: &PgPool,
    withdrawal_id: Uuid,
    reason: Option<String>,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let row = sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, tx_hash, refund_id,
               payout_wallet, to_address, amount, block_number, confirmations, status,
               requested_at, processed_at, rejected_at, reject_reason
        from withdrawals
        where id = $1
        for update
        "#,
    )
    .bind(withdrawal_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?
    .ok_or_else(|| AppError::bad_request("WITHDRAWAL_NOT_FOUND", "withdrawal not found"))?;
    let mut withdrawal = withdrawal_from_row(row);
    if withdrawal.status != "pending_review" && withdrawal.status != "confirming" {
        return Err(AppError::conflict(
            "WITHDRAWAL_STATUS_INVALID",
            "only pending or confirming withdrawals can be rejected",
        ));
    }
    withdrawal.status = "rejected".to_string();
    withdrawal.rejected_at = Some(Utc::now());
    withdrawal.reject_reason = reason;
    sqlx::query(
        r#"
        update withdrawals
        set status = 'rejected', rejected_at = $2, reject_reason = $3
        where id = $1
        "#,
    )
    .bind(withdrawal.id)
    .bind(withdrawal.rejected_at)
    .bind(&withdrawal.reject_reason)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let balance_row = sqlx::query(
        r#"
        select available, frozen
        from balances
        where user_id = $1
        for update
        "#,
    )
    .bind(withdrawal.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let mut available: Decimal = balance_row.get("available");
    let mut frozen: Decimal = balance_row.get("frozen");
    let release = withdrawal.amount.min(frozen);
    frozen -= release;
    available += release;
    sqlx::query(
        r#"
        update balances
        set available = $2, frozen = $3, updated_at = $4
        where user_id = $1
        "#,
    )
    .bind(withdrawal.user_id)
    .bind(available)
    .bind(frozen)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    insert_ledger_tx(
        &mut tx,
        LedgerEntry {
            id: Uuid::now_v7(),
            user_id: withdrawal.user_id,
            asset: "USDT".to_string(),
            entry_type: "withdrawal_rejected".to_string(),
            amount: withdrawal.amount,
            balance_available_after: available,
            balance_frozen_after: frozen,
            ref_type: "withdrawal".to_string(),
            ref_id: withdrawal.id,
            idempotency_key: format!("withdrawal_rejected:{}", withdrawal.id),
            created_at: Utc::now(),
        },
    )
    .await?;
    tx.commit().await.map_err(AppError::from_sqlx)
}

async fn db_create_withdrawal(
    pool: &PgPool,
    withdrawal: &WithdrawalRecord,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let balance_row = sqlx::query(
        r#"
        select available, frozen
        from balances
        where user_id = $1
        for update
        "#,
    )
    .bind(withdrawal.user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?
    .ok_or_else(|| AppError::bad_request("BALANCE_NOT_FOUND", "balance not found"))?;
    let available: Decimal = balance_row.get("available");
    let frozen: Decimal = balance_row.get("frozen");
    if available < withdrawal.amount {
        return Err(AppError::bad_request(
            "BALANCE_INSUFFICIENT",
            "available balance is below withdrawal amount",
        ));
    }
    sqlx::query(
        r#"
        update balances
        set available = $2, frozen = $3, updated_at = $4
        where user_id = $1
        "#,
    )
    .bind(withdrawal.user_id)
    .bind(available - withdrawal.amount)
    .bind(frozen + withdrawal.amount)
    .bind(withdrawal.requested_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    insert_ledger_tx(
        &mut tx,
        LedgerEntry {
            id: Uuid::now_v7(),
            user_id: withdrawal.user_id,
            asset: "USDT".to_string(),
            entry_type: "withdrawal_reserve".to_string(),
            amount: -withdrawal.amount,
            balance_available_after: available - withdrawal.amount,
            balance_frozen_after: frozen + withdrawal.amount,
            ref_type: "withdrawal".to_string(),
            ref_id: withdrawal.id,
            idempotency_key: format!("withdrawal_reserve:{}", withdrawal.id),
            created_at: withdrawal.requested_at,
        },
    )
    .await?;
    sqlx::query(
        r#"
        insert into withdrawals
          (id, user_id, chain, asset, token_contract, contract_address, tx_hash, refund_id,
           payout_wallet, to_address, amount, block_number, confirmations, status,
           requested_at, processed_at, rejected_at, reject_reason)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
        on conflict (refund_id) do nothing
        "#,
    )
    .bind(withdrawal.id)
    .bind(withdrawal.user_id)
    .bind(&withdrawal.chain)
    .bind(&withdrawal.asset)
    .bind(&withdrawal.token_contract)
    .bind(&withdrawal.contract_address)
    .bind(&withdrawal.tx_hash)
    .bind(&withdrawal.refund_id)
    .bind(&withdrawal.payout_wallet)
    .bind(&withdrawal.to_address)
    .bind(withdrawal.amount)
    .bind(withdrawal.block_number)
    .bind(withdrawal.confirmations)
    .bind(&withdrawal.status)
    .bind(withdrawal.requested_at)
    .bind(withdrawal.processed_at)
    .bind(withdrawal.rejected_at)
    .bind(&withdrawal.reject_reason)
    .execute(&mut *tx)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)?;
    tx.commit().await.map_err(AppError::from_sqlx)
}

async fn db_withdrawals_for_user(
    pool: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<WithdrawalRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, tx_hash,
               refund_id, payout_wallet, to_address, amount, block_number, confirmations,
               status, requested_at, processed_at, rejected_at, reject_reason
        from withdrawals
        where user_id = $1
          and ($2::text is null or status = $2)
          and (
            $3::text is null
            or refund_id ilike $3 escape '\'
            or to_address ilike $3 escape '\'
            or tx_hash ilike $3 escape '\'
            or status ilike $3 escape '\'
          )
        order by requested_at desc
        limit $4 offset $5
        "#,
    )
    .bind(user_id)
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(withdrawal_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_admin_withdrawals(
    pool: &PgPool,
    query: &ListQuery,
) -> Result<Vec<WithdrawalRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, tx_hash,
               refund_id, payout_wallet, to_address, amount, block_number, confirmations,
               status, requested_at, processed_at, rejected_at, reject_reason
        from withdrawals
        where ($1::text is null or status = $1)
          and (
            $2::text is null
            or refund_id ilike $2 escape '\'
            or to_address ilike $2 escape '\'
            or tx_hash ilike $2 escape '\'
            or user_id::text ilike $2 escape '\'
            or status ilike $2 escape '\'
          )
        order by requested_at desc
        limit $3 offset $4
        "#,
    )
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(withdrawal_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_withdrawal_by_id(
    pool: &PgPool,
    withdrawal_id: Uuid,
) -> Result<WithdrawalRecord, AppError> {
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, tx_hash,
               refund_id, payout_wallet, to_address, amount, block_number, confirmations,
               status, requested_at, processed_at, rejected_at, reject_reason
        from withdrawals
        where id = $1
        "#,
    )
    .bind(withdrawal_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from_sqlx)?
    .map(withdrawal_from_row)
    .ok_or_else(|| AppError::bad_request("WITHDRAWAL_NOT_FOUND", "withdrawal not found"))
}

fn withdrawal_from_row(row: sqlx::postgres::PgRow) -> WithdrawalRecord {
    WithdrawalRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        chain: row.get("chain"),
        asset: row.get("asset"),
        token_contract: row.get("token_contract"),
        contract_address: row.get("contract_address"),
        tx_hash: row.get("tx_hash"),
        refund_id: row.get("refund_id"),
        payout_wallet: row.get("payout_wallet"),
        to_address: row.get("to_address"),
        amount: row.get("amount"),
        block_number: row.get("block_number"),
        confirmations: row.get("confirmations"),
        status: row.get("status"),
        requested_at: row.get("requested_at"),
        processed_at: row.get("processed_at"),
        rejected_at: row.get("rejected_at"),
        reject_reason: row.get("reject_reason"),
    }
}

async fn db_reserve_signal_fee(
    pool: &PgPool,
    reservation: &SignalReservation,
    settings: &PlatformTradingSettings,
) -> Result<BalanceRecord, AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let balance_row = sqlx::query(
        r#"
        select user_id, asset, available, frozen, updated_at
        from balances
        where user_id = $1
        for update
        "#,
    )
    .bind(reservation.user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?
    .ok_or_else(|| AppError::bad_request("BALANCE_NOT_FOUND", "balance not found"))?;
    let max_active_orders = max_active_event_orders_per_user(settings) as i64;
    let active_orders: i64 = sqlx::query(
        r#"
        select count(*) as count
        from signal_reservations r
        join signals s on s.id = r.signal_id
        where r.user_id = $1
          and s.symbol = $2
          and s.period_seconds = $3
          and s.cycle_start <= (
            select cycle_start from signals where id = $4
          )
          and s.cycle_end > (
            select cycle_start from signals where id = $4
          )
          and r.status in ('reserved', 'order_submitted')
        "#,
    )
    .bind(reservation.user_id)
    .bind(DEFAULT_SYMBOL)
    .bind(DEFAULT_PERIOD_SECONDS)
    .bind(reservation.signal_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?
    .get("count");
    if active_orders >= max_active_orders {
        tx.rollback().await.map_err(AppError::from_sqlx)?;
        return Err(AppError::conflict(
            "ACTIVE_ORDER_LIMIT_REACHED",
            "active event contract order limit reached for this cycle",
        ));
    }
    let available: Decimal = balance_row.get("available");
    let frozen: Decimal = balance_row.get("frozen");
    if available < reservation.fee_reserve_usdt {
        return Err(AppError::bad_request(
            "BALANCE_INSUFFICIENT",
            "available balance is below fee reserve",
        ));
    }
    sqlx::query(
        r#"
        insert into signal_reservations
          (id, user_id, signal_id, stake_usdt, expected_profit_usdt, fee_reserve_usdt,
           status, order_id, execution_report_id, expires_at, created_at, updated_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(reservation.id)
    .bind(reservation.user_id)
    .bind(reservation.signal_id)
    .bind(reservation.stake_usdt)
    .bind(reservation.expected_profit_usdt)
    .bind(reservation.fee_reserve_usdt)
    .bind(&reservation.status)
    .bind(reservation.order_id)
    .bind(reservation.execution_report_id)
    .bind(reservation.expires_at)
    .bind(reservation.created_at)
    .bind(reservation.updated_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let after_available = available - reservation.fee_reserve_usdt;
    let after_frozen = frozen + reservation.fee_reserve_usdt;
    let now = Utc::now();
    let row = sqlx::query(
        r#"
        update balances
        set available = $2, frozen = $3, updated_at = $4
        where user_id = $1
        returning user_id, asset, available, frozen, updated_at
        "#,
    )
    .bind(reservation.user_id)
    .bind(after_available)
    .bind(after_frozen)
    .bind(now)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let after = BalanceRecord {
        user_id: row.get("user_id"),
        asset: row.get("asset"),
        available: row.get("available"),
        frozen: row.get("frozen"),
        updated_at: row.get("updated_at"),
    };
    insert_ledger_tx(
        &mut tx,
        LedgerEntry {
            id: Uuid::now_v7(),
            user_id: reservation.user_id,
            asset: "USDT".to_string(),
            entry_type: "signal_fee_reserve".to_string(),
            amount: -reservation.fee_reserve_usdt,
            balance_available_after: after.available,
            balance_frozen_after: after.frozen,
            ref_type: "signal_reservation".to_string(),
            ref_id: reservation.id,
            idempotency_key: format!("signal_fee_reserve:{}", reservation.id),
            created_at: now,
        },
    )
    .await?;
    tx.commit().await.map_err(AppError::from_sqlx)?;
    Ok(after)
}

async fn db_reserve_or_load_signal_fee(
    pool: &PgPool,
    reservation: &SignalReservation,
    settings: &PlatformTradingSettings,
) -> Result<BalanceRecord, AppError> {
    match db_reserve_signal_fee(pool, reservation, settings).await {
        Ok(balance) => Ok(balance),
        Err(err) if is_unique_violation(&err) => Err(AppError::conflict(
            "SIGNAL_RESERVATION_ALREADY_EXISTS",
            "signal reservation already exists",
        )),
        Err(err) => Err(err),
    }
}

async fn db_signal_reservation_for_user_signal(
    pool: &PgPool,
    user_id: Uuid,
    signal_id: Uuid,
) -> Result<Option<SignalReservation>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, signal_id, stake_usdt, expected_profit_usdt, fee_reserve_usdt,
               status, order_id, execution_report_id, expires_at, created_at, updated_at
        from signal_reservations
        where user_id = $1 and signal_id = $2
        "#,
    )
    .bind(user_id)
    .bind(signal_id)
    .fetch_optional(pool)
    .await
    .map(|row| row.map(signal_reservation_from_row))
    .map_err(AppError::from_sqlx)
}

fn signal_reservation_from_row(row: sqlx::postgres::PgRow) -> SignalReservation {
    SignalReservation {
        id: row.get("id"),
        user_id: row.get("user_id"),
        signal_id: row.get("signal_id"),
        stake_usdt: row.get("stake_usdt"),
        expected_profit_usdt: row.get("expected_profit_usdt"),
        fee_reserve_usdt: row.get("fee_reserve_usdt"),
        status: row.get("status"),
        order_id: row.get("order_id"),
        execution_report_id: row.get("execution_report_id"),
        expires_at: row.get("expires_at"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

async fn db_release_stale_signal_reservations(pool: &PgPool) -> Result<usize, AppError> {
    let now = Utc::now();
    let no_ack_before = now
        - Duration::seconds(env_i64(
            "RESERVATION_NO_ACK_RELEASE_SECONDS",
            DEFAULT_RESERVATION_NO_ACK_RELEASE_SECONDS,
            3,
        ));
    let rows = sqlx::query(
        r#"
        select r.id
        from signal_reservations r
        where r.status = 'reserved'
          and r.order_id is null
          and not exists (
            select 1 from event_contract_orders o where o.reservation_id = r.id
          )
          and (
            r.expires_at <= $1
            or exists (
              select 1
              from signal_deliveries d
              where d.reservation_id = r.id
                and d.trade_authorized = true
                and d.delivery_status = 'failed'
            )
            or (
              not exists (
                select 1
                from signal_deliveries d
                where d.reservation_id = r.id
                  and d.received_at is not null
              )
              and exists (
                select 1
                from signal_deliveries d
                where d.reservation_id = r.id
                  and d.trade_authorized = true
                  and d.sent_at <= $2
              )
            )
          )
        order by r.created_at asc
        limit $3
        "#,
    )
    .bind(now)
    .bind(no_ack_before)
    .bind(env_i64(
        "RESERVATION_RELEASE_LIMIT",
        DEFAULT_RESERVATION_RELEASE_LIMIT,
        1,
    ))
    .fetch_all(pool)
    .await
    .map_err(AppError::from_sqlx)?;

    let mut released = 0usize;
    for row in rows {
        let reservation_id: Uuid = row.get("id");
        if db_release_signal_reservation(
            pool,
            reservation_id,
            "signal_fee_reserve_release_stale",
            "expired",
        )
        .await?
        {
            released += 1;
        }
    }
    Ok(released)
}

async fn db_release_signal_reservation(
    pool: &PgPool,
    reservation_id: Uuid,
    entry_type: &str,
    release_status: &str,
) -> Result<bool, AppError> {
    db_release_signal_reservation_locked(pool, reservation_id, None, entry_type, release_status)
        .await
}

async fn db_release_signal_reservation_for_user(
    pool: &PgPool,
    reservation_id: Uuid,
    user_id: Uuid,
    entry_type: &str,
    release_status: &str,
) -> Result<bool, AppError> {
    db_release_signal_reservation_locked(
        pool,
        reservation_id,
        Some(user_id),
        entry_type,
        release_status,
    )
    .await
}

async fn db_release_signal_reservation_locked(
    pool: &PgPool,
    reservation_id: Uuid,
    user_id: Option<Uuid>,
    entry_type: &str,
    release_status: &str,
) -> Result<bool, AppError> {
    if release_status != "released" && release_status != "expired" {
        return Err(AppError::bad_request(
            "RESERVATION_RELEASE_STATUS_INVALID",
            "release status must be released or expired",
        ));
    }
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let reservation_row = sqlx::query(
        r#"
        select id, user_id, signal_id, stake_usdt, expected_profit_usdt, fee_reserve_usdt,
               status, order_id, execution_report_id, expires_at, created_at, updated_at
        from signal_reservations
        where id = $1
        for update
        "#,
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let Some(row) = reservation_row else {
        tx.rollback().await.map_err(AppError::from_sqlx)?;
        return Ok(false);
    };
    let reservation = signal_reservation_from_row(row);
    if let Some(expected_user_id) = user_id {
        if reservation.user_id != expected_user_id {
            tx.rollback().await.map_err(AppError::from_sqlx)?;
            return Err(AppError::forbidden(
                "RESERVATION_USER_MISMATCH",
                "reservation does not belong to current user",
            ));
        }
    }
    if reservation.status != "reserved" || reservation.order_id.is_some() {
        tx.rollback().await.map_err(AppError::from_sqlx)?;
        return Ok(false);
    }
    let order_exists =
        sqlx::query("select id from event_contract_orders where reservation_id = $1")
            .bind(reservation.id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::from_sqlx)?
            .is_some();
    if order_exists {
        tx.rollback().await.map_err(AppError::from_sqlx)?;
        return Ok(false);
    }
    let balance_row = sqlx::query(
        r#"
        select available, frozen
        from balances
        where user_id = $1
        for update
        "#,
    )
    .bind(reservation.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let available: Decimal = balance_row.get("available");
    let frozen: Decimal = balance_row.get("frozen");
    let release = reservation.fee_reserve_usdt.min(frozen);
    let after_available = available + release;
    let after_frozen = frozen - release;
    let now = Utc::now();
    sqlx::query(
        r#"
        update balances
        set available = $2, frozen = $3, updated_at = $4
        where user_id = $1
        "#,
    )
    .bind(reservation.user_id)
    .bind(after_available)
    .bind(after_frozen)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    sqlx::query(
        r#"
        update signal_reservations
        set status = $2, updated_at = $3
        where id = $1 and status = 'reserved'
        "#,
    )
    .bind(reservation.id)
    .bind(release_status)
    .bind(now)
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    insert_ledger_tx(
        &mut tx,
        LedgerEntry {
            id: Uuid::now_v7(),
            user_id: reservation.user_id,
            asset: "USDT".to_string(),
            entry_type: entry_type.to_string(),
            amount: release,
            balance_available_after: after_available,
            balance_frozen_after: after_frozen,
            ref_type: "signal_reservation".to_string(),
            ref_id: reservation.id,
            idempotency_key: format!("signal_fee_reserve_release:{}", reservation.id),
            created_at: now,
        },
    )
    .await?;
    tx.commit().await.map_err(AppError::from_sqlx)?;
    Ok(true)
}

async fn db_insert_order_with_reservation_validation(
    pool: &PgPool,
    order: &EventContractOrderRecord,
) -> Result<Uuid, AppError> {
    let reservation_id = order.reservation_id.ok_or_else(|| {
        AppError::bad_request("RESERVATION_REQUIRED", "reservation_id is required")
    })?;
    let signal_id = Uuid::parse_str(&order.signal_id)
        .map_err(|_| AppError::bad_request("SIGNAL_ID_INVALID", "signal_id must be a uuid"))?;
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let reservation_row = sqlx::query(
        r#"
        select id, user_id, signal_id, stake_usdt, expected_profit_usdt, fee_reserve_usdt,
               status, order_id, execution_report_id, expires_at, created_at, updated_at
        from signal_reservations
        where id = $1
        for update
        "#,
    )
    .bind(reservation_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?
    .ok_or_else(|| AppError::bad_request("RESERVATION_NOT_FOUND", "reservation not found"))?;
    let reservation = SignalReservation {
        id: reservation_row.get("id"),
        user_id: reservation_row.get("user_id"),
        signal_id: reservation_row.get("signal_id"),
        stake_usdt: reservation_row.get("stake_usdt"),
        expected_profit_usdt: reservation_row.get("expected_profit_usdt"),
        fee_reserve_usdt: reservation_row.get("fee_reserve_usdt"),
        status: reservation_row.get("status"),
        order_id: reservation_row.get("order_id"),
        execution_report_id: reservation_row.get("execution_report_id"),
        expires_at: reservation_row.get("expires_at"),
        created_at: reservation_row.get("created_at"),
        updated_at: reservation_row.get("updated_at"),
    };
    validate_order_against_reservation(order, &reservation, signal_id)?;
    if reservation.status != "reserved" && reservation.status != "order_submitted" {
        return Err(AppError::conflict(
            "RESERVATION_STATUS_INVALID",
            "reservation is not open for order submission",
        ));
    }
    let expected_profit = order.stake_usdt * DEFAULT_PAYOUT_RATIO;
    let expected_payout = order.stake_usdt + expected_profit;
    let saved_id: Uuid = sqlx::query(
        r#"
        insert into event_contract_orders
          (id, user_id, reservation_id, signal_ref, client_order_id, source, symbol,
           period_seconds, direction, stake_usdt, payout_ratio, expected_payout_usdt,
           expected_profit_usdt, automation_profile, automation_confidence, action_trace_id,
           raw_audit, order_status, reported_at, created_at, entry_price)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
        on conflict (user_id, client_order_id) do update
        set reported_at = coalesce(event_contract_orders.reported_at, excluded.reported_at),
            entry_price = coalesce(event_contract_orders.entry_price, excluded.entry_price)
        returning id
        "#,
    )
    .bind(order.id)
    .bind(order.user_id)
    .bind(order.reservation_id)
    .bind(&order.signal_id)
    .bind(&order.client_order_id)
    .bind(&order.source)
    .bind(&order.symbol)
    .bind(order.period_seconds)
    .bind(&order.direction)
    .bind(order.stake_usdt)
    .bind(DEFAULT_PAYOUT_RATIO)
    .bind(expected_payout)
    .bind(expected_profit)
    .bind(&order.automation_profile)
    .bind(order.automation_confidence)
    .bind(&order.action_trace_id)
    .bind(&order.raw_audit)
    .bind(&order.order_status)
    .bind(order.reported_at)
    .bind(order.created_at)
    .bind(order.entry_price)
    .fetch_one(&mut *tx)
    .await
    .map(|row| row.get("id"))
    .map_err(AppError::from_sqlx)?;
    sqlx::query(
        r#"
        update signal_reservations
        set status = 'order_submitted', order_id = $2, updated_at = $3
        where id = $1 and status in ('reserved', 'order_submitted')
        "#,
    )
    .bind(reservation_id)
    .bind(saved_id)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    tx.commit().await.map_err(AppError::from_sqlx)?;
    Ok(saved_id)
}

fn binance_income_to_record(
    user_id: Uuid,
    income: &BinanceIncomeResponse,
) -> Result<Option<BinanceIncomeEventRecord>, AppError> {
    let Some(external_ref) = income
        .info
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let value = parse_decimal(&income.income, "income")?;
    let event_time = DateTime::<Utc>::from_timestamp_millis(income.time).ok_or_else(|| {
        AppError::bad_request("BINANCE_INCOME_TIME_INVALID", "invalid income event time")
    })?;
    Ok(Some(BinanceIncomeEventRecord {
        id: Uuid::now_v7(),
        user_id,
        provider: BINANCE_PROVIDER.to_string(),
        external_ref: external_ref.to_string(),
        income_type: income.income_type.clone(),
        income: value,
        asset: income.asset.clone(),
        event_time,
        tran_id: income.tran_id.as_ref().map(|value| value.to_string()),
        trade_id: income.trade_id.as_ref().map(|value| value.to_string()),
        raw: serde_json::to_value(income).unwrap_or_else(|_| json!({})),
        created_at: Utc::now(),
    }))
}

async fn db_upsert_income_event(
    tx: &mut Transaction<'_, Postgres>,
    event: &BinanceIncomeEventRecord,
) -> Result<Uuid, AppError> {
    sqlx::query(
        r#"
        insert into binance_income_events
          (id, user_id, provider, external_ref, income_type, income, asset,
           event_time, tran_id, trade_id, raw, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        on conflict (user_id, provider, external_ref, income_type, income, event_time) do update set
          tran_id = excluded.tran_id,
          trade_id = excluded.trade_id,
          raw = excluded.raw
        returning id
        "#,
    )
    .bind(event.id)
    .bind(event.user_id)
    .bind(&event.provider)
    .bind(&event.external_ref)
    .bind(&event.income_type)
    .bind(event.income)
    .bind(&event.asset)
    .bind(event.event_time)
    .bind(&event.tran_id)
    .bind(&event.trade_id)
    .bind(&event.raw)
    .bind(event.created_at)
    .fetch_one(&mut **tx)
    .await
    .map(|row| row.get("id"))
    .map_err(AppError::from_sqlx)
}

async fn db_confirm_order_from_exchange(
    pool: &PgPool,
    order_id: Uuid,
    event: &BinanceIncomeEventRecord,
) -> Result<bool, AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let income_event_id = db_upsert_income_event(&mut tx, event).await?;
    let row = sqlx::query(
        r#"
        update event_contract_orders
        set exchange_ref = $2,
            exchange_order_income_id = $3,
            exchange_order_amount = $4,
            exchange_order_time = $5,
            exchange_verification_status = 'confirmed',
            exchange_verified_at = $6,
            order_status = 'exchange_confirmed'
        where id = $1
          and exchange_verification_status in ('unverified', 'pending')
        returning id
        "#,
    )
    .bind(order_id)
    .bind(&event.external_ref)
    .bind(income_event_id)
    .bind(-event.income)
    .bind(event.event_time)
    .bind(Utc::now())
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    tx.commit().await.map_err(AppError::from_sqlx)?;
    Ok(row.is_some())
}

fn event_order_from_row(row: sqlx::postgres::PgRow) -> EventContractOrderRecord {
    EventContractOrderRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        reservation_id: row.get("reservation_id"),
        signal_id: row.get("signal_ref"),
        client_order_id: row.get("client_order_id"),
        source: row.get("source"),
        symbol: row.get("symbol"),
        period_seconds: row.get("period_seconds"),
        direction: row.get("direction"),
        stake_usdt: row.get("stake_usdt"),
        entry_price: row.try_get("entry_price").unwrap_or(None),
        automation_profile: row.get("automation_profile"),
        automation_confidence: row.get("automation_confidence"),
        action_trace_id: row.get("action_trace_id"),
        raw_audit: row.get("raw_audit"),
        order_status: row.get("order_status"),
        reported_at: row.get("reported_at"),
        created_at: row.get("created_at"),
    }
}

fn exchange_worker_slot_from_row(row: sqlx::postgres::PgRow) -> ExchangeWorkerSlot {
    ExchangeWorkerSlot {
        id: row.get("id"),
        name: row.get("name"),
        shard_total: row.get("shard_total"),
        shard_index: row.get("shard_index"),
        bind_local_ip: row.get("bind_local_ip"),
        enabled: row.get("enabled"),
        status: row.get("status"),
        last_started_at: row.get("last_started_at"),
        last_stopped_at: row.get("last_stopped_at"),
        last_error: row.get("last_error"),
        updated_at: row.get("updated_at"),
        created_at: row.get("created_at"),
    }
}

fn validate_exchange_worker_request(req: &SaveExchangeWorkerSlotRequest) -> Result<(), AppError> {
    if req.shard_total < 1 {
        return Err(AppError::bad_request(
            "WORKER_SHARD_TOTAL_INVALID",
            "shard_total must be at least 1",
        ));
    }
    if req.shard_index < 0 || req.shard_index >= req.shard_total {
        return Err(AppError::bad_request(
            "WORKER_SHARD_INDEX_INVALID",
            "shard_index must be between 0 and shard_total - 1",
        ));
    }
    parse_optional_ip(req.bind_local_ip.as_deref(), "bind_local_ip")?;
    Ok(())
}

async fn db_exchange_worker_slots(pool: &PgPool) -> Result<Vec<ExchangeWorkerSlot>, AppError> {
    sqlx::query(
        r#"
        select id, name, shard_total, shard_index, bind_local_ip, enabled, status,
               last_started_at, last_stopped_at, last_error, updated_at, created_at
        from exchange_worker_slots
        order by shard_total asc, shard_index asc, created_at asc
        "#,
    )
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(exchange_worker_slot_from_row)
            .collect()
    })
    .map_err(AppError::from_sqlx)
}

async fn db_save_exchange_worker_slot(
    pool: &PgPool,
    req: &SaveExchangeWorkerSlotRequest,
) -> Result<ExchangeWorkerSlot, AppError> {
    let id = req.id.unwrap_or_else(Uuid::now_v7);
    let now = Utc::now();
    let name = req
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("exchange-worker");
    let bind_local_ip = req
        .bind_local_ip
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    sqlx::query(
        r#"
        insert into exchange_worker_slots
          (id, name, shard_total, shard_index, bind_local_ip, enabled, status,
           last_started_at, last_stopped_at, last_error, updated_at, created_at)
        values ($1, $2, $3, $4, $5, $6, $7,
                case when $6 then $8 else null end,
                case when $6 then null else $8 end,
                null, $8, $8)
        on conflict (id) do update set
            name = excluded.name,
            shard_total = excluded.shard_total,
            shard_index = excluded.shard_index,
            bind_local_ip = excluded.bind_local_ip,
            enabled = excluded.enabled,
            status = excluded.status,
            last_started_at = case when excluded.enabled then excluded.updated_at else exchange_worker_slots.last_started_at end,
            last_stopped_at = case when excluded.enabled then exchange_worker_slots.last_stopped_at else excluded.updated_at end,
            last_error = null,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(req.shard_total)
    .bind(req.shard_index)
    .bind(bind_local_ip)
    .bind(req.enabled)
    .bind(if req.enabled { "running" } else { "stopped" })
    .bind(now)
    .execute(pool)
    .await
    .map_err(AppError::from_sqlx)?;
    db_exchange_worker_slot(pool, id).await
}

async fn db_exchange_worker_slot(pool: &PgPool, id: Uuid) -> Result<ExchangeWorkerSlot, AppError> {
    sqlx::query(
        r#"
        select id, name, shard_total, shard_index, bind_local_ip, enabled, status,
               last_started_at, last_stopped_at, last_error, updated_at, created_at
        from exchange_worker_slots
        where id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from_sqlx)?
    .map(exchange_worker_slot_from_row)
    .ok_or_else(|| AppError::bad_request("WORKER_NOT_FOUND", "exchange worker not found"))
}

async fn db_set_exchange_worker_enabled(
    pool: &PgPool,
    id: Uuid,
    enabled: bool,
) -> Result<ExchangeWorkerSlot, AppError> {
    let now = Utc::now();
    sqlx::query(
        r#"
        update exchange_worker_slots
        set enabled = $2,
            status = case when $2 then 'running' else 'stopped' end,
            last_started_at = case when $2 then $3 else last_started_at end,
            last_stopped_at = case when $2 then last_stopped_at else $3 end,
            last_error = null,
            updated_at = $3
        where id = $1
        "#,
    )
    .bind(id)
    .bind(enabled)
    .bind(now)
    .execute(pool)
    .await
    .map_err(AppError::from_sqlx)?;
    db_exchange_worker_slot(pool, id).await
}

async fn db_delete_exchange_worker_slot(pool: &PgPool, id: Uuid) -> Result<(), AppError> {
    sqlx::query("delete from exchange_worker_slots where id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(AppError::from_sqlx)
}

async fn db_mark_exchange_worker_error(
    pool: &PgPool,
    id: Uuid,
    error: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        update exchange_worker_slots
        set status = 'error',
            last_error = $2,
            updated_at = $3
        where id = $1
        "#,
    )
    .bind(id)
    .bind(error)
    .bind(Utc::now())
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_pending_exchange_orders(
    pool: &PgPool,
    limit: i64,
    config: &ExchangeWorkerConfig,
) -> Result<Vec<EventContractOrderRecord>, AppError> {
    sqlx::query(
        r#"
        select o.id, o.user_id, o.reservation_id, o.signal_ref, o.client_order_id, o.source,
               o.symbol, o.period_seconds, o.direction, o.stake_usdt,
               o.entry_price as entry_price,
               o.automation_profile, o.automation_confidence, o.action_trace_id, o.raw_audit,
               o.order_status, o.reported_at, o.created_at
        from event_contract_orders o
        where o.exchange_verification_status in ('unverified', 'pending')
          and mod(abs(hashtext(o.user_id::text)), $1) = $2
        order by o.created_at asc
        limit $3
        "#,
    )
    .bind(config.shard_total)
    .bind(config.shard_index)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(event_order_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_confirmed_exchange_orders_due_for_settlement(
    pool: &PgPool,
    now: DateTime<Utc>,
    limit: i64,
    config: &ExchangeWorkerConfig,
) -> Result<Vec<EventContractOrderRecord>, AppError> {
    sqlx::query(
        r#"
        select o.id, o.user_id, o.reservation_id, o.signal_ref, o.client_order_id, o.source,
               o.symbol, o.period_seconds, o.direction, o.stake_usdt,
               o.entry_price as entry_price,
               o.automation_profile, o.automation_confidence, o.action_trace_id, o.raw_audit,
               o.order_status, o.reported_at, o.created_at
        from event_contract_orders o
        where o.exchange_verification_status = 'confirmed'
          and o.exchange_ref is not null
          and coalesce(o.reported_at, o.created_at)
                + make_interval(secs => (o.period_seconds + $1)::double precision) <= $2
          and mod(abs(hashtext(o.user_id::text)), $3) = $4
          and not exists (
            select 1 from execution_reports r
            where r.user_id = o.user_id
              and r.signal_id::text = o.signal_ref
          )
        order by o.created_at asc
        limit $5
        "#,
    )
    .bind(BINANCE_SETTLEMENT_GRACE_SECONDS)
    .bind(now)
    .bind(config.shard_total)
    .bind(config.shard_index)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(event_order_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_order_exchange_ref(pool: &PgPool, order_id: Uuid) -> Result<Option<String>, AppError> {
    sqlx::query("select exchange_ref from event_contract_orders where id = $1")
        .bind(order_id)
        .fetch_optional(pool)
        .await
        .map(|row| row.and_then(|row| row.get("exchange_ref")))
        .map_err(AppError::from_sqlx)
}

async fn db_insert_execution_report(
    pool: &PgPool,
    report: &ExecutionReportRecord,
) -> Result<(), AppError> {
    let signal_id = Uuid::parse_str(&report.signal_id).map_err(|_| {
        AppError::bad_request(
            "SIGNAL_ID_INVALID",
            "signal_id must be a uuid for persistence",
        )
    })?;
    sqlx::query(
        r#"
        insert into execution_reports
          (id, user_id, signal_id, client_report_id, source, symbol, period_seconds,
           direction, stake_usdt, entry_price, expiry_price, payout_ratio, gross_profit,
           result, automation_profile, automation_confidence, action_trace_id,
           report_status, reported_at, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
        on conflict (user_id, client_report_id) do nothing
        "#,
    )
    .bind(report.id)
    .bind(report.user_id)
    .bind(signal_id)
    .bind(&report.client_report_id)
    .bind(&report.source)
    .bind(&report.symbol)
    .bind(report.period_seconds)
    .bind(&report.direction)
    .bind(report.stake_usdt)
    .bind(report.entry_price)
    .bind(report.expiry_price)
    .bind(report.payout_ratio)
    .bind(report.gross_profit)
    .bind(&report.result)
    .bind(&report.automation_profile)
    .bind(report.automation_confidence)
    .bind(&report.action_trace_id)
    .bind(&report.report_status)
    .bind(report.reported_at)
    .bind(report.created_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_insert_exchange_execution_report_and_settle(
    pool: &PgPool,
    report: &ExecutionReportRecord,
    payout_event: Option<&BinanceIncomeEventRecord>,
) -> Result<bool, AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let existing = sqlx::query(
        r#"
        select id, report_status
        from execution_reports
        where user_id = $1 and client_report_id = $2
        "#,
    )
    .bind(report.user_id)
    .bind(&report.client_report_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    if let Some(existing) = existing {
        let existing_report_id: Uuid = existing.get("id");
        let status: String = existing.get("report_status");
        if status != "settled" {
            let report_row = sqlx::query(
                r#"
                select id, user_id, signal_id::text as signal_id, client_report_id, source, symbol,
                       period_seconds, direction, stake_usdt, entry_price, expiry_price,
                       payout_ratio, gross_profit, result, automation_profile,
                       automation_confidence, action_trace_id, report_status, reported_at, created_at
                from execution_reports
                where id = $1
                for update
                "#,
            )
            .bind(existing_report_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::from_sqlx)?;
            let existing_report = execution_report_from_row(report_row);
            db_settle_execution_report_tx(&mut tx, &existing_report).await?;
            sqlx::query("update execution_reports set report_status = 'settled' where id = $1")
                .bind(existing_report.id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::from_sqlx)?;
        }
        tx.commit().await.map_err(AppError::from_sqlx)?;
        return Ok(false);
    }
    let payout_event_id = match payout_event {
        Some(event) => Some(db_upsert_income_event(&mut tx, event).await?),
        None => None,
    };
    let signal_id = Uuid::parse_str(&report.signal_id).map_err(|_| {
        AppError::bad_request(
            "SIGNAL_ID_INVALID",
            "signal_id must be a uuid for persistence",
        )
    })?;
    let inserted = sqlx::query(
        r#"
        insert into execution_reports
          (id, user_id, signal_id, client_report_id, source, symbol, period_seconds,
           direction, stake_usdt, entry_price, expiry_price, payout_ratio, gross_profit,
           result, automation_profile, automation_confidence, action_trace_id,
           report_status, reported_at, created_at, exchange_ref, exchange_payout_income_id, exchange_verified_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                $14, $15, $16, $17, 'pending_review', $18, $19, $20, $21, $22)
        on conflict (user_id, client_report_id) do nothing
        returning id
        "#,
    )
    .bind(report.id)
    .bind(report.user_id)
    .bind(signal_id)
    .bind(&report.client_report_id)
    .bind(&report.source)
    .bind(&report.symbol)
    .bind(report.period_seconds)
    .bind(&report.direction)
    .bind(report.stake_usdt)
    .bind(report.entry_price)
    .bind(report.expiry_price)
    .bind(report.payout_ratio)
    .bind(report.gross_profit)
    .bind(&report.result)
    .bind(&report.automation_profile)
    .bind(report.automation_confidence)
    .bind(&report.action_trace_id)
    .bind(report.reported_at)
    .bind(report.created_at)
    .bind(payout_event.map(|event| event.external_ref.clone()))
    .bind(payout_event_id)
    .bind(payout_event.map(|_| Utc::now()))
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    if inserted.is_none() {
        tx.commit().await.map_err(AppError::from_sqlx)?;
        return Ok(false);
    }
    db_settle_execution_report_tx(&mut tx, report).await?;
    sqlx::query("update execution_reports set report_status = 'settled' where id = $1")
        .bind(report.id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
    tx.commit().await.map_err(AppError::from_sqlx)?;
    Ok(true)
}

async fn db_admin_settle_execution_report(pool: &PgPool, report_id: Uuid) -> Result<(), AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    let report_row = sqlx::query(
        r#"
        select id, user_id, signal_id::text as signal_id, client_report_id, source, symbol,
               period_seconds, direction, stake_usdt, entry_price, expiry_price,
               payout_ratio, gross_profit, result, automation_profile,
               automation_confidence, action_trace_id, report_status, reported_at, created_at
        from execution_reports
        where id = $1
        for update
        "#,
    )
    .bind(report_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?
    .ok_or_else(|| AppError::bad_request("REPORT_NOT_FOUND", "execution report not found"))?;
    let report = execution_report_from_row(report_row);
    if report.report_status != "pending_review" {
        return Err(AppError::conflict(
            "REPORT_STATUS_INVALID",
            "only pending_review reports can be settled",
        ));
    }
    db_settle_execution_report_tx(&mut tx, &report).await?;
    sqlx::query("update execution_reports set report_status = 'settled' where id = $1")
        .bind(report.id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
    tx.commit().await.map_err(AppError::from_sqlx)
}

async fn db_settle_execution_report(
    pool: &PgPool,
    report: &ExecutionReportRecord,
) -> Result<(), AppError> {
    let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
    db_settle_execution_report_tx(&mut tx, report).await?;
    tx.commit().await.map_err(AppError::from_sqlx)
}

async fn db_settle_execution_report_tx(
    tx: &mut Transaction<'_, Postgres>,
    report: &ExecutionReportRecord,
) -> Result<(), AppError> {
    let signal_id = Uuid::parse_str(&report.signal_id).map_err(|_| {
        AppError::bad_request(
            "SIGNAL_ID_INVALID",
            "signal_id must be a uuid for settlement",
        )
    })?;
    let result = report.result.to_uppercase();
    let gross_profit = if result == "WIN" {
        report
            .gross_profit
            .unwrap_or(report.stake_usdt * report.payout_ratio.unwrap_or(DEFAULT_PAYOUT_RATIO))
    } else if result == "LOSS" {
        report.gross_profit.unwrap_or(-report.stake_usdt)
    } else {
        Decimal::ZERO
    };
    let profit_share_fee = if result == "WIN" {
        gross_profit.max(Decimal::ZERO) * PROFIT_SHARE_RATE
    } else {
        Decimal::ZERO
    };
    let loss_rebate = if result == "LOSS" {
        report.stake_usdt * PROFIT_SHARE_RATE
    } else {
        Decimal::ZERO
    };
    let user_net_profit = gross_profit - profit_share_fee + loss_rebate;
    let existing = sqlx::query("select id from settlements where execution_report_id = $1")
        .bind(report.id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(AppError::from_sqlx)?;
    if existing.is_some() {
        return Ok(());
    }
    let reservation = sqlx::query(
        r#"
        select id, user_id, signal_id, stake_usdt, expected_profit_usdt, fee_reserve_usdt,
               status, order_id, execution_report_id, expires_at, created_at, updated_at
        from signal_reservations
        where user_id = $1 and signal_id = $2
        for update
        "#,
    )
    .bind(report.user_id)
    .bind(signal_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let reservation = reservation
        .map(|row| SignalReservation {
            id: row.get("id"),
            user_id: row.get("user_id"),
            signal_id: row.get("signal_id"),
            stake_usdt: row.get("stake_usdt"),
            expected_profit_usdt: row.get("expected_profit_usdt"),
            fee_reserve_usdt: row.get("fee_reserve_usdt"),
            status: row.get("status"),
            order_id: row.get("order_id"),
            execution_report_id: row.get("execution_report_id"),
            expires_at: row.get("expires_at"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
        })
        .ok_or_else(|| AppError::bad_request("RESERVATION_NOT_FOUND", "reservation not found"))?;
    validate_report_against_reservation(report, &reservation, signal_id)?;
    if reservation.status != "order_submitted" && reservation.status != "reserved" {
        return Err(AppError::conflict(
            "RESERVATION_STATUS_INVALID",
            "reservation is not open for settlement",
        ));
    }
    let row = sqlx::query("select available, frozen from balances where user_id = $1 for update")
        .bind(report.user_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(AppError::from_sqlx)?;
    let mut available: Decimal = row.get("available");
    let mut frozen: Decimal = row.get("frozen");
    let release = reservation.fee_reserve_usdt.min(frozen);
    frozen -= release;
    available += release;
    if profit_share_fee > Decimal::ZERO {
        available = (available - profit_share_fee).max(Decimal::ZERO);
    }
    if loss_rebate > Decimal::ZERO {
        available += loss_rebate;
    }
    let now = Utc::now();
    let row = sqlx::query(
        r#"
        update balances
        set available = $2, frozen = $3, updated_at = $4
        where user_id = $1
        returning available, frozen
        "#,
    )
    .bind(report.user_id)
    .bind(available)
    .bind(frozen)
    .bind(now)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let after_available: Decimal = row.get("available");
    let after_frozen: Decimal = row.get("frozen");
    let settlement = SettlementRecord {
        id: Uuid::now_v7(),
        execution_report_id: report.id,
        user_id: report.user_id,
        signal_id: Some(report.signal_id.clone()),
        symbol: Some(report.symbol.clone()),
        direction: Some(report.direction.clone()),
        stake_usdt: Some(report.stake_usdt),
        entry_price: report.entry_price,
        expiry_price: report.expiry_price,
        result: Some(report.result.clone()),
        gross_profit,
        profit_share_fee,
        loss_rebate,
        user_net_profit,
        status: "settled".to_string(),
        created_at: now,
    };
    sqlx::query(
        r#"
        insert into settlements
          (id, execution_report_id, user_id, gross_profit, profit_share_fee,
           loss_rebate, user_net_profit, status, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(settlement.id)
    .bind(settlement.execution_report_id)
    .bind(settlement.user_id)
    .bind(settlement.gross_profit)
    .bind(settlement.profit_share_fee)
    .bind(settlement.loss_rebate)
    .bind(settlement.user_net_profit)
    .bind(&settlement.status)
    .bind(settlement.created_at)
    .execute(&mut **tx)
    .await
    .map_err(AppError::from_sqlx)?;
    sqlx::query(
        r#"
        update signal_reservations
        set status = 'settled', execution_report_id = $2, updated_at = $3
        where id = $1
        "#,
    )
    .bind(reservation.id)
    .bind(report.id)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let entry_type = if profit_share_fee > Decimal::ZERO {
        "profit_share_fee"
    } else if loss_rebate > Decimal::ZERO {
        "loss_rebate"
    } else {
        "signal_fee_reserve_release"
    };
    let amount = if profit_share_fee > Decimal::ZERO {
        -profit_share_fee
    } else {
        loss_rebate
    };
    insert_ledger_tx(
        tx,
        LedgerEntry {
            id: Uuid::now_v7(),
            user_id: report.user_id,
            asset: "USDT".to_string(),
            entry_type: entry_type.to_string(),
            amount,
            balance_available_after: after_available,
            balance_frozen_after: after_frozen,
            ref_type: "settlement".to_string(),
            ref_id: settlement.id,
            idempotency_key: format!("settlement:{}", report.id),
            created_at: now,
        },
    )
    .await?;
    if result == "WIN" && gross_profit > Decimal::ZERO {
        let settings = db_platform_settings_tx(tx).await?;
        db_apply_referral_rewards_tx(tx, &settlement, gross_profit, &settings).await?;
    }
    Ok(())
}

async fn db_apply_referral_rewards_tx(
    tx: &mut Transaction<'_, Postgres>,
    settlement: &SettlementRecord,
    base_profit: Decimal,
    settings: &PlatformTradingSettings,
) -> Result<(), AppError> {
    let link = sqlx::query(
        r#"
        select parent_user_id, grandparent_user_id
        from referral_links
        where user_id = $1
        "#,
    )
    .bind(settlement.user_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let Some(link) = link else {
        return Ok(());
    };
    let direct: Option<Uuid> = link.get("parent_user_id");
    let indirect: Option<Uuid> = link.get("grandparent_user_id");
    let recipients = [
        (direct, 1, settings.direct_referral_rate),
        (indirect, 2, settings.indirect_referral_rate),
    ];
    for (recipient_user_id, level, rate) in recipients {
        let Some(recipient_user_id) = recipient_user_id else {
            continue;
        };
        if recipient_user_id == settlement.user_id {
            continue;
        }
        let amount = base_profit * rate;
        if amount <= Decimal::ZERO {
            continue;
        }
        let reward_id = Uuid::now_v7();
        sqlx::query(
            r#"
            insert into referral_rewards
              (id, settlement_id, source_user_id, recipient_user_id, level, rate,
               base_profit, amount, status, created_at)
            values ($1, $2, $3, $4, $5, $6, $7, $8, 'credited', $9)
            on conflict (settlement_id, recipient_user_id, level) do nothing
            "#,
        )
        .bind(reward_id)
        .bind(settlement.id)
        .bind(settlement.user_id)
        .bind(recipient_user_id)
        .bind(level)
        .bind(rate)
        .bind(base_profit)
        .bind(amount)
        .bind(settlement.created_at)
        .execute(&mut **tx)
        .await
        .map_err(AppError::from_sqlx)?;

        let balance_row = sqlx::query(
            r#"
            select available, frozen
            from balances
            where user_id = $1
            for update
            "#,
        )
        .bind(recipient_user_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(AppError::from_sqlx)?;
        let after_available: Decimal = balance_row.get::<Decimal, _>("available") + amount;
        let frozen: Decimal = balance_row.get("frozen");
        sqlx::query(
            r#"
            update balances
            set available = $2, frozen = $3, updated_at = $4
            where user_id = $1
            "#,
        )
        .bind(recipient_user_id)
        .bind(after_available)
        .bind(frozen)
        .bind(settlement.created_at)
        .execute(&mut **tx)
        .await
        .map_err(AppError::from_sqlx)?;
        insert_ledger_tx(
            tx,
            LedgerEntry {
                id: Uuid::now_v7(),
                user_id: recipient_user_id,
                asset: "USDT".to_string(),
                entry_type: if level == 1 {
                    "direct_referral_reward".to_string()
                } else {
                    "indirect_referral_reward".to_string()
                },
                amount,
                balance_available_after: after_available,
                balance_frozen_after: frozen,
                ref_type: "referral_reward".to_string(),
                ref_id: reward_id,
                idempotency_key: format!(
                    "referral:{}:{}:{}",
                    settlement.id, recipient_user_id, level
                ),
                created_at: settlement.created_at,
            },
        )
        .await?;
    }
    Ok(())
}

async fn db_admin_overview(
    pool: &PgPool,
    started_at: DateTime<Utc>,
    prediction_runtime: PredictionRuntimeStatus,
) -> Result<serde_json::Value, AppError> {
    let stats = db_build_stats(pool, started_at, prediction_runtime).await?;
    let users_total = db_count(pool, "users").await?;
    let ready_pages = db_ready_pages(pool).await?;
    let overview_query = ListQuery {
        page: Some(1),
        limit: Some(20),
        search: None,
        status: None,
    };
    let orders = db_admin_orders(pool, &overview_query).await?;
    let reports = db_admin_reports(pool, &overview_query).await?;
    let settlements = db_admin_settlements(pool, &overview_query).await?;
    Ok(json!({
        "stats": stats,
        "users_total": users_total,
        "ready_pages": ready_pages,
        "orders": orders.into_iter().take(20).collect::<Vec<_>>(),
        "reports": reports.into_iter().take(20).collect::<Vec<_>>(),
        "settlements": settlements.into_iter().take(20).collect::<Vec<_>>()
    }))
}

async fn db_build_stats(
    pool: &PgPool,
    started_at: DateTime<Utc>,
    prediction_runtime: PredictionRuntimeStatus,
) -> Result<StatsResponse, AppError> {
    let outcome_row = sqlx::query(
        r#"
        select
          count(*) filter (where upper(result) = 'WIN') as wins,
          count(*) filter (where upper(result) = 'LOSS') as losses
        from signal_outcomes
        where model_version = $1
        "#,
    )
    .bind(PREDICTION_MODEL_VERSION)
    .fetch_one(pool)
    .await
    .map_err(AppError::from_sqlx)?;
    let wins = outcome_row.get::<i64, _>("wins") as usize;
    let losses = outcome_row.get::<i64, _>("losses") as usize;
    let settled = wins + losses;
    let hit_rate = if settled == 0 {
        "0".to_string()
    } else {
        format!("{:.4}", wins as f64 / settled as f64)
    };
    let latest_signal = db_latest_signal(pool)
        .await?
        .map(|signal| SignalApiResponse {
            trade_authorized: false,
            account_match_status: "not_checked".to_string(),
            ..signal_api_response(&signal)
        });
    Ok(StatsResponse {
        users_total: db_count(pool, "users").await? as usize,
        orders_total: db_count(pool, "event_contract_orders").await? as usize,
        reports_total: db_count(pool, "execution_reports").await? as usize,
        settlements_total: db_count(pool, "settlements").await? as usize,
        signals_total: db_count(pool, "signals").await? as usize,
        wins,
        losses,
        hit_rate,
        uptime_seconds: (Utc::now() - started_at).num_seconds(),
        latest_signal,
        prediction: prediction_status(&prediction_runtime),
    })
}

async fn db_app_runtime_summary(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<AppRuntimeSummary, AppError> {
    let orders_total =
        sqlx::query("select count(*) as count from event_contract_orders where user_id = $1")
            .bind(user_id)
            .fetch_one(pool)
            .await
            .map(|row| row.get::<i64, _>("count") as usize)
            .map_err(AppError::from_sqlx)?;
    let report_row = sqlx::query(
        r#"
        select
          count(*) filter (where upper(result) = 'WIN') as wins,
          count(*) filter (where upper(result) = 'LOSS') as losses
        from execution_reports
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::from_sqlx)?;
    let wins = report_row.get::<i64, _>("wins") as usize;
    let losses = report_row.get::<i64, _>("losses") as usize;
    let pnl = sqlx::query(
        r#"
        select coalesce(sum(gross_profit), 0) as pnl
        from settlements
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map(|row| row.get::<Decimal, _>("pnl"))
    .map_err(AppError::from_sqlx)?;
    Ok(AppRuntimeSummary {
        orders_total,
        wins,
        losses,
        win_rate: hit_rate_string(wins, losses),
        pnl_usdt: money(pnl),
        pending_settlements: orders_total.saturating_sub(wins + losses),
    })
}

fn hit_rate_string(wins: usize, losses: usize) -> String {
    let settled = wins + losses;
    if settled == 0 {
        "0".to_string()
    } else {
        format!("{:.4}", wins as f64 / settled as f64)
    }
}

async fn db_count(pool: &PgPool, table: &str) -> Result<i64, AppError> {
    let sql = match table {
        "users" => "select count(*) as count from users",
        "event_contract_orders" => "select count(*) as count from event_contract_orders",
        "execution_reports" => "select count(*) as count from execution_reports",
        "settlements" => "select count(*) as count from settlements",
        "signals" => "select count(*) as count from signals",
        _ => return Ok(0),
    };
    sqlx::query(sql)
        .fetch_one(pool)
        .await
        .map(|row| row.get("count"))
        .map_err(AppError::from_sqlx)
}

async fn db_ready_pages(pool: &PgPool) -> Result<i64, AppError> {
    sqlx::query(
        r#"
        select count(*) as count
        from automation_page_states
        where page_ready = true
          and screen_name = 'event_contract'
          and amount_field_found = true
          and up_button_found = true
          and down_button_found = true
          and observed_at >= now() - ($1 || ' seconds')::interval
        "#,
    )
    .bind(PAGE_GATE_TTL_SECONDS.to_string())
    .fetch_one(pool)
    .await
    .map(|row| row.get("count"))
    .map_err(AppError::from_sqlx)
}

async fn db_admin_users(
    pool: &PgPool,
    query: &ListQuery,
) -> Result<Vec<AdminUserRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select u.id, u.wallet_address, u.status, u.active_device_fingerprint,
               coalesce(b.available, 0) as balance_available,
               coalesce(b.frozen, 0) as balance_frozen,
               b.updated_at as balance_updated_at,
               u.created_at, u.updated_at
        from users u
        left join balances b on b.user_id = u.id and b.asset = 'USDT'
        where ($1::text is null or u.status = $1)
          and (
            $2::text is null
            or u.wallet_address ilike $2 escape '\'
            or u.id::text ilike $2 escape '\'
            or u.active_device_fingerprint ilike $2 escape '\'
            or u.status ilike $2 escape '\'
          )
        order by u.created_at desc
        limit $3 offset $4
        "#,
    )
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|row| AdminUserRecord {
                id: row.get("id"),
                wallet_address: row.get("wallet_address"),
                status: row.get("status"),
                active_device_fingerprint: row.get("active_device_fingerprint"),
                balance_available: money(row.get("balance_available")),
                balance_frozen: money(row.get("balance_frozen")),
                balance_updated_at: row.get("balance_updated_at"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect()
    })
    .map_err(AppError::from_sqlx)
}

async fn db_admin_orders(
    pool: &PgPool,
    query: &ListQuery,
) -> Result<Vec<EventContractOrderRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select o.id, o.user_id, o.reservation_id, o.signal_ref, o.client_order_id, o.source,
               o.symbol, o.period_seconds, o.direction, o.stake_usdt,
               o.entry_price as entry_price,
               o.automation_profile, o.automation_confidence, o.action_trace_id, o.raw_audit,
               o.order_status, o.reported_at, o.created_at
        from event_contract_orders o
        where ($1::text is null or o.order_status = $1)
          and (
            $2::text is null
            or o.client_order_id ilike $2 escape '\'
            or o.signal_ref ilike $2 escape '\'
            or o.user_id::text ilike $2 escape '\'
            or o.symbol ilike $2 escape '\'
            or o.direction ilike $2 escape '\'
            or o.order_status ilike $2 escape '\'
          )
        order by o.created_at desc
        limit $3 offset $4
        "#,
    )
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(order_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_orders_for_user(
    pool: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<EventContractOrderRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select o.id, o.user_id, o.reservation_id, o.signal_ref, o.client_order_id, o.source,
               o.symbol, o.period_seconds, o.direction, o.stake_usdt,
               o.entry_price as entry_price,
               o.automation_profile, o.automation_confidence, o.action_trace_id, o.raw_audit,
               o.order_status, o.reported_at, o.created_at
        from event_contract_orders o
        where o.user_id = $1
          and ($2::text is null or o.order_status = $2)
          and (
            $3::text is null
            or o.client_order_id ilike $3 escape '\'
            or o.signal_ref ilike $3 escape '\'
            or o.symbol ilike $3 escape '\'
            or o.direction ilike $3 escape '\'
            or o.order_status ilike $3 escape '\'
          )
        order by o.created_at desc
        limit $4 offset $5
        "#,
    )
    .bind(user_id)
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(order_from_row).collect())
    .map_err(AppError::from_sqlx)
}

fn order_from_row(row: sqlx::postgres::PgRow) -> EventContractOrderRecord {
    EventContractOrderRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        reservation_id: row.get("reservation_id"),
        signal_id: row.get("signal_ref"),
        client_order_id: row.get("client_order_id"),
        source: row.get("source"),
        symbol: row.get("symbol"),
        period_seconds: row.get("period_seconds"),
        direction: row.get("direction"),
        stake_usdt: row.get("stake_usdt"),
        entry_price: row.try_get("entry_price").unwrap_or(None),
        automation_profile: row.get("automation_profile"),
        automation_confidence: row.get("automation_confidence"),
        action_trace_id: row.get("action_trace_id"),
        raw_audit: row.get("raw_audit"),
        order_status: row.get("order_status"),
        reported_at: row.get("reported_at"),
        created_at: row.get("created_at"),
    }
}

async fn db_admin_reports(
    pool: &PgPool,
    query: &ListQuery,
) -> Result<Vec<ExecutionReportRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select id, user_id, signal_id::text as signal_id, client_report_id, source, symbol,
               period_seconds, direction, stake_usdt, entry_price, expiry_price,
               payout_ratio, gross_profit, result, automation_profile,
               automation_confidence, action_trace_id, report_status, reported_at, created_at
        from execution_reports
        where ($1::text is null or report_status = $1)
          and (
            $2::text is null
            or client_report_id ilike $2 escape '\'
            or signal_id::text ilike $2 escape '\'
            or user_id::text ilike $2 escape '\'
            or symbol ilike $2 escape '\'
            or result ilike $2 escape '\'
            or report_status ilike $2 escape '\'
          )
        order by created_at desc
        limit $3 offset $4
        "#,
    )
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(execution_report_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_reports_for_user(
    pool: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<ExecutionReportRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select id, user_id, signal_id::text as signal_id, client_report_id, source, symbol,
               period_seconds, direction, stake_usdt, entry_price, expiry_price,
               payout_ratio, gross_profit, result, automation_profile,
               automation_confidence, action_trace_id, report_status, reported_at, created_at
        from execution_reports
        where user_id = $1
          and ($2::text is null or report_status = $2)
          and (
            $3::text is null
            or client_report_id ilike $3 escape '\'
            or signal_id::text ilike $3 escape '\'
            or symbol ilike $3 escape '\'
            or result ilike $3 escape '\'
            or report_status ilike $3 escape '\'
          )
        order by created_at desc
        limit $4 offset $5
        "#,
    )
    .bind(user_id)
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(execution_report_from_row).collect())
    .map_err(AppError::from_sqlx)
}

fn execution_report_from_row(row: sqlx::postgres::PgRow) -> ExecutionReportRecord {
    ExecutionReportRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        signal_id: row.get("signal_id"),
        client_report_id: row.get("client_report_id"),
        source: row.get("source"),
        symbol: row.get("symbol"),
        period_seconds: row.get("period_seconds"),
        direction: row.get("direction"),
        stake_usdt: row.get("stake_usdt"),
        entry_price: row.get("entry_price"),
        expiry_price: row.get("expiry_price"),
        payout_ratio: row.get("payout_ratio"),
        gross_profit: row.get("gross_profit"),
        result: row.get("result"),
        automation_profile: row.get("automation_profile"),
        automation_confidence: row.get("automation_confidence"),
        action_trace_id: row.get("action_trace_id"),
        report_status: row.get("report_status"),
        reported_at: row.get("reported_at"),
        created_at: row.get("created_at"),
    }
}

async fn db_admin_settlements(
    pool: &PgPool,
    query: &ListQuery,
) -> Result<Vec<SettlementRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select st.id, st.execution_report_id, st.user_id,
               er.signal_id::text as signal_id, er.symbol, er.direction, er.stake_usdt,
               er.entry_price, er.expiry_price, er.result,
               st.gross_profit, st.profit_share_fee, st.loss_rebate,
               st.user_net_profit, st.status, st.created_at
        from settlements st
        left join execution_reports er on er.id = st.execution_report_id
        where ($1::text is null or st.status = $1)
          and (
            $2::text is null
            or st.id::text ilike $2 escape '\'
            or st.execution_report_id::text ilike $2 escape '\'
            or st.user_id::text ilike $2 escape '\'
            or er.signal_id::text ilike $2 escape '\'
            or er.symbol ilike $2 escape '\'
            or er.direction ilike $2 escape '\'
            or er.result ilike $2 escape '\'
            or st.status ilike $2 escape '\'
          )
        order by st.created_at desc
        limit $3 offset $4
        "#,
    )
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(settlement_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_settlements_for_user(
    pool: &PgPool,
    user_id: Uuid,
    query: &ListQuery,
) -> Result<Vec<SettlementRecord>, AppError> {
    let (_, limit, offset) = page_params(query);
    let search = search_pattern(query);
    let status = status_filter(query);
    sqlx::query(
        r#"
        select st.id, st.execution_report_id, st.user_id,
               er.signal_id::text as signal_id, er.symbol, er.direction, er.stake_usdt,
               er.entry_price, er.expiry_price, er.result,
               st.gross_profit, st.profit_share_fee, st.loss_rebate,
               st.user_net_profit, st.status, st.created_at
        from settlements st
        left join execution_reports er on er.id = st.execution_report_id
        where st.user_id = $1
          and ($2::text is null or st.status = $2)
          and (
            $3::text is null
            or st.id::text ilike $3 escape '\'
            or st.execution_report_id::text ilike $3 escape '\'
            or er.signal_id::text ilike $3 escape '\'
            or er.symbol ilike $3 escape '\'
            or er.direction ilike $3 escape '\'
            or er.result ilike $3 escape '\'
            or st.status ilike $3 escape '\'
          )
        order by st.created_at desc
        limit $4 offset $5
        "#,
    )
    .bind(user_id)
    .bind(status)
    .bind(search)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(settlement_from_row).collect())
    .map_err(AppError::from_sqlx)
}

fn settlement_from_row(row: sqlx::postgres::PgRow) -> SettlementRecord {
    SettlementRecord {
        id: row.get("id"),
        execution_report_id: row.get("execution_report_id"),
        user_id: row.get("user_id"),
        signal_id: row.try_get("signal_id").ok(),
        symbol: row.try_get("symbol").ok(),
        direction: row.try_get("direction").ok(),
        stake_usdt: row.try_get("stake_usdt").ok(),
        entry_price: row.try_get("entry_price").ok(),
        expiry_price: row.try_get("expiry_price").ok(),
        result: row.try_get("result").ok(),
        gross_profit: row.get("gross_profit"),
        profit_share_fee: row.get("profit_share_fee"),
        loss_rebate: row.get("loss_rebate"),
        user_net_profit: row.get("user_net_profit"),
        status: row.get("status"),
        created_at: row.get("created_at"),
    }
}

async fn db_admin_page_states(
    pool: &PgPool,
) -> Result<HashMap<Uuid, AutomationPageState>, AppError> {
    sqlx::query(
        r#"
        select user_id, page_ready, screen_name, confidence, package_name,
               amount_field_found, up_button_found, down_button_found,
               observed_balance_usdt, account_match_status, account_balance_delta_usdt,
               observed_at, updated_at
        from automation_page_states
        order by updated_at desc
        limit 500
        "#,
    )
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|row| {
                (
                    row.get("user_id"),
                    AutomationPageState {
                        page_ready: row.get("page_ready"),
                        screen_name: row.get("screen_name"),
                        confidence: row.get("confidence"),
                        package_name: row.get("package_name"),
                        amount_field_found: row.get("amount_field_found"),
                        up_button_found: row.get("up_button_found"),
                        down_button_found: row.get("down_button_found"),
                        observed_balance_usdt: row.get("observed_balance_usdt"),
                        account_match_status: row.get("account_match_status"),
                        account_balance_delta_usdt: row.get("account_balance_delta_usdt"),
                        observed_at: row.get("observed_at"),
                        updated_at: row.get("updated_at"),
                    },
                )
            })
            .collect()
    })
    .map_err(AppError::from_sqlx)
}

async fn build_stats(state: &SharedState) -> StatsResponse {
    let reports = state.reports.read().await;
    let wins = reports
        .iter()
        .filter(|report| report.result.eq_ignore_ascii_case("WIN"))
        .count();
    let losses = reports
        .iter()
        .filter(|report| report.result.eq_ignore_ascii_case("LOSS"))
        .count();
    let settled = wins + losses;
    let hit_rate = if settled == 0 {
        "0".to_string()
    } else {
        let value = wins as f64 / settled as f64;
        format!("{value:.4}")
    };
    let latest_signal =
        state
            .signals
            .read()
            .await
            .last()
            .cloned()
            .map(|signal| SignalApiResponse {
                trade_authorized: false,
                account_match_status: "not_checked".to_string(),
                ..signal_api_response(&signal)
            });
    StatsResponse {
        users_total: state.users.read().await.len(),
        orders_total: state.orders.read().await.len(),
        reports_total: reports.len(),
        settlements_total: state.settlements.read().await.len(),
        signals_total: state.signals.read().await.len(),
        wins,
        losses,
        hit_rate,
        uptime_seconds: (Utc::now() - state.started_at).num_seconds(),
        latest_signal,
        prediction: prediction_status(&*state.prediction_runtime.read().await),
    }
}

fn prediction_status(runtime: &PredictionRuntimeStatus) -> PredictionStatus {
    PredictionStatus {
        enabled: runtime.enabled,
        mode: "strategy_loop".to_string(),
        status: runtime.status.clone(),
        last_evaluated_at: runtime.last_evaluated_at,
        last_signal_at: runtime.last_signal_at,
        last_signal_age_seconds: runtime
            .last_signal_at
            .map(|created_at| (Utc::now() - created_at).num_seconds()),
        last_direction: runtime.last_direction.clone(),
        last_score: runtime.last_score.map(|value| format!("{value:.4}")),
        last_confidence: runtime.last_confidence.map(|value| format!("{value:.4}")),
        market_source: runtime.market_source.clone(),
        market_age_seconds: runtime.market_age_seconds,
        market_candles: runtime.market_candles,
        message: runtime.message.clone(),
        last_error: runtime.last_error.clone(),
    }
}

fn account_from_auth(req: &AuthRequest) -> Result<String, AppError> {
    normalize_wallet_address(&req.wallet_address)
}

fn client_type_from_auth(req: &AuthRequest) -> Result<ClientType, AppError> {
    match req
        .client_type
        .as_deref()
        .map(str::trim)
        .unwrap_or("android")
    {
        "android" => Ok(ClientType::Android),
        "dapp" => Ok(ClientType::Dapp),
        _ => Err(AppError::bad_request(
            "CLIENT_TYPE_INVALID",
            "client_type must be android or dapp",
        )),
    }
}

fn client_type_from_str(value: &str) -> Option<ClientType> {
    match value.trim() {
        "android" => Some(ClientType::Android),
        "dapp" => Some(ClientType::Dapp),
        _ => None,
    }
}

fn active_device_for_client(client_type: ClientType, device: &str) -> Option<String> {
    if client_type == ClientType::Android {
        Some(device.to_string())
    } else {
        None
    }
}

fn normalize_account(account: &str) -> String {
    account.trim().to_ascii_lowercase()
}

fn normalize_referral_code(code: &str) -> Result<String, AppError> {
    let normalized = code.trim().to_ascii_uppercase();
    if normalized.is_empty() {
        return Ok(String::new());
    }
    if normalized.len() < 6 || normalized.len() > 24 {
        return Err(AppError::bad_request(
            "REFERRAL_CODE_INVALID",
            "referral code length must be between 6 and 24",
        ));
    }
    if !normalized
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit())
    {
        return Err(AppError::bad_request(
            "REFERRAL_CODE_INVALID",
            "referral code must contain only letters and digits",
        ));
    }
    Ok(normalized)
}

fn referral_code_for_user(user_id: Uuid) -> String {
    format!("ES{}", &user_id.simple().to_string()[..10]).to_ascii_uppercase()
}

fn normalize_chain(chain: Option<&str>) -> String {
    chain
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("bsc")
        .to_ascii_lowercase()
}

fn normalize_wallet_address(address: &str) -> Result<String, AppError> {
    let normalized = normalize_account(address);
    if normalized.len() != 42 || !normalized.starts_with("0x") {
        return Err(AppError::bad_request(
            "AUTH_WALLET_INVALID",
            "wallet address must be a 0x-prefixed EVM address",
        ));
    }
    if !normalized[2..].chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::bad_request(
            "AUTH_WALLET_INVALID",
            "wallet address contains invalid characters",
        ));
    }
    Ok(normalized)
}

fn normalize_tx_hash(tx_hash: &str) -> Result<String, AppError> {
    let normalized = normalize_account(tx_hash);
    if normalized.len() != 66 || !normalized.starts_with("0x") {
        return Err(AppError::bad_request(
            "TX_HASH_INVALID",
            "tx_hash must be a 0x-prefixed 32-byte hash",
        ));
    }
    if !normalized[2..].chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::bad_request(
            "TX_HASH_INVALID",
            "tx_hash contains invalid characters",
        ));
    }
    Ok(normalized)
}

fn bsc_usdt_contract() -> String {
    env::var("BSC_USDT_CONTRACT").unwrap_or_else(|_| DEFAULT_BSC_USDT_CONTRACT.to_string())
}

fn bsc_rpc_url() -> Result<String, AppError> {
    env::var("BSC_RPC_URL").map_err(|_| {
        AppError::bad_request(
            "BSC_RPC_URL_MISSING",
            "BSC_RPC_URL must be set before BSC transaction verification",
        )
    })
}

fn bsc_vault_contract() -> Result<String, AppError> {
    normalize_wallet_address(&env::var("BSC_VAULT_CONTRACT").map_err(|_| {
        AppError::bad_request(
            "BSC_VAULT_CONTRACT_MISSING",
            "BSC_VAULT_CONTRACT must be set before contract deposit or refund verification",
        )
    })?)
}

fn bsc_settlement_wallet() -> Result<String, AppError> {
    normalize_wallet_address(&env::var("BSC_SETTLEMENT_WALLET").map_err(|_| {
        AppError::bad_request(
            "BSC_SETTLEMENT_WALLET_MISSING",
            "BSC_SETTLEMENT_WALLET must be set",
        )
    })?)
}

fn deposit_min_confirmations() -> i32 {
    env::var("BSC_MIN_CONFIRMATIONS")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .filter(|value| *value >= 0)
        .unwrap_or(15)
}

fn hex_to_i64(value: &str) -> Result<i64, AppError> {
    i64::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|_| AppError::bad_request("HEX_INVALID", "invalid hex number"))
}

fn topic_to_address(topic: &str) -> Result<String, AppError> {
    let topic = topic.trim_start_matches("0x");
    if topic.len() != 64 || !topic.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::bad_request(
            "DEPOSIT_LOG_INVALID",
            "invalid transfer topic",
        ));
    }
    normalize_wallet_address(&format!("0x{}", &topic[24..]))
}

fn normalize_topic(topic: &str) -> Result<String, AppError> {
    let normalized = normalize_account(topic);
    if normalized.len() != 66 || !normalized.starts_with("0x") {
        return Err(AppError::bad_request(
            "EVENT_TOPIC_INVALID",
            "event topic must be a 0x-prefixed 32-byte value",
        ));
    }
    if !normalized[2..].chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::bad_request(
            "EVENT_TOPIC_INVALID",
            "event topic contains invalid characters",
        ));
    }
    Ok(normalized)
}

fn abi_data_words(data: &str) -> Result<Vec<String>, AppError> {
    let data = data.trim_start_matches("0x");
    if !data.len().is_multiple_of(64) || !data.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::bad_request(
            "EVENT_DATA_INVALID",
            "event data is not valid ABI encoded data",
        ));
    }
    Ok(data
        .as_bytes()
        .chunks(64)
        .map(|chunk| String::from_utf8_lossy(chunk).to_string())
        .collect())
}

fn word_to_address(word: &str) -> Result<String, AppError> {
    if word.len() != 64 || !word.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err(AppError::bad_request(
            "EVENT_DATA_INVALID",
            "address word is invalid",
        ));
    }
    normalize_wallet_address(&format!("0x{}", &word[24..]))
}

fn account_id_topic(user_id: Uuid) -> String {
    format!("0x{:0>64}", hex::encode(user_id.as_bytes()))
}

fn new_bytes32_id() -> String {
    format!("0x{:0>64}", hex::encode(Uuid::now_v7().as_bytes()))
}

fn bytes32_from_uuid(id: Uuid) -> String {
    format!("0x{:0>64}", hex::encode(id.as_bytes()))
}

fn u256_to_decimal(value: U256, decimals: u32) -> Result<Decimal, AppError> {
    let text = value.to_string();
    let decimals = decimals as usize;
    let decimal_text = if text.len() <= decimals {
        format!("0.{:0>width$}", text, width = decimals)
    } else {
        let split = text.len() - decimals;
        format!("{}.{}", &text[..split], &text[split..])
    };
    Decimal::from_str(decimal_text.trim_end_matches('0').trim_end_matches('.')).map_err(|_| {
        AppError::bad_request("DEPOSIT_AMOUNT_INVALID", "transfer amount is too large")
    })
}

fn recover_wallet_address(message: &str, signature: &str) -> Result<String, AppError> {
    let signature = Signature::from_str(signature).map_err(|_| {
        AppError::bad_request("WALLET_SIGNATURE_INVALID", "invalid wallet signature")
    })?;
    let recovered: Address = signature.recover(message).map_err(|_| {
        AppError::bad_request("WALLET_SIGNATURE_INVALID", "invalid wallet signature")
    })?;
    Ok(format!("{recovered:?}").to_ascii_lowercase())
}

fn password_hash(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AppError::internal("PASSWORD_HASH_FAILED", err.to_string()))
}

fn verify_password(stored_hash: &str, password: &str) -> bool {
    if let Ok(parsed) = PasswordHash::new(stored_hash) {
        return Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
    }
    false
}

fn new_token(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::now_v7().simple())
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal, AppError> {
    Decimal::from_str(value.trim()).map_err(|_| {
        AppError::bad_request(
            "DECIMAL_INVALID",
            format!("{field} must be a decimal string"),
        )
    })
}

fn parse_optional_decimal(value: Option<&str>, field: &str) -> Result<Option<Decimal>, AppError> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(|value| parse_decimal(value, field))
        .transpose()
}

fn decimal(value: Decimal) -> String {
    value.normalize().to_string()
}

fn money(value: Decimal) -> String {
    value.round_dp(8).normalize().to_string()
}

fn decimal_from_f64(value: f64, scale: u32) -> Decimal {
    let scale = scale as usize;
    Decimal::from_str(&format!("{value:.scale$}")).unwrap_or(Decimal::ZERO)
}

fn decimal_to_f64(value: Decimal) -> f64 {
    value.to_string().parse::<f64>().unwrap_or(0.0)
}

fn reservation_release_entry_type(reason: &str) -> &'static str {
    match reason {
        "send_failed" => "signal_fee_reserve_release_send_failed",
        "stale" => "signal_fee_reserve_release_stale",
        "missing_suggested_stake" => "signal_fee_reserve_release_missing_stake",
        "unsupported_direction" => "signal_fee_reserve_release_bad_direction",
        "missing_root_window" => "signal_fee_reserve_release_no_window",
        "amount_field_not_found" => "signal_fee_reserve_release_no_amount_field",
        "amount_input_failed" => "signal_fee_reserve_release_input_failed",
        "action_button_not_found" => "signal_fee_reserve_release_no_action_button",
        "action_click_failed" => "signal_fee_reserve_release_action_failed",
        "confirm_button_not_found" => "signal_fee_reserve_release_no_confirm",
        _ => "signal_fee_reserve_release_client_failed",
    }
}

fn is_unique_violation(err: &AppError) -> bool {
    err.code == "DATABASE_CONFLICT"
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl AppError {
    fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code,
            message: message.into(),
        }
    }

    fn unauthorized(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code,
            message: message.into(),
        }
    }

    fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code,
            message: message.into(),
        }
    }

    fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code,
            message: message.into(),
        }
    }

    fn too_many_requests(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code,
            message: message.into(),
        }
    }

    fn internal(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code,
            message: message.into(),
        }
    }

    fn from_sqlx(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db_err) = &err {
            if db_err.is_unique_violation() {
                return Self::conflict("DATABASE_CONFLICT", db_err.message());
            }
        }
        Self::internal("DATABASE_ERROR", err.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "code": self.code,
                "message": self.message,
            })),
        )
            .into_response()
    }
}
