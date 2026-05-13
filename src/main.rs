use std::{collections::HashMap, env, net::SocketAddr, str::FromStr, sync::Arc};

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
use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use ethers_core::types::{Address, Signature, U256};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, Row, Transaction};
use tokio::sync::{broadcast, RwLock};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use uuid::Uuid;

const DEFAULT_SYMBOL: &str = "BTCUSDT";
const DEFAULT_PERIOD_SECONDS: i32 = 600;
const DEFAULT_PAYOUT_RATIO: Decimal = dec!(0.80);
const PROFIT_SHARE_RATE: Decimal = dec!(0.30);
const DEFAULT_STAKE_PERCENT: Decimal = dec!(0.05);
const MIN_EVENT_STAKE: Decimal = dec!(5);
const MAX_EVENT_STAKE: Decimal = dec!(250);
const PAGE_GATE_TTL_SECONDS: i64 = 3;
const MIN_DEPOSIT_AMOUNT: Decimal = dec!(10);
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

    let db = match &cli.database_url {
        Some(database_url) => {
            let pool = PgPoolOptions::new()
                .max_connections(10)
                .connect(database_url)
                .await?;
            sqlx::migrate!("./migrations").run(&pool).await?;
            println!("PostgreSQL connected and migrations applied");
            Some(pool)
        }
        None => None,
    };

    let state = AppState::new(db);
    bootstrap_memory_admin(&state).await;
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
        .route("/v1/wallets/bind-message", post(wallet_bind_message))
        .route("/v1/wallets/verify", post(wallet_verify))
        .route("/v1/stats", get(stats))
        .route("/v1/signals/latest", get(latest_signal))
        .route("/v1/signals/history", get(signal_history))
        .route("/v1/user-settings/trading", get(get_trading_settings))
        .route("/v1/user-settings/trading", put(save_trading_settings))
        .route("/v1/user-settings/automation", get(get_automation_settings))
        .route(
            "/v1/user-settings/automation",
            put(save_automation_settings),
        )
        .route("/v1/automation/page-state", post(save_page_state))
        .route("/v1/balance", get(balance))
        .route("/v1/ledger", get(ledger))
        .route("/v1/deposits/instructions", get(deposit_instructions))
        .route("/v1/deposits/submit-tx", post(submit_deposit_tx))
        .route("/v1/deposits", get(deposits))
        .route("/v1/withdrawals", get(withdrawals).post(create_withdrawal))
        .route("/v1/event-contract-orders", post(submit_order))
        .route("/v1/execution-reports", post(submit_execution_report))
        .route("/v1/settlements", get(settlements))
        .route("/v1/ws", get(ws_handler))
        .route("/v1/admin/login", post(admin_login))
        .route("/v1/admin/overview", get(admin_overview))
        .route("/v1/admin/signals", get(admin_signals))
        .route("/v1/admin/signals/emit", post(admin_emit_signal))
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
        .route("/v1/admin/page-states", get(admin_page_states))
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
    wallet_nonces: RwLock<HashMap<Uuid, WalletBindChallenge>>,
    sessions: RwLock<HashMap<String, SessionRecord>>,
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
            wallet_nonces: RwLock::new(HashMap::new()),
            sessions: RwLock::new(HashMap::new()),
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
    chain: String,
    address: String,
    message: String,
    expires_at: DateTime<Utc>,
}

#[derive(Clone)]
struct SessionRecord {
    user_id: Uuid,
    device_fingerprint: String,
    refresh_token: String,
    client_type: ClientType,
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
            automation_enabled: false,
            execution_mode: "manual".to_string(),
            target_app_profile: Some("binance_event_contract_v1".to_string()),
            require_final_confirmation: true,
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
            model_version: "score-v1".to_string(),
            created_at: now,
        }
    }
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
struct AuthRequest {
    wallet_address: String,
    password: String,
    device_fingerprint: String,
    client_type: Option<String>,
}

#[derive(Serialize)]
struct AuthResponse {
    access_token: String,
    refresh_token: String,
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
struct PageStateRequest {
    page_ready: bool,
    screen_name: String,
    confidence: String,
    package_name: String,
    amount_field_found: bool,
    up_button_found: bool,
    down_button_found: bool,
    observed_at: DateTime<Utc>,
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
    deposit_id: String,
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
    balance_available_after_reserve: String,
    model_version: String,
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
}

async fn health() -> &'static str {
    "OK"
}

