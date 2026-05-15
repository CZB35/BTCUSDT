import { StrictMode, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { ethers } from "ethers";
import { createAppKit, useAppKit, useAppKitAccount, useAppKitProvider } from "@reown/appkit/react";
import { EthersAdapter } from "@reown/appkit-adapter-ethers";
import { ArrowDownToLine, ArrowUpFromLine, Copy, CreditCard, Home, KeyRound, Link2, LogIn, RefreshCw, Share2, UserPlus, Wallet } from "lucide-react";
import "./styles.css";

type Tab = "home" | "deposit" | "trade" | "referral" | "withdraw" | "security";

type Balance = {
  asset: string;
  available: string;
  frozen: string;
  activation_status: string;
};

type DepositInstructions = {
  token_contract: string;
  vault_contract: string;
  settlement_wallet: string;
  account_id: string;
  deposit_id: string;
  bound_wallet?: string | null;
  first_deposit_min_amount: string;
  deposit_fee_rate: string;
  credited_rate: string;
};

type Deposit = {
  id: string;
  amount: string;
  credited_amount: string;
  platform_fee: string;
  status: string;
  confirmations: number;
  tx_hash: string;
  created_at: string;
};

type Withdrawal = {
  id: string;
  amount: string;
  status: string;
  tx_hash?: string | null;
  to_address: string;
  requested_at: string;
};

type Order = {
  id: string;
  symbol: string;
  direction: string;
  stake_usdt: string;
  order_status: string;
  created_at: string;
};

type Report = {
  id: string;
  direction: string;
  stake_usdt: string;
  result: string;
  gross_profit?: string | null;
  report_status: string;
  created_at: string;
};

type Settlement = {
  id: string;
  gross_profit: string;
  profit_share_fee: string;
  loss_rebate: string;
  user_net_profit: string;
  status: string;
  created_at: string;
};

type Ledger = {
  id: string;
  entry_type: string;
  amount: string;
  balance_available_after: string;
  created_at: string;
};

type ReferralReward = {
  id: string;
  level: number;
  rate: string;
  base_profit: string;
  amount: string;
  created_at: string;
};

type ReferralSummary = {
  referral_code: string;
  invited_by_code?: string | null;
  direct_count: number;
  indirect_count: number;
  direct_rate: string;
  indirect_rate: string;
  total_reward: string;
  rewards: ReferralReward[];
};

type RecordRow = {
  id: string;
  main: ReactNode;
  value: ReactNode;
  sub: ReactNode;
  extra: ReactNode;
  tone?: string;
  details?: Record<string, ReactNode>;
};

const TOKEN_KEY = "event_signal_access_token";
const PENDING_DEPOSIT_KEY = "event_signal_pending_deposit";
const API_BASE = (import.meta.env.VITE_API_BASE_URL || "").trim().replace(/\/$/, "");
const projectId = import.meta.env.VITE_REOWN_PROJECT_ID || "00000000000000000000000000000000";
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
  projectId,
  metadata: {
    name: "事件合约",
    description: "事件合约充值与账户中心",
    url: typeof window === "undefined" ? "https://localhost" : window.location.origin,
    icons: []
  }
});

const erc20Abi = ["function approve(address spender,uint256 value) external returns (bool)"];
const vaultAbi = ["function deposit(bytes32 accountId,bytes32 depositId,uint256 amount) external"];

type PendingDeposit = {
  txHash: string;
  amount: string;
  depositId: string;
};

