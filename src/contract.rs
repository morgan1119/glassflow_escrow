use cosmwasm_std::{
    entry_point, Addr, BankMsg, Coin,  DepsMut, Env, MessageInfo, Response, StdResult, Binary, to_binary, Deps
};

use crate::error::ContractError;
use crate::msg::{CreateMsg, ExecuteMsg, InstantiateMsg, DetailsResponse, QueryMsg, ListResponse};
use crate::state::{Escrow, escrows_read, escrows_update, escrows_remove, escrows_save, all_escrow_ids};
// use cw20::{Balance, Cw20ReceiveMsg, Cw20Coin, Cw20CoinVerified};
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
        ExecuteMsg::Create(msg) => try_create(deps, msg, info.funds, &info.sender),
        ExecuteMsg::Approve { id} => try_approve(deps, env, info, id),
        ExecuteMsg::Refund { id } => try_refund(deps, info, id),
        ExecuteMsg::TopUp { id } => try_topup(deps, info.funds, id),
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
        QueryMsg::List {} => to_binary(&query_list(deps)?),
    }
}

fn try_approve(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    id: String
) -> Result<Response, ContractError> {
    let escrow = escrows_read( deps.storage, &id)?;

    if  escrow.arbiter != deps.api.addr_canonicalize(&info.sender.as_str())? {
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
        Ok(send_tokens(deps.api.addr_humanize(&escrow.recipient)?, escrow.balance))
    }
}

fn try_refund(
    deps: DepsMut,
    info: MessageInfo,
    id: String
) -> Result<Response, ContractError> {
    let escrow = escrows_read( deps.storage, &id)?;

    if deps.api.addr_canonicalize(&info.sender.as_str())? != escrow.arbiter
    {
        return Err(ContractError::Unauthorized {});
    } else {
        escrows_remove(deps.storage, &id)?;  // remove the escrow contract because it is no longer needed

        Ok(send_tokens(deps.api.addr_humanize(&escrow.recipient)?, escrow.balance))
    }
}

// this is a helper to move the tokens, so the business logic is easy to read
fn send_tokens(
    to_address: Addr, 
    amount: Vec<Coin>, 
) -> Response {
    Response::new()
        .add_message(BankMsg::Send {
            to_address: to_address.clone().into(),
            amount,
        })
        .add_attribute("to", to_address)
}

pub fn try_create(
    deps: DepsMut,
    msg: CreateMsg,
    balance: Vec<Coin>,
    sender: &Addr,
) -> Result<Response, ContractError>{
    // this fails if no fund is sent from the receiver
    if balance.is_empty() {
        return Err(ContractError::ZeroBalance{})
    }

    let escrow_balance = balance;

    let escrow = Escrow {
        arbiter: deps.api.addr_canonicalize(&msg.arbiter)?,
        recipient: deps.api.addr_canonicalize(&msg.recipient)?,
        source: deps.api.addr_canonicalize(sender.as_str())?,
        end_height: msg.end_height,
        end_time: msg.end_time,
        balance: escrow_balance,
    };

    // try to store it, fail if the id was already in use
    escrows_update(deps.storage, escrow, &msg.id)
}


fn try_topup(
    deps: DepsMut,
    balance: Vec<Coin>,
    id: String,
) -> Result<Response, ContractError> {
    if balance.is_empty() {
        return Err(ContractError::ZeroBalance{});
    }

    let mut escrow = escrows_read( deps.storage, &id)?;
    
    for token in balance {
        let index = escrow.balance.iter().enumerate().find_map(|(i, exist)| {
            if exist.denom == token.denom {
                Some(i)
            } else {
                None
            }
        });
        match index {
            Some(idx) =>  escrow.balance[idx].amount += token.amount,
            None => escrow.balance.push(token),
        }
    }


    escrows_save(deps.storage, &escrow, &id)?;
    Ok(Response::default())

}

fn query_details(
    deps: Deps,
    id: String,
) -> StdResult<DetailsResponse> {
    let escrow = escrows_read(deps.storage, &id)?;

    let details = DetailsResponse {
        id,
        arbiter: deps.api.addr_humanize(&escrow.arbiter)?,
        recipient: deps.api.addr_humanize(&escrow.recipient)?,
        source: deps.api.addr_humanize(&escrow.source)?,
        end_height: escrow.end_height,
        end_time: escrow.end_time,
        balance: escrow.balance
    };
    Ok(details)
}

fn query_list(
    deps: Deps
) ->  StdResult<ListResponse> {
    Ok(ListResponse {
        escrows: all_escrow_ids(deps.storage)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, CosmosMsg};

    #[test]
    fn create_and_approve_escrow() {
        let env = mock_env();
        let mut deps = mock_dependencies();

        let id = "foobar".to_string();
        let arbiter = Addr::unchecked("arbiter");
        let recipient = Addr::unchecked("recipient");
        let source = Addr::unchecked("sender");

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
                arbiter: arbiter.clone(),
                recipient: recipient.clone(),
                source: source.clone(),
                end_height: Some(123456),
                end_time: None,
                balance: balance.clone()
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
}
