#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Lockup, LAST_ID, LOCKUPS};
use cw_utils::must_pay;

pub const DENOM: &str = "uawesome";
pub const MINIMUM_DEPOSIT_AMOUNT: Uint128 = Uint128::new(10_000);
pub const LOCK_PERIOD: u64 = 60 * 60 * 24;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Deposit {} => deposit(deps, env, info),
        ExecuteMsg::Withdraw { ids } => withdraw(deps, env, info, ids),
    }
}

/// Deposit entry point for users
/*
@note
•   There is nothing wrong w this function
•   Also I can't use anyway because the attacket does not have funds
*/
pub fn deposit(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    // check minimum amount and denom
    //@note make sure user can't create lockup request with different denom
    //@note also gets amount of that coin
    let amount = must_pay(&info, DENOM).unwrap();

    //@note user can't submit notes with amount differing from what they withdraw
    if amount < MINIMUM_DEPOSIT_AMOUNT {
        return Err(ContractError::Unauthorized {});
    }

    // increment lock id
    //@note load last lockup id
    let id = LAST_ID.load(deps.storage).unwrap_or(1);
    //@note save the next lockup id
    LAST_ID.save(deps.storage, &(id + 1)).unwrap();

    // create lockup
    //@note uses the lockup id before it was incremented
    let lock = Lockup {
        id,
        owner: info.sender,
        amount,
        release_timestamp: env.block.time.plus_seconds(LOCK_PERIOD),
    };

    // save lockup
    LOCKUPS.save(deps.storage, id, &lock).unwrap();

    Ok(Response::new()
        .add_attribute("action", "deposit")
        .add_attribute("id", lock.id.to_string())
        .add_attribute("owner", lock.owner)
        .add_attribute("amount", lock.amount)
        .add_attribute("release_timestamp", lock.release_timestamp.to_string()))
}

/// Withdrawal entry point for users
/*
@note
•   The bug has to be here
•   The only thing I can provide is a bunch of ids, so there needs
    to be some way I can provide a list of ids that makes the contract
    do something stupid
•   I can't provide the owner's address
•   What happens if I provide duplicate ids?
•   You also know the order in which the IDs get updated, can you provide this
*/
pub fn withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    ids: Vec<u64>,
) -> Result<Response, ContractError> {
    let mut lockups: Vec<Lockup> = vec![];
    let mut total_amount = Uint128::zero();

    // fetch vaults to process
    for lockup_id in ids.clone() {
        let lockup = LOCKUPS.load(deps.storage, lockup_id).unwrap();
        lockups.push(lockup);
    }

    /*
    @note
    •   If the same lockup id is provided over and over in the list,
        then multiple copies of that lockup struct will be placed in
        lockups. removing them at the end of each iteration has no effect
        as total_amount keeps getting increased anyway.
    •   Even with all of this, you still can't withdraw someone else's deposit.
        you have to make your own deposit, then withdraw it multiple times by n
        passing in the same id over and over in the list of ids.
    •   Crazy that I thought of this before but didn't quite understand if it worked
        or not
    •   Core issue is that all the lockups get loaded at once instead of checking for 
        existence before loading / removing duplicates from input lockup ids
    
    •   Make a deposit with at the least the minimum amount, then supply a list containing
        the same lock id over and over again after the lock period is over to withdraw all the 
        funds in the protocol
    */
    for lockup in lockups {
        // validate owner and time
        //@note how am I supposed to get past this?
        if lockup.owner != info.sender || env.block.time < lockup.release_timestamp {
            return Err(ContractError::Unauthorized {});
        }

        // increase total amount
        total_amount += lockup.amount;

        // remove from storage
        //@note what does this do if the id isn't there
        LOCKUPS.remove(deps.storage, lockup.id);
    }

    let msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: vec![Coin {
            denom: DENOM.to_string(),
            amount: total_amount,
        }],
    };

    Ok(Response::new()
        .add_attribute("action", "withdraw")
        .add_attribute("ids", format!("{:?}", ids))
        .add_attribute("total_amount", total_amount)
        .add_message(msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetLockup { id } => to_binary(&get_lockup(deps, id)?),
    }
}

/// Returns lockup information for a specified id
pub fn get_lockup(deps: Deps, id: u64) -> StdResult<Lockup> {
    Ok(LOCKUPS.load(deps.storage, id).unwrap())
}
