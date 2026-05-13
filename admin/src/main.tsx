import { StrictMode, useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  ArrowDownRight,
  ArrowUpRight,
  CircleDollarSign,
  Radio,
  RefreshCw,
  Send,
  ShieldCheck,
  Smartphone,
  Users
} from "lucide-react";
import "./styles.css";

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
    latest_signal?: Signal;
  };
  users_total: number;
  ready_pages: number;
  orders: OrderRecord[];
  reports: ReportRecord[];
  settlements: SettlementRecord[];
};

type Signal = {
  id: string;
  symbol: string;
  direction: string;
  confidence: string;
  suggested_stake: string;
  fee_reserve_usdt: string;
  authorization_status: string;
  cycle_end: string;
};

type OrderRecord = {
  id: string;
  signal_id: string;
  source: string;
  symbol: string;
  direction: string;
  stake_usdt: string;
  order_status: string;
  created_at: string;
};

type ReportRecord = {
  id: string;
  signal_id: string;
  result: string;
  direction: string;
  stake_usdt: string;
  gross_profit?: string;
  report_status: string;
  created_at: string;
};

type SettlementRecord = {
  id: string;
  gross_profit: string;
  profit_share_fee: string;
  loss_rebate: string;
  user_net_profit: string;
  status: string;
  created_at: string;
};

const API_BASE_KEY = "event_signal_admin_api_base";
const TOKEN_KEY = "event_signal_admin_token";

