import { StrictMode, useEffect, useState } from "react";
import type { FormEvent, MouseEvent, ReactNode } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  ArrowDownRight,
  ArrowUpRight,
  CircleDollarSign,
  CreditCard,
  Radio,
  Power,
  RefreshCw,
  Send,
  ShieldCheck,
  SlidersHorizontal,
  Smartphone,
  SquarePen,
  Trash2,
  Users,
  Wallet,
  Zap
} from "lucide-react";
import { BrowserProvider, Contract, parseUnits, type Eip1193Provider } from "ethers";
import { createAppKit, useAppKit, useAppKitAccount, useAppKitProvider } from "@reown/appkit/react";
import { EthersAdapter } from "@reown/appkit-adapter-ethers";
import "./styles.css";

type AdminView =
  | "overview"
  | "settings"
  | "prediction"
  | "signals"
  | "workers"
  | "users"
  | "deposits"
  | "withdrawals"
  | "orders"
  | "settlements";

type AdminOverview = {
  stats: {
    users_total: number;
    orders_total: number;
    reports_total: number;
    settlements_total: number;
    signals_total: number;
    wins: number;
    losses: number;
    hit_rate: string;
    uptime_seconds: number;
    latest_signal?: SignalRecord;
    prediction?: PredictionStatus;
  };
  ready_pages: number;
  orders: OrderRecord[];
  reports: ReportRecord[];
  settlements: SettlementRecord[];
};

type PredictionStatus = {
  enabled: boolean;
  mode: string;
  status: string;
  last_evaluated_at?: string | null;
  last_signal_at?: string | null;
  last_signal_age_seconds?: number | null;
  last_direction?: string | null;
  last_score?: string | null;
  last_confidence?: string | null;
  market_source?: string | null;
  market_age_seconds?: number | null;
  market_candles?: number;
  message: string;
  last_error?: string | null;
};

type PredictionSettings = {
  enabled: boolean;
  interval_seconds: number;
  up_threshold: string;
  down_threshold: string;
  min_confidence: string;
  technical_weight: string;
  probability_weight: string;
  volatility_floor: string;
  calibration_lookback_minutes: number;
  calibration_min_samples: number;
  calibration_min_win_rate: string;
  calibration_min_ev_usdt: string;
  updated_at?: string | null;
};

type PredictionObservation = {
  id: string;
  source: string;
  symbol: string;
  period_seconds: number;
  model_version: string;
  direction: string;
  confidence: string;
  predicted_return: string;
  entry_price: string;
  score?: string | null;
  feature_snapshot: Record<string, unknown>;
  reason?: string | null;
  created_at: string;
};

type SignalRecord = {
  id: string;
  signal_id?: string;
  symbol: string;
  direction: string;
  confidence: string;
  entry_price?: string;
  suggested_stake: string;
  fee_reserve_usdt: string;
  authorization_status: string;
  trade_authorized?: boolean;
  cycle_end: string;
  model_version?: string;
  outcome_result?: string | null;
  outcome_expiry_price?: string | null;
  outcome_settled_at?: string | null;
};

type UserRecord = {
  id: string;
  wallet_address: string;
  status: string;
  active_device_fingerprint?: string | null;
  balance_available: string;
  balance_frozen: string;
  balance_updated_at?: string | null;
  created_at: string;
  updated_at: string;
};

type OrderRecord = {
  id: string;
  user_id: string;
  reservation_id?: string | null;
  signal_id: string;
  client_order_id: string;
  source: string;
  symbol: string;
  direction: string;
  stake_usdt: string;
  entry_price?: string | null;
  automation_confidence?: string | null;
  order_status: string;
  created_at: string;
};

type ReportRecord = {
  id: string;
  user_id: string;
  signal_id: string;
  result: string;
  direction: string;
  stake_usdt: string;
  entry_price?: string | null;
  expiry_price?: string | null;
  gross_profit?: string | null;
  report_status: string;
  created_at: string;
};

type SettlementRecord = {
  id: string;
  user_id: string;
  execution_report_id: string;
  signal_id?: string | null;
  symbol?: string | null;
  direction?: string | null;
  stake_usdt?: string | null;
  entry_price?: string | null;
  expiry_price?: string | null;
  result?: string | null;
  gross_profit: string;
  profit_share_fee: string;
  loss_rebate: string;
  user_net_profit: string;
  status: string;
  created_at: string;
};

type DepositRecord = {
  id: string;
  user_id: string;
  chain: string;
  asset: string;
  token_contract: string;
  contract_address: string;
  deposit_id: string;
  tx_hash: string;
  from_address: string;
  to_address: string;
  amount: string;
  credited_amount: string;
  platform_fee: string;
  block_number: number;
  confirmations: number;
  status: string;
  credited_at?: string | null;
  created_at: string;
};

type WithdrawalRecord = {
  id: string;
  user_id: string;
  chain: string;
  asset: string;
  token_contract: string;
  contract_address: string;
  tx_hash?: string | null;
  refund_id: string;
  payout_wallet?: string | null;
  to_address: string;
  amount: string;
  block_number?: number | null;
  confirmations: number;
  status: string;
  requested_at: string;
  processed_at?: string | null;
  rejected_at?: string | null;
  reject_reason?: string | null;
};

type LedgerRecord = {
  id: string;
  user_id: string;
  asset: string;
  entry_type: string;
  amount: string;
  balance_available_after: string;
  balance_frozen_after: string;
  ref_type: string;
  ref_id: string;
  created_at: string;
};

type PageStateRecord = {
  user_id: string;
  page_ready: boolean;
  screen_name: string;
  confidence: string;
  package_name: string;
  amount_field_found: boolean;
  up_button_found: boolean;
  down_button_found: boolean;
  observed_balance_usdt?: string | null;
  account_match_status?: string | null;
  account_balance_delta_usdt?: string | null;
  observed_at: string;
  updated_at: string;
};

type PlatformTradingSettings = {
  stake_percent: string;
  min_stake_usdt: string;
  max_single_stake_usdt: string;
  max_active_event_orders_per_user: number;
  first_deposit_min_usdt: string;
  deposit_fee_rate: string;
  direct_referral_rate: string;
  indirect_referral_rate: string;
  updated_at?: string | null;
};

type ExchangeWorkerSlot = {
  id: string;
  name: string;
  shard_total: number;
  shard_index: number;
  bind_local_ip?: string | null;
  enabled: boolean;
  status: string;
  last_started_at?: string | null;
  last_stopped_at?: string | null;
  last_error?: string | null;
  updated_at: string;
  created_at: string;
};

type TableRow = {
  id: string;
  cells: ReactNode[];
  details?: Record<string, ReactNode>;
};

const TOKEN_KEY = "event_signal_admin_token";
const ADMIN_USER_KEY = "event_signal_admin_user";
const API_BASE = (import.meta.env.VITE_API_BASE_URL || "").trim().replace(/\/$/, "");
const REOWN_PROJECT_ID = import.meta.env.VITE_REOWN_PROJECT_ID || "00000000000000000000000000000000";
const BSC_USDT = "0x55d398326f99059fF775485246999027B3197955";
const BSC_VAULT = (import.meta.env.VITE_BSC_VAULT_CONTRACT || "").trim();
const BSC_SETTLEMENT_WALLET = (import.meta.env.VITE_BSC_SETTLEMENT_WALLET || "").trim();
const erc20Abi = ["function approve(address spender,uint256 value) external returns (bool)"];
const vaultAbi = ["function payRefund(address recipient,uint256 amount,bytes32 refundId) external"];
const bsc = {
  id: 56,
  caipNetworkId: "eip155:56",
  chainNamespace: "eip155",
  name: "BNB Smart Chain",
  nativeCurrency: { name: "BNB", symbol: "BNB", decimals: 18 },
  rpcUrls: { default: { http: ["https://bsc-dataseed.binance.org/"] } },
  blockExplorers: { default: { name: "BscScan", url: "https://bscscan.com" } }
} as const;

createAppKit({
  adapters: [new EthersAdapter()],
  networks: [bsc],
  projectId: REOWN_PROJECT_ID,
  metadata: {
    name: "Event Signal Admin",
    description: "Event Signal withdrawal operations",
    url: typeof window === "undefined" ? "https://localhost" : window.location.origin,
    icons: []
  }
});

