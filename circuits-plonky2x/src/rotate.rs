use async_trait::async_trait;
use ethers::types::H256;
use log::{debug, Level};
use plonky2x::backend::circuit::Circuit;
use plonky2x::frontend::hint::asynchronous::hint::AsyncHint;
use plonky2x::frontend::uint::uint64::U64Variable;
use plonky2x::frontend::vars::U32Variable;
use plonky2x::prelude::{
    ArrayVariable, Bytes32Variable, CircuitBuilder, Field, PlonkParameters, ValueStream, Variable,
    VariableStream,
};
use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::builder::decoder::FloorDivGenerator;
use crate::builder::header::HeaderMethods;
use crate::builder::justification::{GrandpaJustificationVerifier, HintSimpleJustification};
use crate::builder::rotate::RotateMethods;
use crate::input::RpcDataFetcher;
use crate::vars::{AvailPubkeyVariable, EncodedHeader, EncodedHeaderVariable};

// Fetch a single header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotateHint<const HEADER_LENGTH: usize, const MAX_AUTHORITY_SET_SIZE: usize> {}

#[async_trait]
impl<
        const HEADER_LENGTH: usize,
        const MAX_AUTHORITY_SET_SIZE: usize,
        L: PlonkParameters<D>,
        const D: usize,
    > AsyncHint<L, D> for RotateHint<HEADER_LENGTH, MAX_AUTHORITY_SET_SIZE>
{
    async fn hint(
        &self,
        input_stream: &mut ValueStream<L, D>,
        output_stream: &mut ValueStream<L, D>,
    ) {
        let block_number = input_stream.read_value::<U32Variable>();

        debug!(
            "SingleHeaderFetcherHint: downloading header range of block={}",
            block_number
        );

        let data_fetcher = RpcDataFetcher::new().await;

        let rotate_data = data_fetcher
            .get_header_rotate::<HEADER_LENGTH, MAX_AUTHORITY_SET_SIZE>(block_number)
            .await;

        // Encoded header.
        output_stream.write_value::<EncodedHeaderVariable<HEADER_LENGTH>>(EncodedHeader {
            header_bytes: rotate_data.header_bytes,
            header_size: L::Field::from_canonical_usize(rotate_data.header_size),
        });

        // Number of authorities.
        output_stream
            .write_value::<Variable>(L::Field::from_canonical_usize(rotate_data.num_authorities));

        // Start of consensus log.
        output_stream
            .write_value::<Variable>(L::Field::from_canonical_usize(rotate_data.start_position));

        // End position.
        output_stream
            .write_value::<Variable>(L::Field::from_canonical_usize(rotate_data.end_position));

        // Expected new authority set hash.
        output_stream.write_value::<Bytes32Variable>(H256::from_slice(
            rotate_data.new_authority_set_hash.as_slice(),
        ));

        // Pubkeys of the new authority set.
        output_stream.write_value::<ArrayVariable<AvailPubkeyVariable, MAX_AUTHORITY_SET_SIZE>>(
            rotate_data.padded_pubkeys,
        );
    }
}

#[derive(Clone, Debug)]
pub struct RotateCircuit<
    const MAX_AUTHORITY_SET_SIZE: usize,
    const MAX_HEADER_LENGTH: usize,
    const MAX_HEADER_CHUNK_SIZE: usize,
    // This should be (MAX_AUTHORITY_SET_SIZE + 1) * (PUBKEY_LENGTH + WEIGHT_LENGTH).
    const MAX_SUBARRAY_SIZE: usize,
> {}

