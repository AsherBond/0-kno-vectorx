use plonky2x::frontend::curta::ec::point::CompressedEdwardsYVariable;
use plonky2x::frontend::uint::uint64::U64Variable;
use plonky2x::frontend::vars::{CircuitVariable, U32Variable};
use plonky2x::prelude::{
    ArrayVariable, ByteVariable, Bytes32Variable, CircuitBuilder, Field, PlonkParameters, Variable,
};

use super::decoder::DecodingMethods;
use super::header::HeaderMethods;
use crate::builder::justification::GrandpaJustificationVerifier;
use crate::consts::{
    CONSENSUS_ENGINE_ID_PREFIX_LENGTH, DELAY_LENGTH, MAX_COMPACT_UINT_BYTES, MAX_PREFIX_LENGTH,
    PUBKEY_LENGTH, VALIDATOR_LENGTH, WEIGHT_LENGTH,
};
use crate::vars::*;

pub trait RotateMethods {
    /// Verifies the log is a consensus log.
    fn verify_consensus_log<const MAX_PREFIX_LENGTH: usize>(
        &mut self,
        subarray: &ArrayVariable<ByteVariable, MAX_PREFIX_LENGTH>,
    );

    /// Verify the prefix's scheduled change message length & scheduled change flag. Return the
    /// index in the prefix after the scheduled change flag.
    fn verify_scheduled_change_message_length_and_flag<const MAX_PREFIX_LENGTH: usize>(
        &mut self,
        subarray: &ArrayVariable<ByteVariable, MAX_PREFIX_LENGTH>,
    ) -> Variable;

    /// Verify the encoded new authority set size matches the untrusted_authority_set_size supplied
    /// from the hint and return the index after the encoded new authority set size (which is the
    /// total prefix length).
    fn verify_encoded_num_authorities<const MAX_PREFIX_LENGTH: usize>(
        &mut self,
        subarray: &ArrayVariable<ByteVariable, MAX_PREFIX_LENGTH>,
        prefix_cursor: Variable,
        header_hash: Bytes32Variable,
        untrusted_authority_set_size: Variable,
    ) -> Variable;

    /// Verifies the epoch end header has a valid encoding, and that the new_pubkeys match the header's
    /// encoded pubkeys. The purpose of this function is to ensure that it is difficult for
    /// a malicious prover to prove an incorrect new authority set from a correctly signed header by
    /// adding constraints on the encoding of the new authority set.
    fn verify_epoch_end_header<
        const MAX_HEADER_SIZE: usize,
        const MAX_AUTHORITY_SET_SIZE: usize,
        const MAX_SUBARRAY_SIZE: usize,
    >(
        &mut self,
        header: &EncodedHeaderVariable<MAX_HEADER_SIZE>,
        header_hash: Bytes32Variable,
        num_authorities: &Variable,
        start_position: &Variable,
        new_pubkeys: &ArrayVariable<CompressedEdwardsYVariable, MAX_AUTHORITY_SET_SIZE>,
    );

    // Verify the justification from the current authority set on the epoch end header and extract
    // the new authority set commitment.
    fn rotate<
        const MAX_HEADER_SIZE: usize,
        const MAX_AUTHORITY_SET_SIZE: usize,
        const MAX_SUBARRAY_SIZE: usize,
    >(
        &mut self,
        current_authority_set_id: U64Variable,
        current_authority_set_hash: Bytes32Variable,
        rotate: RotateVariable<MAX_HEADER_SIZE, MAX_AUTHORITY_SET_SIZE>,
    ) -> Bytes32Variable;
}

impl<L: PlonkParameters<D>, const D: usize> RotateMethods for CircuitBuilder<L, D> {
    fn verify_consensus_log<const MAX_PREFIX_LENGTH: usize>(
        &mut self,
        subarray: &ArrayVariable<ByteVariable, MAX_PREFIX_LENGTH>,
    ) {
        // Digest Spec: https://github.com/availproject/avail/blob/188c20d6a1577670da65e0c6e1c2a38bea8239bb/avail-subxt/src/api_dev.rs#L30820-L30842
        // Skip 1 byte.

        // Verify subarray[1] is 0x04 (Consensus Flag = 4u32).
        let consensus_enum_flag = self.constant::<ByteVariable>(4u8);
        let header_consensus_flag = subarray[1];
        self.assert_is_equal(header_consensus_flag, consensus_enum_flag);

        // Verify subarray[2..6] is the Consensus Engine ID: 0x46524e4b [70, 82, 78, 75].
        // Consensus Id: https://github.com/availproject/avail/blob/188c20d6a1577670da65e0c6e1c2a38bea8239bb/avail-subxt/examples/download_digest_items.rs#L41-L56
        let consensus_engine_id_bytes =
            self.constant::<ArrayVariable<ByteVariable, 4>>([70u8, 82u8, 78u8, 75u8].to_vec());
        self.assert_is_equal(
            ArrayVariable::<ByteVariable, 4>::from(subarray[2..6].to_vec()),
            consensus_engine_id_bytes,
        );
    }

