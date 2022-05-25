use std::convert::TryInto;

use derive_more::From;
use ergo_lib::{
    chain::ergo_box::box_builder::{ErgoBoxCandidateBuilder, ErgoBoxCandidateBuilderError},
    ergotree_interpreter::sigma_protocol::prover::ContextExtension,
    ergotree_ir::{
        chain::{
            address::Address,
            ergo_box::{
                box_value::BoxValue,
                NonMandatoryRegisterId::{R4, R5, R6},
            },
            token::Token,
        },
        serialization::SigmaParsingError,
    },
    wallet::{
        box_selector::{BoxSelection, BoxSelector, BoxSelectorError, SimpleBoxSelector},
        tx_builder::{TxBuilder, TxBuilderError},
    },
};
use ergo_node_interface::node_interface::NodeError;
use thiserror::Error;

use crate::{
    actions::TransferOracleTokenAction,
    box_kind::OracleBox,
    oracle_state::{LocalDatapointBoxSource, StageError},
    wallet::WalletDataSource,
};

#[derive(Debug, Error, From)]
pub enum TransferOracleTokenActionError {
    #[error("Oracle box should contain exactly 1 reward token. It contains {0} tokens")]
    IncorrectNumberOfRewardTokensInOracleBox(usize),
    #[error("Destination address not P2PK")]
    IncorrectDestinationAddress,
    #[error("box builder error: {0}")]
    ErgoBoxCandidateBuilder(ErgoBoxCandidateBuilderError),
    #[error("stage error: {0}")]
    StageError(StageError),
    #[error("node error: {0}")]
    Node(NodeError),
    #[error("box selector error: {0}")]
    BoxSelector(BoxSelectorError),
    #[error("Sigma parsing error: {0}")]
    SigmaParse(SigmaParsingError),
    #[error("tx builder error: {0}")]
    TxBuilder(TxBuilderError),
}