function App() {
  const { open } = useAppKit();
  const { address, isConnected } = useAppKitAccount();
  const { walletProvider } = useAppKitProvider("eip155");
  const [token, setToken] = useState(localStorage.getItem(TOKEN_KEY) || "");
  const [adminUser, setAdminUser] = useState(localStorage.getItem(ADMIN_USER_KEY) || "2752186242");
  const [adminPassword, setAdminPassword] = useState("");
  const [view, setView] = useState<AdminView>("overview");
  const [overview, setOverview] = useState<AdminOverview | null>(null);
  const [signals, setSignals] = useState<SignalRecord[]>([]);
  const [users, setUsers] = useState<UserRecord[]>([]);
  const [deposits, setDeposits] = useState<DepositRecord[]>([]);
  const [withdrawals, setWithdrawals] = useState<WithdrawalRecord[]>([]);
  const [ledger, setLedger] = useState<LedgerRecord[]>([]);
  const [orders, setOrders] = useState<OrderRecord[]>([]);
  const [reports, setReports] = useState<ReportRecord[]>([]);
  const [settlements, setSettlements] = useState<SettlementRecord[]>([]);
  const [pageStates, setPageStates] = useState<PageStateRecord[]>([]);
  const [exchangeWorkers, setExchangeWorkers] = useState<ExchangeWorkerSlot[]>([]);
  const [predictionSettings, setPredictionSettings] = useState<PredictionSettings>({
    enabled: true,
    interval_seconds: 5,
    up_threshold: "8",
    down_threshold: "-8",
    min_confidence: "0.62",
    technical_weight: "0.35",
    probability_weight: "0.65",
    volatility_floor: "0.00016",
    calibration_lookback_minutes: 180,
    calibration_min_samples: 30,
    calibration_min_win_rate: "0.60",
    calibration_min_ev_usdt: "0.01"
  });
  const [predictionObservations, setPredictionObservations] = useState<PredictionObservation[]>([]);
  const [workerName, setWorkerName] = useState("");
  const [workerShardTotal, setWorkerShardTotal] = useState("3");
  const [workerShardIndex, setWorkerShardIndex] = useState("0");
  const [workerBindLocalIp, setWorkerBindLocalIp] = useState("");
  const [workerEnabled, setWorkerEnabled] = useState(false);
  const [tradingSettings, setTradingSettings] = useState<PlatformTradingSettings>({
    stake_percent: "0.05",
    min_stake_usdt: "5",
    max_single_stake_usdt: "250",
    max_active_event_orders_per_user: 3,
    first_deposit_min_usdt: "100",
    deposit_fee_rate: "0.25",
    direct_referral_rate: "0.09",
    indirect_referral_rate: "0.03"
  });
  const [direction, setDirection] = useState<"UP" | "DOWN">("DOWN");
  const [confidence, setConfidence] = useState("1");
  const [status, setStatus] = useState(token ? "正在连接后台" : "请输入管理员账号密码");
  const loggedIn = token.trim().length > 0;

  function adminHeaders(nextToken = token) {
    return {
      "content-type": "application/json",
      "x-admin-token": nextToken
    };
  }

  function clearSession(message: string) {
    localStorage.removeItem(TOKEN_KEY);
    setToken("");
    setOverview(null);
    setStatus(message);
  }

  async function adminFetch<T>(path: string, nextToken = token): Promise<T | null> {
    if (!nextToken.trim()) {
      clearSession("请先登录后台");
      return null;
    }
    const res = await fetch(`${API_BASE}${path}`, { headers: adminHeaders(nextToken) });
    if (!res.ok) {
      if (res.status === 401 || res.status === 403) {
        clearSession("登录已失效，请重新登录");
        return null;
      }
      setStatus(`请求失败 ${res.status}`);
      throw new Error(await res.text());
    }
    return res.json();
  }

  async function loadAll(nextToken = token) {
    setStatus("正在刷新");
    const [nextOverview, nextSettings, nextPredictionSettings, nextPredictionObservations, nextSignals, nextWorkers, nextUsers, nextDeposits, nextWithdrawals, nextLedger, nextOrders, nextReports, nextSettlements, nextPageStates] =
      await Promise.all([
        adminFetch<AdminOverview>("/v1/admin/overview", nextToken),
        adminFetch<PlatformTradingSettings>("/v1/admin/trading-settings", nextToken),
        adminFetch<PredictionSettings>("/v1/admin/prediction-settings", nextToken),
        adminFetch<PredictionObservation[]>("/v1/admin/prediction-observations", nextToken),
        adminFetch<SignalRecord[]>("/v1/admin/signals", nextToken),
        adminFetch<ExchangeWorkerSlot[]>("/v1/admin/exchange-workers", nextToken),
        adminFetch<UserRecord[]>("/v1/admin/users", nextToken),
        adminFetch<DepositRecord[]>("/v1/admin/deposits", nextToken),
        adminFetch<WithdrawalRecord[]>("/v1/admin/withdrawals", nextToken),
        adminFetch<LedgerRecord[]>("/v1/admin/ledger", nextToken),
        adminFetch<OrderRecord[]>("/v1/admin/orders", nextToken),
        adminFetch<ReportRecord[]>("/v1/admin/execution-reports", nextToken),
        adminFetch<SettlementRecord[]>("/v1/admin/settlements", nextToken),
        adminFetch<Record<string, Omit<PageStateRecord, "user_id">>>("/v1/admin/page-states", nextToken)
      ]);

    if (nextOverview) setOverview(nextOverview);
    if (nextSettings) setTradingSettings(nextSettings);
    if (nextPredictionSettings) setPredictionSettings(nextPredictionSettings);
    if (nextPredictionObservations) setPredictionObservations(nextPredictionObservations);
    if (nextSignals) setSignals(nextSignals);
    if (nextWorkers) setExchangeWorkers(nextWorkers);
    if (nextUsers) setUsers(nextUsers);
    if (nextDeposits) setDeposits(nextDeposits);
    if (nextWithdrawals) setWithdrawals(nextWithdrawals);
    if (nextLedger) setLedger(nextLedger);
    if (nextOrders) setOrders(nextOrders);
    if (nextReports) setReports(nextReports);
    if (nextSettlements) setSettlements(nextSettlements);
    if (nextPageStates) {
      setPageStates(
        Object.entries(nextPageStates).map(([userId, state]) => ({
          user_id: userId,
          ...state
        }))
      );
    }
    setStatus("已连接");
  }

  async function loadPredictionLive(nextToken = token) {
    if (!nextToken.trim()) return;
    const [nextOverview, nextPredictionObservations] = await Promise.all([
      adminFetch<AdminOverview>("/v1/admin/overview", nextToken),
      adminFetch<PredictionObservation[]>("/v1/admin/prediction-observations", nextToken)
    ]);
    if (nextOverview) setOverview(nextOverview);
    if (nextPredictionObservations) setPredictionObservations(nextPredictionObservations);
  }

  async function adminLogin(event?: FormEvent) {
    event?.preventDefault();
    localStorage.setItem(ADMIN_USER_KEY, adminUser.trim());
    setStatus("正在登录");
    const res = await fetch(`${API_BASE}/v1/admin/login`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        username: adminUser.trim(),
        password: adminPassword
      })
    });
    if (!res.ok) {
      setStatus(`登录失败 ${res.status}`);
      return;
    }
    const data = await res.json();
    localStorage.setItem(TOKEN_KEY, data.access_token);
    setToken(data.access_token);
    setAdminPassword("");
    await loadAll(data.access_token);
  }

  function adminLogout() {
    clearSession("已退出后台");
  }

  async function emitSignal() {
    setStatus("正在发送信号");
    const res = await fetch(`${API_BASE}/v1/admin/signals/emit`, {
      method: "POST",
      headers: adminHeaders(),
      body: JSON.stringify({
        direction,
        suggested_stake: tradingSettings.min_stake_usdt || "5",
        confidence
      })
    });
    if (!res.ok) {
      setStatus(`发送失败 ${res.status}`);
      return;
    }
    await loadAll();
    setStatus("信号已单次下发");
  }

  async function saveTradingSettings(next: PlatformTradingSettings) {
    setStatus("正在保存比例");
    const res = await fetch(`${API_BASE}/v1/admin/trading-settings`, {
      method: "PUT",
      headers: adminHeaders(),
      body: JSON.stringify(next)
    });
    if (!res.ok) {
      setStatus(`保存失败 ${res.status}`);
      return;
    }
    const data = await res.json();
    setTradingSettings(data);
    setStatus("比例已保存");
  }

  async function savePredictionSettings(next: PredictionSettings) {
    setStatus("正在保存预测配置");
    const res = await fetch(`${API_BASE}/v1/admin/prediction-settings`, {
      method: "PUT",
      headers: adminHeaders(),
      body: JSON.stringify(next)
    });
    if (!res.ok) {
      setStatus(`预测配置保存失败 ${res.status}`);
      return;
    }
    const data = await res.json();
    setPredictionSettings(data);
    await loadAll();
    setStatus(data.enabled ? "预测已启用" : "预测已关闭");
  }

  async function saveExchangeWorker(next: Partial<ExchangeWorkerSlot> & { shard_total?: number; shard_index?: number; enabled?: boolean }) {
    const shardTotal = Number(next.shard_total ?? workerShardTotal);
    const shardIndex = Number(next.shard_index ?? workerShardIndex);
    setStatus("姝ｅ湪淇濆瓨 worker");
    const res = await fetch(`${API_BASE}/v1/admin/exchange-workers`, {
      method: "POST",
      headers: adminHeaders(),
      body: JSON.stringify({
        id: next.id ?? null,
        name: next.name ?? (workerName.trim() || null),
        shard_total: Number.isFinite(shardTotal) ? shardTotal : 3,
        shard_index: Number.isFinite(shardIndex) ? shardIndex : 0,
        bind_local_ip: (next.bind_local_ip ?? workerBindLocalIp).trim() || null,
        enabled: next.enabled ?? workerEnabled
      })
    });
    if (!res.ok) {
      setStatus(`淇濆瓨 worker 澶辫触 ${res.status}`);
      return;
    }
    setExchangeWorkers(await res.json());
    setStatus("worker 宸蹭繚瀛?");
  }

  async function toggleExchangeWorker(id: string, enabled: boolean) {
    setStatus(enabled ? "姝ｅ湪鍚敤 worker" : "姝ｅ湪鍏抽棴 worker");
    const res = await fetch(`${API_BASE}/v1/admin/exchange-workers/toggle`, {
      method: "POST",
      headers: adminHeaders(),
      body: JSON.stringify({ id, enabled })
    });
    if (!res.ok) {
      setStatus(`worker 鎿嶄綔澶辫触 ${res.status}`);
      return;
    }
    setExchangeWorkers(await res.json());
    setStatus(enabled ? "worker 宸插惎鐢?" : "worker 宸插叧闂?");
  }

  async function deleteExchangeWorker(id: string) {
    if (!window.confirm("纭畾鍒犻櫎杩欎釜 worker slot ?")) return;
    setStatus("姝ｅ湪鍒犻櫎 worker");
    const res = await fetch(`${API_BASE}/v1/admin/exchange-workers/delete`, {
      method: "POST",
      headers: adminHeaders(),
      body: JSON.stringify({ id })
    });
    if (!res.ok) {
      setStatus(`鍒犻櫎 worker 澶辫触 ${res.status}`);
      return;
    }
    setExchangeWorkers(await res.json());
    setStatus("worker 宸插垹闄?");
  }

  async function settleReport(reportId: string) {
    setStatus("正在审核结算");
    const res = await fetch(`${API_BASE}/v1/admin/execution-reports/settle`, {
      method: "POST",
      headers: adminHeaders(),
      body: JSON.stringify({ report_id: reportId })
    });
    if (!res.ok) {
      setStatus(`结算失败 ${res.status}`);
      return;
    }
    await loadAll();
    setStatus("结算完成");
  }

  async function confirmWithdrawal(withdrawalId: string) {
    const txHash = window.prompt("请输入退款交易哈希");
    if (!txHash?.trim()) return;
    setStatus("姝ｅ湪纭鎻愮幇");
    const res = await fetch(`${API_BASE}/v1/admin/withdrawals/confirm`, {
      method: "POST",
      headers: adminHeaders(),
      body: JSON.stringify({ withdrawal_id: withdrawalId, tx_hash: txHash.trim() })
    });
    if (!res.ok) {
      setStatus(`纭澶辫触 ${res.status}`);
      return;
    }
    await loadAll();
    setStatus("鎻愮幇宸插鐞?");
  }

  async function payWithdrawalOnChain(withdrawal: WithdrawalRecord) {
    if (!isConnected || !walletProvider || !address) {
      setStatus("请先连接管理钱包");
      await open({ view: "Connect" });
      return;
    }
    if (!BSC_VAULT) {
      setStatus("缺少 BSC_VAULT_CONTRACT");
      return;
    }
    if (BSC_SETTLEMENT_WALLET && address.toLowerCase() !== BSC_SETTLEMENT_WALLET.toLowerCase()) {
      setStatus(`请切换到打款钱包 ${shortId(BSC_SETTLEMENT_WALLET, 8)}`);
      return;
    }
    const provider = new BrowserProvider(walletProvider as Eip1193Provider);
    await provider.send("wallet_switchEthereumChain", [{ chainId: "0x38" }]);
    const signer = await provider.getSigner();
    const usdt = new Contract(BSC_USDT, erc20Abi, signer);
    const vault = new Contract(BSC_VAULT, vaultAbi, signer);
    const value = parseUnits(withdrawal.amount, 18);
    setStatus("正在授权退款");
    await (await usdt.approve(BSC_VAULT, value)).wait();
    setStatus("正在执行退款");
    const tx = await vault.payRefund(withdrawal.to_address, value, withdrawal.refund_id);
    setStatus(`退款已发送 ${tx.hash.slice(0, 10)}...`);
    await tx.wait();
    setStatus("链上确认完成，正在回写后台");
    const res = await fetch(`${API_BASE}/v1/admin/withdrawals/confirm`, {
      method: "POST",
      headers: adminHeaders(),
      body: JSON.stringify({ withdrawal_id: withdrawal.id, tx_hash: tx.hash })
    });
    if (!res.ok) {
      setStatus(`后台确认失败 ${res.status}`);
      return;
    }
    await loadAll();
    setStatus("退款已确认");
  }

  async function rejectWithdrawal(withdrawalId: string) {
    const reason = window.prompt("请输入驳回原因（可留空）") ?? "";
    setStatus("姝ｅ湪椹辨帀鎻愮幇");
    const res = await fetch(`${API_BASE}/v1/admin/withdrawals/reject`, {
      method: "POST",
      headers: adminHeaders(),
      body: JSON.stringify({ withdrawal_id: withdrawalId, reason: reason.trim() || null })
    });
    if (!res.ok) {
      setStatus(`椹辨帀澶辫触 ${res.status}`);
      return;
    }
    await loadAll();
    setStatus("鎻愮幇宸茶鍐?");
  }

  useEffect(() => {
    if (!token.trim()) return;
    loadAll(token).catch(() => undefined);
    const id = window.setInterval(() => loadAll(token).catch(() => undefined), 5000);
    return () => window.clearInterval(id);
  }, [token]);

  useEffect(() => {
    if (!token.trim() || view !== "prediction") return;
    loadPredictionLive(token).catch(() => undefined);
    const id = window.setInterval(() => loadPredictionLive(token).catch(() => undefined), 1000);
    return () => window.clearInterval(id);
  }, [token, view]);

  if (!loggedIn) {
    return (
      <div className="login-page">
        <form className="login-panel" onSubmit={adminLogin}>
          <div className="brand login-brand">
            <div className="brand-mark">ES</div>
            <div>
              <strong>事件信号后台</strong>
              <span>管理员登录</span>
            </div>
          </div>
          <label>
            管理员账号
            <input value={adminUser} onChange={(event) => setAdminUser(event.target.value)} autoComplete="username" />
          </label>
          <label>
            管理员密码
            <input
              type="password"
              value={adminPassword}
              onChange={(event) => setAdminPassword(event.target.value)}
              autoComplete="current-password"
            />
          </label>
          <button className="primary login-button" type="submit">
            <ShieldCheck size={18} />
            登录后台
          </button>
          <span className="status">{status}</span>
        </form>
      </div>
    );
  }

  const latest = overview?.stats.latest_signal;

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">ES</div>
          <div>
            <strong>事件信号后台</strong>
            <span>BTCUSDT 10m</span>
          </div>
        </div>
        <nav>
          <NavItem active={view === "withdrawals"} icon={<ArrowDownRight size={18} />} label="提现" onClick={() => setView("withdrawals")} />
          <NavItem active={view === "overview"} icon={<Activity size={18} />} label="总览" onClick={() => setView("overview")} />
          <NavItem active={view === "settings"} icon={<SlidersHorizontal size={18} />} label="设置" onClick={() => setView("settings")} />
          <NavItem active={view === "prediction"} icon={<Zap size={18} />} label="预测" onClick={() => setView("prediction")} />
          <NavItem active={view === "signals"} icon={<Radio size={18} />} label="信号" onClick={() => setView("signals")} />
          <NavItem active={view === "users"} icon={<Users size={18} />} label="用户" onClick={() => setView("users")} />
          <NavItem active={view === "deposits"} icon={<CreditCard size={18} />} label="充值" onClick={() => setView("deposits")} />
          <NavItem active={view === "orders"} icon={<Smartphone size={18} />} label="执行" onClick={() => setView("orders")} />
          <NavItem active={view === "settlements"} icon={<ShieldCheck size={18} />} label="结算" onClick={() => setView("settlements")} />
        </nav>
      </aside>

      <main>
        <header className="topbar">
          <div>
            <h1>{viewTitle(view)}</h1>
            <p>{viewSubtitle(view)}</p>
          </div>
          <div className="topbar-actions">
            <span className="status">{status}</span>
            <button className={isConnected ? "icon-button wallet-button connected" : "icon-button wallet-button"} onClick={() => open({ view: "Connect" })}>
              <Wallet size={18} />
              {isConnected ? shortId(address, 6) : "连接钱包"}
            </button>
            <button className="icon-button" onClick={() => loadAll().catch(() => undefined)}>
              <RefreshCw size={18} />
              刷新
            </button>
            <button className="icon-button secondary-action" onClick={adminLogout}>
              退出
            </button>
          </div>
        </header>

        <section className="connection compact">
          <label>
            登录身份
            <input value={`管理员：${adminUser}`} readOnly />
          </label>
          <label>
            管理钱包
            <input
              value={isConnected ? `${shortId(address, 10)}${BSC_SETTLEMENT_WALLET ? ` / 打款钱包 ${shortId(BSC_SETTLEMENT_WALLET, 10)}` : ""}` : "未连接"}
              readOnly
            />
          </label>
        </section>

        {view === "overview" && (
          <OverviewPage overview={overview} latest={latest} reports={reports} onSettle={settleReport} />
        )}
        {view === "settings" && (
          <SettingsPage settings={tradingSettings} onSave={saveTradingSettings} />
        )}
        {view === "prediction" && (
          <PredictionPage
            settings={predictionSettings}
            runtime={overview?.stats.prediction}
            observations={predictionObservations}
            onSave={savePredictionSettings}
          />
        )}
        {view === "settings" && (
          <ExchangeWorkersPage
            workers={exchangeWorkers}
            draft={{
              name: workerName,
              shardTotal: workerShardTotal,
              shardIndex: workerShardIndex,
              bindLocalIp: workerBindLocalIp,
              enabled: workerEnabled
            }}
            onDraftChange={({ name, shardTotal, shardIndex, bindLocalIp, enabled }) => {
              setWorkerName(name);
              setWorkerShardTotal(shardTotal);
              setWorkerShardIndex(shardIndex);
              setWorkerBindLocalIp(bindLocalIp);
              setWorkerEnabled(enabled);
            }}
            onSave={saveExchangeWorker}
            onToggle={toggleExchangeWorker}
            onDelete={deleteExchangeWorker}
          />
        )}
        {view === "signals" && (
          <SignalsPage
            direction={direction}
            settings={tradingSettings}
            confidence={confidence}
            signals={signals}
            onDirectionChange={setDirection}
            onConfidenceChange={setConfidence}
            onEmit={emitSignal}
          />
        )}
        {view === "users" && <UsersPage users={users} pageStates={pageStates} />}
        {view === "deposits" && <DepositsPage deposits={deposits} ledger={ledger} />}
        {view === "withdrawals" && (
          <WithdrawalsPage
            withdrawals={withdrawals}
            onConfirm={confirmWithdrawal}
            onReject={rejectWithdrawal}
            onChain={(row) => payWithdrawalOnChain(row).catch((err) => setStatus(String(err)))}
          />
        )}
        {view === "orders" && <OrdersPage orders={orders} reports={reports} onSettle={settleReport} />}
        {view === "settlements" && <SettlementsPage settlements={settlements} />}
      </main>
    </div>
  );
}

