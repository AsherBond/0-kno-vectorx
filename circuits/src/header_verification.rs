use plonky2::{iop::target::Target, hash::hash_types::RichField, field::extension::Extendable, plonk::circuit_builder::CircuitBuilder};
use plonky2x::hash::blake2::blake2b::blake2b;

use crate::{utils::{AvailHashTarget, CHUNK_128_BYTES, HASH_SIZE, EncodedHeaderTarget, CircuitBuilderUtils}, decoder::CircuitBuilderHeaderDecoder};

pub const MAX_HEADER_SIZE: usize = CHUNK_128_BYTES * 16; // 2048 bytes

pub trait CircuitBuilderHeaderVerification {
    fn verify_header(
        &mut self,
        encoded_header: &EncodedHeaderTarget,
        encoded_header_size: Target,
        parent_block_num: Target,
        parent_hash: &AvailHashTarget,
        data_root_acc: &AvailHashTarget,
    );
}

impl<F: RichField + Extendable<D>, const D: usize> CircuitBuilderHeaderVerification for CircuitBuilder<F, D> {
    fn verify_header(
        &mut self,
        encoded_header: &EncodedHeaderTarget,
        encoded_header_size: Target,
        parent_block_num: Target,
        parent_hash_target: &AvailHashTarget,
        data_root_acc: &AvailHashTarget,
    ) {
        // Calculate the hash for the current header
        let header_hasher = blake2b::<F, D, MAX_HEADER_SIZE, HASH_SIZE>(self);

        // Input the encoded header bytes into the hasher
        for i in 0..MAX_HEADER_SIZE {
            // Need to split the bytes into bits
            let mut bits = self.split_le(encoded_header.header_bytes[i], 8);

            // Needs to be in bit big endian order for the EDDSA verification circuit
            bits.reverse();
            for (j, bit) in bits.iter().enumerate().take(8){
                self.connect(header_hasher.message[i*8+j].target, bit.target);
            }
        }

        self.connect(header_hasher.message_len, encoded_header_size);
        
        // Convert the digest (vector of bits) to bytes
        let mut header_hash_bytes = Vec::new();
        for byte_chunk in header_hasher.digest.chunks(8) {
            let byte = self.le_sum(byte_chunk.to_vec().iter().rev());
            self.register_public_input(byte);
            header_hash_bytes.push(byte);
        }

        // Get the decoded_header object to retrieve the block numbers and parent hashes
        let decoded_header = self.decode_header(
            encoded_header, 
            AvailHashTarget(header_hash_bytes.as_slice().try_into().unwrap())
        );

        // Verify that this header's block number is one greater than the previous header's block number
        let one = self.one();
        let expected_block_num = self.add(parent_block_num, one);
        self.connect(expected_block_num, decoded_header.block_number);
        self.register_public_input(decoded_header.block_number);

        // Verify that the parent hash is equal to the decoded parent hash
        self.connect_avail_hash(parent_hash_target.clone(), decoded_header.parent_hash);

        // Calculate the hash of the extracted fields and add them into the accumulator
        let data_root_acc_hasher = blake2b::<F, D, MAX_HEADER_SIZE, HASH_SIZE>(self);

        let mut hasher_idx = 0;
        // Input the accumulator
        for hash_byte in data_root_acc.0.iter() {
            let mut bits = self.split_le(*hash_byte, 8);

            bits.reverse();
            assert!(bits.len() == 8);
            for bit in bits.iter() {
                self.connect(data_root_acc_hasher.message[hasher_idx].target, bit.target);
                hasher_idx += 1;
            }
        }

        // Input the header hash
        for bit in header_hasher.digest.iter() {
            self.connect(data_root_acc_hasher.message[hasher_idx].target, bit.target);
            hasher_idx += 1;
        }

        // Input the data root
        for byte in decoded_header.data_root.0.iter() {
            let mut bits = self.split_le(*byte, 8);

            bits.reverse();
            assert!(bits.len() == 8);
            for bit in bits.iter() {
                self.connect(data_root_acc_hasher.message[hasher_idx].target, bit.target);
                hasher_idx += 1;
            }
        }

        for i in hasher_idx..CHUNK_128_BYTES * 8 {
            let zero = self.zero();
            self.connect(data_root_acc_hasher.message[i].target, zero);
        }

        let input_len = self.constant(F::from_canonical_usize((hasher_idx+1)/8));
        self.connect(data_root_acc_hasher.message_len, input_len);

        for byte_chunk in data_root_acc_hasher.digest.chunks(8) {
            let byte = self.le_sum(byte_chunk.to_vec().iter().rev());
            self.register_public_input(byte);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_process_header_ivc() -> Result<()> {
        const D: usize = 2;
        type C = PoseidonGoldilocksConfig;
        type F = <C as GenericConfig<D>>::F;

        let mut builder_logger = env_logger::Builder::from_default_env();
        builder_logger.format_timestamp(None);
        builder_logger.filter_level(log::LevelFilter::Trace);
        builder_logger.try_init()?;

        let config = CircuitConfig::standard_recursion_config();
        let mut builder = CircuitBuilder::<F, D>::new(config);

        let head_block_num = builder.add_virtual_public_input();
        let head_block_hash = builder.add_virtual_avail_hash_target_safe(true);
        let initial_accumulator = builder.add_virtual_avail_hash_target_safe(true);

        let current_block_num = builder.add_virtual_target();
        let current_block_hash = builder.add_virtual_avail_hash_target_safe(false);
        let current_accumulator = builder.add_virtual_avail_hash_target_safe(false);

        let encoded_block_input = builder.add_virtual_encoded_header_target_safe();
        let encoded_block_size = builder.add_virtual_target();

        builder.process_header(
            &encoded_block_input,
            encoded_block_size,
            current_block_num,
            &current_block_hash,
            &current_accumulator,
        );

        let verifier_data_target = builder.add_verifier_data_public_inputs();

        let k_is = vec![1, 7, 49, 343, 2401, 16807, 117649, 823543, 5764801, 40353607, 282475249, 1977326743, 13841287201, 96889010407, 678223072849, 4747561509943, 33232930569601, 232630513987207, 1628413597910449, 11398895185373143, 79792266297612001, 558545864083284007, 3909821048582988049, 8922003270666332022, 7113790686420571191, 12903046666114829695, 16534350385145470581, 5059988279530788141, 16973173887300932666, 8131752794619022736, 1582037354089406189, 11074261478625843323, 3732854072722565977, 7683234439643377518, 16889152938674473984, 7543606154233811962, 15911754940807515092, 701820169165099718, 4912741184155698026, 15942444219675301861, 916645121239607101, 6416515848677249707, 8022122801911579307, 814627405137302186, 5702391835961115302, 3023254712898638472, 2716038920875884983, 565528376716610560, 3958698637016273920, 9264146389699333119, 9508792519651578870, 11221315429317299127, 4762231727562756605, 14888878023524711914, 11988425817600061793, 10132004445542095267, 15583798910550913906, 16852872026783475737, 7289639770996824233, 14133990258148600989, 6704211459967285318, 10035992080941828584, 14911712358349047125, 12148266161370408270, 11250886851934520606, 4969231685883306958, 16337877731768564385, 3684679705892444769, 7346013871832529062, 14528608963998534792, 9466542400916821939, 10925564598174000610, 2691975909559666986, 397087297503084581, 2779611082521592067, 1010533508236560148, 7073734557655921036, 12622653764762278610, 14571600075677612986, 9767480182670369297];
        let k_i_fields = k_is.iter().map(|x| plonky2_field::goldilocks_field::GoldilocksField(*x)).collect::<Vec<_>>();

        let barycentric_weights = vec![17293822565076172801,18374686475376656385,18446744069413535745,281474976645120,17592186044416,18446744069414584577,18446744000695107601,18446744065119617025,1152921504338411520,72057594037927936,18446744069415632897,18446462594437939201,18446726477228539905,18446744069414584065,68719476720,4294967296];
        let barycentric_weights_fields = barycentric_weights.iter().map(|x| plonky2_field::goldilocks_field::GoldilocksField(*x)).collect::<Vec<_>>();

        let common_data = CommonCircuitData::<F, D> {
            config: CircuitConfig {
                num_wires: 135,
                num_routed_wires: 80,
                num_constants: 2,
                use_base_arithmetic_gate: true,
                security_bits: 100,
                num_challenges: 2,
                zero_knowledge: false,
                max_quotient_degree_factor: 8,
                fri_config: FriConfig {
                    rate_bits: 3,
                    cap_height: 4,
                    proof_of_work_bits: 16,
                    reduction_strategy: FriReductionStrategy::ConstantArityBits(4, 5),
                    num_query_rounds: 28
                } 
            },
            fri_params: FriParams {
                config: FriConfig {
                    rate_bits: 3,
                    cap_height: 4,
                    proof_of_work_bits: 16,
                    reduction_strategy: FriReductionStrategy::ConstantArityBits(4, 5),
                    num_query_rounds: 28
                },
                hiding: false,
                degree_bits: 18,
                reduction_arity_bits: vec![4, 4, 4, 4],
            },
            gates: vec![
                GateRef::new(NoopGate{}),
                GateRef::new(ConstantGate{ num_consts: 2 }),
                GateRef::new(PoseidonMdsGate::new()),
                GateRef::new(PublicInputGate{}),
                GateRef::new(BaseSumGate::<2>::new(32)),
                GateRef::new(BaseSumGate::<2>::new(63)),
                GateRef::new(ReducingExtensionGate::new(32)),
                GateRef::new(ReducingGate::new(43)),
                GateRef::new(ArithmeticExtensionGate{num_ops: 10}),
                GateRef::new(ArithmeticGate{num_ops: 20}),
                GateRef::new(MulExtensionGate{num_ops: 13}),
                GateRef::new(RandomAccessGate{bits:2,num_copies:13,num_extra_constants:2, _phantom: std::marker::PhantomData }),
                GateRef::new(ExponentiationGate{num_power_bits: 66, _phantom: std::marker::PhantomData }),
                GateRef::new(U32AddManyGate{num_addends: 3, num_ops: 5, _phantom: std::marker::PhantomData }),
                GateRef::new(RandomAccessGate{bits: 4, num_copies: 4, num_extra_constants: 2, _phantom: std::marker::PhantomData}),
                GateRef::new(CosetInterpolationGate::<F, D>{ subgroup_bits:4, degree:6, barycentric_weights: barycentric_weights_fields, _phantom: std::marker::PhantomData }),
                GateRef::new(PoseidonGate::new()),
            ],
            selectors_info: SelectorsInfo {
                selector_indices: vec![0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 2, 2, 2, 3, 3],
                groups: vec![0..7, 7..12, 12..15, 15..17],
            },
            quotient_degree_factor: 8,
            num_gate_constraints: 123,
            num_constants: 6,
            num_public_inputs: 198,
            k_is: k_i_fields,
            num_partial_products: 9,
        };

        let condition = builder.add_virtual_bool_target_safe();

        // Unpack inner proof's public inputs.
        let inner_cyclic_proof_with_pis = builder.add_virtual_proof_with_pis(&common_data);
        let inner_cyclic_pis = &inner_cyclic_proof_with_pis.public_inputs;
        let inner_cyclic_initial_block_num = inner_cyclic_pis[0];
        let inner_cyclic_initial_block_hash = AvailHashTarget(inner_cyclic_pis[1..33].try_into().unwrap());
        let inner_cyclic_initial_accumulator = AvailHashTarget(inner_cyclic_pis[33..65].try_into().unwrap());
        let inner_cyclic_latest_block_num = inner_cyclic_pis[65];
        let inner_cyclic_latest_block_hash = AvailHashTarget(inner_cyclic_pis[66..98].try_into().unwrap());
        let inner_cyclic_latest_accumulator = AvailHashTarget(inner_cyclic_pis[98..130].try_into().unwrap());

        // Connect our initial values to that of our inner proof. (If there is no inner proof, the
        // initial values will be unconstrained, which is intentional.)
        builder.connect(head_block_num, inner_cyclic_initial_block_num);
        builder.connect_hash(head_block_hash.clone(), inner_cyclic_initial_block_hash);
        builder.connect_hash(initial_accumulator.clone(), inner_cyclic_initial_accumulator);

        // The input values is the previous outputs if we have an inner proof, or the initial values
        // if this is the base case.
        let actual_block_num_in =
            builder.select(condition, inner_cyclic_latest_block_num, head_block_num);
        let actual_block_hash_in =
            builder.select_hash(condition, &inner_cyclic_latest_block_hash, &head_block_hash);
        let actual_accumulator_in =
            builder.select_hash(condition, &inner_cyclic_latest_accumulator, &initial_accumulator);
        builder.connect(current_block_num, actual_block_num_in);
        builder.connect_hash(current_block_hash, actual_block_hash_in);
        builder.connect_hash(current_accumulator, actual_accumulator_in);

        builder.conditionally_verify_cyclic_proof_or_dummy::<C>(
            condition,
            &inner_cyclic_proof_with_pis,
            &common_data,
        )?;

        println!("KJ: building circuit");
        let cyclic_circuit_data = builder.build::<C>();
        println!("KJ: built circuit");

        let headers = vec![
            BLOCK_530508_HEADER.to_vec(),
            BLOCK_530509_HEADER.to_vec(),
            BLOCK_530510_HEADER.to_vec(),
            BLOCK_530511_HEADER.to_vec(),
            BLOCK_530512_HEADER.to_vec(),
            BLOCK_530513_HEADER.to_vec(),
            BLOCK_530514_HEADER.to_vec(),
            BLOCK_530515_HEADER.to_vec(),
            BLOCK_530516_HEADER.to_vec(),
            BLOCK_530517_HEADER.to_vec(),
            BLOCK_530518_HEADER.to_vec(),
            BLOCK_530519_HEADER.to_vec(),
            BLOCK_530520_HEADER.to_vec(),
            BLOCK_530521_HEADER.to_vec(),
            BLOCK_530522_HEADER.to_vec(),
            BLOCK_530523_HEADER.to_vec(),
            BLOCK_530524_HEADER.to_vec(),
            BLOCK_530525_HEADER.to_vec(),
            BLOCK_530526_HEADER.to_vec(),
            BLOCK_530527_HEADER.to_vec(),
        ];
        let head_block_hash_val = hex::decode(BLOCK_530508_PARENT_HASH).unwrap();
        let head_block_num_val = 530507;
        let initial_accumulator_val = [0u8; 32];

        let mut pw = PartialWitness::new();
        pw.set_bool_target(condition, false);
        pw.set_encoded_header_target(&encoded_block_input, headers[0].clone());
        pw.set_target(encoded_block_size, F::from_canonical_u64(headers[0].len() as u64));

        let mut initial_pi = Vec::new();
        initial_pi.push(F::from_canonical_u64(head_block_num_val));
        initial_pi.extend(head_block_hash_val.iter().map(|b| F::from_canonical_u64(*b as u64)));
        initial_pi.extend(initial_accumulator_val.iter().map(|b| F::from_canonical_u64(*b as u64)));
        let initial_pi_map = initial_pi.into_iter().enumerate().collect();

        pw.set_proof_with_pis_target::<C, D>(
            &inner_cyclic_proof_with_pis,
            &cyclic_base_proof(
                &common_data,
                &cyclic_circuit_data.verifier_only,
                initial_pi_map,
            ),
        );
        pw.set_verifier_data_target(&verifier_data_target, &cyclic_circuit_data.verifier_only);

        let mut timing1 = TimingTree::new("proof1 proof gen", Level::Info);
        println!("creating base proof");
        let proof1 = prove::<F, C, D>(&cyclic_circuit_data.prover_only, &cyclic_circuit_data.common, pw, &mut timing1)?;
        println!("created base proof");
        timing1.print();

        check_cyclic_proof_verifier_data(
            &proof1,
            &cyclic_circuit_data.verifier_only,
            &cyclic_circuit_data.common,
        )?;

        cyclic_circuit_data.verify(proof1.clone())?;

        // 1st recursive layer.
        let mut pw = PartialWitness::new();
        pw.set_bool_target(condition, true);
        pw.set_encoded_header_target(&encoded_block_input, headers[1].clone());
        pw.set_target(encoded_block_size, F::from_canonical_u64(headers[1].len() as u64));
        pw.set_proof_with_pis_target(&inner_cyclic_proof_with_pis, &proof1);
        pw.set_verifier_data_target(&verifier_data_target, &cyclic_circuit_data.verifier_only);
        let mut timing2 = TimingTree::new("proof1 proof gen", Level::Info);
        println!("creating 1st recursive proof");
        let proof2 = prove::<F, C, D>(&cyclic_circuit_data.prover_only, &cyclic_circuit_data.common, pw, &mut timing2)?;
        println!("created 1st recursive proof");
        timing2.print();
        check_cyclic_proof_verifier_data(
            &proof2,
            &cyclic_circuit_data.verifier_only,
            &cyclic_circuit_data.common,
        )?;
        cyclic_circuit_data.verify(proof2.clone())?;

        // 2nd recursive layer.
        let mut pw = PartialWitness::new();
        pw.set_bool_target(condition, true);
        pw.set_encoded_header_target(&encoded_block_input, headers[2].clone());
        pw.set_target(encoded_block_size, F::from_canonical_u64(headers[2].len() as u64));
        pw.set_proof_with_pis_target(&inner_cyclic_proof_with_pis, &proof2);
        pw.set_verifier_data_target(&verifier_data_target, &cyclic_circuit_data.verifier_only);
        let mut timing3 = TimingTree::new("proof1 proof gen", Level::Info);
        println!("creating 2nd recursive proof");
        let proof3 = prove::<F, C, D>(&cyclic_circuit_data.prover_only, &cyclic_circuit_data.common, pw, &mut timing3)?;
        println!("created 2nd recursive proof");
        timing3.print();
        check_cyclic_proof_verifier_data(
            &proof3,
            &cyclic_circuit_data.verifier_only,
            &cyclic_circuit_data.common,
        )?;

        println!("proof public inputs: {:?}", proof3.public_inputs.iter().map(|x| x.to_canonical_u64()).collect::<Vec<_>>());

        // TODO: Verify that the proof correctly computes a repeated hash.
        cyclic_circuit_data.verify(proof3)
    }    
}