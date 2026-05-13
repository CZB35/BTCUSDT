# Event Signal Vault

This Foundry workspace contains the BSC USDT vault used by the Event Signal platform.

## Commands

- `forge test` runs the Solidity unit tests.
- `forge build` compiles the vault contract.
- `forge script script/DeployEventSignalVault.s.sol:DeployEventSignalVault --rpc-url $env:BSC_RPC_URL --broadcast` deploys the vault from local environment variables.

## Contract Role

`EventSignalVault` routes user recharge and refund payments while emitting auditable events for backend accounting. It does not enforce product limits such as minimum recharge size or Binance order size; DApp and backend validation own those rules.

- Users deposit USDT with `deposit(accountId, depositId, amount)` after approving the contract. The contract immediately forwards the USDT to `settlementWallet`.
- Backend credits balances only after seeing `DepositReceived` on chain.
- Backend-admin refund wallets approve USDT and call `payRefund(recipient, amount, refundId)`.
- Backend marks withdrawals paid only after seeing `RefundPaid` with the expected refund id, recipient, and amount.

Use BSC USDT as the `asset` constructor argument on deployment, and set `settlementWallet` to the platform receiving wallet.

## Safe Deployment

Do not paste or commit private keys. If a key has appeared in chat, logs, or a file, treat it as compromised and move funds to a new wallet before deploying.

Create `contracts/.env` from `contracts/.env.example` and keep it untracked:

```powershell
Copy-Item contracts\.env.example contracts\.env
```

Set these values:

- `BSC_DEPLOYER_PRIVATE_KEY`: a new funded deployer wallet key.
- `BSC_USDT_CONTRACT`: BSC USDT, normally `0x55d398326f99059fF775485246999027B3197955`.
- `BSC_VAULT_OWNER`: wallet allowed to pause, update USDT, update settlement wallet, and transfer ownership.
- `BSC_SETTLEMENT_WALLET`: wallet that receives user deposits immediately.

Deploy from `contracts/`:

```powershell
$env:BSC_RPC_URL="https://bsc-dataseed.binance.org"
$env:BSC_DEPLOYER_PRIVATE_KEY="0x..."
$env:BSC_USDT_CONTRACT="0x55d398326f99059fF775485246999027B3197955"
$env:BSC_VAULT_OWNER="0x..."
$env:BSC_SETTLEMENT_WALLET="0x..."
forge script script/DeployEventSignalVault.s.sol:DeployEventSignalVault --rpc-url $env:BSC_RPC_URL --broadcast
```

After deployment, set backend `BSC_VAULT_CONTRACT` to the deployed address and keep `BSC_SETTLEMENT_WALLET` identical to the contract constructor value.

## Admin Controls

Only `owner` can update configuration:

- `setAsset(newAsset)` updates the BEP-20 token address if deployment used the wrong USDT address.
- `setSettlementWallet(newSettlementWallet)` updates the platform receiving wallet.
- `transferOwnership(newOwner)` transfers contract administration to another wallet.
- `pause()` / `unpause()` stop or resume deposits and refunds.