function OverviewPage({
  overview,
  latest,
  reports,
  onSettle
}: {
  overview: AdminOverview | null;
  latest?: SignalRecord;
  reports: ReportRecord[];
  onSettle: (reportId: string) => void;
}) {
  return (
    <>
      <section className="stats-grid">
        <Stat icon={<Users />} label="用户" value={overview?.stats.users_total ?? 0} />
        <Stat icon={<Smartphone />} label="页面就绪" value={overview?.ready_pages ?? 0} />
        <Stat icon={<Send />} label="信号" value={overview?.stats.signals_total ?? 0} />
        <Stat icon={<CircleDollarSign />} label="订单" value={overview?.stats.orders_total ?? 0} />
      </section>

      <section className="split">
        <div className="panel">
          <div className="panel-title">
            <div>
              <h2>预测状态</h2>
              <p>服务器端信号生成任务状态。</p>
            </div>
          </div>
          <dl>
            <div><dt>模式</dt><dd>{predictionModeLabel(overview?.stats.prediction?.mode)}</dd></div>
            <div><dt>状态</dt><dd><StatusPill active={Boolean(overview?.stats.prediction?.enabled)}>{predictionStatusLabel(overview?.stats.prediction)}</StatusPill></dd></div>
            <div><dt>最近信号</dt><dd>{shortTime(overview?.stats.prediction?.last_signal_at)}</dd></div>
            <div><dt>说明</dt><dd>{overview?.stats.prediction?.message || "暂无状态"}</dd></div>
          </dl>
        </div>

        <div className="panel">
          <div className="panel-title">
            <div>
              <h2>最新信号</h2>
              <p>后台最后一次生成的 WebSocket 信号。</p>
            </div>
          </div>
          <SignalSummary signal={latest} />
        </div>

        <div className="panel">
          <div className="panel-title">
            <div>
              <h2>命中统计</h2>
              <p>来自执行回报的胜负统计。</p>
            </div>
          </div>
          <dl>
            <div><dt>胜单</dt><dd>{overview?.stats.wins ?? 0}</dd></div>
            <div><dt>负单</dt><dd>{overview?.stats.losses ?? 0}</dd></div>
            <div><dt>命中率</dt><dd>{percent(overview?.stats.hit_rate)}</dd></div>
            <div><dt>运行时间</dt><dd>{duration(overview?.stats.uptime_seconds ?? 0)}</dd></div>
          </dl>
        </div>
      </section>

      <ReportsTable reports={reports.slice(0, 8)} onSettle={onSettle} title="待审核回报" />
    </>
  );
}