function App() {
  const { open } = useAppKit();
  const { address, isConnected } = useAppKitAccount();
  const { walletProvider } = useAppKitProvider("eip155");
  const [tab, setTab] = useState<Tab>("home");
  const [token, setToken] = useState(localStorage.getItem(TOKEN_KEY) || "");
  const [password, setPassword] = useState("");
  const [referralCode, setReferralCode] = useState(new URLSearchParams(location.search).get("ref") || "");
  const [status, setStatus] = useState("未登录");
  const [balance, setBalance] = useState<Balance | null>(null);
  const [instructions, setInstructions] = useState<DepositInstructions | null>(null);
  const [deposits, setDeposits] = useState<Deposit[]>([]);
  const [withdrawals, setWithdrawals] = useState<Withdrawal[]>([]);
  const [orders, setOrders] = useState<Order[]>([]);
  const [reports, setReports] = useState<Report[]>([]);
  const [settlements, setSettlements] = useState<Settlement[]>([]);
  const [ledger, setLedger] = useState<Ledger[]>([]);
  const [referral, setReferral] = useState<ReferralSummary | null>(null);
  const [amount, setAmount] = useState("");
  const [txHash, setTxHash] = useState(() => loadPendingDeposit()?.txHash || "");
  const [withdrawAmount, setWithdrawAmount] = useState("");
  const [withdrawAddress, setWithdrawAddress] = useState("");
  const [newPassword, setNewPassword] = useState("");

  const shortAddress = useMemo(() => shortId(address || ""), [address]);
  const loggedIn = token.trim().length > 0;

  function headers(nextToken = token) {
    return {
      "content-type": "application/json",
      ...(nextToken ? { authorization: `Bearer ${nextToken}` } : {})
    };
  }

  async function request<T>(path: string, options: RequestInit = {}, nextToken = token): Promise<T> {
    const res = await fetch(`${API_BASE}${path}`, { ...options, headers: { ...headers(nextToken), ...(options.headers || {}) } });
    if (!res.ok) {
      const text = await res.text();
      throw new Error(friendlyErrorFromBody(text, res.status));
    }
    return res.json();
  }

  async function login(path: "/v1/auth/login" | "/v1/auth/register") {
    if (!address) {
      setStatus("请先连接钱包");
      await open({ view: "Connect" });
      return;
    }
    if (password.trim().length < 6) {
      setStatus("密码至少 6 位");
      return;
    }
    setStatus(path === "/v1/auth/login" ? "正在登录" : "正在注册");
    const data = await request<{ access_token: string }>(path, {
      method: "POST",
      body: JSON.stringify({
        wallet_address: address,
        password,
        referral_code: referralCode.trim() || null,
        device_fingerprint: `dapp-${navigator.userAgent.slice(0, 96)}`,
        client_type: "dapp"
      })
    });
    localStorage.setItem(TOKEN_KEY, data.access_token);
    setToken(data.access_token);
    setStatus("已登录");
    await loadAll(data.access_token);
  }

  async function bindWallet() {
    if (!address || !walletProvider) return;
    const challenge = await request<{ message: string }>("/v1/wallets/bind-message", {
      method: "POST",
      body: JSON.stringify({ chain: "bsc", address })
    });
    const provider = new ethers.BrowserProvider(walletProvider as ethers.Eip1193Provider);
    const signer = await provider.getSigner();
    const signature = await signer.signMessage(challenge.message);
    await request("/v1/wallets/verify", {
      method: "POST",
      body: JSON.stringify({ chain: "bsc", address, message: challenge.message, signature })
    });
    setStatus("钱包已绑定");
    await loadAll();
  }

  async function changePassword() {
    if (!address || !walletProvider) {
      setStatus("请先连接钱包");
      await open({ view: "Connect" });
      return;
    }
    if (newPassword.trim().length < 6) {
      setStatus("新密码至少 6 位");
      return;
    }
    setStatus("请在钱包中签名确认");
    const challenge = await request<{ message: string }>("/v1/auth/change-password-message", {
      method: "POST",
      body: JSON.stringify({ chain: "bsc", address })
    });
    const provider = new ethers.BrowserProvider(walletProvider as ethers.Eip1193Provider);
    const signer = await provider.getSigner();
    const signature = await signer.signMessage(challenge.message);
    await request("/v1/auth/change-password", {
      method: "POST",
      body: JSON.stringify({
        chain: "bsc",
        address,
        message: challenge.message,
        signature,
        new_password: newPassword.trim()
      })
    });
    localStorage.removeItem(TOKEN_KEY);
    setToken("");
    setPassword("");
    setNewPassword("");
    setStatus("密码已修改，请重新登录");
  }

  async function loadAll(nextToken = token) {
    if (!nextToken) return;
    const [nextBalance, nextInstructions, nextDeposits, nextWithdrawals, nextOrders, nextReports, nextSettlements, nextLedger, nextReferral] =
      await Promise.all([
        request<Balance>("/v1/balance", {}, nextToken),
        request<DepositInstructions>("/v1/deposits/instructions", {}, nextToken),
        request<Deposit[]>("/v1/deposits", {}, nextToken),
        request<Withdrawal[]>("/v1/withdrawals", {}, nextToken),
        request<Order[]>("/v1/event-contract-orders", {}, nextToken),
        request<Report[]>("/v1/execution-reports", {}, nextToken),
        request<Settlement[]>("/v1/settlements", {}, nextToken),
        request<Ledger[]>("/v1/ledger", {}, nextToken),
        request<ReferralSummary>("/v1/referrals/summary", {}, nextToken)
      ]);
    setBalance(nextBalance);
    setInstructions(nextInstructions);
    setDeposits(nextDeposits);
    setWithdrawals(nextWithdrawals);
    setOrders(nextOrders);
    setReports(nextReports);
    setSettlements(nextSettlements);
    setLedger(nextLedger);
    setReferral(nextReferral);
    setStatus("已同步");
  }

  async function sendDeposit() {
    if (!walletProvider || !address) throw new Error("请先连接钱包");
    const info = instructions || await request<DepositInstructions>("/v1/deposits/instructions");
    setInstructions(info);
    const provider = new ethers.BrowserProvider(walletProvider as ethers.Eip1193Provider);
    await provider.send("wallet_switchEthereumChain", [{ chainId: "0x38" }]);
    const signer = await provider.getSigner();
    const units = ethers.parseUnits(amount, 18);
    setStatus("正在授权 USDT");
    const tokenContract = new ethers.Contract(info.token_contract, erc20Abi, signer);
    await (await tokenContract.approve(info.vault_contract, units)).wait();
    setStatus("正在提交充值");
    const vault = new ethers.Contract(info.vault_contract, vaultAbi, signer);
    const tx = await vault.deposit(info.account_id, info.deposit_id, units);
    setTxHash(tx.hash);
    savePendingDeposit({ txHash: tx.hash, amount, depositId: info.deposit_id });
    await tx.wait();
    await submitTx(tx.hash, amount, info.deposit_id);
  }

  async function submitTx(nextTxHash = txHash, nextAmount = amount, depositId = "") {
    const pending = loadPendingDeposit();
    const finalTxHash = nextTxHash.trim() || pending?.txHash || "";
    const finalAmount = nextAmount.trim() || pending?.amount || "";
    const finalDepositId = depositId.trim() || (pending?.txHash === finalTxHash ? pending.depositId : "");
    const deposit = await request<Deposit>("/v1/deposits/submit-tx", {
      method: "POST",
      body: JSON.stringify({
        tx_hash: finalTxHash,
        amount: finalAmount || null,
        deposit_id: finalDepositId || null
      })
    });
    clearPendingDeposit(finalTxHash);
    setTxHash(finalTxHash);
    setStatus(`充值提交成功：${deposit.status}`);
    await loadAll();
  }

  async function requestWithdraw() {
    const row = await request<Withdrawal>("/v1/withdrawals", {
      method: "POST",
      body: JSON.stringify({ amount: withdrawAmount, to_address: withdrawAddress.trim() || null })
    });
    setStatus(`提现申请已提交：${row.status}`);
    await loadAll();
  }

  useEffect(() => {
    if (!token) return;
    loadAll().catch((err) => setStatus(cleanError(err)));
  }, [token]);

  useEffect(() => {
    if (address && !withdrawAddress) setWithdrawAddress(address);
  }, [address, withdrawAddress]);

  if (!loggedIn) {
    return (
      <AuthPage
        isConnected={isConnected}
        shortAddress={shortAddress}
        password={password}
        referralCode={referralCode}
        status={status}
        onPassword={setPassword}
        onReferralCode={setReferralCode}
        onConnect={() => open({ view: "Connect" })}
        onLogin={() => login("/v1/auth/login").catch((err) => setStatus(cleanError(err)))}
        onRegister={() => login("/v1/auth/register").catch((err) => setStatus(cleanError(err)))}
      />
    );
  }

  return (
    <div className="phone-shell">
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">ES</div>
          <div>
            <strong>事件合约</strong>
            <span>{shortAddress || "未连接钱包"}</span>
          </div>
        </div>
        <button className="status-chip" onClick={() => loadAll().catch((err) => setStatus(cleanError(err)))}>
          {status}
        </button>
      </header>

      {tab === "home" && (
        <HomePage
          balance={balance}
          isConnected={isConnected}
          onConnect={() => open({ view: "Connect" })}
          onBind={() => bindWallet().catch((err) => setStatus(cleanError(err)))}
          ledger={ledger}
        />
      )}
      {tab === "deposit" && (
        <DepositPage
          instructions={instructions}
          deposits={deposits}
          amount={amount}
          onAmount={setAmount}
          onRefresh={() => loadAll().catch((err) => setStatus(cleanError(err)))}
          onSend={() => sendDeposit().catch((err) => setStatus(cleanError(err)))}
        />
      )}
      {tab === "trade" && <TradePage orders={orders} reports={reports} settlements={settlements} />}
      {tab === "referral" && <ReferralPage referral={referral} />}
      {tab === "withdraw" && (
        <WithdrawPage
          withdrawals={withdrawals}
          amount={withdrawAmount}
          address={withdrawAddress}
          onAmount={setWithdrawAmount}
          onAddress={setWithdrawAddress}
          onSubmit={() => requestWithdraw().catch((err) => setStatus(cleanError(err)))}
        />
      )}
      {tab === "security" && (
        <SecurityPage
          newPassword={newPassword}
          onNewPassword={setNewPassword}
          onChangePassword={() => changePassword().catch((err) => setStatus(cleanError(err)))}
        />
      )}

      <nav className="tabbar">
        <TabButton active={tab === "home"} icon={<Home size={18} />} label="首页" onClick={() => setTab("home")} />
        <TabButton active={tab === "deposit"} icon={<ArrowDownToLine size={18} />} label="充值" onClick={() => setTab("deposit")} />
        <TabButton active={tab === "trade"} icon={<CreditCard size={18} />} label="交易" onClick={() => setTab("trade")} />
        <TabButton active={tab === "referral"} icon={<Share2 size={18} />} label="推荐" onClick={() => setTab("referral")} />
        <TabButton active={tab === "withdraw"} icon={<ArrowUpFromLine size={18} />} label="提现" onClick={() => setTab("withdraw")} />
        <TabButton active={tab === "security"} icon={<KeyRound size={18} />} label="安全" onClick={() => setTab("security")} />
      </nav>
    </div>
  );
}

