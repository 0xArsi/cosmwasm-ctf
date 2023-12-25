#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, to_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult,
    Uint128,
};
use cw_utils::must_pay;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Balance, Config, BALANCES, CONFIG};

pub const DENOM: &str = "uawesome";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        total_supply: Uint128::zero(),
    };

    CONFIG.save(deps.storage, &config)?;
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
        ExecuteMsg::Mint {} => mint(deps, env, info),
        ExecuteMsg::Burn { shares } => burn(deps, env, info, shares),
    }
}

/// Entry point for users to mint shares
/*
@note
•   Should verify that the user has sent coins of the right denom
•   Should handle case where vault has zero share supply
•   otherwise, should mint amount * current_shares/current_total_supply

•   Burning/minting normally does not change the exchange rate of shares/tokens. This
    is because the amount deposited is subtracted from the balance of the vault before 
    the exchange rate is calculated.

•   However, the contract does not keep track of the coin balance internally (via deposits)
    so you can manipulate the balance of the vault by sending coins to the vault, which will
    make one share worth more than one token in the burn calculation. You lose money doing this
    so for a three-transaction sequence the profit has to take these conflicting actions into n
    account. Namely, p would be
    p = d1  * (d1 + d2 + t)/(d1 + d2) - t
    p = d1 *(1 + t/(d1 + d2)) - t > 0
    which gives
    t < d1 / (1 - (d1/(d1+d2)))

    so 

    t < (d1/d2)(d1+d2)

•   Note that the profitability of this exploit depends on external circumstances like the balance
    the second user deposited, as they determine how much money you can take from the contract.
*/
pub fn mint(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    //@note user must have paid the right coin and denom
    let amount = must_pay(&info, DENOM).unwrap();

    let mut config = CONFIG.load(deps.storage).unwrap();

    let contract_balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), DENOM)
        .unwrap();

    //@note use balance/supply before deposit
    let total_assets = contract_balance.amount - amount;
    let total_supply = config.total_supply;

    // share = asset * total supply / total assets
    let mint_amount = if total_supply.is_zero() {
        //@note assumes that share:coin ratio is 1:1
        amount
    } else {
        //@note after the first deposit, a share will be worth >1 coin
        amount.multiply_ratio(total_supply, total_assets)
    };

    if mint_amount.is_zero() {
        return Err(ContractError::ZeroAmountNotAllowed {});
    }

    // increase total supply
    config.total_supply += mint_amount;
    CONFIG.save(deps.storage, &config)?;

    // increase user balance
    let mut user = BALANCES
        .load(deps.storage, &info.sender)
        .unwrap_or_default();
    user.amount += mint_amount;
    BALANCES.save(deps.storage, &info.sender, &user)?;

    Ok(Response::new()
        .add_attribute("action", "mint")
        .add_attribute("user", info.sender.to_string())
        .add_attribute("asset", amount.to_string())
        .add_attribute("shares", mint_amount.to_string()))
}

/// Entry point for users to burn shares
pub fn burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    shares: Uint128,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage).unwrap();

    let contract_balance = deps
        .querier
        .query_balance(env.contract.address.to_string(), DENOM)
        .unwrap();

    let total_assets = contract_balance.amount;
    let total_supply = config.total_supply;

    // asset = share * total assets / total supply
    let asset_to_return = shares.multiply_ratio(total_assets, total_supply);

    if asset_to_return.is_zero() {
        return Err(ContractError::ZeroAmountNotAllowed {});
    }

    // decrease total supply
    //@note if this does not error, then the user can withdraw any amount of shares that they want
    config.total_supply -= shares;
    CONFIG.save(deps.storage, &config)?;

    // decrease user balance
    let mut user = BALANCES.load(deps.storage, &info.sender)?;
    user.amount -= shares;
    BALANCES.save(deps.storage, &info.sender, &user)?;

    let msg = BankMsg::Send {
        to_address: info.sender.to_string(),
        amount: coins(asset_to_return.u128(), DENOM),
    };

    Ok(Response::new()
        .add_attribute("action", "burn")
        .add_attribute("user", info.sender.to_string())
        .add_attribute("asset", asset_to_return.to_string())
        .add_attribute("shares", shares.to_string())
        .add_message(msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::GetConfig {} => to_binary(&query_config(deps)?),
        QueryMsg::UserBalance { address } => to_binary(&query_user(deps, address)?),
    }
}

pub fn query_config(deps: Deps) -> StdResult<Config> {
    let config = CONFIG.load(deps.storage).unwrap();
    Ok(config)
}

pub fn query_user(deps: Deps, address: String) -> StdResult<Balance> {
    let user = deps.api.addr_validate(&address).unwrap();
    let balance = BALANCES.load(deps.storage, &user).unwrap_or_default();
    Ok(balance)
}