    fn verify_scheduled_change_message_length_and_flag<const MAX_PREFIX_LENGTH: usize>(
        &mut self,
        subarray: &ArrayVariable<ByteVariable, MAX_PREFIX_LENGTH>,
    ) -> Variable {
        let one_v = self.one();

        // The scheduled change section of the prefix starts after the consensus engine id section.
        let mut prefix_cursor = self.constant::<Variable>(L::Field::from_canonical_usize(
            CONSENSUS_ENGINE_ID_PREFIX_LENGTH,
        ));

        // The SCALE-encoded scheduled change message length.
        let encoded_scheduled_change_message_length =
            ArrayVariable::<ByteVariable, MAX_COMPACT_UINT_BYTES>::from(
                subarray[CONSENSUS_ENGINE_ID_PREFIX_LENGTH
                    ..CONSENSUS_ENGINE_ID_PREFIX_LENGTH + MAX_COMPACT_UINT_BYTES]
                    .to_vec(),
            );
        // Note: Discard the value of the scheduled change message length as it is not checked.
        let (_, compress_mode) = self.decode_compact_int(encoded_scheduled_change_message_length);

        // Compute the size in bytes of the compact int representing the scheduled change message length.
        let encoded_scheduled_change_message_length_byte_length =
            self.get_compact_int_byte_length(compress_mode);

        // Skip over the encoded scheduled change message length.
        prefix_cursor = self.add(
            prefix_cursor,
            encoded_scheduled_change_message_length_byte_length,
        );

        // Verify the next byte after the encoded scheduled change message length is the scheduled
        // change enum flag.
        let scheduled_change_enum_flag = self.constant::<ByteVariable>(1u8);
        let header_schedule_change_flag =
            self.select_array_random_gate(&subarray.data, prefix_cursor);
        self.assert_is_equal(header_schedule_change_flag, scheduled_change_enum_flag);

        // Return the index after the scheduled change flag.
        self.add(prefix_cursor, one_v)
    }

    fn verify_encoded_num_authorities<const MAX_PREFIX_LENGTH: usize>(
        &mut self,
        subarray: &ArrayVariable<ByteVariable, MAX_PREFIX_LENGTH>,
        prefix_cursor: Variable,
        header_hash: Bytes32Variable,
        untrusted_authority_set_size: Variable,
    ) -> Variable {
        // Digest Spec: https://github.com/availproject/avail/blob/188c20d6a1577670da65e0c6e1c2a38bea8239bb/avail-subxt/src/api_dev.rs#L30820-L30842

        // Verify the encoded authority set size matches the untrusted_authority_set_size supplied by
        // the hint.
        let encoded_authority_set_size = self
            .get_fixed_subarray::<MAX_PREFIX_LENGTH, MAX_COMPACT_UINT_BYTES>(
                subarray,
                prefix_cursor,
                &header_hash.as_bytes(),
            );
        let (decoded_authority_set_size, compress_mode) =
            self.decode_compact_int(encoded_authority_set_size);
        let num_authorities_u32 =
            U32Variable::from_variables(self, &[untrusted_authority_set_size]);
        self.assert_is_equal(decoded_authority_set_size, num_authorities_u32);

        // Compute the size in bytes of the compact int representing the new authority set size.
        let encoded_new_authority_set_length_size_bytes =
            self.get_compact_int_byte_length(compress_mode);

        // Return the index after the encoded new authority set size in the prefix.
        self.add(prefix_cursor, encoded_new_authority_set_length_size_bytes)
    }

