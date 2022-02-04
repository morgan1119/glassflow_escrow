use cosmwasm_std::{
    entry_point, BankMsg,  DepsMut, Env, MessageInfo, Response, StdResult, Binary, to_binary, Deps, WasmMsg, CosmosMsg, from_binary
};

use crate::error::ContractError;
use crate::msg::{CreateMsg, ExecuteMsg, InstantiateMsg, DetailsResponse, QueryMsg, ReceiveMsg};
use crate::state::{ Escrow, escrows_read, escrows_update, escrows_remove, escrows_save, GenericBalance };
use cw20::{ Balance, Cw20ReceiveMsg, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg };
use cw2::set_contract_version;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:cw20-escrow";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // let state = config_read(deps.storage).load()?;
    match msg {
        ExecuteMsg::Create(msg) => try_create(deps, msg, Balance::from(info.funds), info.sender.to_string()),  // create an escrow with coins
        ExecuteMsg::Approve { id} => try_approve(deps, env, info, id),
        ExecuteMsg::Refund { id } => try_refund(deps, info, id),
        ExecuteMsg::TopUp { id } => try_top_up(deps, Balance::from(info.funds), id),
        ExecuteMsg::Receive(msg) => try_receive(deps, info, msg),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(
    deps: Deps,
    _env: Env,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Details { id } => to_binary(&query_details(deps, id)?),
        // QueryMsg::List {} => to_binary(&query_list(deps)?),
    }
}

pub fn try_receive(
    deps: DepsMut,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let msg = from_binary(&wrapper.msg)?;

    let balance = Balance::Cw20(Cw20CoinVerified {
        address: info.sender.into(),
        amount: wrapper.amount,
    });

    match msg {
        ReceiveMsg::Create(msg) => try_create(deps, msg, balance, wrapper.sender),
        ReceiveMsg::TopUp { id } => try_top_up(deps, balance, id),
    }
}

pub fn try_create(
    deps: DepsMut,
    msg: CreateMsg,
    balance: Balance,
    sender: String,
) -> Result<Response, ContractError>{
    // this fails if no fund is sent from the receiver
    if balance.is_empty() {
        return Err(ContractError::ZeroBalance{})
    }

    let escrow_balance = match balance {
        Balance::Native(balance) => GenericBalance {
            native: balance.0,
            cw20: vec![],
        },
        Balance::Cw20(token) => {
            // make sure the token sent is on the whitelist by default
            GenericBalance {
                native: vec![],
                cw20: vec![token],
            }
        }
    };

    let escrow = Escrow {
        arbiter: msg.arbiter,
        recipient: msg.recipient,
        source: sender,
        end_height: msg.end_height,
        end_time: msg.end_time,
        balance: escrow_balance,
    };

    // try to store it, fail if the id was already in use
    let res = escrows_update(deps.storage, escrow, &msg.id);
    match res {
        Ok(_) => Ok(Response::default()),
        _ =>  Err(ContractError::IdAlreadyExists{}), 
    }
}

fn try_approve(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String
) -> Result<Response, ContractError> {
    let escrow = escrows_read( deps.storage, &id)?;

    if  escrow.arbiter != info.sender.as_str() {
        return Err(ContractError::Unauthorized {});
    }   
    else if escrow.is_expired(&env) {   // throws error if state is expired
        return Err(ContractError::Expired {
            end_height: escrow.end_height,
            end_time: escrow.end_time,
        });
    } else {
        escrows_remove(deps.storage, &id)?;  // remove the escrow contract because it is no longer needed
        // send tokens to the seller
        let msgs = send_tokens(escrow.recipient, &escrow.balance)?;
        Ok(Response::new()
            .add_messages(msgs)
            .add_attribute("action", "approve escrow")
        )
    }
}

fn try_refund(
    deps: DepsMut,
    info: MessageInfo,
    id: String
) -> Result<Response, ContractError> {
    let escrow = escrows_read(deps.storage, &id)?;
    
    if info.sender != escrow.arbiter
    {
        return Err(ContractError::Unauthorized {});
    } else {
        escrows_remove(deps.storage, &id)?;  // remove the escrow contract because it is no longer needed

        let msgs = send_tokens(escrow.recipient, &escrow.balance)?;
        Ok(Response::new()
            .add_messages(msgs)
            .add_attribute("action", "refund")
        )       
    }
}

// this is a helper to move the tokens, so the business logic is easy to read
fn send_tokens(
    to_address: String, 
    amount: &GenericBalance, 
) -> StdResult<Vec<CosmosMsg>> {
    let native_balance = &amount.native;
    let mut msgs = if native_balance.is_empty() {
        vec![]
    } else {
        vec![BankMsg::Send {
            to_address: to_address.clone(),
            amount: native_balance.to_vec(),
        }
        .into()]
    };

    let cw20_balance = &amount.cw20;
    let cw20_msgs: StdResult<Vec<_>> = cw20_balance
        .iter()
        .map(|c| {
            let msg = Cw20ExecuteMsg::Transfer {
                recipient: to_address.clone(),
                amount: c.amount,
            };
            let exec = WasmMsg::Execute {
                contract_addr: c.address.to_string(),
                msg: to_binary(&msg)?,
                funds: vec![],
            };
            Ok(exec.into())
        })
        .collect();

    msgs.append(&mut cw20_msgs?);

    Ok(msgs)
}