function App() {
  const [apiBase, setApiBase] = useState(localStorage.getItem(API_BASE_KEY) || "http://127.0.0.1:8787");
  const [token, setToken] = useState(localStorage.getItem(TOKEN_KEY) || "");
  const [overview, setOverview] = useState<AdminOverview | null>(null);
  const [direction, setDirection] = useState<"UP" | "DOWN">("DOWN");
  const [stake, setStake] = useState("5");
  const [confidence, setConfidence] = useState("1");
  const [status, setStatus] = useState("未连接");
  const latest = overview?.stats.latest_signal;

  const headers = useMemo(
    () => ({
      "content-type": "application/json",
      "x-admin-token": token
    }),
    [token]
  );

  async function loadOverview() {
    localStorage.setItem(API_BASE_KEY, apiBase);
    localStorage.setItem(TOKEN_KEY, token);
    setStatus("刷新中");
    const res = await fetch(`${apiBase.replace(/\/$/, "")}/v1/admin/overview`, { headers });
    if (!res.ok) {
      setStatus(`连接失败 ${res.status}`);
      throw new Error(await res.text());
    }
    setOverview(await res.json());
    setStatus("已连接");
  }

  async function emitSignal() {
    setStatus("发送信号中");
    const res = await fetch(`${apiBase.replace(/\/$/, "")}/v1/admin/signals/emit`, {
      method: "POST",
      headers,
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
    await loadOverview();
    setStatus("信号已发送一次");
  }

  async function settleReport(reportId: string) {
    setStatus("结算审核中");
    const res = await fetch(`${apiBase.replace(/\/$/, "")}/v1/admin/execution-reports/settle`, {
      method: "POST",
      headers,
      body: JSON.stringify({ report_id: reportId })
    });
    if (!res.ok) {
      setStatus(`结算失败 ${res.status}`);
      return;
    }
    await loadOverview();
    setStatus("结算完成");
  }

  useEffect(() => {
    loadOverview().catch(() => undefined);
    const id = window.setInterval(() => loadOverview().catch(() => undefined), 5000);
    return () => window.clearInterval(id);
  }, []);

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
          <a className="active"><Activity size={18} /> 总览</a>
          <a><Radio size={18} /> 信号</a>
          <a><Users size={18} /> 用户</a>
          <a><ShieldCheck size={18} /> 结算</a>
        </nav>
      </aside>

      <main>
        <header className="topbar">
          <div>
            <h1>自动化闸门监控</h1>
            <p>WebSocket 单次下发，页面闸门、余额预扣、执行回报和结算状态集中查看。</p>
          </div>
          <button className="icon-button" onClick={() => loadOverview().catch(() => undefined)}>
            <RefreshCw size={18} />
            刷新
          </button>
        </header>

        <section className="connection">
          <label>
            后端地址
            <input value={apiBase} onChange={(e) => setApiBase(e.target.value)} />
          </label>
          <label>
            管理令牌
            <input value={token} onChange={(e) => setToken(e.target.value)} />
          </label>
          <span className="status">{status}</span>
        </section>

        <section className="stats-grid">
          <Stat icon={<Users />} label="用户" value={overview?.stats.users_total ?? 0} />
          <Stat icon={<Smartphone />} label="页面就绪" value={overview?.ready_pages ?? 0} />
          <Stat icon={<Send />} label="信号" value={overview?.stats.signals_total ?? 0} />
          <Stat icon={<CircleDollarSign />} label="订单" value={overview?.stats.orders_total ?? 0} />
        </section>

        <section className="split">
          <div className="panel signal-panel">
            <div className="panel-title">
              <div>
                <h2>发送测试信号</h2>
                <p>只发一次，不做补偿重放。</p>
              </div>
            </div>
            <div className="trade-buttons">
              <button className={direction === "UP" ? "up active" : "up"} onClick={() => setDirection("UP")}>
                <ArrowUpRight size={18} /> 看涨
              </button>
              <button className={direction === "DOWN" ? "down active" : "down"} onClick={() => setDirection("DOWN")}>
                <ArrowDownRight size={18} /> 看跌
              </button>
            </div>
            <div className="form-row">
              <label>
                金额 USDT
                <input value={stake} onChange={(e) => setStake(e.target.value)} />
              </label>
              <label>
                置信度
                <input value={confidence} onChange={(e) => setConfidence(e.target.value)} />
              </label>
            </div>
            <button className="primary" onClick={emitSignal}>
              <Send size={18} />
              下发信号
            </button>
          </div>

          <div className="panel">
            <div className="panel-title">
              <div>
                <h2>最新信号</h2>
                <p>展示后台最后一次生成的信号。</p>
              </div>
            </div>
            <div className="latest">
              <strong>{latest?.symbol || "BTCUSDT"}</strong>
              <span className={latest?.direction === "UP" ? "price-up" : latest?.direction === "DOWN" ? "price-down" : ""}>
                {labelDirection(latest?.direction)}
              </span>
              <dl>
                <div><dt>建议金额</dt><dd>{latest?.suggested_stake || "-"} USDT</dd></div>
                <div><dt>预扣</dt><dd>{latest?.fee_reserve_usdt || "-"} USDT</dd></div>
                <div><dt>状态</dt><dd>{latest?.authorization_status || "-"}</dd></div>
              </dl>
            </div>
          </div>
        </section>

        <section className="panel">
          <div className="panel-title">
            <div>
              <h2>执行记录</h2>
              <p>Android 点击确认后会上报订单，结算回报再进入扣费/返还流程。</p>
            </div>
          </div>
          <Table
            columns={["时间", "方向", "金额", "来源", "状态"]}
            rows={(overview?.orders || []).map((order) => [
              shortTime(order.created_at),
              labelDirection(order.direction),
              `${order.stake_usdt} USDT`,
              order.source,
              order.order_status
            ])}
          />
        </section>

        <section className="panel">
          <div className="panel-title">
            <div>
              <h2>待审核回报</h2>
              <p>执行结果需人工确认后才会扣费或返还。</p>
            </div>
          </div>
          <Table
            columns={["时间", "方向", "金额", "结果", "毛盈利", "状态", "操作"]}
            rows={(overview?.reports || []).map((report) => [
              shortTime(report.created_at),
              labelDirection(report.direction),
              `${report.stake_usdt} USDT`,
              report.result,
              report.gross_profit || "-",
              report.report_status,
              report.report_status === "pending_review"
                ? <button className="table-action" onClick={() => settleReport(report.id)}>确认结算</button>
                : <span>-</span>
            ])}
          />
        </section>

        <section className="panel">
          <div className="panel-title">
            <div>
              <h2>结算</h2>
              <p>赢单从盈利扣 30%，输单按本金的 30% 返到平台余额。</p>
            </div>
          </div>
          <Table
            columns={["时间", "毛盈利", "平台扣费", "亏损返还", "状态"]}
            rows={(overview?.settlements || []).map((row) => [
              shortTime(row.created_at),
              row.gross_profit,
              row.profit_share_fee,
              row.loss_rebate,
              row.status
            ])}
          />
        </section>
      </main>
    </div>
  );
}

function Stat({ icon, label, value }: { icon: React.ReactNode; label: string; value: number }) {
  return (
    <div className="stat">
      <div className="stat-icon">{icon}</div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function Table({ columns, rows }: { columns: string[]; rows: React.ReactNode[][] }) {
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

function labelDirection(direction?: string) {
  if (direction === "UP") return "看涨";
  if (direction === "DOWN") return "看跌";
  if (direction === "HOLD") return "观望";
  return "-";
}

function shortTime(value?: string) {
  if (!value) return "-";
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