function AuthPage({
  isConnected,
  shortAddress,
  password,
  referralCode,
  status,
  onPassword,
  onReferralCode,
  onConnect,
  onLogin,
  onRegister
}: {
  isConnected: boolean;
  shortAddress: string;
  password: string;
  referralCode: string;
  status: string;
  onPassword: (value: string) => void;
  onReferralCode: (value: string) => void;
  onConnect: () => void;
  onLogin: () => void;
  onRegister: () => void;
}) {
  const passwordReady = password.trim().length >= 6;
  return (
    <div className="phone-shell auth-shell">
      <section className="hero-card auth-card">
        <div className="brand auth-brand">
          <div className="brand-mark">ES</div>
          <div>
            <strong>事件合约</strong>
            <span>{isConnected ? shortAddress : "请先连接钱包"}</span>
          </div>
        </div>
        <button className={isConnected ? "secondary" : "primary"} onClick={onConnect}>
          <Wallet size={18} />
          {isConnected ? "切换钱包" : "连接钱包"}
        </button>
        <label>
          登录密码
          <input
            type="password"
            value={password}
            onChange={(event) => onPassword(event.target.value)}
            placeholder="至少 6 位"
            autoComplete="current-password"
          />
        </label>
        <label>
          邀请码
          <input value={referralCode} onChange={(event) => onReferralCode(event.target.value)} placeholder="没有可不填，注册时绑定" />
        </label>
        <div className="row">
          <button className="primary" disabled={!isConnected || !passwordReady} onClick={onLogin}>
            <LogIn size={18} />
            登录
          </button>
          <button className="secondary" disabled={!isConnected || !passwordReady} onClick={onRegister}>
            <UserPlus size={18} />
            注册
          </button>
        </div>
        <p className="auth-status">{status}</p>
      </section>
    </div>
  );
}

