//! Simple NEAR transaction signing and RPC without near-primitives
//! Uses only ed25519-dalek + borsh + HTTP for WASM compatibility
//! Adapted from intents-ark/src/near_tx.rs
#![allow(dead_code)]

use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;
use wasi_http_client::Client;

// ============================================================================
// NEAR Transaction Types (minimal borsh-serializable versions)
// ============================================================================

#[derive(BorshSerialize, BorshDeserialize)]
struct Transaction {
    signer_id: String,
    public_key: PublicKey,
    nonce: u64,
    receiver_id: String,
    block_hash: [u8; 32],
    actions: Vec<Action>,
}

#[derive(BorshSerialize, BorshDeserialize)]
enum PublicKey {
    ED25519([u8; 32]),
}

// All variants must be present for correct borsh enum index encoding
#[derive(BorshSerialize, BorshDeserialize)]
enum Action {
    CreateAccount,
    DeployContract(Vec<u8>),
    FunctionCall(FunctionCallAction),
    Transfer(u128),
    Stake { stake: u128, public_key: PublicKey },
    AddKey { public_key: PublicKey, access_key: AccessKey },
    DeleteKey(PublicKey),
    DeleteAccount(String),
}

#[derive(BorshSerialize, BorshDeserialize)]
struct AccessKey {
    nonce: u64,
    permission: AccessKeyPermission,
}

