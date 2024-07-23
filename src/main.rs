use std::time::Duration;
use std::{str::FromStr, sync::Arc};

use alloy_primitives::{address, Address};
use alloy_sol_types::sol;
use alloy_sol_types::SolCall;
use clap::Parser;
use ethers::{prelude::*, providers::Provider, utils::parse_units};
use eyre::{bail, eyre, Context, ErrReport, Result};
use lazy_static::lazy_static;
use serde_json::Value;
use spoof::State;
use transaction::eip2718::TypedTransaction;

pub const ARB_WASM_ADDRESS: Address = address!("0000000000000000000000000000000000000071");

lazy_static! {
    /// Address of the ArbWasm precompile.
    pub static ref ARB_WASM_H160: H160 = H160(*ARB_WASM_ADDRESS.0);
}

sol! {
    interface ArbWasm {
        function activateProgram(address program)
            external
            payable
            returns (uint16 version, uint256 dataFee);
    }
}

#[derive(Parser, Debug, Clone)]
#[command(bin_name = "activate-stylus-program")]
#[command(author = "rauljordan")]
#[command(propagate_version = true)]
#[command(version)]
pub struct CommonConfig {
    #[arg(long)]
    private_key: String,
    #[arg(long)]
    endpoint: String,
    #[arg(long)]
    address: H160,
    #[arg(long)]
    bump_fee_percent: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = CommonConfig::parse();
    activate_stylus_program(&cfg).await
}

/// Activates a Stylus program at a specified address by estimating its activation
/// data fee from the ArbOS precompile. Then, it sends a tx to activate the program
/// with a desired bump percentage on the estimated data fee.
pub async fn activate_stylus_program(cfg: &CommonConfig) -> Result<()> {
    let provider = Arc::new(new_provider(&cfg.endpoint)?);
    let chain_id = provider.get_chainid().await?.as_u64();
    let wallet = LocalWallet::from_str(&cfg.private_key)?;
    let signer = SignerMiddleware::new(provider.clone(), wallet.with_chain_id(chain_id));

    let mut data_fee = estimate_activation_data_fee(cfg.address, &signer.provider())
        .await
        .wrap_err("failed to check activation via spoofed eth_call")?;
    println!("Obtained estimated activation data fee {} wei", data_fee);
    if let Some(bump_percent) = cfg.bump_fee_percent {
        println!("Bumping estimated activation data fee by {}%", bump_percent);
        data_fee = bump_data_fee(data_fee, bump_percent);
    }

    let program: Address = cfg.address.to_fixed_bytes().into();
    let data = ArbWasm::activateProgramCall { program }.abi_encode();
    let tx = Eip1559TransactionRequest::new()
        .from(signer.address())
        .to(*ARB_WASM_H160)
        .value(data_fee)
        .data(data);
    let tx = TypedTransaction::Eip1559(tx);
    let tx = signer.send_transaction(tx, None).await?;
    match tx.await? {
        Some(receipt) => {
            println!(
                "Successfully activated program {} with tx {}",
                cfg.address,
                hex::encode(receipt.transaction_hash),
            );
            println!("Receipt: {:?}", receipt);
        }
        None => {
            bail!("Failed to activate program {}", cfg.address);
        }
    }
    Ok(())
}

async fn estimate_activation_data_fee(address: H160, provider: &Provider<Http>) -> Result<U256> {
    let program = Address::from(address.to_fixed_bytes());
    let data = ArbWasm::activateProgramCall { program }.abi_encode();
    let tx = Eip1559TransactionRequest::new()
        .to(*ARB_WASM_H160)
        .data(data)
        .value(parse_units("1", "ether")?);
    let code = provider.get_code(address, None).await?;
    let state: spoof::State = spoof::code(address, code);
    let outs = funded_eth_call(tx, state, provider).await??;
    let ArbWasm::activateProgramReturn { dataFee, .. } =
        ArbWasm::activateProgramCall::abi_decode_returns(&outs, true)?;

    Ok(ethers::types::U256::from_little_endian(
        dataFee.as_le_slice(),
    ))
}

struct EthCallError {
    #[allow(dead_code)]
    pub data: Vec<u8>,
    pub msg: String,
}

impl From<EthCallError> for ErrReport {
    fn from(value: EthCallError) -> Self {
        eyre!(value.msg)
    }
}

async fn funded_eth_call(
    tx: Eip1559TransactionRequest,
    mut state: State,
    provider: &Provider<Http>,
) -> Result<Result<Vec<u8>, EthCallError>> {
    let tx = TypedTransaction::Eip1559(tx);
    state.account(Default::default()).balance = Some(ethers::types::U256::MAX); // infinite balance

    match provider.call_raw(&tx).state(&state).await {
        Ok(bytes) => Ok(Ok(bytes.to_vec())),
        Err(ProviderError::JsonRpcClientError(error)) => {
            let error = error
                .as_error_response()
                .ok_or_else(|| eyre!("json RPC failure: {error}"))?;

            let msg = error.message.clone();
            let data = match &error.data {
                Some(Value::String(data)) => {
                    hex::decode(data.strip_prefix("0x").unwrap_or(data))?.to_vec()
                }
                Some(value) => bail!("failed to decode RPC failure: {value}"),
                None => vec![],
            };
            Ok(Err(EthCallError { data, msg }))
        }
        Err(error) => Err(error.into()),
    }
}

fn new_provider(url: &str) -> Result<Provider<Http>> {
    let mut provider = Provider::<Http>::try_from(url).wrap_err("failed to init http provider")?;
    provider.set_interval(Duration::from_millis(250));
    Ok(provider)
}

fn bump_data_fee(fee: U256, pct: u64) -> U256 {
    let num = 100 + pct;
    fee * U256::from(num) / U256::from(100)
}