function HomePage({
  balance,
  isConnected,
  onConnect,
  onBind,
  ledger
}: {
  balance: Balance | null;
  isConnected: boolean;
  onConnect: () => void;
  onBind: () => void;
  ledger: Ledger[];
}) {
  return (
    <>
      <section className="hero-card">
        <div className="balance-line">
          <div>
            <p className="muted">可用额度</p>
            <strong>{balance?.available || "0"}U</strong>
          </div>
          <span className="status-chip">{balance?.activation_status === "active" ? "已激活" : "未激活"}</span>
        </div>
        <div className="metrics">
          <Metric label="冻结" value={`${balance?.frozen || "0"}U`} />
          <Metric label="资产" value={balance?.asset || "USDT"} />
          <Metric label="钱包" value={isConnected ? "已连接" : "未连接"} />
        </div>
        <div className="wallet-row">
          <button className={isConnected ? "secondary" : "primary"} onClick={onConnect}>
            <Wallet size={18} />
            {isConnected ? "切换钱包" : "连接钱包"}
          </button>
          <button className="secondary" onClick={onBind}><Link2 size={18} />签名绑定</button>
        </div>
      </section>

      <RecordList title="最近流水" rows={ledger.map((row) => ({
        id: row.id,
        main: row.entry_type,
        value: `${row.amount}U`,
        sub: shortTime(row.created_at),
        extra: `余额 ${row.balance_available_after}U`,
        details: {
          流水ID: row.id,
          类型: row.entry_type,
          金额: `${row.amount}U`,
          可用余额: `${row.balance_available_after}U`,
          时间: shortTime(row.created_at)
        }
      }))} />
    </>
  );
}