function SignalsPage({
  direction,
  settings,
  confidence,
  signals,
  onDirectionChange,
  onConfidenceChange,
  onEmit
}: {
  direction: "UP" | "DOWN";
  settings: PlatformTradingSettings;
  confidence: string;
  signals: SignalRecord[];
  onDirectionChange: (direction: "UP" | "DOWN") => void;
  onConfidenceChange: (confidence: string) => void;
  onEmit: () => void;
}) {
  return (
    <>
      <section className="panel signal-panel">
        <div className="panel-title">
          <div>
            <h2>发送测试信号</h2>
            <p>只发一次，不做补偿重放。生产信号仍应由后端预测任务生成。</p>
          </div>
        </div>
        <div className="trade-buttons">
          <button className={direction === "UP" ? "up active" : "up"} onClick={() => onDirectionChange("UP")}>
            <ArrowUpRight size={18} /> 看涨
          </button>
          <button className={direction === "DOWN" ? "down active" : "down"} onClick={() => onDirectionChange("DOWN")}>
            <ArrowDownRight size={18} /> 看跌
          </button>
        </div>
        <div className="form-row">
          <label>
            后台比例
            <input value={`${stakePercentLabel(settings.stake_percent)}，最低 ${settings.min_stake_usdt}U，最高 ${settings.max_single_stake_usdt}U`} readOnly />
          </label>
          <label>
            置信度
            <input value={confidence} onChange={(event) => onConfidenceChange(event.target.value)} />
          </label>
        </div>
        <button className="primary" onClick={onEmit}>
          <Send size={18} />
          下发信号
        </button>
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>信号历史</h2>
            <p>展示最近 200 条后台信号。</p>
          </div>
        </div>
        <DataTable
          columns={["时间", "交易对", "方向", "入场价", "到期价", "结果", "金额", "置信度", "状态"]}
          rows={signals.map((signal) => ({
            id: signal.id,
            cells: [
              shortTime(signal.cycle_end),
              signal.symbol,
              <ToneText tone={directionTone(signal.direction)}>{labelDirection(signal.direction)}</ToneText>,
              signal.entry_price || "-",
              signal.outcome_expiry_price || "-",
              <ToneText tone={resultTone(signal.outcome_result)}>{signalOutcomeLabel(signal.outcome_result)}</ToneText>,
              `${signal.suggested_stake || "-"} USDT`,
              percent(signal.confidence),
              signal.authorization_status || "-"
            ],
            details: {
              信号ID: signal.signal_id || signal.id,
              交易对: signal.symbol,
              方向: labelDirection(signal.direction),
              入场价: signal.entry_price || "-",
              到期价: signal.outcome_expiry_price || "-",
              信号结果: signalOutcomeLabel(signal.outcome_result),
              结算时间: shortTime(signal.outcome_settled_at),
              建议金额: `${signal.suggested_stake || "-"} USDT`,
              预扣金额: `${signal.fee_reserve_usdt || "-"} USDT`,
              置信度: percent(signal.confidence),
              授权状态: signal.authorization_status || "-",
              是否授权: yesNo(Boolean(signal.trade_authorized)),
              模型版本: signal.model_version || "-",
              周期结束: shortTime(signal.cycle_end)
            }
          }))}
        />
      </section>
    </>
  );
}

function SettingsPage({
  settings,
  onSave
}: {
  settings: PlatformTradingSettings;
  onSave: (settings: PlatformTradingSettings) => void;
}) {
  const [draft, setDraft] = useState(settings);

  useEffect(() => {
    setDraft(settings);
  }, [settings]);

  return (
    <section className="panel signal-panel">
      <div className="panel-title">
        <div>
          <h2>下单比例</h2>
          <p>所有用户按币安 U 本位合约可用余额计算下单金额，信号瞬间不再查询币安接口。</p>
        </div>
      </div>
      <div className="form-row">
        <label>
          下单比例
          <input
            value={draft.stake_percent}
            onChange={(event) => setDraft({ ...draft, stake_percent: event.target.value })}
            placeholder="0.05"
          />
        </label>
        <label>
          最低金额 USDT
          <input
            value={draft.min_stake_usdt}
            onChange={(event) => setDraft({ ...draft, min_stake_usdt: event.target.value })}
            placeholder="5"
          />
        </label>
      </div>
      <div className="form-row">
        <label>
          最高金额 USDT
          <input
            value={draft.max_single_stake_usdt}
            onChange={(event) => setDraft({ ...draft, max_single_stake_usdt: event.target.value })}
            placeholder="250"
          />
        </label>
        <label>
          同周期最多连开单数
          <input
            value={draft.max_active_event_orders_per_user}
            onChange={(event) =>
              setDraft({
                ...draft,
                max_active_event_orders_per_user: Number(event.target.value) || 3
              })
            }
            placeholder="3"
          />
        </label>
      </div>
      <div className="form-row">
        <label>
          更新时间
          <input value={shortTime(settings.updated_at)} readOnly />
        </label>
      </div>
      <div className="form-row">
        <label>
          首充激活金额 USDT
          <input
            value={draft.first_deposit_min_usdt}
            onChange={(event) => setDraft({ ...draft, first_deposit_min_usdt: event.target.value })}
            placeholder="100"
          />
        </label>
        <label>
          充值手续费比例
          <input
            value={draft.deposit_fee_rate}
            onChange={(event) => setDraft({ ...draft, deposit_fee_rate: event.target.value })}
            placeholder="0.25"
          />
        </label>
      </div>
      <div className="form-row">
        <label>
          直推返佣比例
          <input
            value={draft.direct_referral_rate}
            onChange={(event) => setDraft({ ...draft, direct_referral_rate: event.target.value })}
            placeholder="0.09"
          />
        </label>
        <label>
          间推返佣比例
          <input
            value={draft.indirect_referral_rate}
            onChange={(event) => setDraft({ ...draft, indirect_referral_rate: event.target.value })}
            placeholder="0.03"
          />
        </label>
      </div>
      <button className="primary" onClick={() => onSave(draft)}>
        <SlidersHorizontal size={18} />
        保存设置
      </button>
    </section>
  );
}