    fn verify_epoch_end_header<
        const MAX_HEADER_SIZE: usize,
        const MAX_AUTHORITY_SET_SIZE: usize,
        const MAX_SUBARRAY_SIZE: usize,
    >(
        &mut self,
        header: &EncodedHeaderVariable<MAX_HEADER_SIZE>,
        header_hash: Bytes32Variable,
        num_authorities: &Variable,
        start_position: &Variable,
        new_pubkeys: &ArrayVariable<CompressedEdwardsYVariable, MAX_AUTHORITY_SET_SIZE>,
    ) {
        let false_v = self._false();
        let true_v = self._true();

        // Assert num_authorities is not 0.
        let num_authorities_zero = self.is_zero(*num_authorities);
        self.assert_is_equal(num_authorities_zero, false_v);

        // Initialize the cursor to the start position, which is the start of the consensus log
        // corresponding to an authority set change event in the epoch end header.
        let mut cursor = *start_position;

        // Get the subarray of the header bytes to verify. The header_hash is used as the seed for
        // randomness.
        let prefix_subarray = self.get_fixed_subarray::<MAX_HEADER_SIZE, MAX_PREFIX_LENGTH>(
            &header.header_bytes,
            cursor,
            &header_hash.as_bytes(),
        );

        // Verify the log start_position corresponds to is a consensus log.
        // Note: Checking encoding makes it more difficult for a malicious prover to witness an
        // incorrect new authority set by using a fake start_position.
        self.verify_consensus_log(&prefix_subarray);

        // Verify the SCALE-encoded scheduled change message length and scheduled change flag and get
        // the index after the scheduled change flag.
        let prefix_idx_after_scheduled_change_flag =
            self.verify_scheduled_change_message_length_and_flag(&prefix_subarray);

        // Verify the encoded authority set size and get the total prefix length.
        let total_prefix_length = self.verify_encoded_num_authorities(
            &prefix_subarray,
            prefix_idx_after_scheduled_change_flag,
            header_hash,
            *num_authorities,
        );

        // Add the total length of the prefix to get to the start of the encoded authority set.
        cursor = self.add(cursor, total_prefix_length);

        // Note: All validators have a voting power of 1 in Avail.
        // Spec: https://github.com/availproject/polkadot-sdk/blob/70e569d5112f879001a987e94402ff70f9683cb5/substrate/frame/grandpa/src/lib.rs#L585
        let expected_weight_bytes = self.constant::<ArrayVariable<ByteVariable, WEIGHT_LENGTH>>(
            [1u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8, 0u8].to_vec(),
        );
        // Expected delay for the authority set.
        let expected_delay_bytes =
            self.constant::<ArrayVariable<ByteVariable, 4>>([0u8, 0u8, 0u8, 0u8].to_vec());

        let enc_validator_subarray = self.get_fixed_subarray::<MAX_HEADER_SIZE, MAX_SUBARRAY_SIZE>(
            &header.header_bytes,
            cursor,
            &header_hash.as_bytes(),
        );

        let mut validator_disabled = self._false();
        // Verify num_authorities validators are present and valid.
        // Spec: https://github.com/paritytech/subxt/blob/cb67f944558a76f53167be7855c4725cdf80580c/testing/integration-tests/src/full_client/codegen/polkadot.rs#L9484-L9501
        for i in 0..(MAX_AUTHORITY_SET_SIZE) {
            let idx = i * VALIDATOR_LENGTH;
            let curr_validator = self.constant::<Variable>(L::Field::from_canonical_usize(i + 1));

            // Verify the correctness of the extracted pubkey for each enabled validator and
            // increment the cursor by the pubkey length.
            let extracted_pubkey =
                Bytes32Variable::from(&enc_validator_subarray[idx..idx + PUBKEY_LENGTH]);
            let pubkey_match = self.is_equal(extracted_pubkey, new_pubkeys[i].0);
            let pubkey_check = self.or(pubkey_match, validator_disabled);
            self.assert_is_equal(pubkey_check, true_v);

            // Verify the correctness of the extracted weight for each enabled validator and
            // increment the cursor by the weight length.
            let extracted_weight = ArrayVariable::<ByteVariable, WEIGHT_LENGTH>::from(
                enc_validator_subarray[idx + PUBKEY_LENGTH..idx + VALIDATOR_LENGTH].to_vec(),
            );
            let weight_match = self.is_equal(extracted_weight, expected_weight_bytes.clone());
            let weight_check = self.or(weight_match, validator_disabled);
            self.assert_is_equal(weight_check, true_v);

            // Set validator_disabled to true if this is the last validator.
            let at_end = self.is_equal(curr_validator, *num_authorities);
            validator_disabled = self.select(at_end, true_v, validator_disabled);

            let not_at_end = self.not(at_end);

            // If at the end of the authority set, verify the correctness of the delay bytes.
            let extracted_delay = ArrayVariable::<ByteVariable, DELAY_LENGTH>::from(
                enc_validator_subarray
                    [idx + VALIDATOR_LENGTH..idx + VALIDATOR_LENGTH + DELAY_LENGTH]
                    .to_vec(),
            );
            let delay_match = self.is_equal(extracted_delay, expected_delay_bytes.clone());
            let delay_check = self.or(delay_match, not_at_end);
            self.assert_is_equal(delay_check, true_v);
        }
    }