function DepositPage(props: {
  instructions: DepositInstructions | null;
  deposits: Deposit[];
  amount: string;
  onAmount: (value: string) => void;
  onRefresh: () => void;
  onSend: () => void;
}) {
  const feeRate = Number(props.instructions?.deposit_fee_rate || 0);
  const credited = Number(props.amount || 0) * (1 - feeRate);
  return (
    <>
      <section className="card">
        <div className="section-title">
          <h2>USDT 充值</h2>
          <button className="ghost" onClick={props.onRefresh}><RefreshCw size={16} />刷新</button>
        </div>
        <div className="metrics">
          <Metric label="首次激活" value={`${props.instructions?.first_deposit_min_amount || "100"}U`} />
          <Metric label="手续费" value={`${(feeRate * 100).toFixed(0)}%`} />
          <Metric label="预计到账" value={`${Number.isFinite(credited) ? credited.toFixed(2) : "0"}U`} />
        </div>
        <label>充值金额<input value={props.amount} onChange={(event) => props.onAmount(event.target.value)} placeholder="首次至少 100U" /></label>
        <button className="primary" onClick={props.onSend}>钱包充值</button>
      </section>
      <RecordList title="充值记录" rows={props.deposits.map((row) => ({
        id: row.id,
        main: `${row.amount}U`,
        value: row.status,
        sub: `到账 ${row.credited_amount}U / 手续费 ${row.platform_fee}U`,
        extra: shortTime(row.created_at),
        details: {
          充值ID: row.id,
          充值金额: `${row.amount}U`,
          到账金额: `${row.credited_amount}U`,
          平台手续费: `${row.platform_fee}U`,
          状态: row.status,
          确认数: row.confirmations,
          交易哈希: row.tx_hash,
          创建时间: shortTime(row.created_at)
        }
      }))} />
    </>
  );
}