function PredictionPage({
  settings,
  runtime,
  observations,
  onSave
}: {
  settings: PredictionSettings;
  runtime?: PredictionStatus;
  observations: PredictionObservation[];
  onSave: (settings: PredictionSettings) => void | Promise<void>;
}) {
  const [draft, setDraft] = useState(settings);
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (!editing) setDraft(settings);
  }, [settings, editing]);

  function updateDraft(patch: Partial<PredictionSettings>) {
    setEditing(true);
    setDraft({ ...draft, ...patch });
  }

  async function saveDraft() {
    await onSave(draft);
    setEditing(false);
  }

  function observationMetrics(row: PredictionObservation) {
    return predictionMetrics(row.feature_snapshot);
  }

  return (
    <>
      <section className="connection">
        <label className="switch-line">
          <input
            type="checkbox"
            checked={draft.enabled}
            onChange={(event) => updateDraft({ enabled: event.target.checked })}
          />
          <span>启用自动预测循环</span>
        </label>
        <label>
          检查间隔（秒）
          <input
            value={draft.interval_seconds}
            onChange={(event) => updateDraft({ interval_seconds: Number(event.target.value) || 5 })}
          />
        </label>
        <label>
          看涨概率门槛
          <input value={draft.min_confidence} onChange={(event) => updateDraft({ min_confidence: event.target.value })} />
        </label>
        <label>
          波动率下限
          <input value={draft.volatility_floor} onChange={(event) => updateDraft({ volatility_floor: event.target.value })} />
        </label>
        <label>
          校准窗口（分钟）
          <input
            value={draft.calibration_lookback_minutes}
            onChange={(event) =>
              updateDraft({ calibration_lookback_minutes: Number(event.target.value) || 180 })
            }
          />
        </label>
        <label>
          校准最小样本
          <input
            value={draft.calibration_min_samples}
            onChange={(event) =>
              updateDraft({ calibration_min_samples: Number(event.target.value) || 30 })
            }
          />
        </label>
        <label>
          校准最低胜率
          <input
            value={draft.calibration_min_win_rate}
            onChange={(event) => updateDraft({ calibration_min_win_rate: event.target.value })}
          />
        </label>
        <label>
          校准最低EV
          <input
            value={draft.calibration_min_ev_usdt}
            onChange={(event) => updateDraft({ calibration_min_ev_usdt: event.target.value })}
          />
        </label>
        <button className="primary" onClick={saveDraft}>
          <Power size={18} />
          保存预测配置
        </button>
      </section>

      <section className="split">
        <div className="panel">
          <div className="panel-title">
            <div>
              <h2>运行状态</h2>
              <p>后端预测循环实时状态。</p>
            </div>
          </div>
          <dl>
            <div><dt>状态</dt><dd><StatusPill active={Boolean(runtime?.enabled)}>{predictionStatusLabel(runtime)}</StatusPill></dd></div>
            <div><dt>最近评估</dt><dd>{shortTime(runtime?.last_evaluated_at)}</dd></div>
            <div><dt>最近信号</dt><dd>{shortTime(runtime?.last_signal_at)}</dd></div>
            <div><dt>方向</dt><dd>{labelDirection(runtime?.last_direction || undefined)}</dd></div>
            <div><dt>分数</dt><dd>{runtime?.last_score || "-"}</dd></div>
            <div><dt>置信度</dt><dd>{runtime?.last_confidence || "-"}</dd></div>
            <div><dt>行情源</dt><dd>{runtime?.market_source || "-"}</dd></div>
            <div><dt>行情延迟</dt><dd>{runtime?.market_age_seconds ?? "-"} 秒</dd></div>
            <div><dt>K线预热</dt><dd>{runtime?.market_candles ?? 0} 根</dd></div>
            <div><dt>说明</dt><dd>{runtime?.message || "-"}</dd></div>
            <div><dt>错误</dt><dd>{runtime?.last_error || "-"}</dd></div>
          </dl>
        </div>
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>预测运行记录</h2>
            <p>包含 HOLD 和实际下发前的每次策略评估。</p>
          </div>
        </div>
        <DataTable
          columns={["时间", "方向", "价格", "模型分", "涨概率", "涨结构", "有效门槛", "候选", "原因"]}
          pageSize={20}
          rows={observations.map((row) => ({
            id: row.id,
            cells: (() => {
              const metrics = observationMetrics(row);
              return [
                shortTime(row.created_at),
                <ToneText tone={directionTone(row.direction)}>{labelDirection(row.direction)}</ToneText>,
                row.entry_price || "-",
                metrics.modelScore || row.score || "-",
                metrics.upProbability,
                metrics.upStructure,
                metrics.effectiveMinConfidence,
                metrics.candidateRule,
                row.reason || "-"
              ];
            })(),
            details: (() => {
              const metrics = observationMetrics(row);
              return {
                时间: shortTime(row.created_at),
                来源: row.source,
                交易对: row.symbol,
                周期: `${row.period_seconds}s`,
                模型: row.model_version,
                方向: labelDirection(row.direction),
                模型分: metrics.modelScore || row.score || "-",
                技术分: metrics.technicalScore,
                概率分: metrics.probabilityScore,
                技术权重: metrics.technicalWeight,
                概率权重: metrics.probabilityWeight,
                上涨概率: metrics.upProbability,
                下跌概率: metrics.downProbability,
                上涨结构: metrics.upStructure,
                下跌结构: metrics.downStructure,
                候选模式: metrics.candidateMode,
                候选规则: metrics.candidateRule,
                有效门槛: metrics.effectiveMinConfidence,
                边际概率: metrics.edgeProbability,
                原始置信度: metrics.rawConfidence,
                下发置信度: percent(row.confidence),
                预期到期涨跌幅: metrics.expectedExpiryReturn,
                到期波动率: metrics.expiryVolatility,
                波动率下限: metrics.volatilityFloor,
                入场参考: metrics.strikeReference,
                预测收益: row.predicted_return,
                入场价: row.entry_price,
                原因: row.reason || "-",
                特征: JSON.stringify(row.feature_snapshot)
              };
            })()
          }))}
        />
      </section>
    </>
  );
}