async fn register(
    State(state): State<SharedState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    validate_auth_request(&req)?;
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
    let user = UserRecord {
        id: user_id,
        wallet_address: account.clone(),
        password_hash: password_hash(&req.password)?,
        status: "active".to_string(),
        active_device_fingerprint: Some(req.device_fingerprint.clone()),
        created_at: now,
        updated_at: now,
    };

    if let Some(pool) = state.db.as_ref() {
        let mut tx = pool.begin().await.map_err(AppError::from_sqlx)?;
        sqlx::query(
            r#"
            insert into users (id, wallet_address, password_hash, status, active_device_fingerprint, created_at, updated_at)
            values ($1, $2, $3, $4, $5, $6, $7)
            "#,
        )
        .bind(user.id)
        .bind(&user.wallet_address)
        .bind(&user.password_hash)
        .bind(&user.status)
        .bind(&user.active_device_fingerprint)
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
        .bind(false)
        .bind("manual")
        .bind("binance_event_contract_v1")
        .bind(true)
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
        tx.commit().await.map_err(AppError::from_sqlx)?;
    }

    state.users.write().await.insert(user_id, user);
    state.user_by_wallet.write().await.insert(account, user_id);
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
        create_session(
            &state,
            user_id,
            &req.device_fingerprint,
            client_type_from_auth(&req)?,
        )
        .await?,
    ))
}

async fn login(
    State(state): State<SharedState>,
    Json(req): Json<AuthRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    validate_auth_request(&req)?;
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
        if let Some(active) = &user.active_device_fingerprint {
            if active != &req.device_fingerprint {
                return Err(AppError::conflict(
                    "ACCOUNT_ALREADY_LOGGED_IN_ON_ANOTHER_DEVICE",
                    "this account already has an active device session",
                ));
            }
        }
        sqlx::query(
            "update users set active_device_fingerprint = $2, updated_at = $3 where id = $1",
        )
        .bind(user.id)
        .bind(&req.device_fingerprint)
        .bind(Utc::now())
        .execute(pool)
        .await
        .map_err(AppError::from_sqlx)?;
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
        if let Some(active) = &user.active_device_fingerprint {
            if active != &req.device_fingerprint {
                return Err(AppError::conflict(
                    "ACCOUNT_ALREADY_LOGGED_IN_ON_ANOTHER_DEVICE",
                    "this account already has an active device session",
                ));
            }
        }
        user.active_device_fingerprint = Some(req.device_fingerprint.clone());
        user.updated_at = Utc::now();
        user_id
    };

    Ok(Json(
        create_session(
            &state,
            user_id,
            &req.device_fingerprint,
            client_type_from_auth(&req)?,
        )
        .await?,
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
        if let Some(user) = state.users.write().await.get_mut(&session.user_id) {
            if user.active_device_fingerprint.as_deref() == Some(&session.device_fingerprint) {
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
    Ok(Json(
        create_session(
            &state,
            session.user_id,
            &session.device_fingerprint,
            session.client_type,
        )
        .await?,
    ))
}

async fn stats(State(state): State<SharedState>) -> Json<StatsResponse> {
    if let Some(pool) = state.db.as_ref() {
        if let Ok(stats) = db_build_stats(pool, state.started_at).await {
            return Json(stats);
        }
    }
    Json(build_stats(&state).await)
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
        None => state.signals.read().await.clone(),
    };
    let mut out = Vec::with_capacity(signals.len());
    for signal in signals.iter().rev().take(50) {
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
    Json(req): Json<TradingSettings>,
) -> Result<Json<TradingSettings>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    validate_trading_settings(&req)?;
    if let Some(pool) = state.db.as_ref() {
        db_save_trading_settings(pool, user_id, &req).await?;
    }
    state
        .trading_settings
        .write()
        .await
        .insert(user_id, req.clone());
    Ok(Json(req))
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
    let page_state = AutomationPageState {
        page_ready: req.page_ready,
        screen_name: req.screen_name,
        confidence,
        package_name: req.package_name,
        amount_field_found: req.amount_field_found,
        up_button_found: req.up_button_found,
        down_button_found: req.down_button_found,
        observed_at: req.observed_at,
        updated_at: Utc::now(),
    };
    if let Some(pool) = state.db.as_ref() {
        db_save_page_state(pool, user_id, &page_state).await?;
    }
    state.page_states.write().await.insert(user_id, page_state);
    Ok(Json(json!({ "ok": true })))
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
) -> Result<Json<Vec<LedgerEntry>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_ledger_for_user(pool, user_id).await?));
    }
    let rows = state
        .ledger
        .read()
        .await
        .iter()
        .filter(|entry| entry.user_id == user_id)
        .cloned()
        .collect();
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
    Ok(Json(json!({
        "chain": "bsc",
        "asset": "USDT",
        "token_contract": bsc_usdt_contract(),
        "vault_contract": bsc_vault_contract()?,
        "settlement_wallet": bsc_settlement_wallet()?,
        "account_id": account_id_topic(user_id),
        "deposit_id": new_bytes32_id(),
        "bound_wallet": bound_wallet,
        "min_amount": money(MIN_DEPOSIT_AMOUNT)
    })))
}