function TradePage({ orders, reports, settlements }: { orders: Order[]; reports: Report[]; settlements: Settlement[] }) {
  return (
    <>
      <RecordList title="下单记录" rows={orders.map((row) => ({
        id: row.id,
        main: `${row.symbol} ${labelDirection(row.direction)}`,
        value: `${row.stake_usdt}U`,
        sub: row.order_status,
        extra: shortTime(row.created_at),
        tone: row.direction === "DOWN" ? "down" : "up",
        details: {
          订单ID: row.id,
          交易对: row.symbol,
          方向: labelDirection(row.direction),
          下单金额: `${row.stake_usdt}U`,
          状态: row.order_status,
          创建时间: shortTime(row.created_at)
        }
      }))} />
      <RecordList title="结算结果" rows={reports.map((row) => ({
        id: row.id,
        main: `${labelDirection(row.direction)} ${row.result}`,
        value: row.gross_profit ? `${row.gross_profit}U` : "-",
        sub: row.report_status,
        extra: shortTime(row.created_at),
        tone: row.result === "WIN" ? "up" : "down",
        details: {
          回报ID: row.id,
          方向: labelDirection(row.direction),
          下单金额: `${row.stake_usdt}U`,
          结果: row.result,
          毛盈利: row.gross_profit ? `${row.gross_profit}U` : "-",
          状态: row.report_status,
          创建时间: shortTime(row.created_at)
        }
      }))} />
      <RecordList title="平台结算" rows={settlements.map((row) => ({
        id: row.id,
        main: `净额 ${row.user_net_profit}U`,
        value: row.status,
        sub: `扣费 ${row.profit_share_fee}U / 亏损返还 ${row.loss_rebate}U`,
        extra: shortTime(row.created_at),
        details: {
          结算ID: row.id,
          毛盈利: `${row.gross_profit}U`,
          平台扣费: `${row.profit_share_fee}U`,
          亏损返还: `${row.loss_rebate}U`,
          用户净额: `${row.user_net_profit}U`,
          状态: row.status,
          创建时间: shortTime(row.created_at)
        }
      }))} />
    </>
  );
}

