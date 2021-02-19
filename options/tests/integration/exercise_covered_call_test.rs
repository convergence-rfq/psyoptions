use crate::{
  option_helpers::{create_and_add_option_writer, create_exerciser, init_option_market},
  solana_helpers
};
use solana_client::rpc_client::RpcClient;
use solana_options::market::OptionMarket;
use solana_program::{
  clock::Clock,
  program_pack::Pack,
  sysvar::{clock, Sysvar}
};
use solana_sdk::{
  account::create_account_infos,
  commitment_config::CommitmentConfig,
  signature::Signer,
};
use spl_token::state::{Account, Mint};
use std::{
  thread,
  time::{Duration},
};
use serial_test::serial;

#[test]
#[serial]
pub fn test_sucessful_exercise_covered_call() {
  // Create the options market
  let client = RpcClient::new_with_commitment(
    "http://localhost:8899".to_string(),
    CommitmentConfig::processed(),
  );
  let options_program_id = solana_helpers::load_bpf_program(&client, "solana_options");
  let amount_per_contract = 100;
  let strike_price = 5;
  let expiry = 999_999_999_999_999_999;
  // Create the option market
  let (
    underlying_asset_mint_keys,
    quote_asset_mint_keys,
    option_mint_keys,
    asset_authority_keys,
    underlying_asset_pool_key,
    option_market_key,
  ) = init_option_market(
    &client,
    &options_program_id,
    amount_per_contract,
    strike_price,
    expiry,
  )
  .unwrap();

  // Add 2 option writers to it
  let (contract_token_keys, contract_token_authority_keys) = create_and_add_option_writer(
    &client,
    &options_program_id,
    &underlying_asset_mint_keys,
    &asset_authority_keys,
    &quote_asset_mint_keys,
    &option_mint_keys,
    &underlying_asset_pool_key,
    &option_market_key,
    amount_per_contract,
  ).unwrap();
  create_and_add_option_writer(
    &client,
    &options_program_id,
    &underlying_asset_mint_keys,
    &asset_authority_keys,
    &quote_asset_mint_keys,
    &option_mint_keys,
    &underlying_asset_pool_key,
    &option_market_key,
    amount_per_contract,
  ).unwrap();

  // pick one of the option writers from the OptionMarket account
  let option_market_data = client.get_account_data(&option_market_key).unwrap();
  let option_market = OptionMarket::unpack(&option_market_data[..]).unwrap();
  let option_writer = &option_market.option_writer_registry[0];
  // create an option exerciser with SPL accounts we can check
  let (exerciser_authority_keys, exerciser_quote_asset_keys, exerciser_underlying_asset_keys) =
    create_exerciser(
      &client,
      &asset_authority_keys,
      &underlying_asset_mint_keys,
      &quote_asset_mint_keys,
      &option_market,
    )
    .unwrap();

  // generate the exercise_covered_call instruction
  let exercise_covered_call_ix = solana_options::instruction::exercise_covered_call(
    &options_program_id,
    option_writer,
    &option_mint_keys.pubkey(),
    &option_market_key,
    &exerciser_quote_asset_keys.pubkey(),
    &exerciser_underlying_asset_keys.pubkey(),
    &exerciser_authority_keys.pubkey(),
    &option_market.asset_pool_address,
    &contract_token_keys.pubkey(),
    &contract_token_authority_keys.pubkey(),
  )
  .unwrap();
  let underlying_asset_pool_acct_data =
    client.get_account_data(&underlying_asset_pool_key).unwrap();
  let initial_underlying_asset_pool_acct = Account::unpack(&underlying_asset_pool_acct_data[..]).unwrap();

  // Hold some initial values in memory for assertions
  let exerciser_quote_asset_acct_data =
    client.get_account_data(&exerciser_quote_asset_keys.pubkey()).unwrap();
  let exerciser_quote_asset_acct = Account::unpack(&exerciser_quote_asset_acct_data[..]).unwrap();

  let option_mint_data = client.get_account_data(&option_mint_keys.pubkey()).unwrap();
  let option_mint = Mint::unpack(&option_mint_data[..]).unwrap();
  assert_eq!(option_mint.supply, 2);

  // Send the transaction
  let signers = vec![&exerciser_authority_keys, &contract_token_authority_keys];
  solana_helpers::send_and_confirm_transaction(
    &client,
    exercise_covered_call_ix,
    &exerciser_authority_keys.pubkey(),
    signers,
  )
  .unwrap();

  // assert that the total supply of options has decremented by 1
  let option_mint_data = client.get_account_data(&option_mint_keys.pubkey()).unwrap();
  let option_mint = Mint::unpack(&option_mint_data[..]).unwrap();
  assert_eq!(option_mint.supply, 1);

  let option_market_data = client.get_account_data(&option_market_key).unwrap();
  let updated_option_market = OptionMarket::unpack(&option_market_data[..]).unwrap();
  // assert that the OptionMarket.registry_length decremented
  assert_eq!(updated_option_market.registry_length, option_market.registry_length - 1);
  // assert that the 1 OptionWriter is removed from the OptionMarket.option_writer_registry
  // TODO make this more robust/exhaustive
  assert_ne!(*option_writer, updated_option_market.option_writer_registry[0]);

  // assert that the underlying_asset_pool size decreased by amount_per_contract
  let underlying_asset_pool_acct_data =
    client.get_account_data(&underlying_asset_pool_key).unwrap();
  let underlying_asset_pool_acct = Account::unpack(&underlying_asset_pool_acct_data[..]).unwrap();
  assert_eq!(
    underlying_asset_pool_acct.mint,
    underlying_asset_mint_keys.pubkey()
  );
  let expected_pool_amount = initial_underlying_asset_pool_acct.amount - amount_per_contract;
  assert_eq!(underlying_asset_pool_acct.amount, expected_pool_amount);
  // assert that the exerciser received the underlying asset
  let exerciser_underlying_asset_acct_data =
    client.get_account_data(&exerciser_underlying_asset_keys.pubkey()).unwrap();
  let exerciser_underlying_asset_acct = Account::unpack(&exerciser_underlying_asset_acct_data[..]).unwrap();
  assert_eq!(exerciser_underlying_asset_acct.amount, option_market.amount_per_contract);
  // assert that the exerciser's quote asset account is less the amount required to close the contract
  let exerciser_quote_asset_acct_data =
    client.get_account_data(&exerciser_quote_asset_keys.pubkey()).unwrap();
  let updated_exerciser_quote_asset_acct = Account::unpack(&exerciser_quote_asset_acct_data[..]).unwrap();
  assert_eq!(updated_exerciser_quote_asset_acct.amount, exerciser_quote_asset_acct.amount - (option_market.amount_per_contract * option_market.strike_price));
}