pub fn build_transfer_oracle_token_action(
    local_datapoint_box_source: &dyn LocalDatapointBoxSource,
    wallet: &dyn WalletDataSource,
    oracle_token_destination: Address,
    height: u32,
    change_address: Address,
) -> Result<TransferOracleTokenAction, TransferOracleTokenActionError> {
    let in_oracle_box = local_datapoint_box_source.get_local_oracle_datapoint_box()?;
    let num_reward_tokens = *in_oracle_box.reward_token().amount.as_u64();
    if num_reward_tokens != 1 {
        return Err(
            TransferOracleTokenActionError::IncorrectNumberOfRewardTokensInOracleBox(
                num_reward_tokens as usize,
            ),
        );
    }
    if let Address::P2Pk(p) = &oracle_token_destination {
        // Build the new oracle box
        let mut builder = ErgoBoxCandidateBuilder::new(
            in_oracle_box.get_box().value,
            in_oracle_box.get_box().ergo_tree.clone(),
            height,
        );
        let ec_point = *p.h.clone();
        builder.set_register_value(R4, ec_point.into());
        builder.set_register_value(R5, (in_oracle_box.epoch_counter() as i32).into());
        builder.set_register_value(R6, (in_oracle_box.rate() as i64).into());
        builder.add_token(in_oracle_box.oracle_token().clone());

        let single_reward_token = Token {
            token_id: in_oracle_box.reward_token().token_id.clone(),
            amount: 1.try_into().unwrap(),
        };
        builder.add_token(single_reward_token);
        let oracle_box_candidate = builder.build()?;

        let unspent_boxes = wallet.get_unspent_wallet_boxes()?;

        let target_balance = BoxValue::SAFE_USER_MIN;

        let box_selector = SimpleBoxSelector::new();
        let selection = box_selector.select(unspent_boxes, target_balance, &[])?;
        let mut input_boxes = vec![in_oracle_box.get_box()];
        input_boxes.append(selection.boxes.as_vec().clone().as_mut());
        let box_selection = BoxSelection {
            boxes: input_boxes.try_into().unwrap(),
            change_boxes: selection.change_boxes,
        };
        let mut tx_builder = TxBuilder::new(
            box_selection,
            vec![oracle_box_candidate],
            height,
            target_balance,
            change_address,
            BoxValue::MIN,
        );
        // The following context value ensures that `outIndex` in the oracle contract is properly set.
        let ctx_ext = ContextExtension {
            values: vec![(0, 0i32.into())].into_iter().collect(),
        };
        tx_builder.set_context_extension(in_oracle_box.get_box().box_id(), ctx_ext);
        let tx = tx_builder.build()?;
        Ok(TransferOracleTokenAction { tx })
    } else {
        Err(TransferOracleTokenActionError::IncorrectDestinationAddress)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryInto;

    use super::*;
    use crate::commands::test_utils::{
        find_input_boxes, make_datapoint_box, make_wallet_unspent_box, OracleBoxMock,
        WalletDataMock,
    };
    use crate::contracts::refresh::RefreshContract;
    use ergo_lib::chain::ergo_state_context::ErgoStateContext;
    use ergo_lib::ergotree_interpreter::sigma_protocol::private_input::DlogProverInput;
    use ergo_lib::ergotree_ir::chain::address::AddressEncoder;
    use ergo_lib::ergotree_ir::chain::ergo_box::box_value::BoxValue;
    use ergo_lib::ergotree_ir::chain::token::{Token, TokenId};
    use ergo_lib::wallet::signing::TransactionContext;
    use ergo_lib::wallet::Wallet;
    use sigma_test_util::force_any_val;

    #[test]
    fn test_transfer_oracle_datapoint() {
        let ctx = force_any_val::<ErgoStateContext>();
        let height = ctx.pre_header.height;
        let refresh_contract = RefreshContract::new();
        let reward_token_id =
            TokenId::from_base64("RytLYlBlU2hWbVlxM3Q2dzl6JEMmRilKQE1jUWZUalc=").unwrap();
        dbg!(&reward_token_id);
        let secret = force_any_val::<DlogProverInput>();
        let wallet = Wallet::from_secrets(vec![secret.clone().into()]);
        let oracle_pub_key = secret.public_image().h;

        let oracle_box = make_datapoint_box(
            *oracle_pub_key,
            200,
            1,
            refresh_contract.oracle_nft_token_id(),
            Token::from((reward_token_id, 1u64.try_into().unwrap())),
            BoxValue::SAFE_USER_MIN.checked_mul_u32(100).unwrap(),
            height - 9,
        )
        .try_into()
        .unwrap();
        let local_datapoint_box_source = OracleBoxMock { oracle_box };

        let change_address =
            AddressEncoder::new(ergo_lib::ergotree_ir::chain::address::NetworkPrefix::Mainnet)
                .parse_address_from_str("9iHyKxXs2ZNLMp9N9gbUT9V8gTbsV7HED1C1VhttMfBUMPDyF7r")
                .unwrap();

        let wallet_unspent_box = make_wallet_unspent_box(
            secret.public_image(),
            BoxValue::SAFE_USER_MIN.checked_mul_u32(10000).unwrap(),
        );
        let wallet_mock = WalletDataMock {
            unspent_boxes: vec![wallet_unspent_box],
        };
        let action = build_transfer_oracle_token_action(
            &local_datapoint_box_source,
            &wallet_mock,
            change_address.clone(),
            height,
            change_address,
        )
        .unwrap();

        let mut possible_input_boxes = vec![local_datapoint_box_source
            .get_local_oracle_datapoint_box()
            .unwrap()
            .get_box()];
        possible_input_boxes.append(&mut wallet_mock.get_unspent_wallet_boxes().unwrap());

        let tx_context = TransactionContext::new(
            action.tx.clone(),
            find_input_boxes(action.tx, possible_input_boxes),
            None,
        )
        .unwrap();

        let _signed_tx = wallet.sign_transaction(tx_context, &ctx, None).unwrap();
    }
}
