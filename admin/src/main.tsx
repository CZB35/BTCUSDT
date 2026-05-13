import { StrictMode, useEffect, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  ArrowDownRight,
  ArrowUpRight,
  CircleDollarSign,
  CreditCard,
  Radio,
  RefreshCw,
  Send,
  ShieldCheck,
  Smartphone,
  Users
} from "lucide-react";
import "./styles.css";

type AdminView = "overview" | "signals" | "users" | "deposits" | "withdrawals" | "orders" | "settlements";

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
  };
  ready_pages: number;
  orders: OrderRecord[];
  reports: ReportRecord[];
  settlements: SettlementRecord[];
};

type SignalRecord = {
  id: string;
  signal_id?: string;
  symbol: string;
  direction: string;
  confidence: string;
  suggested_stake: string;
  fee_reserve_usdt: string;
  authorization_status: string;
  trade_authorized?: boolean;
  cycle_end: string;
  model_version?: string;
};

type UserRecord = {
  id: string;
  wallet_address: string;
  status: string;
  active_device_fingerprint?: string | null;
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
  gross_profit?: string | null;
  report_status: string;
  created_at: string;
};

type SettlementRecord = {
  id: string;
  user_id: string;
  execution_report_id: string;
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
  observed_at: string;
  updated_at: string;
};

const TOKEN_KEY = "event_signal_admin_token";
const ADMIN_USER_KEY = "event_signal_admin_user";
const API_BASE = (import.meta.env.VITE_API_BASE_URL || "http://127.0.0.1:8787").trim().replace(/\/$/, "");