#[test]
#[should_panic(expected = "Error processing Instruction 0: custom program error: 0x5")]
pub fn test_panic_when_expiration_has_not_passed() {
  // Create the options market
  let client = RpcClient::new_with_commitment(
    "http://localhost:8899".to_string(),
    CommitmentConfig::processed(),
  );
  let options_program_id = solana_helpers::load_bpf_program(&client, "solana_options");
  let amount_per_contract = 100;
  let strike_price = 5;
  // Get the current network clock time to use as the basis for the expiration
  let sysvar_clock_acct = client.get_account(&clock::id()).unwrap();
  let accounts = &mut [(clock::id(), sysvar_clock_acct)];
  let sysvar_clock_acct_info = &create_account_infos(accounts)[0];
  let clock = Clock::from_account_info(&sysvar_clock_acct_info).unwrap();
  let expiry = clock.unix_timestamp + 30;
  // Create the option market
  let (
    underlying_asset_mint_keys,
    quote_asset_mint_keys,
    option_mint_keys,
    asset_authority_keys,
    underlying_asset_pool_key,
    option_market_key,
  ) = init_option_market(
    &client,
    &options_program_id,
    amount_per_contract,
    strike_price,
    expiry,
  )
  .unwrap();

  // Add 2 option writers to it
  let (contract_token_keys, contract_token_authority_keys) = create_and_add_option_writer(
    &client,
    &options_program_id,
    &underlying_asset_mint_keys,
    &asset_authority_keys,
    &quote_asset_mint_keys,
    &option_mint_keys,
    &underlying_asset_pool_key,
    &option_market_key,
    amount_per_contract,
  ).unwrap();
  create_and_add_option_writer(
    &client,
    &options_program_id,
    &underlying_asset_mint_keys,
    &asset_authority_keys,
    &quote_asset_mint_keys,
    &option_mint_keys,
    &underlying_asset_pool_key,
    &option_market_key,
    amount_per_contract,
  ).unwrap();

  // pick one of the option writers from the OptionMarket account
  let option_market_data = client.get_account_data(&option_market_key).unwrap();
  let option_market = OptionMarket::unpack(&option_market_data[..]).unwrap();
  let option_writer = &option_market.option_writer_registry[0];
  // create an option exerciser with SPL accounts we can check
  let (exerciser_authority_keys, exerciser_quote_asset_keys, exerciser_underlying_asset_keys) =
    create_exerciser(
      &client,
      &asset_authority_keys,
      &underlying_asset_mint_keys,
      &quote_asset_mint_keys,
      &option_market,
    )
    .unwrap();
  
  thread::sleep(Duration::from_secs(20));

  // generate the exercise_covered_call instruction
  let exercise_covered_call_ix = solana_options::instruction::exercise_covered_call(
    &options_program_id,
    option_writer,
    &option_mint_keys.pubkey(),
    &option_market_key,
    &exerciser_quote_asset_keys.pubkey(),
    &exerciser_underlying_asset_keys.pubkey(),
    &exerciser_authority_keys.pubkey(),
    &option_market.asset_pool_address,
    &contract_token_keys.pubkey(),
    &contract_token_authority_keys.pubkey(),
  )
  .unwrap();
  // Send the transaction
  let signers = vec![&exerciser_authority_keys, &contract_token_authority_keys];
  solana_helpers::send_and_confirm_transaction(
    &client,
    exercise_covered_call_ix,
    &exerciser_authority_keys.pubkey(),
    signers,
  )
  .unwrap();
}