async fn submit_deposit_tx(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Json(req): Json<DepositTxRequest>,
) -> Result<Json<DepositRecord>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    let expected_amount = req
        .amount
        .as_deref()
        .map(|value| parse_decimal(value, "amount"))
        .transpose()?;
    let tx_hash = normalize_tx_hash(&req.tx_hash)?;
    let deposit_id = normalize_topic(&req.deposit_id)?;
    let bound_wallet = bound_wallet_for_user(&state, user_id).await?;
    let verified = verify_bsc_contract_deposit(
        &tx_hash,
        user_id,
        &bound_wallet,
        &deposit_id,
        expected_amount,
    )
    .await?;
    let now = Utc::now();
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

async fn deposits(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DepositRecord>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_deposits_for_user(pool, user_id).await?));
    }
    let rows = state
        .deposits
        .read()
        .await
        .iter()
        .filter(|deposit| deposit.user_id == user_id)
        .cloned()
        .collect();
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
) -> Result<Json<Vec<WithdrawalRecord>>, AppError> {
    let user_id = require_dapp_user(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_withdrawals_for_user(pool, user_id).await?));
    }
    let rows = state
        .withdrawals
        .read()
        .await
        .iter()
        .filter(|withdrawal| withdrawal.user_id == user_id)
        .cloned()
        .collect();
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
        balance.available += deposit.amount;
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
            amount: deposit.amount,
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
    expected_deposit_id: &str,
    expected_amount: Option<Decimal>,
) -> Result<VerifiedDeposit, AppError> {
    let rpc_url = bsc_rpc_url()?;
    let token_contract = normalize_wallet_address(&bsc_usdt_contract())?;
    let vault_contract = bsc_vault_contract()?;
    let settlement_wallet = bsc_settlement_wallet()?;
    let bound_wallet = normalize_wallet_address(bound_wallet)?;
    let expected_account_id = account_id_topic(user_id);

    let receipt: Option<RpcTransactionReceipt> =
        rpc_call(&rpc_url, "eth_getTransactionReceipt", json!([tx_hash])).await?;
    let Some(receipt) = receipt else {
        return Ok(VerifiedDeposit {
            tx_hash: tx_hash.to_string(),
            token_contract,
            contract_address: vault_contract.clone(),
            deposit_id: expected_deposit_id.to_string(),
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
    if deposit.deposit_id != expected_deposit_id {
        return Err(AppError::bad_request(
            "DEPOSIT_ID_MISMATCH",
            "deposit event deposit_id does not match request",
        ));
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

async fn rpc_call<T: DeserializeOwned>(
    rpc_url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<Option<T>, AppError> {
    let client = reqwest::Client::new();
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
    if let Some(pool) = state.db.as_ref() {
        db_insert_order_with_reservation_validation(pool, &order).await?;
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
    }
    state.orders.write().await.push(order);
    Ok(Json(SubmitOrderResponse {
        order_id,
        status: "submitted".to_string(),
    }))
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

async fn settlements(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SettlementRecord>>, AppError> {
    let user_id = require_android_user(&state, &headers).await?;
    let rows = state
        .settlements
        .read()
        .await
        .iter()
        .filter(|settlement| settlement.user_id == user_id)
        .cloned()
        .collect();
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
    Ok(ws.on_upgrade(move |socket| websocket_loop(socket, state, session.user_id)))
}

async fn websocket_loop(mut socket: WebSocket, state: SharedState, user_id: Uuid) {
    let mut rx = state.signal_tx.subscribe();
    let _ = socket
        .send(Message::Text(
            json!({"type": "connection.ready", "data": {"server_time": Utc::now()}}).to_string(),
        ))
        .await;
    while let Ok(signal) = rx.recv().await {
        let payload = authorize_signal_for_user(&state, user_id, &signal).await;
        let text = json!({
            "type": "signal.created",
            "data": payload,
        })
        .to_string();
        if socket.send(Message::Text(text)).await.is_err() {
            break;
        }
    }
}

async fn admin_overview(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        let overview = db_admin_overview(pool, state.started_at).await?;
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
    let confidence = match req.confidence {
        Some(value) => parse_decimal(&value, "confidence")?,
        None => dec!(1),
    };
    let signal = SignalCore::new(direction, stake, confidence);
    if let Some(pool) = state.db.as_ref() {
        db_insert_signal(pool, &signal).await?;
    }
    state.signals.write().await.push(signal.clone());
    let _ = state.signal_tx.send(signal.clone());
    Ok(Json(signal_api_response(&signal)))
}

async fn admin_signals(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<SignalApiResponse>>, AppError> {
    ensure_admin(&state, &headers).await?;
    let signals = match state.db.as_ref() {
        Some(pool) => db_signal_history(pool, 200).await?,
        None => state.signals.read().await.clone(),
    };
    Ok(Json(
        signals
            .iter()
            .rev()
            .take(200)
            .map(signal_api_response)
            .collect(),
    ))
}

async fn admin_users(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<AdminUserRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_users(pool).await?));
    }
    Ok(Json(
        state
            .users
            .read()
            .await
            .values()
            .cloned()
            .map(AdminUserRecord::from)
            .collect(),
    ))
}

async fn admin_orders(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<EventContractOrderRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_orders(pool).await?));
    }
    Ok(Json(state.orders.read().await.clone()))
}

async fn admin_deposits(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DepositRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_deposits(pool).await?));
    }
    Ok(Json(state.deposits.read().await.clone()))
}