    fn rotate<
        const MAX_HEADER_SIZE: usize,
        const MAX_AUTHORITY_SET_SIZE: usize,
        const MAX_SUBARRAY_SIZE: usize,
    >(
        &mut self,
        current_authority_set_id: U64Variable,
        current_authority_set_hash: Bytes32Variable,
        rotate: RotateVariable<MAX_HEADER_SIZE, MAX_AUTHORITY_SET_SIZE>,
    ) -> Bytes32Variable {
        assert_eq!(
            MAX_SUBARRAY_SIZE,
            MAX_AUTHORITY_SET_SIZE * VALIDATOR_LENGTH + DELAY_LENGTH,
            "MAX_SUBARRAY_SIZE must be equal to MAX_AUTHORITY_SET_SIZE * VALIDATOR_LENGTH + DELAY_LENGTH."
        );

        // Hash the header at epoch_end_block.
        let target_header_hash = self.hash_encoded_header::<MAX_HEADER_SIZE>(&rotate.target_header);

        // Verify the justification from the current authority set on the epoch end header.
        // Note: current_authority_set_id and current_authority_set_hash are trusted at this point.
        self.verify_simple_justification::<MAX_AUTHORITY_SET_SIZE>(
            rotate.epoch_end_block_number,
            target_header_hash,
            current_authority_set_id,
            current_authority_set_hash,
        );

        // Verify the epoch end header and the new authority set are valid.
        // Note: The target_header and target_header_hash are trusted at this point.
        self.verify_epoch_end_header::<MAX_HEADER_SIZE, MAX_AUTHORITY_SET_SIZE, MAX_SUBARRAY_SIZE>(
            &rotate.target_header,
            target_header_hash,
            &rotate.target_header_num_authorities,
            &rotate.next_authority_set_start_position,
            &rotate.new_pubkeys,
        );

        // Compute the authority set commitment of the new authority set. The order of the validators
        // in the authority set commitment matches the order of the encoded validator data in the epoch end header.
        // Note: target_header_num_authorities and next_authority_set_start_position are trusted at this point.
        self.compute_authority_set_commitment(
            rotate.target_header_num_authorities,
            &rotate.new_pubkeys,
        )
    }
}

#[cfg(test)]
pub mod tests {
    use std::env;

    use plonky2x::frontend::uint::uint64::U64Variable;
    use plonky2x::prelude::{DefaultBuilder, VariableStream};

    use crate::builder::rotate::RotateMethods;
    use crate::consts::{MAX_HEADER_SIZE, MAX_PREFIX_LENGTH};
    use crate::rotate::RotateHint;
    use crate::vars::RotateVariable;

    #[test]
    #[cfg_attr(feature = "ci", ignore)]
    fn test_verify_prefix_epoch_end_header() {
        env::set_var("RUST_LOG", "debug");
        env_logger::try_init().unwrap_or_default();

        const NUM_AUTHORITIES: usize = 100;
        const MAX_HEADER_LENGTH: usize = MAX_HEADER_SIZE;

        let mut builder = DefaultBuilder::new();

        let authority_set_id = builder.read::<U64Variable>();

        // Fetch the header at epoch_end_block.
        let header_fetcher = RotateHint::<MAX_HEADER_LENGTH, NUM_AUTHORITIES> {};
        let mut input_stream = VariableStream::new();
        input_stream.write(&authority_set_id);
        let output_stream = builder.async_hint(input_stream, header_fetcher);

        let rotate_var =
            output_stream.read::<RotateVariable<MAX_HEADER_LENGTH, NUM_AUTHORITIES>>(&mut builder);

        // Note: In prod, get_fixed_subarray uses the header_hash as the seed for randomness. The
        // below is unsafe, but it's fine for testing purposes.
        let target_header_dummy_hash = &rotate_var.target_header.header_bytes.as_vec()[0..32];
        let prefix_subarray = builder.get_fixed_subarray::<MAX_HEADER_SIZE, MAX_PREFIX_LENGTH>(
            &rotate_var.target_header.header_bytes,
            rotate_var.next_authority_set_start_position,
            target_header_dummy_hash,
        );

        builder.verify_consensus_log(&prefix_subarray);

        let circuit = builder.build();
        let mut input = circuit.input();

        let authority_set_id = 1u64;
        input.write::<U64Variable>(authority_set_id);
        let (proof, output) = circuit.prove(&input);

        circuit.verify(&proof, &input, &output);
    }

