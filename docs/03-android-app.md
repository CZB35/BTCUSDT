# Android App

## Technology Choice

Use Kotlin and Jetpack Compose for the first version.

Reasons:

- Android-only product.
- Needs native overlay window.
- Needs foreground service and persistent notification.
- Needs stable permission handling.
- Needs background lifecycle control and vendor battery guidance.
- Avoids Flutter native-plugin complexity for overlay and service features.

## App Modules

```text
app
  ui
    login
    wallet
    balance
    signal
    settings
    history
  data
    api
    websocket
    repository
    storage
  service
    FloatingSignalService
    SignalForegroundService
    NotificationHandler
    AutomationForegroundService
    ScreenRecognitionService
    ActionRunnerService
  domain
    auth
    wallet
    signal
    ledger
    automation
```

## Core Screens

### Login

- Email/password.
- Device ID registration.
- Token refresh handling.

### Wallet Binding

- User inputs BSC wallet address.
- App requests signature message from backend.
- User signs message with external wallet or WalletConnect.
- Backend verifies signature and binds address.

Signature message example:

```text
Bind wallet to BTCUSDT Event Signal Platform
User ID: {user_id}
Nonce: {nonce}
Issued At: {timestamp}
```

### Recharge

- Show platform BSC USDT receiving address.
- Show bound wallet address.
- Show minimum recharge amount.
- Require user to transfer from bound wallet address.
- Allow user to paste `txHash` for faster verification.
- Show deposit status: `pending`, `confirming`, `credited`, `rejected`.

### Signal Dashboard

- Current BTCUSDT price.
- Current 10-minute cycle.
- Latest signal direction: `UP`, `DOWN`, `HOLD`.
- Confidence.
- Suggested stake calculated from platform balance.
- Countdown to expiry.
- Signal history.
- Automation status for the current signal when enabled.

### Trading Preferences

Users can configure and save signal-related trading preferences in the app:

```text
下单百分比: 1% - 20%, default 5%
单笔最高金额: optional, default from platform config
最低信号置信度: optional, default from platform config
信号提醒开关: on/off
悬浮窗开关: on/off
```

Save behavior:

- User edits the percentage using a numeric input or slider.
- App validates the range locally before submission.
- App calls backend `PUT /v1/user-settings/trading`.
- Backend persists the setting and returns normalized values.
- App updates local cache after successful save.
- Suggested stake shown in the floating window must use the backend-returned value, not unconfirmed local input.

### Automation Settings

Users configure automation separately from trading preferences:

```text
automation_enabled: on/off
execution_mode: manual | assisted | automatic
target_app_profile: selected supported profile
require_final_confirmation: on/off
emergency_stop_enabled: on
```

Save behavior:

- App calls backend `PUT /v1/user-settings/automation`.
- App starts automation only when backend settings, local permissions, and target app profile are all valid.
- App disables automation locally if accessibility permission, screen capture permission, or target profile validation fails.

### Floating Window

The floating window should be compact and readable:

```text
BTCUSDT 10m
方向: 看涨
置信度: 61%
建议: 20 USDT
倒计时: 02:14
```

Controls:

- Expand/collapse.
- Move window.
- Hide until next signal.
- Open main app.
- Copy suggested amount.
- Copy direction text.
- Mark signal as watched or manually participated.

## Suggested Stake Display

The Android app receives suggested stake from backend and displays it without recalculating business logic locally.

Formula implemented on backend:

```text
if available_balance < 10 USDT:
  signal_access = limited
  suggested_stake = null
else:
  raw_stake = max(available_balance * user_stake_percent, 10 USDT)
  suggested_stake = min(raw_stake, user_single_order_limit, global_single_order_limit)
```

Display examples:

```text
余额: 200 USDT, 百分比5% -> 建议金额: 10 USDT
余额: 500 USDT, 百分比5% -> 建议金额: 25 USDT
余额: 500 USDT, 百分比10% -> 建议金额: 50 USDT
余额: 8 USDT -> 余额不足，最低10 USDT
```

Clipboard helpers:

- `复制金额`: copies `suggested_stake`.
- `复制方向`: copies `看涨` for `UP`, `看跌` for `DOWN`, and `观望` for `HOLD`.

## Automation Module

The automation module executes user-authorized event contract actions from backend-delivered signals. Keep it separate from signal display code so automation can be disabled, audited, or replaced without changing core app behavior.

Core responsibilities:

- Maintain target app profiles, screen templates, and action sequences.
- Read the latest signal, suggested stake, confidence, and user limits from repository state.
- Recognize target app screens before each action.
- Enter direction and amount only when the recognized state matches the expected profile.
- Submit execution reports to `POST /v1/execution-reports`.
- Stop immediately when the user taps emergency stop, the signal expires, confidence is below threshold, balance/risk limits fail, or screen recognition is uncertain.

Suggested execution modes:

- `manual`: show signal and copy helpers only.
- `assisted`: prepare fields and ask the user before final submission.
- `automatic`: run the configured action sequence and report the outcome.

The first implementation can study Smart-AutoClicker style concepts: image conditions, trigger groups, actions, pause/resume, and scenario storage. Do not mix GPL-licensed source into this app unless the product accepts GPL-3.0 obligations.

## Android Permissions

- `SYSTEM_ALERT_WINDOW` for overlay.
- Accessibility service permission for action execution.
- Screen capture permission if image recognition uses MediaProjection.
- `POST_NOTIFICATIONS` for Android 13+.
- Foreground service permissions depending on target SDK.
- Battery optimization exemption guide, but do not force abusive behavior.

## Foreground Service

The foreground service keeps signal receiving and floating window updates reliable.

Responsibilities:

- Maintain WebSocket while app is active or floating window is enabled.
- Show persistent notification.
- Keep automation state visible while an action sequence is active.
- Restart lightweight connection after network changes.
- Stop cleanly when user disables floating signal mode.

## WebSocket Behavior

- Connect after login.
- Subscribe to user-specific signal channel.
- Heartbeat every 20 seconds.
- Exponential reconnect with jitter.
- If WebSocket fails, rely on FCM as fallback.

## Data Security On Device

- Store access tokens in Android Keystore-backed encrypted storage.
- Do not store private keys.
- Do not request wallet seed phrases.
- Store automation profiles locally with versioning and integrity checks.
- Never log passwords, wallet signatures, access tokens, private keys, or full screen captures unless the user explicitly exports an audit package.