async fn admin_withdrawals(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<WithdrawalRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_withdrawals(pool).await?));
    }
    Ok(Json(state.withdrawals.read().await.clone()))
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
) -> Result<Json<Vec<LedgerEntry>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_ledger(pool).await?));
    }
    Ok(Json(state.ledger.read().await.clone()))
}

async fn admin_reports(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> Result<Json<Vec<ExecutionReportRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_reports(pool).await?));
    }
    Ok(Json(state.reports.read().await.clone()))
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
) -> Result<Json<Vec<SettlementRecord>>, AppError> {
    ensure_admin(&state, &headers).await?;
    if let Some(pool) = state.db.as_ref() {
        return Ok(Json(db_admin_settlements(pool).await?));
    }
    Ok(Json(state.settlements.read().await.clone()))
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
    let expires_in = 3600;
    let expires_at = Utc::now() + Duration::seconds(expires_in);
    if let Some(pool) = state.db.as_ref() {
        sqlx::query(
            r#"
            insert into auth_sessions
              (access_token, user_id, device_fingerprint, refresh_token, client_type, expires_at)
            values ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(&access_token)
        .bind(user_id)
        .bind(device)
        .bind(&refresh_token)
        .bind(client_type.as_str())
        .bind(expires_at)
        .execute(pool)
        .await
        .map_err(AppError::from_sqlx)?;
    }
    state.sessions.write().await.insert(
        access_token.clone(),
        SessionRecord {
            user_id,
            device_fingerprint: device.to_string(),
            refresh_token: refresh_token.clone(),
            client_type,
            expires_at,
        },
    );
    Ok(AuthResponse {
        access_token,
        refresh_token,
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
    require_client(state, headers, &[ClientType::Android]).await
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
    Ok(session)
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
    if req.password.len() < 6 {
        return Err(AppError::bad_request(
            "AUTH_PASSWORD_WEAK",
            "password must be at least 6 characters",
        ));
    }
    if req.device_fingerprint.trim().len() < 8 {
        return Err(AppError::bad_request(
            "AUTH_DEVICE_INVALID",
            "device fingerprint is required",
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

fn validate_trading_settings(settings: &TradingSettings) -> Result<(), AppError> {
    if settings.stake_percent < dec!(0.01) || settings.stake_percent > dec!(0.20) {
        return Err(AppError::bad_request(
            "USER_SETTING_OUT_OF_RANGE",
            "stake_percent must be between 0.01 and 0.20",
        ));
    }
    if settings.min_stake_usdt <= Decimal::ZERO {
        return Err(AppError::bad_request(
            "USER_SETTING_OUT_OF_RANGE",
            "min_stake_usdt must be positive",
        ));
    }
    if let Some(max) = settings.max_single_stake_usdt {
        if !(MIN_EVENT_STAKE..=MAX_EVENT_STAKE).contains(&max) {
            return Err(AppError::bad_request(
                "USER_SETTING_OUT_OF_RANGE",
                "max_single_stake_usdt must be between 5 and 250",
            ));
        }
    }
    if let Some(confidence) = settings.min_confidence {
        if confidence < Decimal::ZERO || confidence > Decimal::ONE {
            return Err(AppError::bad_request(
                "USER_SETTING_OUT_OF_RANGE",
                "min_confidence must be between 0 and 1",
            ));
        }
    }
    Ok(())
}

fn validate_automation_settings(settings: &AutomationSettings) -> Result<(), AppError> {
    if !matches!(
        settings.execution_mode.as_str(),
        "manual" | "assisted" | "automatic"
    ) {
        return Err(AppError::bad_request(
            "AUTOMATION_SETTING_OUT_OF_RANGE",
            "execution_mode must be manual, assisted, or automatic",
        ));
    }
    if settings.execution_mode == "automatic" && !settings.automation_enabled {
        return Err(AppError::bad_request(
            "AUTOMATION_SETTING_OUT_OF_RANGE",
            "automatic mode requires automation_enabled",
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
        if let Ok(Some(settings)) = db_load_trading_settings(pool, user_id).await {
            return settings;
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

async fn display_signal_for_user(
    state: &SharedState,
    user_id: Uuid,
    signal: &SignalCore,
) -> SignalApiResponse {
    let settings = load_trading_settings(state, user_id).await;
    let balance = load_balance(state, user_id).await;
    let stake = calculated_stake(balance.available, &settings, Some(signal.suggested_stake));
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
        suggested_stake_rule: "display-only; no reserve is created by latest/history endpoints"
            .to_string(),
        trade_authorized: false,
        reservation_id: None,
        fee_reserve_usdt: money(Decimal::ZERO),
        authorization_status: "not_reserved".to_string(),
        balance_available_after_reserve: money(balance.available),
        model_version: signal.model_version.clone(),
    }
}

async fn authorize_signal_for_user(
    state: &SharedState,
    user_id: Uuid,
    signal: &SignalCore,
) -> SignalApiResponse {
    let trading = load_trading_settings(state, user_id).await;
    let automation = load_automation_settings(state, user_id).await;
    let balance = load_balance(state, user_id).await;
    let stake = calculated_stake(balance.available, &trading, Some(signal.suggested_stake));
    let expected_profit = stake * DEFAULT_PAYOUT_RATIO;
    let fee_reserve = expected_profit * PROFIT_SHARE_RATE;

    let mut status = "reserved".to_string();
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
    if status == "reserved" {
        match page {
            None => status = "page_state_missing".to_string(),
            Some(page) if !page_is_fresh(&page) => status = "page_state_stale".to_string(),
            Some(page) if !page_is_fresh_and_ready(&page) => status = "page_not_ready".to_string(),
            Some(_) => {}
        }
    }
    if status == "reserved" && balance.available < fee_reserve {
        status = "fee_reserve_insufficient".to_string();
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
            expires_at: signal.cycle_end + Duration::seconds(30),
            created_at: now,
            updated_at: now,
        };

        if let Some(pool) = state.db.as_ref() {
            match db_reserve_signal_fee(pool, &reservation).await {
                Ok(after) => {
                    balance_after = after.available;
                    reservation_id = Some(id);
                }
                Err(err) if err.code == "BALANCE_INSUFFICIENT" => {
                    status = "fee_reserve_insufficient".to_string();
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
        if status == "reserved" && reservation_id.is_some() {
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
        suggested_stake_rule: "clamp(max(balance * user_stake_percent, 5 USDT), 5, 250 USDT); fee reserve = expected profit * 30%".to_string(),
        trade_authorized: status == "reserved",
        reservation_id: reservation_id.map(|id| id.to_string()),
        fee_reserve_usdt: money(fee_reserve),
        authorization_status: status,
        balance_available_after_reserve: money(balance_after),
        model_version: signal.model_version.clone(),
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
        balance_available_after_reserve: money(Decimal::ZERO),
        model_version: signal.model_version.clone(),
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
        suggested_stake_rule: "clamp(max(balance * user_stake_percent, 5 USDT), 5, 250 USDT); fee reserve = expected profit * 30%".to_string(),
        trade_authorized: false,
        reservation_id: None,
        fee_reserve_usdt: money(fee_reserve),
        authorization_status: status.to_string(),
        balance_available_after_reserve: money(balance_after),
        model_version: signal.model_version.clone(),
    }
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
    stake.round_dp(2)
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
        select id, wallet_address, password_hash, status, active_device_fingerprint, created_at, updated_at
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

fn user_from_row(row: sqlx::postgres::PgRow) -> UserRecord {
    UserRecord {
        id: row.get("id"),
        wallet_address: row.get("wallet_address"),
        password_hash: row.get("password_hash"),
        status: row.get("status"),
        active_device_fingerprint: row.get("active_device_fingerprint"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
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
            "select user_id, device_fingerprint, refresh_token, client_type, expires_at from auth_sessions where access_token = $1"
        }
        "refresh_token" => {
            "select user_id, device_fingerprint, refresh_token, client_type, expires_at from auth_sessions where refresh_token = $1"
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
                client_type: client_type_from_str(row.get::<String, _>("client_type").as_str())
                    .unwrap_or(ClientType::Android),
                expires_at: row.get("expires_at"),
            })
        })
        .map_err(AppError::from_sqlx)
}

async fn db_load_trading_settings(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<TradingSettings>, AppError> {
    sqlx::query(
        r#"
        select stake_percent, min_stake_usdt, max_single_stake_usdt, min_confidence,
               signal_alert_enabled, floating_window_enabled
        from user_trading_settings
        where user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map(|row| {
        row.map(|row| TradingSettings {
            stake_percent: row.get("stake_percent"),
            min_stake_usdt: row.get("min_stake_usdt"),
            max_single_stake_usdt: row.get("max_single_stake_usdt"),
            min_confidence: row.get("min_confidence"),
            signal_alert_enabled: row.get("signal_alert_enabled"),
            floating_window_enabled: row.get("floating_window_enabled"),
        })
    })
    .map_err(AppError::from_sqlx)
}

async fn db_save_trading_settings(
    pool: &PgPool,
    user_id: Uuid,
    settings: &TradingSettings,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        insert into user_trading_settings
          (user_id, stake_percent, min_stake_usdt, max_single_stake_usdt, min_confidence,
           signal_alert_enabled, floating_window_enabled, updated_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8)
        on conflict (user_id) do update set
          stake_percent = excluded.stake_percent,
          min_stake_usdt = excluded.min_stake_usdt,
          max_single_stake_usdt = excluded.max_single_stake_usdt,
          min_confidence = excluded.min_confidence,
          signal_alert_enabled = excluded.signal_alert_enabled,
          floating_window_enabled = excluded.floating_window_enabled,
          updated_at = excluded.updated_at
        "#,
    )
    .bind(user_id)
    .bind(settings.stake_percent)
    .bind(settings.min_stake_usdt)
    .bind(settings.max_single_stake_usdt)
    .bind(settings.min_confidence)
    .bind(settings.signal_alert_enabled)
    .bind(settings.floating_window_enabled)
    .bind(Utc::now())
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
               up_button_found, down_button_found, observed_at, updated_at
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
           up_button_found, down_button_found, observed_at, updated_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        on conflict (user_id) do update set
          page_ready = excluded.page_ready,
          screen_name = excluded.screen_name,
          confidence = excluded.confidence,
          package_name = excluded.package_name,
          amount_field_found = excluded.amount_field_found,
          up_button_found = excluded.up_button_found,
          down_button_found = excluded.down_button_found,
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
    .bind(page.observed_at)
    .bind(page.updated_at)
    .execute(pool)
    .await
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_insert_signal(pool: &PgPool, signal: &SignalCore) -> Result<(), AppError> {
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
    .map(|_| ())
    .map_err(AppError::from_sqlx)
}

async fn db_latest_signal(pool: &PgPool) -> Result<Option<SignalCore>, AppError> {
    sqlx::query(
        r#"
        select id, symbol, period_seconds, cycle_start, cycle_end, direction, confidence,
               predicted_return, entry_price, suggested_stake, model_version, created_at
        from signals
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
               predicted_return, entry_price, suggested_stake, model_version, created_at
        from signals
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
    }
}

async fn db_ledger_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<LedgerEntry>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, asset, entry_type, amount, balance_available_after,
               balance_frozen_after, ref_type, ref_id, idempotency_key, created_at
        from ledger_entries
        where user_id = $1
        order by created_at desc
        limit 100
        "#,
    )
    .bind(user_id)
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
           from_address, to_address, amount, block_number, confirmations, status, credited_at,
           created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
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
        if existing_user_id != deposit.user_id || existing_amount != deposit.amount {
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
                block_number = case when $5 > 0 then $5 else block_number end
            where tx_hash = $1
            "#,
        )
        .bind(&deposit.tx_hash)
        .bind(deposit.confirmations)
        .bind(&deposit.status)
        .bind(deposit.credited_at)
        .bind(deposit.block_number)
        .execute(&mut *tx)
        .await
        .map_err(AppError::from_sqlx)?;
        if existing_status != "credited" && deposit.status == "credited" {
            db_apply_deposit_credit_tx(
                &mut tx,
                deposit.user_id,
                existing_id,
                &deposit.tx_hash,
                deposit.amount,
            )
            .await?;
        }
        tx.commit().await.map_err(AppError::from_sqlx)?;
        return Ok(());
    }

    insert_deposit_tx(&mut tx, deposit).await?;
    if deposit.status == "credited" {
        db_apply_deposit_credit_tx(
            &mut tx,
            deposit.user_id,
            deposit.id,
            &deposit.tx_hash,
            deposit.amount,
        )
        .await?;
    }
    tx.commit().await.map_err(AppError::from_sqlx)
}

async fn db_apply_deposit_credit_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    deposit_id: Uuid,
    tx_hash: &str,
    amount: Decimal,
) -> Result<(), AppError> {
    let balance_row = sqlx::query(
        r#"
        select user_id, asset, available, frozen, updated_at
        from balances
        where user_id = $1
        for update
        "#,
    )
    .bind(user_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(AppError::from_sqlx)?;
    let available: Decimal = balance_row.get("available");
    let frozen: Decimal = balance_row.get("frozen");
    let after_available = available + amount;
    let now = Utc::now();
    sqlx::query(
        r#"
        update balances
        set available = $2, frozen = $3, updated_at = $4
        where user_id = $1
        "#,
    )
    .bind(user_id)
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
            user_id,
            asset: "USDT".to_string(),
            entry_type: "deposit_credit".to_string(),
            amount,
            balance_available_after: after_available,
            balance_frozen_after: frozen,
            ref_type: "deposit".to_string(),
            ref_id: deposit_id,
            idempotency_key: format!("deposit:{tx_hash}"),
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

async fn db_deposits_for_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<DepositRecord>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, deposit_id, tx_hash,
               from_address, to_address, amount, block_number, confirmations, status,
               credited_at, created_at
        from deposits
        where user_id = $1
        order by created_at desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(deposit_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_admin_deposits(pool: &PgPool) -> Result<Vec<DepositRecord>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, deposit_id, tx_hash,
               from_address, to_address, amount, block_number, confirmations, status,
               credited_at, created_at
        from deposits
        order by created_at desc
        limit 200
        "#,
    )
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
        block_number: row.get("block_number"),
        confirmations: row.get("confirmations"),
        status: row.get("status"),
        credited_at: row.get("credited_at"),
        created_at: row.get("created_at"),
    }
}

async fn db_admin_ledger(pool: &PgPool) -> Result<Vec<LedgerEntry>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, asset, entry_type, amount, balance_available_after,
               balance_frozen_after, ref_type, ref_id, idempotency_key, created_at
        from ledger_entries
        order by created_at desc
        limit 200
        "#,
    )
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
) -> Result<Vec<WithdrawalRecord>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, tx_hash,
               refund_id, payout_wallet, to_address, amount, block_number, confirmations,
               status, requested_at, processed_at, rejected_at, reject_reason
        from withdrawals
        where user_id = $1
        order by requested_at desc
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map(|rows| rows.into_iter().map(withdrawal_from_row).collect())
    .map_err(AppError::from_sqlx)
}

async fn db_admin_withdrawals(pool: &PgPool) -> Result<Vec<WithdrawalRecord>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, chain, asset, token_contract, contract_address, tx_hash,
               refund_id, payout_wallet, to_address, amount, block_number, confirmations,
               status, requested_at, processed_at, rejected_at, reject_reason
        from withdrawals
        order by requested_at desc
        limit 200
        "#,
    )
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

async fn db_insert_order_with_reservation_validation(
    pool: &PgPool,
    order: &EventContractOrderRecord,
) -> Result<(), AppError> {
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
    sqlx::query(
        r#"
        insert into event_contract_orders
          (id, user_id, reservation_id, signal_ref, client_order_id, source, symbol,
           period_seconds, direction, stake_usdt, payout_ratio, expected_payout_usdt,
           expected_profit_usdt, automation_profile, automation_confidence, action_trace_id,
           raw_audit, order_status, reported_at, created_at)
        values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
        on conflict (user_id, client_order_id) do nothing
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
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    sqlx::query(
        r#"
        update signal_reservations
        set status = 'order_submitted', order_id = $2, updated_at = $3
        where id = $1 and status in ('reserved', 'order_submitted')
        "#,
    )
    .bind(reservation_id)
    .bind(order.id)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await
    .map_err(AppError::from_sqlx)?;
    tx.commit().await.map_err(AppError::from_sqlx)?;
    Ok(())
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
    Ok(())
}

async fn db_admin_overview(
    pool: &PgPool,
    started_at: DateTime<Utc>,
) -> Result<serde_json::Value, AppError> {
    let stats = db_build_stats(pool, started_at).await?;
    let users_total = db_count(pool, "users").await?;
    let ready_pages = db_ready_pages(pool).await?;
    let orders = db_admin_orders(pool).await?;
    let reports = db_admin_reports(pool).await?;
    let settlements = db_admin_settlements(pool).await?;
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
) -> Result<StatsResponse, AppError> {
    let reports = db_admin_reports(pool).await?;
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
        format!("{:.4}", wins as f64 / settled as f64)
    };
    let latest_signal = db_latest_signal(pool)
        .await?
        .map(|signal| SignalApiResponse {
            trade_authorized: false,
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
    })
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

async fn db_admin_users(pool: &PgPool) -> Result<Vec<AdminUserRecord>, AppError> {
    sqlx::query(
        r#"
        select id, wallet_address, password_hash, status, active_device_fingerprint, created_at, updated_at
        from users
        order by created_at desc
        limit 200
        "#,
    )
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(user_from_row)
            .map(AdminUserRecord::from)
            .collect()
    })
    .map_err(AppError::from_sqlx)
}

async fn db_admin_orders(pool: &PgPool) -> Result<Vec<EventContractOrderRecord>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, reservation_id, signal_ref, client_order_id, source, symbol,
               period_seconds, direction, stake_usdt, automation_profile,
               automation_confidence, action_trace_id, raw_audit, order_status,
               reported_at, created_at
        from event_contract_orders
        order by created_at desc
        limit 200
        "#,
    )
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|row| EventContractOrderRecord {
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
                automation_profile: row.get("automation_profile"),
                automation_confidence: row.get("automation_confidence"),
                action_trace_id: row.get("action_trace_id"),
                raw_audit: row.get("raw_audit"),
                order_status: row.get("order_status"),
                reported_at: row.get("reported_at"),
                created_at: row.get("created_at"),
            })
            .collect()
    })
    .map_err(AppError::from_sqlx)
}

async fn db_admin_reports(pool: &PgPool) -> Result<Vec<ExecutionReportRecord>, AppError> {
    sqlx::query(
        r#"
        select id, user_id, signal_id::text as signal_id, client_report_id, source, symbol,
               period_seconds, direction, stake_usdt, entry_price, expiry_price,
               payout_ratio, gross_profit, result, automation_profile,
               automation_confidence, action_trace_id, report_status, reported_at, created_at
        from execution_reports
        order by created_at desc
        limit 200
        "#,
    )
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

async fn db_admin_settlements(pool: &PgPool) -> Result<Vec<SettlementRecord>, AppError> {
    sqlx::query(
        r#"
        select id, execution_report_id, user_id, gross_profit, profit_share_fee,
               loss_rebate, user_net_profit, status, created_at
        from settlements
        order by created_at desc
        limit 200
        "#,
    )
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .map(|row| SettlementRecord {
                id: row.get("id"),
                execution_report_id: row.get("execution_report_id"),
                user_id: row.get("user_id"),
                gross_profit: row.get("gross_profit"),
                profit_share_fee: row.get("profit_share_fee"),
                loss_rebate: row.get("loss_rebate"),
                user_net_profit: row.get("user_net_profit"),
                status: row.get("status"),
                created_at: row.get("created_at"),
            })
            .collect()
    })
    .map_err(AppError::from_sqlx)
}

async fn db_admin_page_states(
    pool: &PgPool,
) -> Result<HashMap<Uuid, AutomationPageState>, AppError> {
    sqlx::query(
        r#"
        select user_id, page_ready, screen_name, confidence, package_name,
               amount_field_found, up_button_found, down_button_found, observed_at, updated_at
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

fn normalize_account(account: &str) -> String {
    account.trim().to_ascii_lowercase()
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