    #[test]
    #[cfg_attr(feature = "ci", ignore)]
    fn test_verify_epoch_end_header_small_authority_set() {
        env::set_var("RUST_LOG", "debug");
        env_logger::try_init().unwrap_or_default();

        const NUM_AUTHORITIES: usize = 16;
        const MAX_HEADER_LENGTH: usize = MAX_HEADER_SIZE;

        let mut builder = DefaultBuilder::new();

        let authority_set_id = builder.read::<U64Variable>();

        // Fetch the header at epoch_end_block.
        let header_fetcher = RotateHint::<MAX_HEADER_LENGTH, NUM_AUTHORITIES> {};
        let mut input_stream = VariableStream::new();
        input_stream.write(&authority_set_id);
        let output_stream = builder.async_hint(input_stream, header_fetcher);

        let rotate_var =
            output_stream.read::<RotateVariable<MAX_HEADER_LENGTH, NUM_AUTHORITIES>>(&mut builder);

        // Note: In prod, get_fixed_subarray uses the header_hash as the seed for randomness. The
        // below is unsafe, but it's fine for testing purposes.
        let target_header_dummy_hash = &rotate_var.target_header.header_bytes.as_vec()[0..32];
        let prefix_subarray = builder.get_fixed_subarray::<MAX_HEADER_SIZE, MAX_PREFIX_LENGTH>(
            &rotate_var.target_header.header_bytes,
            rotate_var.next_authority_set_start_position,
            target_header_dummy_hash,
        );

        builder.verify_consensus_log(&prefix_subarray);

        let circuit = builder.build();
        let mut input = circuit.input();

        let authority_set_id = 1u64;
        input.write::<U64Variable>(authority_set_id);
        let (proof, output) = circuit.prove(&input);

        circuit.verify(&proof, &input, &output);
    }

    #[test]
    #[cfg_attr(feature = "ci", ignore)]
    fn test_verify_epoch_end_header_large_authority_set() {
        env::set_var("RUST_LOG", "debug");
        env_logger::try_init().unwrap_or_default();

        const NUM_AUTHORITIES: usize = 100;
        const MAX_HEADER_LENGTH: usize = MAX_HEADER_SIZE;

        let mut builder = DefaultBuilder::new();

        let authority_set_id = builder.read::<U64Variable>();

        // Fetch the header at epoch_end_block.
        let header_fetcher = RotateHint::<MAX_HEADER_LENGTH, NUM_AUTHORITIES> {};
        let mut input_stream = VariableStream::new();
        input_stream.write(&authority_set_id);
        let output_stream = builder.async_hint(input_stream, header_fetcher);

        let rotate_var =
            output_stream.read::<RotateVariable<MAX_HEADER_LENGTH, NUM_AUTHORITIES>>(&mut builder);

        // Note: In prod, get_fixed_subarray uses the header_hash as the seed for randomness. The
        // below is unsafe, but it's fine for testing purposes.
        let target_header_dummy_hash = &rotate_var.target_header.header_bytes.as_vec()[0..32];
        let prefix_subarray = builder.get_fixed_subarray::<MAX_HEADER_SIZE, MAX_PREFIX_LENGTH>(
            &rotate_var.target_header.header_bytes,
            rotate_var.next_authority_set_start_position,
            target_header_dummy_hash,
        );

        builder.verify_consensus_log(&prefix_subarray);

        let circuit = builder.build();
        let mut input = circuit.input();

        let authority_set_id = 1u64;
        input.write::<U64Variable>(authority_set_id);
        let (proof, output) = circuit.prove(&input);

        circuit.verify(&proof, &input, &output);
    }
}