function App() {
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
  const [direction, setDirection] = useState<"UP" | "DOWN">("DOWN");
  const [stake, setStake] = useState("5");
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
    const [nextOverview, nextSignals, nextUsers, nextDeposits, nextWithdrawals, nextLedger, nextOrders, nextReports, nextSettlements, nextPageStates] =
      await Promise.all([
        adminFetch<AdminOverview>("/v1/admin/overview", nextToken),
        adminFetch<SignalRecord[]>("/v1/admin/signals", nextToken),
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
    if (nextSignals) setSignals(nextSignals);
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
        suggested_stake: stake,
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
        </section>

        {view === "overview" && (
          <OverviewPage overview={overview} latest={latest} reports={reports} onSettle={settleReport} />
        )}
        {view === "signals" && (
          <SignalsPage
            direction={direction}
            stake={stake}
            confidence={confidence}
            signals={signals}
            onDirectionChange={setDirection}
            onStakeChange={setStake}
            onConfidenceChange={setConfidence}
            onEmit={emitSignal}
          />
        )}
        {view === "users" && <UsersPage users={users} pageStates={pageStates} />}
        {view === "deposits" && <DepositsPage deposits={deposits} ledger={ledger} />}
        {view === "withdrawals" && <WithdrawalsPage withdrawals={withdrawals} onConfirm={confirmWithdrawal} onReject={rejectWithdrawal} />}
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
  stake,
  confidence,
  signals,
  onDirectionChange,
  onStakeChange,
  onConfidenceChange,
  onEmit
}: {
  direction: "UP" | "DOWN";
  stake: string;
  confidence: string;
  signals: SignalRecord[];
  onDirectionChange: (direction: "UP" | "DOWN") => void;
  onStakeChange: (stake: string) => void;
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
            金额 USDT
            <input value={stake} onChange={(event) => onStakeChange(event.target.value)} />
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
        <Table
          columns={["时间", "交易对", "方向", "金额", "置信度", "预扣", "状态"]}
          rows={signals.map((signal) => [
            shortTime(signal.cycle_end),
            signal.symbol,
            <ToneText tone={directionTone(signal.direction)}>{labelDirection(signal.direction)}</ToneText>,
            `${signal.suggested_stake || "-"} USDT`,
            percent(signal.confidence),
            `${signal.fee_reserve_usdt || "-"} USDT`,
            signal.authorization_status || "-"
          ])}
        />
      </section>
    </>
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
        <Table
          columns={["注册时间", "钱包地址", "状态", "当前设备", "更新时间"]}
          rows={users.map((user) => [
            shortTime(user.created_at),
            shortId(user.wallet_address, 10),
            user.status,
            user.active_device_fingerprint ? shortId(user.active_device_fingerprint, 14) : "-",
            shortTime(user.updated_at)
          ])}
        />
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>页面闸门</h2>
            <p>Android 上报的事件合约页面识别状态。</p>
          </div>
        </div>
        <Table
          columns={["用户", "页面", "识别", "金额框", "上涨", "下跌", "包名", "更新时间"]}
          rows={pageStates.map((state) => [
            shortId(state.user_id, 8),
            state.screen_name,
            <StatusPill active={state.page_ready}>{state.page_ready ? "就绪" : "未就绪"}</StatusPill>,
            yesNo(state.amount_field_found),
            yesNo(state.up_button_found),
            yesNo(state.down_button_found),
            state.package_name,
            shortTime(state.updated_at)
          ])}
        />
      </section>
    </>
  );
}

function OrdersPage({
  orders,
  reports,
  onSettle
}: {
  orders: OrderRecord[];
  reports: ReportRecord[];
  onSettle: (reportId: string) => void;
}) {
  return (
    <>
      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>执行订单</h2>
            <p>Android 点击确认后上报的订单记录。</p>
          </div>
        </div>
        <Table
          columns={["时间", "用户", "方向", "金额", "来源", "状态", "信号"]}
          rows={orders.map((order) => [
            shortTime(order.created_at),
            shortId(order.user_id, 8),
            <ToneText tone={directionTone(order.direction)}>{labelDirection(order.direction)}</ToneText>,
            `${order.stake_usdt} USDT`,
            order.source,
            order.order_status,
            shortId(order.signal_id, 8)
          ])}
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
        <Table
          columns={["时间", "用户", "金额", "状态", "确认数", "From", "To", "Tx"]}
          rows={deposits.map((deposit) => [
            shortTime(deposit.created_at),
            shortId(deposit.user_id, 8),
            `${deposit.amount} ${deposit.asset}`,
            <StatusPill active={deposit.status === "credited"}>{deposit.status}</StatusPill>,
            deposit.confirmations,
            shortId(deposit.from_address, 8),
            shortId(deposit.to_address, 8),
            shortId(deposit.tx_hash, 8)
          ])}
        />
      </section>

      <section className="panel">
        <div className="panel-title">
          <div>
            <h2>余额流水</h2>
            <p>所有余额变化都需要对应 ledger 记录。</p>
          </div>
        </div>
        <Table
          columns={["时间", "用户", "类型", "金额", "可用余额", "冻结余额", "引用"]}
          rows={ledger.map((row) => [
            shortTime(row.created_at),
            shortId(row.user_id, 8),
            row.entry_type,
            row.amount,
            row.balance_available_after,
            row.balance_frozen_after,
            `${row.ref_type}:${shortId(row.ref_id, 6)}`
          ])}
        />
      </section>
    </>
  );
}

function WithdrawalsPage({
  withdrawals,
  onConfirm,
  onReject
}: {
  withdrawals: WithdrawalRecord[];
  onConfirm: (withdrawalId: string) => void;
  onReject: (withdrawalId: string) => void;
}) {
  return (
    <section className="panel">
      <div className="panel-title">
        <div>
          <h2>提现审核</h2>
          <p>用户申请后先冻结余额，管理员确认链上退款后再释放。</p>
        </div>
      </div>
      <Table
        columns={["时间", "用户", "金额", "状态", "退款ID", "目标地址", "交易哈希", "操作"]}
        rows={withdrawals.map((row) => [
          shortTime(row.requested_at),
          shortId(row.user_id, 8),
          `${row.amount} ${row.asset}`,
          <StatusPill active={row.status === "paid"}>{row.status}</StatusPill>,
          shortId(row.refund_id, 8),
          shortId(row.to_address, 8),
          row.tx_hash ? shortId(row.tx_hash, 8) : "-",
          row.status === "pending_review" || row.status === "confirming" ? (
            <div className="row-actions">
              <button className="table-action" onClick={() => onConfirm(row.id)}>确认</button>
              <button className="table-action secondary-action" onClick={() => onReject(row.id)}>驳回</button>
            </div>
          ) : (
            <span>-</span>
          )
        ])}
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
      <Table
        columns={["时间", "用户", "方向", "金额", "结果", "毛盈利", "状态", "操作"]}
        rows={reports.map((report) => [
          shortTime(report.created_at),
          shortId(report.user_id, 8),
          <ToneText tone={directionTone(report.direction)}>{labelDirection(report.direction)}</ToneText>,
          `${report.stake_usdt} USDT`,
          resultLabel(report.result),
          report.gross_profit || "-",
          <StatusPill active={report.report_status === "settled"}>{report.report_status}</StatusPill>,
          report.report_status === "pending_review"
            ? <button className="table-action" onClick={() => onSettle(report.id)}>确认结算</button>
            : <span>-</span>
        ])}
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
      <Table
        columns={["时间", "用户", "毛盈利", "平台扣费", "亏损返还", "用户净额", "状态"]}
        rows={settlements.map((row) => [
          shortTime(row.created_at),
          shortId(row.user_id, 8),
          row.gross_profit,
          row.profit_share_fee,
          row.loss_rebate,
          row.user_net_profit,
          row.status
        ])}
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

function Table({ columns, rows }: { columns: string[]; rows: ReactNode[][] }) {
  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>{columns.map((col) => <th key={col}>{col}</th>)}</tr>
        </thead>
        <tbody>
          {rows.length === 0 ? (
            <tr><td colSpan={columns.length}>暂无记录</td></tr>
          ) : rows.map((row, index) => (
            <tr key={index}>{row.map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}</tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function StatusPill({ active, children }: { active: boolean; children: ReactNode }) {
  return <span className={active ? "pill active" : "pill"}>{children}</span>;
}

function ToneText({ tone, children }: { tone: string; children: ReactNode }) {
  return <span className={tone}>{children}</span>;
}

function viewTitle(view: AdminView) {
  if (view === "signals") return "信号管理";
  if (view === "users") return "用户与页面闸门";
  if (view === "deposits") return "充值与流水";
  if (view === "orders") return "执行与回报";
  if (view === "settlements") return "结算管理";
  return "自动化闸门监控";
}

function viewSubtitle(view: AdminView) {
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

function shortTime(value?: string) {
  if (!value) return "-";
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
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