fn try_top_up(
    deps: DepsMut,
    balance: Balance,
    id: String,
) -> Result<Response, ContractError> {
    if balance.is_empty() {
        return Err(ContractError::ZeroBalance{});
    }

    let mut escrow = escrows_read( deps.storage, &id)?;
    
    escrow.balance.add_tokens(balance);

    escrows_save(deps.storage, &escrow, &id)?;
    Ok(Response::new().add_attribute("action", "top_up"))
}

fn query_details(
    deps: Deps,
    id: String,
) -> StdResult<DetailsResponse> {
    let escrow = escrows_read(deps.storage, &id)?;

    // transform tokens
    let native_balance = escrow.balance.native;

    let cw20_balance: StdResult<Vec<_>> = escrow
        .balance
        .cw20
        .into_iter()
        .map(|token| {
            Ok(Cw20Coin {
                address: token.address.to_string(),
                amount: token.amount,
            })
        })
        .collect();

    let details = DetailsResponse {
        id,
        arbiter:escrow.arbiter,
        recipient: escrow.recipient,
        source: escrow.source,
        end_height: escrow.end_height,
        end_time: escrow.end_time,
        native_balance,
        cw20_balance: cw20_balance?
    };
    Ok(details)
}

// fn query_list(
//     deps: Deps
// ) ->  StdResult<ListResponse> {
//     Ok( 
//         ListResponse{
//             escrows: all_escrow_ids(deps.storage).unwrap()
//         },
//     )
// }

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{CosmosMsg, Uint128};
    
    #[test]
    fn create_and_approve_escrow() {
        let env = mock_env();
        let mut deps = mock_dependencies();

        let id = "foobar".to_string();
        let arbiter = "arbiter".to_string();
        let recipient = "recipient".to_string();
        let source = "sender".to_string();

        let msg = CreateMsg {
            id: id.clone(),
            arbiter: arbiter.clone().into(),
            recipient: recipient.clone().into(),
            end_time: None,
            end_height: Some(123456),
        };
        let balance = coins(100, "tokens");
        let info = mock_info("sender", &balance);
        let execute_res = execute(deps.as_mut(), env, info, ExecuteMsg::Create(msg)).unwrap();
        

        assert_eq!(0, execute_res.messages.len());
        // ensure the details is what we expect
        let details = query_details(deps.as_ref(), "foobar".to_string()).unwrap();
        assert_eq!(
            details,
            DetailsResponse {
                id: id.clone(),
                arbiter: arbiter.clone().to_string(),
                recipient: recipient.clone().to_string(),
                source: source.clone().to_string(),
                end_height: Some(123456),
                end_time: None,
                native_balance: balance.clone(), 
                cw20_balance: vec![]
            }
        );

        // beneficiary cannot release it
        let env = mock_env();
        let info = mock_info("beneficiary", &[]);
        let approve_res = execute(deps.as_mut(), env, info, ExecuteMsg::Approve{id:id.clone()});
        match approve_res.unwrap_err() {
            ContractError::Unauthorized { .. } => {}
            e => panic!("unexpected error: {:?}", e),
        }

        // approve it by arbiter
        let env = mock_env();
        let info = mock_info("arbiter", &[]);
        let approve_res = execute(deps.as_mut(), env, info, ExecuteMsg::Approve{id:id.clone()}).unwrap();
        assert_eq!(1, approve_res.messages.len());
        assert_eq!(
            approve_res.messages.get(0).expect("no message").msg, 
            CosmosMsg::Bank(BankMsg::Send{
                to_address: recipient.clone().into(),
                amount: balance.clone(),
            })
        );
    }

    fn create_and_approve_escrow_with_cw20() {
        let env = mock_env();
        let mut deps = mock_dependencies();

        let id = String::from("foobar");
        let arbiter = String::from("arbiter");
        let recipient = String::from("recipient");
        let source = String::from("sender");
        let token_contract_addr = String::from("token_contract");
        let info = mock_info(token_contract_addr.as_str(), &vec![]);

        let crt_msg = CreateMsg {
            id: id.clone(),
            arbiter: arbiter.clone().into(),
            recipient: recipient.clone().into(),
            end_time: None,
            end_height: Some(123456),
        };
        let rev_msg = Cw20ReceiveMsg {
            sender: source.clone(),
            amount: Uint128::from(100u128),
            msg: to_binary(&ExecuteMsg::Create(crt_msg)).unwrap(),
        };
        let execute_res = execute(deps.as_mut(), env, info, ExecuteMsg::Receive(rev_msg)).unwrap();
        assert_eq!(0, execute_res.messages.len());

        let details = query_details(deps.as_ref(), "foobar".to_string()).unwrap();
        assert_eq!(
            details,
            DetailsResponse{
                id: id.clone(),
                arbiter: arbiter.clone(),
                recipient: recipient.clone(),
                source: source.clone(),
                end_height: Some(123456),
                end_time: None,
                native_balance: vec![],
                cw20_balance: vec![Cw20Coin{
                    address: token_contract_addr.clone(),
                    amount: Uint128::from(100u128),
                }]
            }
        );

        // approve it by arbiter
        let env = mock_env();
        let info = mock_info("arbiter", &[]);
        let approve_res = execute(deps.as_mut(), env, info, ExecuteMsg::Approve{id:id.clone()}).unwrap();
        let send_msg = Cw20ExecuteMsg::Transfer {
            recipient: recipient.clone(),
            amount: Uint128::from(100u128),
        };

        assert_eq!(1, approve_res.messages.len());
        assert_eq!(
            approve_res.messages.get(0).expect("no message").msg, 
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: token_contract_addr.clone(),
                msg: to_binary(&send_msg).unwrap(),
                funds: vec![],
            })
        );
    }
}
