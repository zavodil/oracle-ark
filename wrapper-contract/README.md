# Oracle Wrapper Contract

Demo contract showing how to integrate with the OutLayer Oracle via `oracle_call`.

The wrapper pays for oracle calls from its own balance (0.02 NEAR per call) — users don't need to attach deposit.

## How it works

```
User --get_price("wrap.near")--> Wrapper --oracle_call--> Oracle (price-oracle.testnet)
                                 Wrapper <--oracle_on_call-- Oracle
                                 (logs price, returns Price)
```

## Build

```bash
cd wrapper-contract
bash build_local.sh
```

## Deploy & Init

```bash
near contract deploy price-oracle-wrapper.testnet \
  use-file res/oracle_wrapper_contract.wasm \
  with-init-call new \
  json-args '{"oracle_contract_id": "price-oracle.testnet"}' \
  prepaid-gas '10 Tgas' \
  attached-deposit '0 NEAR' \
  network-config testnet \
  sign-with-keychain send

# Fund the contract so it can pay for oracle calls
near tokens your-account.testnet send-near price-oracle-wrapper.testnet '1 NEAR' network-config testnet sign-with-keychain send
```

## Usage

### Get price

```bash
near contract call-function as-transaction price-oracle-wrapper.testnet get_price \
  json-args '{"token_id": "wrap.near"}' \
  prepaid-gas '300 Tgas' \
  attached-deposit '0 NEAR' \
  sign-as zavodil2.testnet \
  network-config testnet \
  sign-with-keychain send
```

### Prediction market

1. Make a prediction — guess the price:

```bash
near contract call-function as-transaction price-oracle-wrapper.testnet predict \
  json-args '{"token_id": "wrap.near", "predicted_price": 4.52}' \
  prepaid-gas '10 Tgas' \
  attached-deposit '0 NEAR' \
  sign-as zavodil2.testnet \
  network-config testnet \
  sign-with-keychain send
```

2. Resolve — fetch actual price and get the verdict:

```bash
near contract call-function as-transaction price-oracle-wrapper.testnet resolve \
  json-args '{}' \
  prepaid-gas '300 Tgas' \
  attached-deposit '0 NEAR' \
  sign-as zavodil2.testnet \
  network-config testnet \
  sign-with-keychain send
```

Logs will show the result:
```
Prediction result for zavodil2.testnet: wrap.near predicted zavodil2.testnet = 4.52 USD, actual = 4.58 USD -> higher
```

Verdict: **correct** (within 1%), **higher** (actual > predicted), or **lower** (actual < predicted).

## Gas

Use 300 TGas for `get_price` and `resolve` because the full call chain is:

```
get_price/resolve (wrapper) → oracle_call (oracle) → request_execution (OutLayer) → on_outlayer_result (oracle) → oracle_on_call (wrapper)
```

Each hop consumes gas. With 300 TGas the entire chain has enough room.
