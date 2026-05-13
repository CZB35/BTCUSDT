# BSC USDT Deposits

## Chain And Token

- Chain: BSC.
- Asset: USDT BEP20.
- USDT contract: `0x55d398326f99059fF775485246999027B3197955`.
- Platform receiving address: one shared address controlled by the platform.

## User Binding Requirement

Because all users recharge into one shared receiving address, the system must identify the payer. The safest first-release rule is:

```text
Credit deposits only when the transfer comes from the user's verified bound wallet address.

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
2. App shows platform BSC receiving address and bound wallet address.
3. User transfers BEP20 USDT from bound wallet to platform address.
4. User optionally submits tx hash.
5. Backend watches chain logs and/or verifies submitted tx hash.
6. Backend validates token contract, from, to, amount, and confirmations.
7. Backend credits user ledger.

## Validation Rules

A deposit is valid only if:

- Transaction exists on BSC.
- Transaction contains a USDT `Transfer` event.
- `token_contract == BSC_USDT_CONTRACT`.
- `from == user's verified wallet address`.
- `to == PLATFORM_BSC_RECEIVER`.
- `amount >= min_deposit_amount`.
- Transaction hash has not been credited before.
- Confirmation count is greater than or equal to configured minimum.

## Suggested Confirmations

Use at least 15 confirmations for normal crediting. For large amounts, use stricter review.

```text
< 100 USDT: 15 confirmations
100 - 1000 USDT: 20 confirmations
> 1000 USDT: manual review or 30+ confirmations
```

## Deposit Status

```text
submitted
detected
confirming
credited
rejected
manual_review
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
manual_review_required
```

## Security Notes

- Never ask for wallet private key or seed phrase.
- Never sign arbitrary user-provided messages.
- Make signature messages clear and scoped only to wallet binding.
- Rate-limit tx hash submissions.
- Store raw chain event payloads for audit.