function ExchangeWorkersPage({
  workers,
  draft,
  onDraftChange,
  onSave,
  onToggle,
  onDelete
}: {
  workers: ExchangeWorkerSlot[];
  draft: {
    name: string;
    shardTotal: string;
    shardIndex: string;
    bindLocalIp: string;
    enabled: boolean;
  };
  onDraftChange: (draft: {
    name: string;
    shardTotal: string;
    shardIndex: string;
    bindLocalIp: string;
    enabled: boolean;
  }) => void;
  onSave: (slot: Partial<ExchangeWorkerSlot> & { shard_total?: number; shard_index?: number; enabled?: boolean }) => void;
  onToggle: (id: string, enabled: boolean) => void;
  onDelete: (id: string) => void;
}) {
  const updateDraft = (patch: Partial<typeof draft>) => onDraftChange({ ...draft, ...patch });
  return (
    <section className="panel signal-panel">
      <div className="panel-title">
        <div>
          <h2>Exchange Worker</h2>
          <p>配置 Binance income 查询分片和本地出口 IP；IP 留空时使用服务器默认出口。</p>
        </div>
      </div>
      <div className="form-row">
        <label>
          名称
          <input value={draft.name} onChange={(event) => updateDraft({ name: event.target.value })} placeholder="worker-0" />
        </label>
        <label>
          本地出口 IP
          <input value={draft.bindLocalIp} onChange={(event) => updateDraft({ bindLocalIp: event.target.value })} placeholder="可留空，例如 10.0.0.4" />
        </label>
      </div>
      <div className="form-row">
        <label>
          分片总数
          <input value={draft.shardTotal} onChange={(event) => updateDraft({ shardTotal: event.target.value })} placeholder="3" />
        </label>
        <label>
          当前分片
          <input value={draft.shardIndex} onChange={(event) => updateDraft({ shardIndex: event.target.value })} placeholder="0" />
        </label>
      </div>
      <div className="row-actions">
        <button
          className={draft.enabled ? "table-action" : "table-action secondary-action"}
          onClick={() => updateDraft({ enabled: !draft.enabled })}
        >
          <Power size={14} />
          {draft.enabled ? "创建后启用" : "创建后停用"}
        </button>
        <button
          className="primary"
          onClick={() =>
            onSave({
              name: draft.name,
              shard_total: Number(draft.shardTotal),
              shard_index: Number(draft.shardIndex),
              bind_local_ip: draft.bindLocalIp,
              enabled: draft.enabled
            })
          }
        >
          <SquarePen size={18} />
          保存 Worker
        </button>
      </div>

      <DataTable
        columns={["名称", "分片", "出口 IP", "状态", "最后启动", "错误", "操作"]}
        rows={workers.map((worker) => ({
          id: worker.id,
          cells: [
            worker.name,
            `${worker.shard_index}/${worker.shard_total}`,
            worker.bind_local_ip || "默认出口",
            <StatusPill active={worker.enabled && worker.status === "running"}>{worker.enabled ? worker.status : "stopped"}</StatusPill>,
            shortTime(worker.last_started_at),
            worker.last_error ? shortId(worker.last_error, 18) : "-",
            <div className="row-actions">
              <button className="table-action" onClick={(event) => stopRowClick(event, () => onToggle(worker.id, !worker.enabled))}>
                <Power size={14} />
                {worker.enabled ? "停用" : "启用"}
              </button>
              <button
                className="table-action secondary-action"
                onClick={(event) =>
                  stopRowClick(event, () =>
                    onSave({
                      id: worker.id,
                      name: worker.name,
                      shard_total: worker.shard_total,
                      shard_index: worker.shard_index,
                      bind_local_ip: worker.bind_local_ip || "",
                      enabled: worker.enabled
                    })
                  )
                }
              >
                <SquarePen size={14} />
                保存
              </button>
              <button className="table-action secondary-action" onClick={(event) => stopRowClick(event, () => onDelete(worker.id))}>
                <Trash2 size={14} />
                删除
              </button>
            </div>
          ],
          details: {
            WorkerID: worker.id,
            名称: worker.name,
            分片: `${worker.shard_index}/${worker.shard_total}`,
            出口IP: worker.bind_local_ip || "默认出口",
            启用: yesNo(worker.enabled),
            状态: worker.status,
            最后启动: shortTime(worker.last_started_at),
            最后停止: shortTime(worker.last_stopped_at),
            错误: worker.last_error || "-",
            更新时间: shortTime(worker.updated_at),
            创建时间: shortTime(worker.created_at)
          }
        }))}
      />
    </section>
  );
}

function UsersPage({ users, pageStates }: { users: UserRecord[]; pageStates: PageStateRecord[] }) {
  return (
    <>
      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>用户账号</h2>
            <p>钱包地址账号、登录设备和账号状态。</p>
          </div>
        </div>
        <DataTable
          columns={["注册时间", "钱包地址", "可用余额", "冻结余额", "状态", "当前设备", "更新时间"]}
          rows={users.map((user) => ({
            id: user.id,
            cells: [
              shortTime(user.created_at),
              shortId(user.wallet_address, 10),
              `${formatNumber(user.balance_available)} USDT`,
              `${formatNumber(user.balance_frozen)} USDT`,
              user.status,
              user.active_device_fingerprint ? shortId(user.active_device_fingerprint, 14) : "-",
              shortTime(user.updated_at)
            ],
            details: {
              用户ID: user.id,
              钱包地址: user.wallet_address,
              可用余额: `${formatNumber(user.balance_available)} USDT`,
              冻结余额: `${formatNumber(user.balance_frozen)} USDT`,
              余额更新时间: shortTime(user.balance_updated_at),
              状态: user.status,
              当前设备: user.active_device_fingerprint || "-",
              注册时间: shortTime(user.created_at),
              更新时间: shortTime(user.updated_at)
            }
          }))}
        />
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>页面闸门</h2>
            <p>Android 上报的事件合约页面识别状态。</p>
          </div>
        </div>
        <DataTable
          columns={["用户", "页面", "实时", "识别", "账号", "页面余额", "上报延迟", "更新时间"]}
          rows={pageStates.map((state) => {
            const liveReady = pageStateLiveReady(state);
            const fresh = pageStateFresh(state);
            return {
              id: `${state.user_id}-${state.updated_at}`,
              cells: [
                shortId(state.user_id, 8),
                state.screen_name,
                <StatusPill active={liveReady}>{liveReady ? "可执行" : fresh ? "不可执行" : "已过期"}</StatusPill>,
                <StatusPill active={state.page_ready}>{state.page_ready ? "就绪" : "未就绪"}</StatusPill>,
                <StatusPill active={state.account_match_status === "matched"}>
                  {accountMatchText(state.account_match_status)}
                </StatusPill>,
                state.observed_balance_usdt ? `${formatNumber(state.observed_balance_usdt)} USDT` : "-",
                ageLabel(state.updated_at),
                shortTime(state.updated_at)
              ],
              details: {
                用户ID: state.user_id,
                页面: state.screen_name,
                实时可执行: yesNo(liveReady),
                状态时效: fresh ? "有效" : `已过期，${ageLabel(state.updated_at)}`,
                识别通过: yesNo(state.page_ready),
                账号一致性: accountMatchText(state.account_match_status),
                页面余额: state.observed_balance_usdt ? `${formatNumber(state.observed_balance_usdt)} USDT` : "-",
                余额差值: state.account_balance_delta_usdt ? `${formatNumber(state.account_balance_delta_usdt)} USDT` : "-",
                置信度: percent(state.confidence),
                金额框: yesNo(state.amount_field_found),
                上涨按钮: yesNo(state.up_button_found),
                下跌按钮: yesNo(state.down_button_found),
                包名: state.package_name,
                观察时间: shortTime(state.observed_at),
                更新时间: shortTime(state.updated_at)
              }
            };
          })}
        />
      </section>
    </>
  );
}

function OrdersPage({ orders, reports, onSettle }: { orders: OrderRecord[]; reports: ReportRecord[]; onSettle: (reportId: string) => void }) {
  return (
    <>
      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>执行订单</h2>
            <p>Android 点击确认后上报的订单记录。</p>
          </div>
        </div>
        <DataTable
          columns={["时间", "用户", "方向", "入场价", "金额", "来源", "状态", "信号"]}
          rows={orders.map((order) => ({
            id: order.id,
            cells: [
              shortTime(order.created_at),
              shortId(order.user_id, 8),
              <ToneText tone={directionTone(order.direction)}>{labelDirection(order.direction)}</ToneText>,
              order.entry_price || "-",
              `${order.stake_usdt} USDT`,
              order.source,
              order.order_status,
              shortId(order.signal_id, 8)
            ],
            details: {
              订单ID: order.id,
              用户ID: order.user_id,
              信号ID: order.signal_id,
              客户端订单: order.client_order_id,
              交易对: order.symbol,
              方向: labelDirection(order.direction),
              入场价: order.entry_price || "-",
              下单金额: `${order.stake_usdt} USDT`,
              来源: order.source,
              自动化置信度: order.automation_confidence || "-",
              状态: order.order_status,
              创建时间: shortTime(order.created_at)
            }
          }))}
        />
      </section>

      <ReportsTable reports={reports} onSettle={onSettle} title="执行回报" />
    </>
  );
}

function DepositsPage({ deposits, ledger }: { deposits: DepositRecord[]; ledger: LedgerRecord[] }) {
  return (
    <>
      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>充值记录</h2>
            <p>链上 USDT 转账校验后才会显示 credited 并入账。</p>
          </div>
        </div>
        <DataTable
          columns={["时间", "用户", "充值", "到账", "手续费", "状态", "确认数", "Tx"]}
          rows={deposits.map((deposit) => ({
            id: deposit.id,
            cells: [
              shortTime(deposit.created_at),
              shortId(deposit.user_id, 8),
              `${deposit.amount} ${deposit.asset}`,
              `${deposit.credited_amount} ${deposit.asset}`,
              `${deposit.platform_fee} ${deposit.asset}`,
              <StatusPill active={deposit.status === "credited"}>{deposit.status}</StatusPill>,
              deposit.confirmations,
              shortId(deposit.tx_hash, 8)
            ],
            details: {
              充值ID: deposit.id,
              用户ID: deposit.user_id,
              链: deposit.chain,
              资产: deposit.asset,
              充值金额: `${deposit.amount} ${deposit.asset}`,
              到账金额: `${deposit.credited_amount} ${deposit.asset}`,
              平台手续费: `${deposit.platform_fee} ${deposit.asset}`,
              状态: deposit.status,
              确认数: deposit.confirmations,
              区块高度: deposit.block_number,
              From: deposit.from_address,
              To: deposit.to_address,
              合约地址: deposit.contract_address,
              交易哈希: deposit.tx_hash,
              入账时间: shortTime(deposit.credited_at),
              创建时间: shortTime(deposit.created_at)
            }
          }))}
        />
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>余额流水</h2>
            <p>所有余额变化都需要对应 ledger 记录。</p>
          </div>
        </div>
        <DataTable
          columns={["时间", "用户", "类型", "金额", "可用余额", "冻结余额", "引用"]}
          rows={ledger.map((row) => ({
            id: row.id,
            cells: [
              shortTime(row.created_at),
              shortId(row.user_id, 8),
              row.entry_type,
              row.amount,
              row.balance_available_after,
              row.balance_frozen_after,
              `${row.ref_type}:${shortId(row.ref_id, 6)}`
            ],
            details: {
              流水ID: row.id,
              用户ID: row.user_id,
              资产: row.asset,
              类型: row.entry_type,
              变动金额: row.amount,
              可用余额: row.balance_available_after,
              冻结余额: row.balance_frozen_after,
              引用类型: row.ref_type,
              引用ID: row.ref_id,
              创建时间: shortTime(row.created_at)
            }
          }))}
        />
      </section>
    </>
  );
}

