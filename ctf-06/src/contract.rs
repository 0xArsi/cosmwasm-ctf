#[cfg(not(feature = "library"))]
use cosmwasm_std::{
    entry_point, from_binary, to_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo,
    QueryRequest, Response, StdResult, Uint128, WasmQuery,
};

use crate::error::ContractError;
use crate::msg::{Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{Config, Proposal, CONFIG, PROPOSAL};
use cw20::{BalanceResponse, Cw20QueryMsg, Cw20ReceiveMsg, TokenInfoResponse};

pub const DENOM: &str = "uawesome";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        voting_window: msg.window,
        voting_token: deps.api.addr_validate(&msg.token)?,
        owner: deps.api.addr_validate(&msg.owner)?,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("voting_window", msg.window.to_string())
        .add_attribute("voting_token", msg.token)
        .add_attribute("owner", msg.owner))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Propose {} => propose(deps, env, info),
        ExecuteMsg::ResolveProposal {} => resolve_proposal(deps, env, info),
        ExecuteMsg::OwnerAction { action } => owner_action(deps, info, action),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
    }
}

/// Entry point when receiving CW20 tokens
/*
@note
•   People vote by sending cw20 tokens to the token contract,
    then the token sends a hook msg to this contract (that's 
    presumably why the sender must be the token contract for 
    vote casting messages)

•   Is this frontrunnable? USER2 can wait until USER1 has proposed themselves
    as owner and admin approves, then this contract has 100k in it. USER2 then 
    calls resolve_proposal() so that the current proposal gets removed, then
    USER2 calls propose() and resolve_proposal in order, which will get approved
    as the admin's funds (majority of the supply) are already in the contract.
*/
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    //@note this function will error if there is no proposal
    let current_proposal = PROPOSAL.load(deps.storage)?;

    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::CastVote {}) => {
            //@note only token can cast a vote
            if config.voting_token != info.sender {
                return Err(ContractError::Unauthorized {});
            }

            if current_proposal
                .timestamp
                .plus_seconds(config.voting_window)
                < env.block.time
            {
                return Err(ContractError::VotingWindowClosed {});
            }

            Ok(Response::default()
                .add_attribute("action", "Vote casting")
                .add_attribute("voter", cw20_msg.sender)
                .add_attribute("power", cw20_msg.amount))
        }
        //@note this should revert everything not silently return
        //@note is there a msg you can send that should result in error instead of this
        _ => Ok(Response::default()),
    }
}

/// Propose a new proposal
//@note looks fine
pub fn propose(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    //@note does not need to unwrap error because of check immediately after
    let current_proposal = PROPOSAL.load(deps.storage);

    //@note errors if there is an existing proposal
    if current_proposal.is_ok() {
        return Err(ContractError::ProposalAlreadyExists {});
    }

    //@note sender cannot propose anyone other than themselves
    PROPOSAL.save(
        deps.storage,
        &Proposal {
            proposer: info.sender.clone(),
            timestamp: env.block.time,
        },
    )?;

    Ok(Response::new()
        .add_attribute("action", "New proposal")
        .add_attribute("proposer", info.sender))
}

/// Resolve an existing proposal
/*
@note
•   if voting tokens are already in the contract (not enough to pass a vote), 
    a user with enough tokens can add enough to make a vote pass after proposing
    themselves (removing a proposal via this method if they need to)

*/
pub fn resolve_proposal(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let current_proposal = PROPOSAL.load(deps.storage)?;

    if current_proposal
        .timestamp
        .plus_seconds(config.voting_window)
        < env.block.time
    {
        return Err(ContractError::ProposalNotReady {});
    }

    //@note get voting token info
    //@note code to get token info and balance is all fine
    let vtoken_info: TokenInfoResponse =
        deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.voting_token.to_string(),
            msg: to_binary(&Cw20QueryMsg::TokenInfo {})?,
        }))?;

    //@note how much voting token does this contract have?
    let balance: BalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: config.voting_token.to_string(),
        msg: to_binary(&Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        })?,
    }))?;

    let mut response = Response::new().add_attribute("action", "resolve_proposal");

    //@note rounding error, should use checked_ratio
    if balance.balance >= (vtoken_info.total_supply / Uint128::from(3u32)) {
        CONFIG.update(deps.storage, |mut config| -> StdResult<_> {
            config.owner = current_proposal.proposer;
            Ok(config)
        })?;
        response = response.add_attribute("result", "Passed");
    } else {
        PROPOSAL.remove(deps.storage);
        response = response.add_attribute("result", "Failed");
    }

    Ok(response)
}

/// Entry point for owner to execute arbitrary Cosmos messages
//@note cannot do anything with this
pub fn owner_action(
    deps: DepsMut,
    info: MessageInfo,
    msg: CosmosMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    Ok(Response::new()
        .add_attribute("action", "owner_action")
        .add_message(msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Proposal {} => to_binary(&query_proposal(deps)?),
        QueryMsg::Balance {} => to_binary(&query_balance(deps, env)?),
    }
}

/// Returns contract configuration
pub fn query_config(deps: Deps) -> StdResult<Config> {
    CONFIG.load(deps.storage)
}

/// Returns proposal information
pub fn query_proposal(deps: Deps) -> StdResult<Proposal> {
    PROPOSAL.load(deps.storage)
}

/// Returns balance of voting token in this contract
pub fn query_balance(deps: Deps, env: Env) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;
    let balance: BalanceResponse = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: config.voting_token.to_string(),
        msg: to_binary(&Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        })?,
    }))?;

    Ok(balance.balance)
}