#[derive(BorshSerialize, BorshDeserialize)]
enum AccessKeyPermission {
    FunctionCall {
        allowance: Option<u128>,
        receiver_id: String,
        method_names: Vec<String>,
    },
    FullAccess,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct FunctionCallAction {
    method_name: String,
    args: Vec<u8>,
    gas: u64,
    deposit: u128,
}

#[derive(BorshSerialize, BorshDeserialize)]
struct SignedTransaction {
    transaction: Transaction,
    signature: Signature,
}

#[derive(BorshSerialize, BorshDeserialize)]
enum Signature {
    ED25519([u8; 64]),
}

// ============================================================================
// Public API
// ============================================================================

/// Parse a NEAR ed25519 private key into a signing key
///
/// Accepts "ed25519:<base58>" or bare base58, and both the 32-byte seed and the
/// 64-byte NEAR JSON format (seed || public key)
pub fn parse_signing_key(private_key: &str) -> Result<SigningKey, Box<dyn std::error::Error>> {
    let key_str = private_key.strip_prefix("ed25519:").unwrap_or(private_key);

    let key_bytes = bs58::decode(key_str)
        .into_vec()
        .map_err(|e| format!("Failed to decode private key: {}", e))?;

    if key_bytes.len() != 32 && key_bytes.len() != 64 {
        return Err(format!("Invalid private key length: {}", key_bytes.len()).into());
    }

    let mut seed = [0u8; 32];
    seed.copy_from_slice(&key_bytes[..32]);
    Ok(SigningKey::from_bytes(&seed))
}

/// Derive implicit account ID and public key from a private key
/// Returns (implicit_account_id, ed25519:base58_pubkey)
///
/// NEAR implicit account = hex-encoded ed25519 public key (64 chars)
pub fn derive_implicit_account(private_key: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let verifying_key = parse_signing_key(private_key)?.verifying_key();

    let implicit_account_id = hex::encode(verifying_key.to_bytes());
    let public_key = format!("ed25519:{}", bs58::encode(verifying_key.to_bytes()).into_string());
    Ok((implicit_account_id, public_key))
}

/// Sign an arbitrary message with a NEAR ed25519 private key
/// Returns the raw 64-byte signature (no hashing — the message is signed as-is)
pub fn sign_message(private_key: &str, message: &[u8]) -> Result<[u8; 64], Box<dyn std::error::Error>> {
    Ok(parse_signing_key(private_key)?.sign(message).to_bytes())
}

/// Send a function call transaction, returns transaction hash
pub fn call(
    rpc_url: &str,
    signer_account_id: &str,
    signer_private_key: &str,
    contract_id: &str,
    method_name: &str,
    args: &str,
    gas: u64,
    deposit: u128,
) -> Result<String, Box<dyn std::error::Error>> {
    eprintln!("Call: {}.{}", contract_id, method_name);

    send_function_call_transaction(
        rpc_url,
        signer_account_id,
        signer_private_key,
        contract_id,
        method_name,
        args.as_bytes(),
        gas,
        deposit,
    )
}

// ============================================================================
// Internal: Build, sign, send transaction
// ============================================================================

fn send_function_call_transaction(
    rpc_url: &str,
    signer_account_id: &str,
    signer_private_key: &str,
    receiver_id: &str,
    method_name: &str,
    args: &[u8],
    gas: u64,
    deposit: u128,
) -> Result<String, Box<dyn std::error::Error>> {
    // Parse private key (handles "ed25519:" prefix and both 32/64-byte key formats)
    let signing_key = parse_signing_key(signer_private_key)?;
    let verifying_key = signing_key.verifying_key();

    // Get nonce and block hash from RPC
    let (nonce, block_hash) = get_access_key_info(rpc_url, signer_account_id, &verifying_key)?;

    eprintln!("Nonce: {}, Block hash: {}", nonce, hex::encode(&block_hash));

    // Build transaction
    let transaction = Transaction {
        signer_id: signer_account_id.to_string(),
        public_key: PublicKey::ED25519(verifying_key.to_bytes()),
        nonce: nonce + 1,
        receiver_id: receiver_id.to_string(),
        block_hash,
        actions: vec![Action::FunctionCall(FunctionCallAction {
            method_name: method_name.to_string(),
            args: args.to_vec(),
            gas,
            deposit,
        })],
    };

    // Serialize and hash transaction
    let tx_bytes = borsh::to_vec(&transaction)?;
    let mut hasher = Sha256::new();
    hasher.update(&tx_bytes);
    let tx_hash = hasher.finalize();

    // Sign transaction
    let signature = signing_key.sign(&tx_hash);

    let signed_tx = SignedTransaction {
        transaction,
        signature: Signature::ED25519(signature.to_bytes()),
    };

    // Send transaction via RPC
    send_transaction(rpc_url, &signed_tx)
}

// ============================================================================
// RPC Helper Types
// ============================================================================

#[derive(Serialize)]
struct JsonRpcRequest<T> {
    jsonrpc: String,
    id: String,
    method: String,
    params: T,
}

#[derive(Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Deserialize, Debug)]
struct JsonRpcError {
    message: String,
}

// ============================================================================
// Transaction Outcome Structures
// ============================================================================

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum TxExecutionError {
    ActionError {
        #[serde(rename = "ActionError")]
        action_error: ActionError,
    },
    InvalidTxError {
        #[serde(rename = "InvalidTxError")]
        invalid_tx_error: serde_json::Value,
    },
}

#[derive(Deserialize, Debug)]
struct ActionError {
    index: Option<u64>,
    kind: ActionErrorKind,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum ActionErrorKind {
    FunctionCallError {
        #[serde(rename = "FunctionCallError")]
        function_call_error: FunctionCallErrorKind,
    },
    Other(serde_json::Value),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum FunctionCallErrorKind {
    ExecutionError {
        #[serde(rename = "ExecutionError")]
        execution_error: String,
    },
    Other(serde_json::Value),
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum ExecutionStatusView {
    Failure {
        #[serde(rename = "Failure")]
        failure: TxExecutionError,
    },
    SuccessValue {
        #[serde(rename = "SuccessValue")]
        success_value: String,
    },
    SuccessReceiptId {
        #[serde(rename = "SuccessReceiptId")]
        success_receipt_id: String,
    },
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum FinalExecutionStatus {
    Failure {
        #[serde(rename = "Failure")]
        failure: TxExecutionError,
    },
    SuccessValue {
        #[serde(rename = "SuccessValue")]
        success_value: String,
    },
    NotStarted,
    Started,
}

#[derive(Deserialize, Debug)]
struct ExecutionOutcomeView {
    logs: Vec<String>,
    status: ExecutionStatusView,
}

#[derive(Deserialize, Debug)]
struct ExecutionOutcomeWithIdView {
    outcome: ExecutionOutcomeView,
}

#[derive(Deserialize, Debug)]
struct FinalExecutionOutcomeView {
    status: FinalExecutionStatus,
    transaction_outcome: ExecutionOutcomeWithIdView,
    receipts_outcome: Vec<ExecutionOutcomeWithIdView>,
}

// ============================================================================
// RPC Functions
// ============================================================================

fn get_access_key_info(
    rpc_url: &str,
    account_id: &str,
    public_key: &VerifyingKey,
) -> Result<(u64, [u8; 32]), Box<dyn std::error::Error>> {
    let public_key_str = format!("ed25519:{}", bs58::encode(public_key.to_bytes()).into_string());

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: "dontcare".to_string(),
        method: "query".to_string(),
        params: serde_json::json!({
            "request_type": "view_access_key",
            "finality": "final",
            "account_id": account_id,
            "public_key": public_key_str
        }),
    };

    let response = Client::new()
        .post(rpc_url)
        .header("Content-Type", "application/json")
        .connect_timeout(Duration::from_secs(10))
        .body(serde_json::to_string(&request)?.as_bytes())
        .send()?;

    let status = response.status();
    if status != 200 {
        return Err(format!("RPC returned status {}", status).into());
    }

    let body = response.body()?;
    let json_value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| format!("Failed to parse RPC response: {}", e))?;

    if let Some(error) = json_value.get("error") {
        return Err(format!("RPC error: {}", error).into());
    }

    let result = json_value.get("result")
        .ok_or("No 'result' field in RPC response")?;

    let nonce = result.get("nonce")
        .and_then(|n| n.as_u64())
        .ok_or("Missing or invalid 'nonce' field")?;

    let block_hash_str = result.get("block_hash")
        .and_then(|b| b.as_str())
        .ok_or("Missing or invalid 'block_hash' field")?;

    let block_hash_bytes = bs58::decode(block_hash_str)
        .into_vec()
        .map_err(|e| format!("Failed to decode block hash: {}", e))?;

    if block_hash_bytes.len() != 32 {
        return Err(format!("Invalid block hash length: {} bytes", block_hash_bytes.len()).into());
    }

    let mut block_hash = [0u8; 32];
    block_hash.copy_from_slice(&block_hash_bytes);

    Ok((nonce, block_hash))
}

fn send_transaction(
    rpc_url: &str,
    signed_tx: &SignedTransaction,
) -> Result<String, Box<dyn std::error::Error>> {
    let tx_bytes = borsh::to_vec(signed_tx)?;
    use base64::Engine;
    let tx_base64 = base64::engine::general_purpose::STANDARD.encode(&tx_bytes);

    let request = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: "dontcare".to_string(),
        method: "broadcast_tx_commit".to_string(),
        params: vec![tx_base64],
    };

    eprintln!("Sending transaction to NEAR RPC...");

    let response = Client::new()
        .post(rpc_url)
        .header("Content-Type", "application/json")
        .connect_timeout(Duration::from_secs(60))
        .body(serde_json::to_string(&request)?.as_bytes())
        .send()?;

    let status = response.status();
    if status != 200 {
        let body = response.body().unwrap_or_default();
        let error_text = String::from_utf8_lossy(&body);
        return Err(format!("RPC returned status {}: {}", status, error_text).into());
    }

    let body = response.body()?;
    let json_response: JsonRpcResponse<serde_json::Value> = serde_json::from_slice(&body)?;

    if let Some(error) = json_response.error {
        return Err(format!("Transaction failed: {}", error.message).into());
    }

    let result = json_response.result.ok_or("No result in RPC response")?;

    let tx_hash = result
        .get("transaction")
        .and_then(|tx| tx.get("hash"))
        .and_then(|h| h.as_str())
        .ok_or("No transaction hash in response")?
        .to_string();

    // Parse the full execution outcome to check for failures
    let outcome: FinalExecutionOutcomeView = serde_json::from_value(result.clone())
        .map_err(|e| format!("Failed to parse execution outcome: {}", e))?;

    // Check top-level status
    match &outcome.status {
        FinalExecutionStatus::Failure { failure: err } => {
            let error_msg = format_tx_error(err);
            return Err(format!("Transaction failed: {}", error_msg).into());
        }
        FinalExecutionStatus::NotStarted => {
            return Err("Transaction not started".into());
        }
        FinalExecutionStatus::Started => {
            return Err("Transaction still in progress".into());
        }
        FinalExecutionStatus::SuccessValue { .. } => {}
    }

    // Check transaction_outcome status
    if let ExecutionStatusView::Failure { failure: err } = &outcome.transaction_outcome.outcome.status {
        let error_msg = format_tx_error(err);
        return Err(format!("Transaction outcome failed: {}", error_msg).into());
    }

    // Check all receipt outcomes for failures
    for (i, receipt_outcome) in outcome.receipts_outcome.iter().enumerate() {
        if let ExecutionStatusView::Failure { failure: err } = &receipt_outcome.outcome.status {
            let error_msg = format_tx_error(err);
            return Err(format!("Receipt {} failed: {}", i, error_msg).into());
        }
    }

    eprintln!("Transaction successful: {}", tx_hash);
    Ok(tx_hash)
}

fn format_tx_error(err: &TxExecutionError) -> String {
    match err {
        TxExecutionError::ActionError { action_error } => {
            let index_str = action_error.index.map(|i| format!("action {}: ", i)).unwrap_or_default();
            match &action_error.kind {
                ActionErrorKind::FunctionCallError { function_call_error } => {
                    match function_call_error {
                        FunctionCallErrorKind::ExecutionError { execution_error } => {
                            format!("{}Smart contract panicked: {}", index_str, execution_error)
                        }
                        FunctionCallErrorKind::Other(val) => {
                            format!("{}Function call error: {:?}", index_str, val)
                        }
                    }
                }
                ActionErrorKind::Other(val) => {
                    format!("{}Action error: {:?}", index_str, val)
                }
            }
        }
        TxExecutionError::InvalidTxError { invalid_tx_error } => {
            format!("Invalid transaction: {:?}", invalid_tx_error)
        }
    }
}