function WithdrawalsPage({
  withdrawals,
  onConfirm,
  onReject,
  onChain
}: {
  withdrawals: WithdrawalRecord[];
  onConfirm: (withdrawalId: string) => void;
  onReject: (withdrawalId: string) => void;
  onChain: (withdrawal: WithdrawalRecord) => void;
}) {
  return (
    <section className="panel">
      <div className="panel-title">
        <div>
          <h2>提现审核</h2>
          <p>用户申请后先冻结余额，管理员确认链上退款后再释放。</p>
        </div>
      </div>
      <DataTable
        columns={["时间", "用户", "金额", "状态", "退款ID", "目标地址", "交易哈希", "操作"]}
        rows={withdrawals.map((row) => ({
          id: row.id,
          cells: [
            shortTime(row.requested_at),
            shortId(row.user_id, 8),
            `${row.amount} ${row.asset}`,
            <StatusPill active={row.status === "paid"}>{row.status}</StatusPill>,
            shortId(row.refund_id, 8),
            shortId(row.to_address, 8),
            row.tx_hash ? shortId(row.tx_hash, 8) : "-",
            row.status === "pending_review" || row.status === "confirming" ? (
              <div className="row-actions">
                <button className="table-action" onClick={(event) => stopRowClick(event, () => onChain(row))}>链上退款</button>
                <button className="table-action" onClick={(event) => stopRowClick(event, () => onConfirm(row.id))}>确认</button>
                <button className="table-action secondary-action" onClick={(event) => stopRowClick(event, () => onReject(row.id))}>驳回</button>
              </div>
            ) : (
              <span>-</span>
            )
          ],
          details: {
            提现ID: row.id,
            用户ID: row.user_id,
            链: row.chain,
            资产: row.asset,
            金额: `${row.amount} ${row.asset}`,
            状态: row.status,
            退款ID: row.refund_id,
            付款钱包: row.payout_wallet || "-",
            目标地址: row.to_address,
            交易哈希: row.tx_hash || "-",
            确认数: row.confirmations,
            请求时间: shortTime(row.requested_at),
            处理时间: shortTime(row.processed_at),
            驳回时间: shortTime(row.rejected_at),
            驳回原因: row.reject_reason || "-"
          }
        }))}
      />
    </section>
  );
}

function ReportsTable({
  reports,
  onSettle,
  title
}: {
  reports: ReportRecord[];
  onSettle: (reportId: string) => void;
  title: string;
}) {
  return (
    <section className="panel">
      <div className="panel-title">
        <div>
          <h2>{title}</h2>
          <p>执行结果需人工确认后才会扣费或返还。</p>
        </div>
      </div>
      <DataTable
        columns={["时间", "用户", "方向", "入场价", "到期价", "金额", "结果", "毛盈利", "状态", "操作"]}
        rows={reports.map((report) => ({
          id: report.id,
          cells: [
            shortTime(report.created_at),
            shortId(report.user_id, 8),
            <ToneText tone={directionTone(report.direction)}>{labelDirection(report.direction)}</ToneText>,
            report.entry_price || "-",
            report.expiry_price || "-",
            `${report.stake_usdt} USDT`,
            resultLabel(report.result),
            report.gross_profit || "-",
            <StatusPill active={report.report_status === "settled"}>{report.report_status}</StatusPill>,
            report.report_status === "pending_review"
              ? <button className="table-action" onClick={(event) => stopRowClick(event, () => onSettle(report.id))}>确认结算</button>
              : <span>-</span>
          ],
          details: {
            回报ID: report.id,
            用户ID: report.user_id,
            信号ID: report.signal_id,
            方向: labelDirection(report.direction),
            入场价: report.entry_price || "-",
            到期价: report.expiry_price || "-",
            下单金额: `${report.stake_usdt} USDT`,
            结果: resultLabel(report.result),
            毛盈利: report.gross_profit || "-",
            状态: report.report_status,
            创建时间: shortTime(report.created_at)
          }
        }))}
      />
    </section>
  );
}

function SettlementsPage({ settlements }: { settlements: SettlementRecord[] }) {
  return (
    <section className="panel">
      <div className="panel-title">
        <div>
          <h2>结算流水</h2>
          <p>赢单从盈利扣 30%，输单按本金的 30% 返还到平台余额。</p>
        </div>
      </div>
      <DataTable
        columns={["时间", "用户", "方向", "入场价", "到期价", "毛盈利", "平台扣费", "亏损返还", "用户净额", "状态"]}
        rows={settlements.map((row) => ({
          id: row.id,
          cells: [
            shortTime(row.created_at),
            shortId(row.user_id, 8),
            <ToneText tone={directionTone(row.direction || undefined)}>{labelDirection(row.direction || undefined)}</ToneText>,
            row.entry_price || "-",
            row.expiry_price || "-",
            row.gross_profit,
            row.profit_share_fee,
            row.loss_rebate,
            row.user_net_profit,
            row.status
          ],
          details: {
            结算ID: row.id,
            用户ID: row.user_id,
            回报ID: row.execution_report_id,
            信号ID: row.signal_id || "-",
            交易对: row.symbol || "-",
            方向: labelDirection(row.direction || undefined),
            下单金额: row.stake_usdt ? `${row.stake_usdt} USDT` : "-",
            入场价: row.entry_price || "-",
            到期价: row.expiry_price || "-",
            结果: resultLabel(row.result || undefined),
            毛盈利: row.gross_profit,
            平台扣费: row.profit_share_fee,
            亏损返还: row.loss_rebate,
            用户净额: row.user_net_profit,
            状态: row.status,
            创建时间: shortTime(row.created_at)
          }
        }))}
      />
    </section>
  );
}

function SignalSummary({ signal }: { signal?: SignalRecord }) {
  return (
    <div className="latest">
      <strong>{signal?.symbol || "BTCUSDT"}</strong>
      <span className={directionTone(signal?.direction)}>{labelDirection(signal?.direction)}</span>
      <dl>
        <div><dt>建议金额</dt><dd>{signal?.suggested_stake || "-"} USDT</dd></div>
        <div><dt>预扣</dt><dd>{signal?.fee_reserve_usdt || "-"} USDT</dd></div>
        <div><dt>状态</dt><dd>{signal?.authorization_status || "-"}</dd></div>
        <div><dt>结果</dt><dd>{signalOutcomeLabel(signal?.outcome_result)}</dd></div>
        <div><dt>到期价</dt><dd>{signal?.outcome_expiry_price || "-"}</dd></div>
        <div><dt>周期结束</dt><dd>{shortTime(signal?.cycle_end)}</dd></div>
      </dl>
    </div>
  );
}

function NavItem({
  active,
  icon,
  label,
  onClick
}: {
  active: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button className={active ? "active" : ""} onClick={onClick}>
      {icon}
      {label}
    </button>
  );
}