function ReferralPage({ referral }: { referral: ReferralSummary | null }) {
  const link = referral ? `${location.origin}${location.pathname}?ref=${referral.referral_code}` : "";
  return (
    <>
      <section className="hero-card">
        <p className="muted">我的邀请码</p>
        <div className="code-box">
          <span>{referral?.referral_code || "-"}</span>
          <button className="ghost" onClick={() => link && navigator.clipboard.writeText(link)}><Copy size={16} /></button>
        </div>
        <div className="metrics">
          <Metric label="直推人数" value={String(referral?.direct_count || 0)} />
          <Metric label="间推人数" value={String(referral?.indirect_count || 0)} />
          <Metric label="累计返佣" value={`${referral?.total_reward || "0"}U`} />
        </div>
        <p className="muted">直推盈利返佣 {percent(referral?.direct_rate)}，间推盈利返佣 {percent(referral?.indirect_rate)}，返佣直接进入平台可用额度。</p>
      </section>
      <RecordList title="返佣记录" rows={(referral?.rewards || []).map((row) => ({
        id: row.id,
        main: row.level === 1 ? "直推返佣" : "间推返佣",
        value: `${row.amount}U`,
        sub: `基准盈利 ${row.base_profit}U / 比例 ${percent(row.rate)}`,
        extra: shortTime(row.created_at),
        details: {
          返佣ID: row.id,
          层级: row.level === 1 ? "直推" : "间推",
          基准盈利: `${row.base_profit}U`,
          比例: percent(row.rate),
          返佣金额: `${row.amount}U`,
          创建时间: shortTime(row.created_at)
        }
      }))} />
    </>
  );
}

function WithdrawPage(props: {
  withdrawals: Withdrawal[];
  amount: string;
  address: string;
  onAmount: (value: string) => void;
  onAddress: (value: string) => void;
  onSubmit: () => void;
}) {
  return (
    <>
      <section className="card">
        <div className="section-title"><h2>申请退款</h2></div>
        <label>金额<input value={props.amount} onChange={(event) => props.onAmount(event.target.value)} placeholder="输入 USDT 金额" /></label>
        <label>到账钱包<input value={props.address} onChange={(event) => props.onAddress(event.target.value)} placeholder="默认绑定钱包" /></label>
        <button className="primary" onClick={props.onSubmit}>提交申请</button>
      </section>
      <RecordList title="提现记录" rows={props.withdrawals.map((row) => ({
        id: row.id,
        main: `${row.amount}U`,
        value: row.status,
        sub: shortId(row.to_address),
        extra: row.tx_hash ? shortId(row.tx_hash) : shortTime(row.requested_at),
        details: {
          提现ID: row.id,
          金额: `${row.amount}U`,
          状态: row.status,
          到账钱包: row.to_address,
          交易哈希: row.tx_hash || "-",
          申请时间: shortTime(row.requested_at)
        }
      }))} />
    </>
  );
}

