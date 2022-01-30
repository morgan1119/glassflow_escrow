use cosmwasm_std::{ Env, Storage, CanonicalAddr, Coin, StdResult, Response, Order};
use cosmwasm_storage::{bucket_read, bucket, prefixed, ReadonlyBucket};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::ContractError;

// use cw20_atomic_swap::balance::Balance;

const PREFIX_ESCROW: &[u8] = b"liability";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Escrow {
    pub arbiter: CanonicalAddr,
    pub recipient: CanonicalAddr,
    pub source: CanonicalAddr,
    pub end_height: Option<u64>,
    pub end_time: Option<u64>,
    pub balance: Vec<Coin>,
    // pub whitelist: Vec<CanonicalAddr>
}

impl Escrow {
    pub fn is_expired(&self, env: &Env) -> bool {
        if let Some(end_height) = self.end_height {
            if env.block.height > end_height {
                return true;
            }
        }

        if let Some(end_time) = self.end_time {
            if env.block.time.nanos() > end_time * 1000 {
                return true;
            }
        }
        false
    }
}

pub fn escrows_read(storage: &dyn Storage, id: &String) -> StdResult<Escrow> {
    bucket_read(storage, PREFIX_ESCROW).load(id.as_bytes())
}

pub fn escrows_save(
    storage: &mut dyn Storage, 
    escrow: &Escrow,
    id: &String
) -> StdResult<()> {
    bucket(storage, PREFIX_ESCROW).save(id.as_bytes(), escrow)
}

pub fn escrows_update(
    storage: &mut dyn Storage,
    escrow: Escrow,
    id: &String
) ->  Result<Response, ContractError> {

    let res = bucket(storage, PREFIX_ESCROW).update(id.as_bytes(), | existing | match existing {
        None => Ok(escrow),
        Some(_) => Err(ContractError::IdAlreadyExists{}),
    });

    match res {
        Ok(_) => Ok(Response::default()),
        Err(x) => Err(x)
    }
}

pub fn escrows_remove(
    storage: &mut dyn Storage,
    id: &String,
) -> Result<Response, ContractError> {
    prefixed(storage, PREFIX_ESCROW).remove(id.as_bytes());
    Ok(Response::default())
}

pub fn all_escrow_ids(
    storage: &dyn Storage,
)  -> StdResult<Vec<String>> {
    let escrow_bucket: ReadonlyBucket<String> = bucket_read(storage, PREFIX_ESCROW);

    escrow_bucket    
        .range(None, None, Order::Ascending)
        .map(| elem| {
            let (k, _) = elem?;
            Ok(String::from_utf8(k).unwrap())
        })
        .collect()
}