impl<
        const MAX_AUTHORITY_SET_SIZE: usize,
        const MAX_HEADER_LENGTH: usize,
        const MAX_HEADER_CHUNK_SIZE: usize,
        const MAX_SUBARRAY_SIZE: usize,
    > Circuit
    for RotateCircuit<
        MAX_AUTHORITY_SET_SIZE,
        MAX_HEADER_LENGTH,
        MAX_HEADER_CHUNK_SIZE,
        MAX_SUBARRAY_SIZE,
    >
{
    fn define<L: PlonkParameters<D>, const D: usize>(builder: &mut CircuitBuilder<L, D>)
    where
        <<L as PlonkParameters<D>>::Config as plonky2::plonk::config::GenericConfig<D>>::Hasher:
            plonky2::plonk::config::AlgebraicHasher<L::Field>,
    {
        // Read the on-chain inputs.
        // The validators that sign epoch_end_block_number are defined by authority_set_id and authority_set_hash.

        let authority_set_id = builder.evm_read::<U64Variable>();
        builder.watch_with_level(
            &authority_set_id,
            "rotate circuit input - authority set id",
            Level::Debug,
        );
        let authority_set_hash = builder.evm_read::<Bytes32Variable>();
        builder.watch_with_level(
            &authority_set_hash,
            "rotate circuit input - authority set hash",
            Level::Debug,
        );
        // Note: If the user passes in a block number that is not an epoch end block, the circuit will error.
        let epoch_end_block_number = builder.evm_read::<U32Variable>();
        builder.watch_with_level(
            &epoch_end_block_number,
            "rotate circuit input - epoch end block",
            Level::Debug,
        );

        // Fetch the header at epoch_end_block.
        let header_fetcher = RotateHint::<MAX_HEADER_LENGTH, MAX_AUTHORITY_SET_SIZE> {};
        let mut input_stream = VariableStream::new();
        input_stream.write(&epoch_end_block_number);
        let output_stream = builder.async_hint(input_stream, header_fetcher);

        let target_header = output_stream.read::<EncodedHeaderVariable<MAX_HEADER_LENGTH>>(builder);
        let num_authorities = output_stream.read::<Variable>(builder);
        let start_position = output_stream.read::<Variable>(builder);
        let end_position = output_stream.read::<Variable>(builder);
        let expected_new_authority_set_hash = output_stream.read::<Bytes32Variable>(builder);
        let new_pubkeys = output_stream
            .read::<ArrayVariable<AvailPubkeyVariable, MAX_AUTHORITY_SET_SIZE>>(builder);

        // TODO: USE target_header_hash as randomness!
        // // Hash the header at epoch_end_block.
        let target_header_hash =
            builder.hash_encoded_header::<MAX_HEADER_LENGTH, MAX_HEADER_CHUNK_SIZE>(&target_header);
        // let mut hasher = sha2::Sha256::new();
        // hasher.update("Hello World");
        // let target_header_hash =
        //     builder.constant::<Bytes32Variable>(H256::from_slice(&hasher.finalize()));

        // Call rotate on the header.
        builder.rotate::<MAX_HEADER_LENGTH, MAX_AUTHORITY_SET_SIZE, MAX_SUBARRAY_SIZE>(
            &target_header,
            &target_header_hash,
            &num_authorities,
            &start_position,
            &end_position,
            &new_pubkeys,
            &expected_new_authority_set_hash,
        );

        // // Verify the epoch end block header is valid.
        // builder.verify_simple_justification::<MAX_AUTHORITY_SET_SIZE>(
        //     epoch_end_block_number,
        //     target_header_hash,
        //     authority_set_id,
        //     authority_set_hash,
        // );

        // TODO: Write the hash of the authority set to the output
        builder.evm_write::<Bytes32Variable>(expected_new_authority_set_hash);
    }

    fn register_generators<L: PlonkParameters<D>, const D: usize>(
        generator_registry: &mut plonky2x::prelude::HintRegistry<L, D>,
    ) where
        <<L as PlonkParameters<D>>::Config as plonky2::plonk::config::GenericConfig<D>>::Hasher:
            plonky2::plonk::config::AlgebraicHasher<L::Field>,
    {
        generator_registry
            .register_async_hint::<RotateHint<MAX_HEADER_LENGTH, MAX_AUTHORITY_SET_SIZE>>();
        generator_registry.register_async_hint::<HintSimpleJustification<MAX_AUTHORITY_SET_SIZE>>();

        let floor_div_id = FloorDivGenerator::<L::Field, D>::id();
        generator_registry.register_simple::<FloorDivGenerator<L::Field, D>>(floor_div_id);
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use ethers::types::H256;
    use plonky2x::prelude::{DefaultBuilder, GateRegistry, HintRegistry};

    use super::*;
    use crate::consts::MAX_HEADER_SIZE;

    #[test]
    #[cfg_attr(feature = "ci", ignore)]
    fn test_rotate_serialization() {
        env::set_var("RUST_LOG", "debug");
        env_logger::try_init().unwrap_or_default();

        const NUM_AUTHORITIES: usize = 4;
        const MAX_HEADER_LENGTH: usize = MAX_HEADER_SIZE;
        const MAX_HEADER_CHUNK_SIZE: usize = 100;
        const MAX_SUBARRAY_SIZE: usize = (NUM_AUTHORITIES + 1) * 40;

        let mut builder = DefaultBuilder::new();

        log::debug!("Defining circuit");
        RotateCircuit::<NUM_AUTHORITIES, MAX_HEADER_LENGTH, MAX_HEADER_CHUNK_SIZE, MAX_SUBARRAY_SIZE>::define(
            &mut builder,
        );
        let circuit = builder.build();
        log::debug!("Done building circuit");

        let mut hint_registry = HintRegistry::new();
        let mut gate_registry = GateRegistry::new();
        RotateCircuit::<NUM_AUTHORITIES, MAX_HEADER_LENGTH, MAX_HEADER_CHUNK_SIZE, MAX_SUBARRAY_SIZE>::register_generators(
            &mut hint_registry,
        );
        RotateCircuit::<NUM_AUTHORITIES, MAX_HEADER_LENGTH, MAX_HEADER_CHUNK_SIZE, MAX_SUBARRAY_SIZE>::register_gates(
            &mut gate_registry,
        );

        circuit.test_serializers(&gate_registry, &hint_registry);
    }

    #[test]
    #[cfg_attr(feature = "ci", ignore)]
    fn test_rotate_1() {
        env::set_var("RUST_LOG", "debug");
        env_logger::try_init().unwrap_or_default();

        const NUM_AUTHORITIES: usize = 100;
        const MAX_HEADER_LENGTH: usize = MAX_HEADER_SIZE;
        const MAX_HEADER_CHUNK_SIZE: usize = 100;
        const MAX_SUBARRAY_SIZE: usize = (NUM_AUTHORITIES + 1) * 40;

        let mut builder = DefaultBuilder::new();

        log::debug!("Defining circuit");
        RotateCircuit::<NUM_AUTHORITIES, MAX_HEADER_LENGTH, MAX_HEADER_CHUNK_SIZE, MAX_SUBARRAY_SIZE>::define(
            &mut builder,
        );

        log::debug!("Building circuit");
        let circuit = builder.build();
        log::debug!("Done building circuit");

        // These inputs are taken from: https://kate.avail.tools/#/explorer/query/485710
        let mut input = circuit.input();
        let authority_set_id = 299u64;
        let authority_set_hash: [u8; 32] = [0u8; 32]; // Placeholder for now
        let epoch_end_block_number = 318937u32;

        input.evm_write::<U64Variable>(authority_set_id);
        input.evm_write::<Bytes32Variable>(H256::from_slice(authority_set_hash.as_slice()));
        input.evm_write::<U32Variable>(epoch_end_block_number);

        log::debug!("Generating proof");
        let (proof, mut output) = circuit.prove(&input);
        log::debug!("Done generating proof");

        circuit.verify(&proof, &input, &output);
        let new_authority_set_hash = output.evm_read::<Bytes32Variable>();
        println!("new_authority_set_hash {:?}", new_authority_set_hash);
    }
}
