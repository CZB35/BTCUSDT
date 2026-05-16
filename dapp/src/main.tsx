import { StrictMode, useEffect, useMemo, useState, useCallback } from "react";
import type { ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { ethers } from "ethers";
import { createAppKit, useAppKit, useAppKitAccount, useAppKitProvider } from "@reown/appkit/react";
import { EthersAdapter } from "@reown/appkit-adapter-ethers";
import { 
  ArrowDownToLine, 
  Copy, 
  CreditCard, 
  Home, 
  KeyRound, 
  LogIn, 
  RefreshCw, 
  Share2, 
  UserPlus, 
  Wallet,
  Eye, 
  EyeOff, 
  Zap, 
  ChevronRight, 
  Activity 
} from "lucide-react";
import logo from "./assets/logo.png"; // 推荐将你的 logo.png 放入 src/assets/ 目录下
import "./styles.css";

// ================= 类型定义 =================
type Tab = "home" | "funds" | "trade" | "referral" | "security";
type AuthMode = "login" | "register";
type TradeStat = { count: number; stake: number; net: number };
type Balance = { asset: string; available: string; frozen: string; activation_status: string; };
type DepositInstructions = { token_contract: string; vault_contract: string; settlement_wallet: string; account_id: string; deposit_id: string; bound_wallet?: string | null; first_deposit_min_amount: string; deposit_fee_rate: string; credited_rate: string; };
type Deposit = { id: string; amount: string; credited_amount: string; platform_fee: string; status: string; confirmations: number; tx_hash: string; created_at: string; };
type Withdrawal = { id: string; amount: string; status: string; tx_hash?: string | null; to_address: string; requested_at: string; };
type Order = { id: string; symbol: string; direction: string; stake_usdt: string; order_status: string; created_at: string; };
type Report = { id: string; direction: string; stake_usdt: string; result: string; gross_profit?: string | null; report_status: string; created_at: string; };
type Settlement = { id: string; gross_profit: string; profit_share_fee: string; loss_rebate: string; user_net_profit: string; status: string; created_at: string; };
type Ledger = { id: string; entry_type: string; amount: string; balance_available_after: string; created_at: string; };
type ReferralReward = { id: string; level: number; rate: string; base_profit: string; amount: string; created_at: string; };
type ReferralSummary = { referral_code: string; invited_by_code?: string | null; direct_count: number; indirect_count: number; direct_rate: string; indirect_rate: string; total_reward: string; rewards: ReferralReward[]; };
type RecordRow = { id: string; main: ReactNode; value: ReactNode; sub: ReactNode; extra: ReactNode; tone?: string; details?: Record<string, ReactNode>; };
type PendingDeposit = { txHash: string; amount: string; depositId: string; };

// ================= 全局配置 =================
const TOKEN_KEY = "event_signal_access_token";
const PENDING_DEPOSIT_KEY = "event_signal_pending_deposit";
const API_BASE = (import.meta.env.VITE_API_BASE_URL || "").trim().replace(/\/$/, "");
const projectId = import.meta.env.VITE_REOWN_PROJECT_ID || "00000000000000000000000000000000";
const bsc = { id: 56, caipNetworkId: "eip155:56", chainNamespace: "eip155", name: "BNB Smart Chain", nativeCurrency: { name: "BNB", symbol: "BNB", decimals: 18 }, rpcUrls: { default: { http: ["https://bsc-dataseed.binance.org/"] } }, blockExplorers: { default: { name: "BscScan", url: "https://bscscan.com" } } } as const;

const appKit = createAppKit({
  adapters: [new EthersAdapter()],
  networks: [bsc],
  projectId,
  features: { swaps: false, onramp: false, receive: false, send: false, email: false, socials: false, history: false, analytics: false, allWallets: false },
  metadata: { name: "事件合约", description: "事件合约充值与账户中心", url: typeof window === "undefined" ? "https://localhost" : window.location.origin, icons: [] }
});

const erc20Abi = ["function approve(address spender,uint256 value) external returns (bool)"];
const vaultAbi = ["function deposit(bytes32 accountId,bytes32 depositId,uint256 amount) external"];

// ================= 自定义 Hook =================
function useAppLogic() {
  const [token, setToken] = useState(localStorage.getItem(TOKEN_KEY) || "");
  const [toast, setToast] = useState("");
  const [data, setData] = useState({
    balance: null as Balance | null,
    instructions: null as DepositInstructions | null,
    deposits: [] as Deposit[],
    withdrawals: [] as Withdrawal[],
    orders: [] as Order[],
    reports: [] as Report[],
    settlements: [] as Settlement[],
    ledger: [] as Ledger[],
    referral: null as ReferralSummary | null,
  });

  const notify = useCallback((message: string) => setToast(message), []);

  const clearAuth = useCallback((message: string) => {
    localStorage.removeItem(TOKEN_KEY);
    setToken("");
    setData({ balance: null, instructions: null, deposits: [], withdrawals: [], orders: [], reports: [], settlements: [], ledger: [], referral: null });
    notify(message);
  }, [notify]);

  const request = useCallback(async <T,>(path: string, options: RequestInit = {}, nextToken = token): Promise<T> => {
    const headers = { "content-type": "application/json", ...(nextToken ? { authorization: `Bearer ${nextToken}` } : {}) };
    const res = await fetch(`${API_BASE}${path}`, { ...options, headers: { ...headers, ...(options.headers || {}) } });
    if (!res.ok) {
      const text = await res.text();
      if (res.status === 401 && isAuthExpired(text)) clearAuth("登录已过期，请重新登录");
      throw new Error(friendlyErrorFromBody(text, res.status));
    }
    return res.json();
  }, [token, clearAuth]);

  const loadAll = useCallback(async (nextToken = token) => {
    if (!nextToken) return;
    try {
      const [balance, instructions, deposits, withdrawals, orders, reports, settlements, ledger, referral] = await Promise.all([
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
      setData({ balance, instructions, deposits, withdrawals, orders, reports, settlements, ledger, referral });
    } catch (err) {
      notify(cleanError(err));
    }
  }, [request, token, notify]);

  useEffect(() => { loadAll(); }, [loadAll]);

  return { token, setToken, toast, setToast, data, request, loadAll, notify, clearAuth };
}

// ================= 主应用 =================
function App() {
  const { open } = useAppKit();
  const { address, isConnected } = useAppKitAccount();
  const { walletProvider } = useAppKitProvider("eip155");
  const { token, setToken, toast, setToast, data, request, loadAll, notify } = useAppLogic();

  const [tab, setTab] = useState<Tab>("home");
  const [authMode, setAuthMode] = useState<AuthMode>("login");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [referralCode, setReferralCode] = useState(new URLSearchParams(location.search).get("ref") || "");
  
  const [amount, setAmount] = useState("");
  const [txHash, setTxHash] = useState(() => loadPendingDeposit()?.txHash || "");
  const [withdrawAmount, setWithdrawAmount] = useState("");
  const [withdrawAddress, setWithdrawAddress] = useState("");
  const [newPassword, setNewPassword] = useState("");

  const displayAddress = useMemo(() => address || "", [address]);
  const shortAddr = useMemo(() => shortId(address || ""), [address]);
  const loggedIn = token.trim().length > 0;

  useEffect(() => { if (address && !withdrawAddress) setWithdrawAddress(address); }, [address, withdrawAddress]);

  // --- 业务方法 ---
  async function handleLogin(path: "/v1/auth/login" | "/v1/auth/register") {
    if (!address) return notify("请先连接钱包"), open({ view: "Connect" });
    if (password.trim().length < 6) return notify("密码至少 6 位");
    if (path === "/v1/auth/register" && password !== confirmPassword) return notify("两次输入的密码不一致");
    
    notify(path === "/v1/auth/login" ? "正在登录..." : "正在注册...");
    try {
      const res = await request<{ access_token: string }>(path, {
        method: "POST",
        body: JSON.stringify({ wallet_address: address, password, referral_code: referralCode.trim() || null, device_fingerprint: `dapp-${navigator.userAgent.slice(0, 96)}`, client_type: "dapp" })
      });
      localStorage.setItem(TOKEN_KEY, res.access_token);
      setToken(res.access_token);
      notify("登录成功！");
      await loadAll(res.access_token);
    } catch (err) { notify(cleanError(err)); }
  }

  async function handleSendDeposit() {
    try {
      if (!walletProvider || !address) throw new Error("请先连接钱包");
      const info = data.instructions || await request<DepositInstructions>("/v1/deposits/instructions");
      const provider = new ethers.BrowserProvider(walletProvider as ethers.Eip1193Provider);
      await provider.send("wallet_switchEthereumChain", [{ chainId: "0x38" }]);
      const signer = await provider.getSigner();
      const units = ethers.parseUnits(amount, 18);
      
      notify("请在钱包中授权 USDT...");
      const tokenContract = new ethers.Contract(info.token_contract, erc20Abi, signer);
      await (await tokenContract.approve(info.vault_contract, units)).wait();
      
      notify("正在提交充值...");
      const vault = new ethers.Contract(info.vault_contract, vaultAbi, signer);
      const tx = await vault.deposit(info.account_id, info.deposit_id, units);
      
      setTxHash(tx.hash);
      savePendingDeposit({ txHash: tx.hash, amount, depositId: info.deposit_id });
      await tx.wait();
      
      const depositRes = await request<Deposit>("/v1/deposits/submit-tx", { method: "POST", body: JSON.stringify({ tx_hash: tx.hash, amount, deposit_id: info.deposit_id }) });
      clearPendingDeposit(tx.hash);
      notify(`充值提交成功：${depositRes.status}`);
      await loadAll();
    } catch (err) { notify(cleanError(err)); }
  }

  async function handleRequestWithdraw() {
    try {
      const row = await request<Withdrawal>("/v1/withdrawals", { method: "POST", body: JSON.stringify({ amount: withdrawAmount, to_address: withdrawAddress.trim() || null }) });
      notify(`提现申请已提交：${row.status}`);
      await loadAll();
    } catch (err) { notify(cleanError(err)); }
  }

  async function handleChangePassword() {
    try {
      if (!address || !walletProvider) return notify("请先连接钱包"), open({ view: "Connect" });
      if (newPassword.trim().length < 6) return notify("新密码至少 6 位");
      
      notify("请在钱包中签名确认...");
      const challenge = await request<{ message: string }>("/v1/auth/change-password-message", { method: "POST", body: JSON.stringify({ chain: "bsc", address }) });
      const provider = new ethers.BrowserProvider(walletProvider as ethers.Eip1193Provider);
      const signer = await provider.getSigner();
      const signature = await signer.signMessage(challenge.message);
      
      await request("/v1/auth/change-password", { method: "POST", body: JSON.stringify({ chain: "bsc", address, message: challenge.message, signature, new_password: newPassword.trim() }) });
      
      localStorage.removeItem(TOKEN_KEY);
      setToken(""); setPassword(""); setConfirmPassword(""); setNewPassword("");
      notify("密码已修改，请重新登录");
    } catch (err) { notify(cleanError(err)); }
  }

  // --- 渲染视图 ---
  if (!loggedIn) {
    return (
      <>
        <AuthPage isConnected={isConnected} shortAddress={displayAddress} mode={authMode} password={password} confirmPassword={confirmPassword} referralCode={referralCode} onPassword={setPassword} onConfirmPassword={setConfirmPassword} onReferralCode={setReferralCode} onMode={setAuthMode} onConnect={() => open({ view: "Connect" })} onLogin={() => handleLogin("/v1/auth/login")} onRegister={() => handleLogin("/v1/auth/register")} />
        <Toast message={toast} onClose={() => setToast("")} />
      </>
    );
  }

  return (
    <div className="phone-shell">
      {/* 顶栏顶奢黑金升级：完美嵌入你的独家黑龙 Logo */}
      <header className="topbar">
        <div className="brand">
          <img src={logo} className="brand-logo-img" alt="Logo" />
          <div className="brand-text">
            <strong>DrakeX</strong>
            <span className="brand-sub">Quant</span>
          </div>
        </div>
        <button className="wallet-menu-button" onClick={() => open({ view: isConnected ? "Account" : "Connect" })}>
          <Wallet size={14} />{isConnected ? shortAddr : "连接钱包"}
        </button>
      </header>

      {tab === "home" && <HomePage balance={data.balance} orders={data.orders} reports={data.reports} referral={data.referral} onFunds={() => setTab("funds")} onTrade={() => setTab("trade")} onReferral={() => setTab("referral")} />}
      {tab === "funds" && <FundsPage instructions={data.instructions} deposits={data.deposits} withdrawals={data.withdrawals} amount={amount} withdrawAmount={withdrawAmount} withdrawAddress={withdrawAddress} onAmount={setAmount} onWithdrawAmount={setWithdrawAmount} onWithdrawAddress={setWithdrawAddress} onRefresh={() => loadAll().then(() => notify("已刷新"))} onSend={handleSendDeposit} onWithdraw={handleRequestWithdraw} />}
      {tab === "trade" && <TradePage orders={data.orders} reports={data.reports} settlements={data.settlements} />}
      {tab === "referral" && <ReferralPage referral={data.referral} />}
      {tab === "security" && <UserCenterPage balance={data.balance} referral={data.referral} isConnected={isConnected} ledger={data.ledger} newPassword={newPassword} onNewPassword={setNewPassword} onChangePassword={handleChangePassword} onLogout={() => { localStorage.removeItem(TOKEN_KEY); setToken(""); }} />}

      <Toast message={toast} onClose={() => setToast("")} />

      <nav className="tabbar">
        <TabButton active={tab === "home"} icon={<Home size={20} />} label="首页" onClick={() => setTab("home")} />
        <TabButton active={tab === "funds"} icon={<ArrowDownToLine size={20} />} label="充提" onClick={() => setTab("funds")} />
        <TabButton active={tab === "trade"} icon={<CreditCard size={20} />} label="交易" onClick={() => setTab("trade")} />
        <TabButton active={tab === "referral"} icon={<Share2 size={20} />} label="返佣" onClick={() => setTab("referral")} />
        <TabButton active={tab === "security"} icon={<KeyRound size={20} />} label="我的" onClick={() => setTab("security")} />
      </nav>
    </div>
  );
}

// ================= 页面组件 =================
function AuthPage(props: any) {
  const isReg = props.mode === "register";
  return (
    <div className="phone-shell" style={{justifyContent: 'center', padding: '0 20px'}}>
      <div className="hero-card" style={{margin: 0}}>
        <div style={{textAlign: 'center', marginBottom: '24px'}}>
           {/* 登录页同步引入你的酷炫龙图 Logo */}
           <img src={logo} alt="Logo" style={{width: '64px', height: '64px', objectFit: 'contain', marginBottom: '12px', filter: 'drop-shadow(0 0 8px rgba(255,166,0,0.3))'}} />
           <h2>{isReg ? "新用户注册" : "欢迎回来"}</h2>
           <p className="muted" style={{marginTop: '8px'}}>连接钱包并{isReg ? "注册" : "登录"}您的账户</p>
        </div>
        {!props.isConnected ? (
          <button className="primary" onClick={props.onConnect}><Wallet size={18}/> 连接钱包</button>
        ) : (
          <div className="status-chip" style={{textAlign: 'center', marginBottom: '16px', background: 'var(--bg-glass)'}}>已连接: {shortId(props.shortAddress)}</div>
        )}
        <label>登录密码<input type="password" value={props.password} onChange={e => props.onPassword(e.target.value)} placeholder="至少 6 位" /></label>
        {isReg && <label>确认密码<input type="password" value={props.confirmPassword} onChange={e => props.onConfirmPassword(e.target.value)} placeholder="再次输入密码" /></label>}
        {isReg && <label>邀请码 (选填)<input value={props.referralCode} onChange={e => props.onReferralCode(e.target.value)} placeholder="如有请填写" /></label>}
        
        <button className="primary" onClick={isReg ? props.onRegister : props.onLogin} disabled={!props.isConnected}>
          {isReg ? <UserPlus size={18}/> : <LogIn size={18}/>} {isReg ? "立即注册" : "安全登录"}
        </button>
        <p style={{textAlign: 'center', marginTop: '20px', fontSize: '13px'}}>
          <span className="muted">{isReg ? "已有账户？" : "还没账户？"} </span>
          <span style={{color: '#FFB700', cursor: 'pointer', fontWeight: 'bold'}} onClick={() => props.onMode(isReg ? "login" : "register")}>{isReg ? "去登录" : "去注册"}</span>
        </p>
      </div>
    </div>
  );
}

function HomePage({
  balance,
  orders,
  reports,
  referral,
  onFunds,
  onTrade,
  onReferral
}: {
  balance: Balance | null;
  orders: Order[];
  reports: Report[];
  referral: ReferralSummary | null;
  onFunds: () => void;
  onTrade: () => void;
  onReferral: () => void;
}) {
  const latestOrder = orders[0];
  const latestReport = reports[0];
  const [showBalance, setShowBalance] = useState(true);

  return (
    <>
      {/* 头部英雄区：流动琥珀金炫光 */}
      <section className="home-hero-v3">
        <div className="home-hero-glow-amber" />
        <div className="home-hero-content">
          <div className="platform-title-amber">
            <h1>DrakeX 量化交易平台</h1>
            <span>资金、交易、返佣一屏掌握</span>
          </div>
        </div>

        {/* 核心资产卡片：高级币安风暗黑鎏金 */}
        <div className="home-balance-card-amber">
          <div className="balance-header">
            <div className="balance-title">
              <span>可用额度 (USDT)</span>
              <button className="eye-button-amber" onClick={() => setShowBalance(!showBalance)}>
                {showBalance ? <Eye size={14} /> : <EyeOff size={14} />}
              </button>
            </div>
            <span className={`activation-badge-amber ${balance?.activation_status === "active" ? "active" : "warning"}`}>
              <span className="pulse-dot-amber"></span>
              <Activity size={12} />
              {balance?.activation_status === "active" ? "已激活" : "未激活"}
            </span>
          </div>
          <div className="balance-main">
            <strong>{showBalance ? formatAmount(balance?.available) : "••••••"}</strong>
            <span className="currency-unit-amber">USDT</span>
          </div>
          <div className="balance-footer">
            {balance?.activation_status === "active" ? (
              <p>✓ 账户运行良好，尊享 100% 信号跟单速率</p>
            ) : (
              <p>⚠ 当前处于未激活状态，充值 100U 激活信号跟单</p>
            )}
          </div>
        </div>
      </section>

      {/* 金刚区：微拟物暗黑按压反馈 */}
      <section className="home-actions-v3">
        <button onClick={onFunds} className="action-btn-amber">
          <div className="icon-box deposit"><ArrowDownToLine size={18} /></div>
          <div className="btn-text">
            <span>资金划转</span>
            <small>USDT 极速充提</small>
          </div>
          <ChevronRight size={14} className="arrow" />
        </button>
        <button onClick={onTrade} className="action-btn-amber">
          <div className="icon-box trade"><CreditCard size={18} /></div>
          <div className="btn-text">
            <span>信号记录</span>
            <small>结算历史追踪</small>
          </div>
          <ChevronRight size={14} className="arrow" />
        </button>
        <button onClick={onReferral} className="action-btn-amber">
          <div className="icon-box referral"><Share2 size={18} /></div>
          <div className="btn-text">
            <span>返佣中心</span>
            <small>多级高额分红</small>
          </div>
          <ChevronRight size={14} className="arrow" />
        </button>
      </section>

      {/* 底部融合网格面板 */}
      <section className="home-overview-v3">
        {/* 面板 1：分布式资产度量网格 */}
        <div className="amber-panel">
          <div className="panel-header">
            <h3>资产度量中心</h3>
          </div>
          <div className="metrics-grid-v3">
            <div className="metric-box-v3">
              <span className="label">冻结保证金</span>
              <strong className="val value-frozen">{showBalance ? `${formatAmount(balance?.frozen)}` : "•••"} <small>U</small></strong>
            </div>
            <div className="metric-box-v3">
              <span className="label">联署佣金</span>
              <strong className="val value-reward">{showBalance ? `${formatAmount(referral?.total_reward)}` : "•••"} <small>U</small></strong>
            </div>
            <div className="metric-box-v3">
              <span className="label">有效群组成员</span>
              <strong className="val value-people">{referral?.direct_count || 0}<small>直</small> / {referral?.indirect_count || 0}<small>间</small></strong>
            </div>
          </div>
        </div>

        {/* 面板 2：实时信号馈送流 */}
        <div className="amber-panel">
          <div className="panel-header">
            <h3>最新数据脉冲</h3>
            <span className="pulse-label-amber"><Zap size={10} /> Live Feed</span>
          </div>
          <div className="feed-list-v3">
            {/* 订单流 */}
            <div className="feed-row-v3">
              <div className="row-left">
                <span className="feed-badge badge-order">ORDER</span>
                <div className="feed-meta">
                  <h4>{latestOrder ? latestOrder.symbol : "等待网络触发..."}</h4>
                  <small>{latestOrder ? shortTime(latestOrder.created_at) : "系统心跳保持中"}</small>
                </div>
              </div>
              <div className="row-right">
                {latestOrder ? (
                  <>
                    <span className={`dir-pill ${latestOrder.direction === "DOWN" ? "down" : "up"}`}>
                      {labelDirection(latestOrder.direction)}
                    </span>
                    <strong className="amount">{latestOrder.stake_usdt} U</strong>
                  </>
                ) : (
                  <span className="waiting">监听中</span>
                )}
              </div>
            </div>

            {/* 结算流 */}
            <div className="feed-row-v3">
              <div className="row-left">
                <span className="feed-badge badge-settle">SETTLE</span>
                <div className="feed-meta">
                  <h4>策略结算回报</h4>
                  <small>{latestReport ? shortTime(latestReport.created_at) : "心跳保持正常"}</small>
                </div>
              </div>
              <div className="row-right">
                {latestReport ? (
                  <>
                    <span className={`res-pill ${latestReport.result === "WIN" ? "win" : "loss"}`}>
                      {latestReport.result === "WIN" ? "套利成功" : "风控止损"}
                    </span>
                    <strong className={`amount ${latestReport.result === "WIN" ? "txt-up" : "txt-down"}`}>
                      {latestReport.gross_profit ? `${latestReport.gross_profit} U` : "0.00 U"}
                    </strong>
                  </>
                ) : (
                  <span className="waiting">无未结交易</span>
                )}
              </div>
            </div>
          </div>
        </div>
      </section>
    </>
  );
}

function FundsPage(props: any) {
  const [mode, setMode] = useState<"deposit" | "withdraw">("deposit");
  const feeRate = Number(props.instructions?.deposit_fee_rate || 0);
  const credited = Number(props.amount || 0) * (1 - feeRate);
  const withdrawCredited = Number(props.withdrawAmount || 0) * 0.98;
  return (
    <>
      <section className="card">
        <div className="section-title">
          <div><h2>资金管理</h2><p className="muted">USDT</p></div>
          <button className="ghost" onClick={props.onRefresh}><RefreshCw size={14} />刷新</button>
        </div>
        <div className="segmented-tabs">
          <button className={mode === "deposit" ? "active" : ""} onClick={() => setMode("deposit")}>充值</button>
          <button className={mode === "withdraw" ? "active" : ""} onClick={() => setMode("withdraw")}>提现</button>
        </div>
        <div className="metrics" style={{background: 'var(--bg-glass)', padding: '12px', borderRadius: '8px', marginBottom: '16px'}}>
          <Metric label={mode === "deposit" ? "激活条件" : "最低提现"} value="100U" />
          <Metric label="网络手续费" value={mode === "deposit" ? `${feeRate*100}%` : "2%"} />
          <Metric label="预计到账" value={`${Number.isFinite(mode === "deposit" ? credited : withdrawCredited) ? (mode === "deposit" ? credited : withdrawCredited).toFixed(2) : "0"}U`} />
        </div>

        {mode === "deposit" ? <>
          <label>充值金额<input type="number" value={props.amount} onChange={e => props.onAmount(e.target.value)} placeholder="输入 USDT 金额" /></label>
          <button className="primary" onClick={props.onSend}>充值</button>
        </> : <>
          <label>提现金额<input type="number" value={props.withdrawAmount} onChange={e => props.onWithdrawAmount(e.target.value)} placeholder="输入 USDT 金额" /></label>
          <div className="readonly-address-field">
            <span>接收钱包地址</span>
            <strong>{props.withdrawAddress || "连接钱包后自动填充"}</strong>
          </div>
          <button className="primary" onClick={props.onWithdraw}>提现申请</button>
        </>}
      </section>

      <RecordList title={mode === "deposit" ? "充值记录" : "提现记录"} rows={(mode === "deposit" ? props.deposits : props.withdrawals).map((row: any) => ({
        id: row.id,
        main: `${row.amount}U`,
        value: <span className={row.status === "COMPLETED" ? "up" : "muted"}>{row.status}</span>,
        sub: mode === "deposit" ? `到账 ${row.credited_amount}U` : shortId(row.to_address),
        extra: shortTime(row.created_at || row.requested_at),
        details: row
      }))} />
    </>
  );
}

function TradePage({ orders, reports, settlements }: any) {
  const stats = tradeStats(orders, settlements);
  return (
    <>
      <div className="summary-grid">
          <TradeStatCard title="今日收益" stat={stats.today} />
          <TradeStatCard title="本周收益" stat={stats.week} />
          <TradeStatCard title="本月收益" stat={stats.month} />
          <TradeStatCard title="累计净利" stat={stats.total} />
      </div>
      <RecordList title="交易单记录" rows={orders.map((row: any) => ({
        id: row.id,
        main: `${row.symbol} ${labelDirection(row.direction)}`,
        value: `${row.stake_usdt}U`,
        sub: row.order_status,
        extra: shortTime(row.created_at),
        tone: row.direction === "DOWN" ? "down" : "up",
        details: row
      }))} />
      <RecordList title="结算记录" rows={settlements.map((row: any) => ({
        id: row.id,
        main: `净盈亏`,
        value: <span className={profitTone(row.user_net_profit)}>{row.user_net_profit}U</span>,
        sub: row.status,
        extra: shortTime(row.created_at),
        details: row
      }))} />
    </>
  );
}

function ReferralPage({ referral }: any) {
  const link = referral ? `${location.origin}${location.pathname}?ref=${referral.referral_code}` : "";
  return (
    <>
      <section className="card">
        <p className="muted" style={{textAlign: 'center'}}>专属邀请码</p>
        <div style={{background: 'var(--bg-glass)', border: '1px dashed #FFB700', padding: '16px', borderRadius: '12px', textAlign: 'center', margin: '12px 0', display: 'flex', justifyContent: 'center', alignItems: 'center', gap: '12px'}}>
          <span style={{fontSize: '24px', fontFamily: 'var(--font-mono)', fontWeight: 'bold'}}>{referral?.referral_code || "-"}</span>
          <button className="ghost" onClick={() => { if(link) navigator.clipboard.writeText(link); }}><Copy size={18} /></button>
        </div>
        <div className="metrics">
          <Metric label="直推人数" value={String(referral?.direct_count || 0)} />
          <Metric label="间推人数" value={String(referral?.indirect_count || 0)} />
          <Metric label="累计返佣(U)" value={`${referral?.total_reward || "0"}`} />
        </div>
        <div className="metrics referral-rate-grid">
          <Metric label="直推返佣比例" value={percent(referral?.direct_rate)} />
          <Metric label="间推返佣比例" value={percent(referral?.indirect_rate)} />
        </div>
      </section>
      <RecordList title="返佣明细" rows={(referral?.rewards || []).map((row: any) => ({
        id: row.id,
        main: row.level === 1 ? "直推奖励" : "间推奖励",
        value: <span className="up">+{row.amount}U</span>,
        sub: `比例 ${percent(row.rate)}`,
        extra: shortTime(row.created_at),
        details: row
      }))} />
    </>
  );
}

function UserCenterPage({ balance, referral, isConnected, ledger, newPassword, onNewPassword, onChangePassword, onLogout }: any) {
  return (
    <>
      <section className="card" style={{background: 'linear-gradient(135deg, #1E2329, #12161A)', border: '1px solid rgba(255, 166, 0, 0.15)'}}>
        <div style={{display: 'flex', justifyContent: 'space-between', alignItems: 'center'}}>
          <div><p className="muted">当前可用余额</p><strong style={{fontSize: '28px', fontFamily: 'var(--font-mono)', color: '#FFB700'}}>{balance?.available || "0"} <span style={{fontSize:'16px', color:'#fff'}}>USDT</span></strong></div>
          <span className="status-chip" style={{background: balance?.activation_status === "active" ? 'rgba(14,203,129,0.15)' : 'rgba(252,213,53,0.16)', color: balance?.activation_status === "active" ? '#0ecb81' : '#fcd535'}}>{balance?.activation_status === "active" ? "🟢 已激活" : "未激活"}</span>
        </div>
        <div className="user-assets-grid">
          <Metric label="冻结保证金" value={`${formatAmount(balance?.frozen)}U`} />
          <Metric label="返佣金额" value={`${formatAmount(referral?.total_reward)}U`} />
        </div>
      </section>
      <section className="card">
        <div className="section-title"><h2>安全设置</h2></div>
        <label>修改密码<input type="password" value={newPassword} onChange={e => onNewPassword(e.target.value)} placeholder="输入新密码 (至少6位)" /></label>
        <button className="secondary" onClick={onChangePassword}>签名修改密码</button>
        <button className="ghost" style={{color: '#f6465d', marginTop: '24px', width: '100%', justifyContent: 'center'}} onClick={onLogout}>退出登录</button>
      </section>
      <RecordList title="资金流水" rows={ledger.map((row: any) => ({
        id: row.id,
        main: ledgerTypeLabel(row.entry_type),
        value: <span className={Number(row.amount) > 0 ? "up" : ""}>{Number(row.amount) > 0 ? '+' : ''}{formatAmount(row.amount)}U</span>,
        sub: shortTime(row.created_at),
        extra: `余额 ${formatAmount(row.balance_available_after)}`,
        details: row
      }))} />
    </>
  );
}

// ================= 共享 UI 组件 =================
function Metric({ label, value }: { label: string; value: string }) { return <div className="metric"><span>{label}</span><strong>{value}</strong></div>; }
function TradeStatCard({ title, stat }: { title: string; stat: TradeStat }) {
  const tone = profitTone(stat.net);
  return (
    <div className="stat-card" style={{borderLeft: stat.net > 0 ? '3px solid #0ecb81' : stat.net < 0 ? '3px solid #f6465d' : '3px solid rgba(255,255,255,0.22)'}}>
      <span>{title}</span>
      <strong className={tone}>{stat.net > 0 ? '+' : ''}{formatAmount(String(stat.net))}</strong>
      <span style={{fontSize: '11px', opacity: 0.7}}>{stat.count}单 | {formatAmount(String(stat.stake))}U</span>
    </div>
  );
}
function RecordList({ title, rows, pageSize = 10 }: { title: string; rows: RecordRow[]; pageSize?: number }) {
  const [selected, setSelected] = useState<RecordRow | null>(null);
  const visibleRows = rows.slice(0, pageSize);
  return (
    <>
      <div className="list-card">
        <div className="section-title"><h2>{title}</h2><span className="muted">{rows.length} 条</span></div>
        {visibleRows.length === 0 ? <div className="muted" style={{textAlign: 'center', padding: '20px'}}>暂无记录</div> : visibleRows.map(row => (
          <div className="list-row" key={row.id}>
            <button className="record-button" onClick={() => row.details && setSelected(row)}>
              <div className="list-row-main"><div className={row.tone || ""}>{row.main}</div><div>{row.value}</div></div>
              <div className="list-row-sub"><div>{row.sub}</div><div>{row.extra}</div></div>
            </button>
          </div>
        ))}
      </div>
      {selected?.details && <MobileDetails row={selected} onClose={() => setSelected(null)} />}
    </>
  );
}
function MobileDetails({ row, onClose }: { row: RecordRow; onClose: () => void }) {
  return (
    <div className="sheet-backdrop" onClick={onClose}>
      <div className="sheet" onClick={e => e.stopPropagation()}>
        <div className="sheet-handle" />
        <div className="section-title"><h2>记录详情</h2><button className="ghost" onClick={onClose}>关闭</button></div>
        <div className="detail-grid">
          {Object.entries(row.details || {}).map(([label, value]) => (
            <div key={label}><span>{label}</span><strong>{String(value)}</strong></div>
          ))}
        </div>
      </div>
    </div>
  );
}
function Toast({ message, onClose }: { message: string; onClose: () => void }) {
  useEffect(() => { if (message) { const t = setTimeout(onClose, 2500); return () => clearTimeout(t); } }, [message, onClose]);
  if (!message) return null;
  return <div className="toast-layer"><div className="toast-card">{message}</div></div>;
}
function TabButton({ active, icon, label, onClick }: { active: boolean; icon: ReactNode; label: string; onClick: () => void }) {
  return <button className={active ? "active" : ""} onClick={onClick}>{icon}<span>{label}</span></button>;
}

// ================= 工具函数 =================
function shortId(v?: string|null, s=6) { return v && v.length > s*2+3 ? `${v.slice(0,s)}...${v.slice(-s)}` : (v||""); }
function shortTime(v?: string|null) { return v ? new Date(v).toLocaleString("zh-CN", { month:'2-digit', day:'2-digit', hour:'2-digit', minute:'2-digit', hour12: false }) : "-"; }
function labelDirection(v: string) { return v === "UP" ? "🟢 看涨" : v === "DOWN" ? "🔴 看跌" : v || "-"; }
function formatAmount(v?: string|null) { const p = Number(v||0); return Number.isFinite(p) ? p.toFixed(2) : "0.00"; }
function profitTone(value: string | number) { const n = Number(value || 0); return n > 0 ? "up" : n < 0 ? "down" : "neutral-profit"; }
function percent(v?: string|null) { return `${(Number(v||0)*100).toFixed(0)}%`; }
function cleanError(e: unknown) { return e instanceof Error ? translateError(e.message).slice(0, 100) : "操作失败，请稍后重试"; }
function ledgerTypeLabel(v: string) { const l:Record<string,string> = { deposit_credit: "充值入账", withdrawal_paid: "提现成功", profit_share_fee: "平台扣费", user_net_profit: "净利入账" }; return l[v] || v; }

function tradeStats(orders: Order[], settlements: Settlement[]) {
  const n = new Date(), dStart = new Date(n.getFullYear(), n.getMonth(), n.getDate()).getTime();
  const wStart = dStart - ((n.getDay()+6)%7)*86400000, mStart = new Date(n.getFullYear(), n.getMonth(), 1).getTime();
  const build = (start?: number) => {
    const o = start ? orders.filter(r => Date.parse(r.created_at) >= start) : orders;
    const s = start ? settlements.filter(r => Date.parse(r.created_at) >= start) : settlements;
    return { count: o.length, stake: o.reduce((sum, r) => sum + Number(r.stake_usdt||0), 0), net: s.reduce((sum, r) => sum + Number(r.user_net_profit||0), 0) };
  };
  return { today: build(dStart), week: build(wStart), month: build(mStart), total: build() };
}

function friendlyErrorFromBody(body: string, status: number) {
  try { const p = JSON.parse(body); return translateError(p.code || p.message || body, status); } catch { return translateError(body, status); }
}
function isAuthExpired(body: string) { return /EXPIRED|REVOKED|invalid token/i.test(body); }
function translateError(message: string, status?: number) {
  const text = String(message || "");
  const rules: Array<[RegExp, string]> = [
    [/AUTH_ACCOUNT_EXISTS|already registered|wallet address already registered/i, "当前钱包已注册，请直接登录"],
    [/AUTH_INVALID_CREDENTIALS|invalid credentials/i, "当前钱包未注册或密码错误"],
    [/AUTH_MISSING_TOKEN|missing bearer token/i, "请先登录"],
    [/AUTH_TOKEN_EXPIRED|EXPIRED|REVOKED|invalid token/i, "登录已过期，请重新登录"],
    [/AUTH_INVALID_USER|user account not found/i, "账户不存在，请先注册"],
    [/REFERRAL_SELF_INVALID/i, "不能填写自己的邀请码"],
    [/REFERRAL.*NOT.*FOUND|referral/i, "邀请码无效，请检查后重试"],
    [/WALLET_NOT_BOUND|WALLET_NONCE_MISSING|bind and verify/i, "请先连接并验证钱包"],
    [/signature|SIGNATURE|签名/i, "钱包签名失败或已取消"],
    [/insufficient|balance/i, "余额不足"],
    [/WITHDRAWAL.*MIN|minimum.*withdraw|withdraw/i, "提现金额或地址不符合要求"],
    [/DEPOSIT_UNCONFIRMED|transaction is not mined/i, "交易还未上链确认，请稍后再试"],
    [/DEPOSIT_EVENT_NOT_FOUND|DepositReceived event not found/i, "未找到充值事件，请确认交易是否正确"],
    [/DEPOSIT.*INVALID|invalid deposit|invalid transfer amount/i, "充值交易信息无效"],
    [/BSC_BLOCK_UNAVAILABLE|RPC|network|fetch|Failed to fetch/i, "网络或链上服务异常，请稍后重试"],
    [/user rejected|denied|cancelled|canceled|rejected/i, "操作已取消"],
    [/password/i, "密码格式不正确"],
    [/Error 400/i, "请求参数不正确"],
    [/Error 401/i, "登录已失效，请重新登录"],
    [/Error 403/i, "暂无权限执行此操作"],
    [/Error 404/i, "请求的数据不存在"],
    [/Error 5\d\d/i, "服务器异常，请稍后重试"]
  ];
  for (const [pattern, label] of rules) if (pattern.test(text)) return label;
  if (status === 400) return "请求参数不正确";
  if (status === 401) return "登录已失效，请重新登录";
  if (status === 403) return "暂无权限执行此操作";
  if (status === 404) return "请求的数据不存在";
  if (status && status >= 500) return "服务器异常，请稍后重试";
  return text && !/[a-z]{3,}/i.test(text) ? text : "操作失败，请稍后重试";
}
function loadPendingDeposit(): PendingDeposit | null { try { return JSON.parse(localStorage.getItem(PENDING_DEPOSIT_KEY)||'{}'); } catch { return null; } }
function savePendingDeposit(d: PendingDeposit) { localStorage.setItem(PENDING_DEPOSIT_KEY, JSON.stringify(d)); }
function clearPendingDeposit(hash: string) { const p = loadPendingDeposit(); if(p?.txHash === hash) localStorage.removeItem(PENDING_DEPOSIT_KEY); }

createRoot(document.getElementById("root")!).render(<StrictMode><App /></StrictMode>);