function SecurityPage(props: {
  newPassword: string;
  onNewPassword: (value: string) => void;
  onChangePassword: () => void;
}) {
  return (
    <section className="card">
      <div className="section-title"><h2>账号安全</h2></div>
      <label>新密码<input type="password" value={props.newPassword} onChange={(event) => props.onNewPassword(event.target.value)} placeholder="至少 6 位" /></label>
      <button className="primary" onClick={props.onChangePassword}>钱包签名修改密码</button>
      <p className="muted">修改后 App 和 DApp 的旧登录会话都会失效，需要重新登录。</p>
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="metric"><span>{label}</span><strong>{value}</strong></div>;
}

function RecordList({ title, rows, pageSize = 8 }: { title: string; rows: RecordRow[]; pageSize?: number }) {
  const [page, setPage] = useState(0);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<RecordRow | null>(null);
  const normalizedQuery = query.trim().toLowerCase();
  const filteredRows = normalizedQuery
    ? rows.filter((row) =>
        row.id.toLowerCase().includes(normalizedQuery)
        || String(row.main).toLowerCase().includes(normalizedQuery)
        || String(row.value).toLowerCase().includes(normalizedQuery)
        || String(row.sub).toLowerCase().includes(normalizedQuery)
        || String(row.extra).toLowerCase().includes(normalizedQuery)
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
      <section className="list-card">
        <div className="list-row list-head">
          <h2>{title}</h2>
          <span>{filteredRows.length} 条</span>
        </div>
        <div className="list-search">
          <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索记录" />
        </div>
        {visibleRows.length === 0 ? <div className="empty">暂无记录</div> : visibleRows.map((row) => (
          <button className="list-row record-button" key={row.id} onClick={() => row.details && setSelected(row)}>
            <div className="list-row-main"><span className={row.tone || ""}>{row.main}</span><span>{row.value}</span></div>
            <div className="list-row-sub"><span>{row.sub}</span><span>{row.extra}</span></div>
          </button>
        ))}
        {filteredRows.length > pageSize && (
          <div className="pager">
            <button className="ghost" disabled={safePage === 0} onClick={() => setPage((value) => Math.max(0, value - 1))}>上一页</button>
            <button className="ghost" disabled={safePage >= totalPages - 1} onClick={() => setPage((value) => Math.min(totalPages - 1, value + 1))}>下一页</button>
          </div>
        )}
      </section>
      {selected?.details && <MobileDetails row={selected} onClose={() => setSelected(null)} />}
    </>
  );
}

function MobileDetails({ row, onClose }: { row: RecordRow; onClose: () => void }) {
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <section className="sheet" onClick={(event) => event.stopPropagation()}>
        <div className="sheet-handle" />
        <div className="section-title">
          <h2>记录详情</h2>
          <button className="ghost" onClick={onClose}>关闭</button>
        </div>
        <div className="detail-grid">
          {Object.entries(row.details || {}).map(([label, value]) => (
            <div key={label}>
              <span>{label}</span>
              <strong>{value}</strong>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function TabButton({ active, icon, label, onClick }: { active: boolean; icon: ReactNode; label: string; onClick: () => void }) {
  return <button className={active ? "active" : ""} onClick={onClick}>{icon}<span>{label}</span></button>;
}

function shortId(value?: string | null, size = 6) {
  if (!value) return "";
  return value.length > size * 2 + 3 ? `${value.slice(0, size)}...${value.slice(-size)}` : value;
}

function shortTime(value?: string | null) {
  if (!value) return "-";
  return new Date(value).toLocaleString("zh-CN", { hour12: false });
}

function labelDirection(value: string) {
  if (value === "UP") return "看涨";
  if (value === "DOWN") return "看跌";
  return value || "-";
}

function percent(value?: string | null) {
  const parsed = Number(value || 0);
  return `${(parsed * 100).toFixed(0)}%`;
}

function cleanError(err: unknown) {
  return err instanceof Error ? err.message.slice(0, 120) : "请求失败";
}

function friendlyErrorFromBody(body: string, status: number) {
  try {
    const parsed = JSON.parse(body) as { code?: string; message?: string };
    if (parsed.code === "AUTH_PASSWORD_WEAK") return "密码至少 6 位";
    if (parsed.code === "AUTH_INVALID_CREDENTIALS") return "账号或密码错误，未注册请点注册";
    if (parsed.code === "AUTH_ACCOUNT_EXISTS") return "该钱包已注册，请直接登录";
    if (parsed.code === "ACCOUNT_ALREADY_LOGGED_IN_ON_ANOTHER_DEVICE") return "该账号已在其他设备登录";
    if (parsed.code === "WALLET_NOT_BOUND") return "请先签名绑定钱包";
    if (parsed.message) return parsed.message;
  } catch {
    if (body.trim()) return body.trim();
  }
  return `请求失败 ${status}`;
}

function loadPendingDeposit(): PendingDeposit | null {
  try {
    const raw = localStorage.getItem(PENDING_DEPOSIT_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<PendingDeposit>;
    if (!parsed.txHash) return null;
    return {
      txHash: parsed.txHash,
      amount: parsed.amount || "",
      depositId: parsed.depositId || ""
    };
  } catch {
    return null;
  }
}

function savePendingDeposit(deposit: PendingDeposit) {
  localStorage.setItem(PENDING_DEPOSIT_KEY, JSON.stringify(deposit));
}

function clearPendingDeposit(txHash: string) {
  const pending = loadPendingDeposit();
  if (!pending || pending.txHash === txHash) {
    localStorage.removeItem(PENDING_DEPOSIT_KEY);
  }
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
