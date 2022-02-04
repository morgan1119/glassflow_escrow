use cosmwasm_std::{ Env, Storage, Coin, StdResult};
use cosmwasm_storage::{bucket_read, bucket, prefixed};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::ContractError;
use cw20::{ Balance, Cw20CoinVerified };

const PREFIX_ESCROW: &[u8] = b"liability";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Escrow {
    pub arbiter: String,
    pub recipient: String,
    pub source: String,
    pub end_height: Option<u64>,
    pub end_time: Option<u64>,
    pub balance: GenericBalance,
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
) ->  Result<Escrow, ContractError> {
    bucket(storage, PREFIX_ESCROW).update(id.as_bytes(), | existing | match existing {
        None => Ok(escrow),
        Some(_) => Err(ContractError::IdAlreadyExists{}),
    })
}

pub fn escrows_remove(
    storage: &mut dyn Storage,
    id: &String,
) -> StdResult<()> {
    prefixed(storage, PREFIX_ESCROW).remove(id.as_bytes());
    Ok(())
}

#[derive(Serialize, Deserialize, Clone, PartialEq, JsonSchema, Debug, Default)]
pub struct GenericBalance {
    pub native: Vec<Coin>,
    pub cw20: Vec<Cw20CoinVerified>,
}

impl GenericBalance {
    pub fn add_tokens(&mut self, add: Balance) {
        match add {
            Balance::Native(balance) => {
                for token in balance.0 {
                    let index = self.native.iter().enumerate().find_map(|(i, exist)| {
                        if exist.denom == token.denom {
                            Some(i)
                        } else {
                            None
                        }
                    });
                    match index {
                        Some(idx) => self.native[idx].amount += token.amount,
                        None => self.native.push(token),
                    }
                }
            }
            Balance::Cw20(token) => {
                let index = self.cw20.iter().enumerate().find_map(|(i, exist)| {
                    if exist.address == token.address {
                        Some(i)
                    } else {
                        None
                    }
                });
                match index {
                    Some(idx) => self.cw20[idx].amount += token.amount,
                    None => self.cw20.push(token),
                }
            }
        };
    }
}


// pub fn all_escrow_ids(
//     storage: &dyn Storage,
// )  -> Result<Vec<String>, ContractError> {
//     let escrow_bucket: ReadonlyBucket<String> = bucket_read(storage, PREFIX_ESCROW);

//     escrow_bucket    
//         .range(None, None, Order::Ascending)
//         .map(| elem| {
//             let (k, _) = elem?;
//             Ok(String::from_utf8(k).unwrap())
//         })
//         .collect()
// }