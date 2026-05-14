# BSC USDT Deposits

## Chain And Token

- Chain: BSC.
- Asset: USDT BEP20.
- USDT contract: `0x55d398326f99059fF775485246999027B3197955`.
- Vault contract: one BSC contract configured with the platform settlement wallet.

## User Binding Requirement

Because all users recharge through one vault contract, the system must identify both the account and payer. The first-release rule is:

```text
Credit deposits only when the vault emits DepositReceived for the user's account_id and the payer is the user's verified bound wallet address.

Deposits are account scoped. The user must log in first, bind a wallet to that account, and submit/confirm the transaction under that account. The wallet proves source ownership, but the credited balance belongs to the platform account.
```

This means exchange direct withdrawals are not supported for automatic crediting in the first release, because exchange withdrawals usually come from exchange hot wallets, not from the user's bound address.

## Binding Flow

1. User enters BSC wallet address.
2. Backend creates a nonce and message.
3. User signs the message.
4. Backend recovers address from signature.
5. Backend binds the recovered address to user account.

## Deposit Flow

1. User opens recharge page.
2. Backend returns `vault_contract`, `account_id`, `deposit_id`, token contract, settlement wallet, and bound wallet.
3. DApp asks the wallet to approve USDT spending for `vault_contract`.
4. DApp calls `EventSignalVault.deposit(account_id, deposit_id, amount)`.
5. DApp submits the returned tx hash and `deposit_id` to the backend.
6. Backend verifies `DepositReceived` and the matching USDT `Transfer`.
7. Backend credits user ledger after the required confirmations.

## Validation Rules

A deposit is valid only if:

- Transaction exists on BSC.
- Transaction contains `DepositReceived(account_id, deposit_id, payer, receiver, amount)`.
- Transaction contains a matching USDT `Transfer` event.
- `token_contract == BSC_USDT_CONTRACT`.
- `payer == user's verified wallet address`.
- `receiver == BSC_SETTLEMENT_WALLET`.
- `deposit_id` matches the backend-issued deposit id.
- `amount >= min_deposit_amount`.
- Transaction hash has not been credited before.
- Confirmation count is greater than or equal to configured minimum.

## Suggested Confirmations

Use at least 15 confirmations for normal crediting. For large amounts, use stricter review.

```text
< 100 USDT: 15 confirmations
100 - 1000 USDT: 20 confirmations
> 1000 USDT: admin review or 30+ confirmations
```

## Deposit Status

```text
submitted
detected
confirming
credited
rejected
admin_review
```

## Rejection Reasons

```text
tx_not_found
wrong_chain
wrong_token
from_address_mismatch
to_address_mismatch
amount_too_small
already_credited
reorg_detected
admin_review_required
```

## Security Notes

- Never ask for wallet private key or seed phrase.
- Never sign arbitrary user-provided messages.
- Make signature messages clear and scoped only to wallet binding.
- Rate-limit tx hash submissions.
- Store raw chain event payloads for audit.