function Stat({ icon, label, value }: { icon: ReactNode; label: string; value: number }) {
  return (
    <div className="stat">
      <div className="stat-icon">{icon}</div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function DataTable({ columns, rows, pageSize = 10 }: { columns: string[]; rows: TableRow[]; pageSize?: number }) {
  const [page, setPage] = useState(0);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<TableRow | null>(null);
  const normalizedQuery = query.trim().toLowerCase();
  const filteredRows = normalizedQuery
    ? rows.filter((row) =>
        row.id.toLowerCase().includes(normalizedQuery)
        || row.cells.some((cell) => String(cell).toLowerCase().includes(normalizedQuery))
        || Object.values(row.details || {}).some((value) => String(value).toLowerCase().includes(normalizedQuery))
      )
    : rows;
  const totalPages = Math.max(1, Math.ceil(filteredRows.length / pageSize));
  const safePage = Math.min(page, totalPages - 1);
  const visibleRows = filteredRows.slice(safePage * pageSize, safePage * pageSize + pageSize);

  useEffect(() => {
    if (page > totalPages - 1) setPage(totalPages - 1);
  }, [page, totalPages]);

  useEffect(() => {
    setPage(0);
  }, [query, rows.length]);

  return (
    <>
      <div className="table-toolbar">
        <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索记录" />
        <span>共 {filteredRows.length} 条</span>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>{columns.map((col) => <th key={col}>{col}</th>)}</tr>
          </thead>
          <tbody>
            {visibleRows.length === 0 ? (
              <tr><td colSpan={columns.length}>暂无记录</td></tr>
            ) : visibleRows.map((row) => (
              <tr className={row.details ? "clickable-row" : ""} key={row.id} onClick={() => row.details && setSelected(row)}>
                {row.cells.map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {filteredRows.length > pageSize && (
        <div className="pagination">
          <span>第 {safePage + 1}/{totalPages} 页</span>
          <div className="row-actions">
            <button className="table-action secondary-action" disabled={safePage === 0} onClick={() => setPage((value) => Math.max(0, value - 1))}>上一页</button>
            <button className="table-action secondary-action" disabled={safePage >= totalPages - 1} onClick={() => setPage((value) => Math.min(totalPages - 1, value + 1))}>下一页</button>
          </div>
        </div>
      )}
      {selected?.details && <DetailsDrawer row={selected} onClose={() => setSelected(null)} />}
    </>
  );
}

function Table({ columns, rows }: { columns: string[]; rows: ReactNode[][] }) {
  return <DataTable columns={columns} rows={rows.map((cells, index) => ({ id: String(index), cells }))} />;
}

function DetailsDrawer({ row, onClose }: { row: TableRow; onClose: () => void }) {
  return (
    <div className="drawer-backdrop" onClick={onClose}>
      <aside className="drawer" onClick={(event) => event.stopPropagation()}>
        <div className="drawer-title">
          <div>
            <h2>记录详情</h2>
            <p>{row.id}</p>
          </div>
          <button className="table-action secondary-action" onClick={onClose}>关闭</button>
        </div>
        <dl className="detail-list">
          {Object.entries(row.details || {}).map(([label, value]) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd>{value}</dd>
            </div>
          ))}
        </dl>
      </aside>
    </div>
  );
}

function StatusPill({ active, children }: { active: boolean; children: ReactNode }) {
  return <span className={active ? "pill active" : "pill"}>{children}</span>;
}

function ToneText({ tone, children }: { tone: string; children: ReactNode }) {
  return <span className={tone}>{children}</span>;
}

function stopRowClick(event: MouseEvent, action: () => void) {
  event.stopPropagation();
  action();
}

function viewTitle(view: AdminView) {
  if (view === "settings") return "交易设置";
  if (view === "prediction") return "预测控制";
  if (view === "signals") return "信号管理";
  if (view === "users") return "用户与页面闸门";
  if (view === "deposits") return "充值与流水";
  if (view === "orders") return "执行与回报";
  if (view === "settlements") return "结算管理";
  return "自动化闸门监控";
}

function viewSubtitle(view: AdminView) {
  if (view === "settings") return "配置所有用户的 U 本位余额下单比例和 5U 到 250U 限制。";
  if (view === "prediction") return "控制后端自动预测开关、阈值，并查看实时运行记录。";
  if (view === "signals") return "下发测试信号并查看历史信号。";
  if (view === "users") return "查看钱包账号、登录设备和页面识别状态。";
  if (view === "deposits") return "查看链上充值校验、入账状态和余额流水。";
  if (view === "orders") return "核对自动化订单与执行结果。";
  if (view === "settlements") return "查看利润分成和亏损返还流水。";
  return "WebSocket 单次下发，页面闸门、余额预扣、执行回报和结算状态集中查看。";
}

function labelDirection(direction?: string) {
  if (direction === "UP") return "看涨";
  if (direction === "DOWN") return "看跌";
  if (direction === "HOLD") return "观望";
  return "-";
}

function predictionModeLabel(mode?: string) {
  if (mode === "strategy_loop") return "策略自动预测";
  if (mode === "manual_signal") return "后台测试信号";
  return mode || "-";
}

function predictionStatusLabel(prediction?: PredictionStatus) {
  if (!prediction) return "-";
  if (prediction.status === "running") return "运行中";
  if (prediction.status === "holding") return "观望";
  if (prediction.status === "matched") return "命中";
  if (prediction.status === "sent") return "已下发";
  if (prediction.status === "signal_ready") return "命中";
  if (prediction.status === "duplicate_skipped") return "已跳过重复";
  if (prediction.status === "disabled") return "未启用";
  if (prediction.status === "error") return "异常";
  return prediction.status || "-";
}

function stakePercentLabel(value?: string) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return value || "-";
  return `${(parsed * 100).toFixed(2)}%`;
}

function directionTone(direction?: string) {
  if (direction === "UP") return "price-up";
  if (direction === "DOWN") return "price-down";
  return "";
}

function resultLabel(result?: string) {
  if (result === "WIN") return "盈利";
  if (result === "LOSS") return "亏损";
  return result || "-";
}

function signalOutcomeLabel(result?: string | null) {
  if (!result) return "待结算";
  if (result.toLowerCase() === "win") return "赢";
  if (result.toLowerCase() === "loss") return "输";
  return result;
}

function resultTone(result?: string | null) {
  if (result?.toLowerCase() === "win") return "price-up";
  if (result?.toLowerCase() === "loss") return "price-down";
  return "";
}

function shortTime(value?: string | null) {
  if (!value) return "-";
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

function secondsSince(value?: string | null) {
  if (!value) return Number.POSITIVE_INFINITY;
  const ms = Date.now() - new Date(value).getTime();
  if (!Number.isFinite(ms)) return Number.POSITIVE_INFINITY;
  return Math.max(0, Math.floor(ms / 1000));
}

function ageLabel(value?: string | null) {
  const seconds = secondsSince(value);
  if (!Number.isFinite(seconds)) return "-";
  if (seconds < 60) return `${seconds}秒前`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}分钟前`;
  const hours = Math.floor(minutes / 60);
  const restMinutes = minutes % 60;
  if (hours < 24) return `${hours}小时${restMinutes}分钟前`;
  const days = Math.floor(hours / 24);
  return `${days}天前`;
}

function pageStateFresh(state: PageStateRecord) {
  return secondsSince(state.updated_at) <= 10;
}

function pageStateLiveReady(state: PageStateRecord) {
  return (
    pageStateFresh(state) &&
    state.page_ready &&
    state.screen_name === "event_contract" &&
    state.amount_field_found &&
    state.up_button_found &&
    state.down_button_found
  );
}

function shortId(value?: string | null, size = 10) {
  if (!value) return "-";
  if (value.length <= size * 2 + 3) return value;
  return `${value.slice(0, size)}...${value.slice(-size)}`;
}

function percent(value?: string) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return value || "-";
  return `${(parsed * 100).toFixed(0)}%`;
}

function percentValue(value: unknown, digits = 1) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return "-";
  return `${(parsed * 100).toFixed(digits)}%`;
}

function formatNumber(value?: string | null) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return value || "-";
  return parsed.toLocaleString("zh-CN", { maximumFractionDigits: 4 });
}

function formatMetric(value: unknown, digits = 4) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return "-";
  return parsed.toFixed(digits);
}

function snapshotValue(snapshot: Record<string, unknown>, path: string[]) {
  let current: unknown = snapshot;
  for (const key of path) {
    if (!current || typeof current !== "object" || !(key in current)) return undefined;
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function predictionMetrics(snapshot: Record<string, unknown>) {
  return {
    modelScore: formatMetric(snapshotValue(snapshot, ["scores", "model_score"])),
    rawConfidence: percentValue(snapshotValue(snapshot, ["scores", "raw_confidence"])),
    technicalScore: formatMetric(snapshotValue(snapshot, ["scores", "technical_score"])),
    probabilityScore: formatMetric(snapshotValue(snapshot, ["scores", "probability_score"])),
    technicalWeight: percentValue(snapshotValue(snapshot, ["scores", "technical_weight"])),
    probabilityWeight: percentValue(snapshotValue(snapshot, ["scores", "probability_weight"])),
    upProbability: percentValue(snapshotValue(snapshot, ["expiry_model", "up_probability"])),
    downProbability: percentValue(snapshotValue(snapshot, ["expiry_model", "down_probability"])),
    upStructure: percentValue(snapshotValue(snapshot, ["structure", "up_score"])),
    downStructure: percentValue(snapshotValue(snapshot, ["structure", "down_score"])),
    candidateMode: String(snapshotValue(snapshot, ["decision", "candidate_mode"]) || "-"),
    candidateRule: String(snapshotValue(snapshot, ["decision", "candidate_rule"]) || "-"),
    effectiveMinConfidence: percentValue(snapshotValue(snapshot, ["config", "effective_min_confidence"])),
    edgeProbability: percentValue(snapshotValue(snapshot, ["expiry_model", "edge_probability"])),
    expectedExpiryReturn: percentValue(snapshotValue(snapshot, ["expiry_model", "expected_expiry_return"]), 3),
    expiryVolatility: percentValue(snapshotValue(snapshot, ["expiry_model", "expiry_volatility"]), 3),
    volatilityFloor: percentValue(snapshotValue(snapshot, ["expiry_model", "volatility_floor"]), 3),
    strikeReference: String(snapshotValue(snapshot, ["expiry_model", "strike_reference"]) || "-")
  };
}

function accountMatchText(status?: string | null) {
  if (status === "matched") return "一致";
  if (status === "mismatch") return "疑似不一致";
  if (status === "observed_balance_missing") return "未识别余额";
  if (status === "exchange_balance_missing") return "未绑定API余额";
  if (status === "exchange_balance_stale") return "等待余额刷新";
  return "未校验";
}

function duration(seconds: number) {
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours}小时${minutes}分钟`;
  return `${minutes}分钟`;
}

function yesNo(value: boolean) {
  return <StatusPill active={value}>{value ? "是" : "否"}</StatusPill>;
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
