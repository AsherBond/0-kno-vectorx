use std::marker::PhantomData;
use plonky2lib_succinct::hash_functions::blake2b::CHUNK_128_BYTES;

pub const NUM_AUTHORITIES: usize = 10;
pub const NUM_AUTHORITIES_PADDED: usize = 16;  // The random access gadget requires a power of 2, so we pad the authority set to 16
pub const QUORUM_SIZE: usize = 7;  // 2/3 + 1 of NUM_VALIDATORS
pub const MAX_NUM_HEADERS_PER_STEP: usize = 20;

//pub const MAX_HEADER_SIZE: usize = CHUNK_128_BYTES * 16; // 2048 bytes
pub const MAX_HEADER_SIZE: usize = CHUNK_128_BYTES * 10; // 1280 bytes.  Keep this for now.
pub const HASH_SIZE: usize = 32;                         // in bytes
pub const PUB_KEY_SIZE: usize = 32;                      // in bytes


pub const ENCODED_PRECOMMIT_LENGTH: usize = 53;

use plonky2::{
    iop::{
        target::Target,
        generator::{SimpleGenerator, GeneratedValues},
        witness::{PartitionWitness, Witness, WitnessWrite}
    },
    hash::hash_types::RichField,
    plonk::{circuit_builder::CircuitBuilder}, util::serialization::{Buffer, IoResult, Read, Write}
};
use plonky2_field::{extension::Extendable, types::{PrimeField, PrimeField64}};

#[derive(Clone)]
pub struct AvailHashTarget(pub [Target; HASH_SIZE]);

pub trait WitnessAvailHash<F: PrimeField64>: Witness<F> {
    fn get_avail_hash_target(&self, target: AvailHashTarget) -> [u8; HASH_SIZE];
    fn set_avail_hash_target(&mut self, target: &AvailHashTarget, value: &[u8; HASH_SIZE]);
}

impl<T: Witness<F>, F: PrimeField64> WitnessAvailHash<F> for T {
    fn get_avail_hash_target(&self, target: AvailHashTarget) -> [u8; HASH_SIZE] {
        target.0
        .iter()
        .map(|t| u8::try_from(self.get_target(*t).to_canonical_u64()).unwrap())
        .collect::<Vec<u8>>()
        .try_into()
        .unwrap()
    }

    fn set_avail_hash_target(&mut self, target: &AvailHashTarget, value: &[u8; HASH_SIZE]) {
        for i in 0..HASH_SIZE {
            self.set_target(target.0[i], F::from_canonical_u8(value[i]));
        }
    }
}

pub trait GeneratedValuesAvailHash<F: PrimeField> {
    fn set_avail_hash_target(&mut self, target: &AvailHashTarget, value: [u8; HASH_SIZE]);
}

impl<F: PrimeField> GeneratedValuesAvailHash<F> for GeneratedValues<F> {
    fn set_avail_hash_target(&mut self, target: &AvailHashTarget, value: [u8; HASH_SIZE]) {
        for i in 0..HASH_SIZE {
            self.set_target(target.0[i], F::from_canonical_u8(value[i]));
        }
    }
}

#[derive(Debug)]
pub struct EncodedHeaderTarget {
    pub header_bytes: [Target; MAX_HEADER_SIZE],
    pub header_size: Target,
}

pub trait WitnessEncodedHeader<F: PrimeField64>: Witness<F> {
    fn get_encoded_header_target(&self, target: EncodedHeaderTarget) -> Vec<u8>;
    fn set_encoded_header_target(&mut self, target: &EncodedHeaderTarget, value: Vec<u8>);
}

impl<T: Witness<F>, F: PrimeField64> WitnessEncodedHeader<F> for T {
    fn get_encoded_header_target(&self, target: EncodedHeaderTarget) -> Vec<u8> {
        let header_size = self.get_target(target.header_size).to_canonical_u64();
        target.header_bytes
        .iter()
        .take(header_size as usize)
        .map(|t| u8::try_from(self.get_target(*t).to_canonical_u64()).unwrap())
        .collect::<Vec<u8>>()
        .try_into()
        .unwrap()
    }

    fn set_encoded_header_target(&mut self, target: &EncodedHeaderTarget, value: Vec<u8>) {
        let header_size = value.len();
        self.set_target(target.header_size, F::from_canonical_u64(header_size as u64));
        for i in 0..header_size {
            self.set_target(target.header_bytes[i], F::from_canonical_u8(value[i]));
        }

        for i in header_size..MAX_HEADER_SIZE {
            self.set_target(target.header_bytes[i], F::from_canonical_u8(0));
        }
    }
}

pub trait GeneratedValuesEncodedHeader<F: PrimeField> {
    fn set_encoded_header_target(&mut self, target: &EncodedHeaderTarget, value: Vec<u8>);
}

impl<F: PrimeField> GeneratedValuesEncodedHeader<F> for GeneratedValues<F> {
    fn set_encoded_header_target(&mut self, target: &EncodedHeaderTarget, value: Vec<u8>) {
        let header_size = value.len();
        self.set_target(target.header_size, F::from_canonical_u64(header_size as u64));
        for i in 0..header_size {
            self.set_target(target.header_bytes[i], F::from_canonical_u8(value[i]));
        }

        for i in header_size..MAX_HEADER_SIZE {
            self.set_target(target.header_bytes[i], F::from_canonical_u8(0));
        }
    }
}

pub trait CircuitBuilderUtils {
    fn add_virtual_avail_hash_target_safe(
        &mut self,
        set_as_public: bool
    ) -> AvailHashTarget;

    fn add_virtual_encoded_header_target_safe(
        &mut self
    ) -> EncodedHeaderTarget;

    fn connect_hash(
        &mut self,
        x: AvailHashTarget,
        y: AvailHashTarget
    );

    fn int_div(
        &mut self,
        dividend: Target,
        divisor: Target,
    ) -> Target;

    fn random_access_vec(
        &mut self,
        index: Target,
        targets: &Vec<Vec<Target>>,
    ) -> Vec<Target>;
}

impl<F: RichField + Extendable<D>, const D: usize> CircuitBuilderUtils for CircuitBuilder<F, D> {
    fn add_virtual_avail_hash_target_safe(&mut self, set_as_public: bool) -> AvailHashTarget {
        let mut hash_target = Vec::new();
        for _ in 0..HASH_SIZE {
            let byte = self.add_virtual_target();
            if set_as_public {
                self.register_public_input(byte);
            }
            self.range_check(byte, 8);
            hash_target.push(byte);
        }

        AvailHashTarget(hash_target.try_into().unwrap())
    }

    fn add_virtual_encoded_header_target_safe(&mut self) -> EncodedHeaderTarget {
        let mut header_bytes = Vec::new();
        for _j in 0..MAX_HEADER_SIZE {
            let byte = self.add_virtual_target();
            self.range_check(byte, 8);
            header_bytes.push(byte);
        }

        let header_size = self.add_virtual_target();

        EncodedHeaderTarget {
            header_bytes: header_bytes.try_into().unwrap(),
            header_size,
        }
    }

    fn connect_hash(
        &mut self,
        x: AvailHashTarget,
        y: AvailHashTarget
    ) {
        for i in 0..HASH_SIZE {
            self.connect(x.0[i], y.0[i]);
        }
    }

    fn int_div(
        &mut self,
        dividend: Target,
        divisor: Target,
    ) -> Target {
        let quotient = self.add_virtual_target();
        let remainder = self.add_virtual_target();
    
        self.add_simple_generator(FloorDivGenerator::<F, D> {
            divisor,
            dividend,
            quotient,
            remainder,
            _marker: PhantomData
        });
        let base = self.mul(quotient, divisor);
        let rhs = self.add(base, remainder);
        let is_equal = self.is_equal(rhs, dividend);
        self.assert_one(is_equal.target);
        quotient
    }

    fn random_access_vec(
        &mut self,
        index: Target,
        targets: &Vec<Vec<Target>>,
    ) -> Vec<Target> {
        assert!(!targets.is_empty());

        let v_size = targets[0].len();

        // Assert that all vectors have the same length
        targets.iter().for_each(|t| {
            assert_eq!(t.len(), v_size);
        });

        (0..v_size).map(|i| {
            self.random_access(
                index,
                targets.iter().map(|t| {
                    t[i]
                }).collect::<Vec<Target>>())
        }).collect::<Vec<Target>>()
    }

}


#[derive(Debug, Default)]
struct FloorDivGenerator<
    F: RichField + Extendable<D>,
    const D: usize
> {
    divisor: Target,
    dividend: Target,
    quotient: Target,
    remainder: Target,
    _marker: PhantomData<F>,
}


impl<
    F: RichField + Extendable<D>,
    const D: usize,
> SimpleGenerator<F> for FloorDivGenerator<F, D> {
    fn id(&self) -> String {
        "FloorDivGenerator".to_string()
    }

    fn serialize(&self, dst: &mut Vec<u8>) -> IoResult<()> {
        dst.write_target(self.divisor)?;
        dst.write_target(self.dividend)?;
        dst.write_target(self.quotient)?;
        dst.write_target(self.remainder)
    }

    fn deserialize(src: &mut Buffer) -> IoResult<Self> {
        let divisor = src.read_target()?;
        let dividend = src.read_target()?;
        let quotient = src.read_target()?;
        let remainder = src.read_target()?;
        Ok(Self { divisor, dividend, quotient, remainder, _marker: PhantomData })
    }

    fn dependencies(&self) -> Vec<Target> {
        Vec::from([self.dividend])
    }

    fn run_once(&self, witness: &PartitionWitness<F>, out_buffer: &mut GeneratedValues<F>) {
        let divisor = witness.get_target(self.divisor);
        let dividend = witness.get_target(self.dividend);
        let divisor_int = divisor.to_canonical_u64() as u32;
        let dividend_int = dividend.to_canonical_u64() as u32;
        let quotient = dividend_int / divisor_int;
        let remainder = dividend_int % divisor_int;
        out_buffer.set_target(self.quotient, F::from_canonical_u32(quotient));
        out_buffer.set_target(self.remainder, F::from_canonical_u32(remainder));
    }    
}


pub mod default {
    use std::marker::PhantomData;

    use plonky2::plonk::config::{GenericConfig, AlgebraicHasher};
    use plonky2::recursion::dummy_circuit::DummyProofGenerator;
    use plonky2::{impl_gate_serializer, get_gate_tag_impl, read_gate_impl, impl_generator_serializer, get_generator_tag_impl, read_generator_impl};
    use plonky2_field::extension::Extendable;

    use plonky2::gates::arithmetic_base::ArithmeticGate;
    use plonky2::gates::arithmetic_extension::ArithmeticExtensionGate;
    use plonky2::gates::base_sum::BaseSumGate;
    use plonky2::gates::constant::ConstantGate;
    use plonky2::gates::coset_interpolation::CosetInterpolationGate;
    use plonky2::gates::exponentiation::ExponentiationGate;
    use plonky2::gates::multiplication_extension::MulExtensionGate;
    use plonky2::gates::noop::NoopGate;
    use plonky2::gates::poseidon::PoseidonGate;
    use plonky2::gates::poseidon_mds::PoseidonMdsGate;
    use plonky2::gates::public_input::PublicInputGate;
    use plonky2::gates::random_access::RandomAccessGate;
    use plonky2::gates::reducing::ReducingGate;
    use plonky2::gates::reducing_extension::ReducingExtensionGate;
    use plonky2::hash::hash_types::RichField;
    use plonky2::util::serialization::{GateSerializer, WitnessGeneratorSerializer};
    use plonky2_u32::gates::add_many_u32::U32AddManyGate;
    use plonky2_u32::gates::arithmetic_u32::U32ArithmeticGate;
    use plonky2_u32::gates::comparison::ComparisonGate;
    use plonky2_u32::gates::range_check_u32::U32RangeCheckGate;
    use plonky2_u32::gates::subtraction_u32::U32SubtractionGate;
    use plonky2lib_succinct::hash_functions::bit_operations::{XOR3Gate, XOR3Generator};


    use plonky2::gadgets::arithmetic::EqualityGenerator;
    use plonky2::gadgets::arithmetic_extension::QuotientGeneratorExtension;
    use plonky2::gadgets::range_check::LowHighGenerator;
    use plonky2::gadgets::split_base::BaseSumGenerator;
    use plonky2::gadgets::split_join::{SplitGenerator, WireSplitGenerator};
    use plonky2::gates::arithmetic_base::ArithmeticBaseGenerator;
    use plonky2::gates::arithmetic_extension::ArithmeticExtensionGenerator;
    use plonky2::gates::base_sum::BaseSplitGenerator;
    use plonky2::gates::coset_interpolation::InterpolationGenerator;
    use plonky2::gates::exponentiation::ExponentiationGenerator;
    use plonky2::gates::multiplication_extension::MulExtensionGenerator;
    use plonky2::gates::poseidon::PoseidonGenerator;
    use plonky2::gates::poseidon_mds::PoseidonMdsGenerator;
    use plonky2::gates::random_access::RandomAccessGenerator;
    use plonky2::gates::reducing::ReducingGenerator;
    use plonky2::gates::reducing_extension::ReducingGenerator as ReducingExtensionGenerator;
    use plonky2::iop::generator::{
        ConstantGenerator, CopyGenerator, NonzeroTestGenerator, RandomValueGenerator,
    };
    use plonky2_ecdsa::gadgets::biguint::BigUintDivRemGenerator;
    use plonky2_ecdsa::gadgets::nonnative::NonNativeAdditionGenerator;
    use plonky2_ecdsa::gadgets::nonnative::NonNativeInverseGenerator;
    use plonky2_ecdsa::gadgets::nonnative::NonNativeMultipleAddsGenerator;
    use plonky2_ecdsa::gadgets::nonnative::NonNativeMultiplicationGenerator;
    use plonky2_ecdsa::gadgets::nonnative::NonNativeSubtractionGenerator;
    use plonky2_u32::gates::add_many_u32::U32AddManyGenerator;
    use plonky2_u32::gates::arithmetic_u32::U32ArithmeticGenerator;
    use plonky2_u32::gates::comparison::ComparisonGenerator;
    use plonky2_u32::gates::range_check_u32::U32RangeCheckGenerator;
    use plonky2_u32::gates::subtraction_u32::U32SubtractionGenerator;
    use plonky2lib_succinct::ed25519::gadgets::curve::CurvePointDecompressionGenerator;
    use plonky2lib_succinct::ed25519::curve::ed25519::Ed25519;

    use super::FloorDivGenerator;

    pub struct AvailGateSerializer;
    impl<F: RichField + Extendable<D>, const D: usize> GateSerializer<F, D> for AvailGateSerializer {
        impl_gate_serializer! {
            DefaultGateSerializer,
            ArithmeticGate,
            ArithmeticExtensionGate<D>,
            BaseSumGate<2>,
            BaseSumGate<4>,
            ComparisonGate<F, D>,
            ConstantGate,
            CosetInterpolationGate<F, D>,
            ExponentiationGate<F, D>,
            MulExtensionGate<D>,
            NoopGate,
            PoseidonMdsGate<F, D>,
            PoseidonGate<F, D>,
            PublicInputGate,
            RandomAccessGate<F, D>,
            ReducingExtensionGate<D>,
            ReducingGate<D>,
            U32AddManyGate<F, D>,
            U32ArithmeticGate<F, D>,
            U32RangeCheckGate<F, D>,
            U32SubtractionGate<F, D>,
            XOR3Gate
        }
    }

    pub struct AvailGeneratorSerializer<C: GenericConfig<D>, const D: usize> {
        pub _phantom: PhantomData<C>,
    }

    impl<F, C, const D: usize> WitnessGeneratorSerializer<F, D> for AvailGeneratorSerializer<C, D>
    where
        F: RichField + Extendable<D>,
        C: GenericConfig<D, F = F> + 'static,
        C::Hasher: AlgebraicHasher<F>,
    {
        impl_generator_serializer! {
            DefaultGeneratorSerializer,
            DummyProofGenerator<F, C, D>,
            ArithmeticBaseGenerator<F, D>,
            ArithmeticExtensionGenerator<F, D>,
            BaseSplitGenerator<2>,
            BaseSumGenerator<2>,
            BaseSumGenerator<4>,
            BigUintDivRemGenerator<F, D>,
            ComparisonGenerator<F, D>,
            ConstantGenerator<F>,
            CopyGenerator,
            CurvePointDecompressionGenerator<F, D, Ed25519>,
            EqualityGenerator,
            ExponentiationGenerator<F, D>,
            FloorDivGenerator<F, D>,
            InterpolationGenerator<F, D>,
            LowHighGenerator,
            MulExtensionGenerator<F, D>,
            NonNativeAdditionGenerator<F, D, <Ed25519 as plonky2lib_succinct::ed25519::curve::curve_types::Curve>::BaseField>,
            NonNativeInverseGenerator<F, D, <Ed25519 as plonky2lib_succinct::ed25519::curve::curve_types::Curve>::BaseField>,
            NonNativeMultipleAddsGenerator<F, D, <Ed25519 as plonky2lib_succinct::ed25519::curve::curve_types::Curve>::BaseField>,
            NonNativeMultiplicationGenerator<F, D, <Ed25519 as plonky2lib_succinct::ed25519::curve::curve_types::Curve>::BaseField>,
            NonNativeSubtractionGenerator<F, D, <Ed25519 as plonky2lib_succinct::ed25519::curve::curve_types::Curve>::BaseField>,
            NonzeroTestGenerator,
            PoseidonGenerator<F, D>,
            PoseidonMdsGenerator<D>,
            QuotientGeneratorExtension<D>,
            RandomAccessGenerator<F, D>,
            RandomValueGenerator,
            ReducingGenerator<D>,
            ReducingExtensionGenerator<D>,
            SplitGenerator,
            WireSplitGenerator,
            U32AddManyGenerator<F, D>,
            U32ArithmeticGenerator<F, D>,
            U32RangeCheckGenerator<F, D>,
            U32SubtractionGenerator<F, D>,
            XOR3Generator<F, D>
        }
    }

}

// Will convert each byte into 8 bits (big endian)
pub fn to_bits(msg: Vec<u8>) -> Vec<bool> {
    let mut res = Vec::new();
    for i in 0..msg.len() {
        let char = msg[i];
        for j in 0..8 {
            if (char & (1 << 7 - j)) != 0 {
                res.push(true);
            } else {
                res.push(false);
            }
        }
    }
    res
}


#[cfg(test)]
#[allow(dead_code)]
pub (crate) mod tests {
    use super::{ENCODED_PRECOMMIT_LENGTH, QUORUM_SIZE, NUM_AUTHORITIES_PADDED};

    // Block 576728 contains a new authorities event
    pub const BLOCK_576728_BLOCK_HASH: &str = "b71429ef80257a25358e386e4ca1debe72c38ea69d833e23416a4225fabb1a78";
    pub const BLOCK_576728_HEADER: [u8; 1277] = [145, 37, 123, 201, 49, 223, 45, 154, 145, 243, 45, 174, 108, 166, 7, 174, 158, 65, 27, 56, 237, 135, 56, 115, 142, 175, 231, 187, 129, 109, 20, 100, 98, 51, 35, 0, 17, 7, 105, 180, 197, 184, 80, 189, 59, 130, 118, 179, 157, 175, 109, 236, 227, 36, 206, 246, 46, 33, 76, 55, 104, 167, 161, 45, 167, 168, 255, 124, 230, 60, 111, 63, 40, 141, 233, 163, 233, 202, 220, 147, 131, 92, 72, 137, 41, 229, 135, 197, 106, 156, 67, 240, 79, 42, 225, 216, 1, 144, 18, 92, 16, 6, 66, 65, 66, 69, 181, 1, 1, 0, 0, 0, 0, 242, 170, 2, 5, 0, 0, 0, 0, 132, 24, 218, 28, 195, 207, 205, 210, 155, 177, 68, 219, 195, 68, 95, 191, 78, 185, 118, 68, 23, 159, 105, 110, 197, 91, 230, 232, 78, 134, 191, 107, 168, 242, 68, 176, 161, 6, 240, 222, 175, 33, 113, 91, 182, 59, 198, 239, 156, 91, 35, 117, 88, 6, 8, 113, 180, 114, 223, 61, 248, 151, 228, 15, 219, 250, 82, 182, 184, 109, 108, 67, 40, 72, 64, 61, 19, 182, 101, 51, 156, 38, 223, 194, 83, 99, 123, 85, 63, 209, 122, 230, 61, 147, 255, 8, 4, 66, 65, 66, 69, 201, 6, 1, 40, 216, 130, 184, 127, 107, 182, 17, 4, 134, 185, 135, 25, 24, 132, 218, 176, 59, 56, 65, 133, 163, 68, 166, 208, 244, 42, 71, 152, 248, 40, 102, 126, 1, 0, 0, 0, 0, 0, 0, 0, 136, 184, 35, 131, 230, 139, 198, 231, 194, 236, 202, 246, 95, 203, 45, 254, 175, 104, 76, 11, 209, 108, 207, 30, 224, 165, 71, 31, 86, 146, 102, 18, 1, 0, 0, 0, 0, 0, 0, 0, 60, 63, 94, 197, 118, 215, 152, 192, 190, 181, 59, 63, 172, 47, 127, 56, 92, 143, 47, 142, 222, 133, 60, 222, 189, 5, 107, 142, 9, 184, 86, 43, 1, 0, 0, 0, 0, 0, 0, 0, 4, 228, 188, 183, 251, 160, 190, 148, 254, 193, 64, 107, 99, 37, 198, 184, 16, 125, 105, 157, 194, 107, 152, 89, 233, 194, 83, 247, 184, 13, 159, 111, 1, 0, 0, 0, 0, 0, 0, 0, 208, 164, 119, 102, 76, 144, 255, 230, 250, 232, 44, 62, 72, 2, 68, 28, 84, 51, 223, 186, 8, 232, 130, 158, 128, 189, 42, 115, 38, 237, 8, 9, 1, 0, 0, 0, 0, 0, 0, 0, 218, 80, 70, 177, 218, 5, 120, 187, 184, 89, 83, 46, 10, 100, 110, 58, 67, 59, 41, 197, 193, 209, 225, 93, 11, 208, 58, 209, 130, 106, 11, 42, 1, 0, 0, 0, 0, 0, 0, 0, 16, 55, 57, 204, 187, 248, 50, 52, 120, 246, 27, 83, 30, 119, 77, 120, 189, 32, 80, 46, 166, 12, 120, 128, 25, 128, 51, 197, 31, 52, 22, 77, 1, 0, 0, 0, 0, 0, 0, 0, 128, 216, 166, 193, 89, 148, 120, 136, 54, 143, 214, 96, 155, 176, 65, 100, 65, 247, 3, 144, 182, 52, 177, 187, 126, 101, 232, 253, 15, 57, 223, 58, 1, 0, 0, 0, 0, 0, 0, 0, 2, 138, 164, 210, 183, 107, 97, 45, 227, 101, 125, 94, 203, 82, 12, 224, 140, 27, 138, 166, 62, 219, 109, 150, 162, 217, 146, 83, 10, 41, 122, 56, 1, 0, 0, 0, 0, 0, 0, 0, 128, 33, 54, 130, 231, 82, 188, 228, 30, 95, 19, 157, 13, 71, 83, 165, 146, 166, 91, 247, 160, 152, 19, 19, 22, 132, 236, 176, 188, 157, 130, 57, 1, 0, 0, 0, 0, 0, 0, 0, 110, 123, 152, 217, 186, 110, 152, 80, 229, 112, 144, 152, 1, 101, 244, 202, 125, 164, 14, 61, 87, 22, 132, 66, 184, 37, 234, 255, 75, 115, 3, 11, 4, 70, 82, 78, 75, 89, 6, 1, 40, 12, 123, 33, 122, 98, 180, 207, 61, 186, 237, 4, 107, 63, 210, 223, 239, 5, 145, 32, 107, 79, 193, 173, 22, 234, 109, 207, 184, 194, 97, 76, 85, 1, 0, 0, 0, 0, 0, 0, 0, 141, 155, 21, 234, 131, 53, 39, 5, 16, 19, 91, 127, 124, 94, 249, 78, 13, 247, 14, 117, 29, 60, 95, 149, 253, 26, 166, 215, 118, 105, 41, 182, 1, 0, 0, 0, 0, 0, 0, 0, 225, 40, 141, 149, 212, 140, 18, 56, 155, 67, 152, 210, 191, 118, 153, 142, 148, 82, 196, 14, 2, 43, 214, 63, 157, 165, 41, 133, 93, 66, 123, 36, 1, 0, 0, 0, 0, 0, 0, 0, 204, 109, 230, 68, 163, 95, 75, 32, 86, 3, 250, 18, 86, 18, 223, 33, 29, 79, 157, 117, 224, 124, 132, 216, 92, 211, 94, 163, 42, 107, 28, 237, 1, 0, 0, 0, 0, 0, 0, 0, 228, 192, 138, 6, 142, 114, 164, 102, 226, 243, 119, 232, 98, 181, 178, 237, 71, 60, 79, 14, 88, 215, 210, 101, 161, 35, 173, 17, 254, 242, 167, 151, 1, 0, 0, 0, 0, 0, 0, 0, 43, 167, 192, 11, 252, 193, 43, 86, 163, 6, 196, 30, 196, 76, 65, 16, 66, 208, 184, 55, 164, 13, 128, 252, 101, 47, 165, 140, 207, 183, 134, 0, 1, 0, 0, 0, 0, 0, 0, 0, 7, 149, 144, 223, 52, 205, 31, 162, 248, 60, 177, 239, 119, 11, 62, 37, 74, 187, 0, 250, 125, 191, 178, 247, 242, 27, 56, 58, 122, 114, 107, 178, 1, 0, 0, 0, 0, 0, 0, 0, 51, 90, 68, 109, 85, 107, 216, 177, 45, 46, 135, 178, 194, 176, 162, 182, 18, 248, 156, 149, 154, 198, 15, 149, 92, 51, 68, 137, 192, 54, 62, 67, 1, 0, 0, 0, 0, 0, 0, 0, 212, 187, 136, 245, 207, 81, 198, 76, 152, 253, 220, 241, 56, 57, 164, 141, 227, 88, 89, 128, 78, 78, 59, 109, 178, 39, 233, 177, 87, 216, 50, 236, 1, 0, 0, 0, 0, 0, 0, 0, 72, 62, 116, 144, 188, 18, 164, 231, 130, 34, 74, 81, 59, 191, 88, 29, 253, 133, 232, 145, 23, 180, 224, 245, 102, 59, 119, 7, 94, 4, 16, 151, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 68, 38, 184, 111, 57, 251, 224, 57, 76, 122, 252, 3, 59, 213, 125, 237, 203, 217, 95, 110, 161, 172, 125, 243, 205, 231, 220, 141, 176, 144, 126, 20, 109, 214, 126, 193, 204, 47, 243, 254, 84, 203, 155, 185, 169, 244, 136, 224, 72, 157, 110, 212, 194, 170, 111, 181, 227, 52, 184, 92, 208, 199, 56, 141, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 178, 84, 15, 97, 151, 104, 188, 59, 189, 239, 81, 107, 33, 200, 101, 245, 169, 243, 185, 20, 93, 71, 169, 146, 85, 96, 42, 98, 209, 39, 111, 83, 100, 63, 153, 43, 69, 194, 55, 129, 127, 71, 16, 205, 1, 65, 47, 74, 178, 84, 15, 97, 151, 104, 188, 59, 189, 239, 81, 107, 33, 200, 101, 245, 169, 243, 185, 20, 93, 71, 169, 146, 85, 96, 42, 98, 209, 39, 111, 83, 100, 63, 153, 43, 69, 194, 55, 129, 127, 71, 16, 205, 1, 65, 47, 74, 4, 0];
    pub const BLOCK_576728_PARENT_HASH: &str = "91257bc931df2d9a91f32dae6ca607ae9e411b38ed8738738eafe7bb816d1464";
    pub const BLOCK_576728_STATE_ROOT: &str = "110769b4c5b850bd3b8276b39daf6dece324cef62e214c3768a7a12da7a8ff7c";



    // Sequence of blocks from 530508 to 530528, with block 530528's justification
    pub const BLOCK_530508_BLOCK_HASH: &str = "a0c3627de86be3e8843fc36c508e8c7580a993a416bdaf28a7b65f5b734061cd";
    pub const BLOCK_530508_HEADER: [u8; 423] = [47, 101, 102, 199, 65, 30, 223, 235, 160, 78, 188, 214, 140, 94, 205, 188, 142, 149, 191, 101, 218, 71, 239, 85, 61, 109, 133, 55, 32, 11, 116, 250, 50, 97, 32, 0, 243, 56, 163, 79, 50, 190, 245, 166, 174, 154, 24, 226, 216, 77, 87, 86, 137, 243, 105, 31, 72, 212, 237, 21, 87, 167, 90, 84, 73, 174, 7, 149, 52, 0, 30, 243, 53, 14, 105, 96, 159, 192, 217, 146, 180, 133, 138, 175, 203, 171, 200, 167, 14, 102, 75, 140, 28, 245, 207, 215, 194, 183, 168, 161, 8, 6, 66, 65, 66, 69, 181, 1, 1, 5, 0, 0, 0, 69, 246, 1, 5, 0, 0, 0, 0, 184, 136, 138, 88, 143, 203, 26, 170, 150, 119, 221, 166, 40, 97, 6, 109, 235, 223, 233, 24, 57, 46, 88, 125, 206, 98, 138, 129, 22, 213, 202, 59, 166, 126, 58, 116, 136, 186, 249, 135, 23, 30, 32, 248, 125, 156, 236, 248, 168, 184, 170, 110, 188, 93, 67, 94, 8, 230, 247, 217, 158, 208, 221, 8, 3, 48, 200, 39, 136, 79, 56, 78, 127, 169, 226, 174, 152, 157, 0, 117, 28, 50, 20, 205, 126, 86, 119, 219, 27, 146, 223, 179, 76, 68, 199, 2, 5, 66, 65, 66, 69, 1, 1, 52, 142, 115, 168, 107, 207, 124, 25, 246, 151, 159, 82, 194, 239, 123, 188, 17, 156, 54, 179, 56, 192, 156, 98, 64, 135, 58, 132, 153, 252, 173, 67, 46, 215, 176, 193, 96, 61, 2, 54, 38, 111, 221, 207, 204, 139, 116, 156, 79, 193, 60, 37, 135, 176, 187, 162, 196, 18, 199, 45, 140, 4, 176, 142, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 166, 157, 148, 144, 78, 31, 185, 153, 131, 232, 227, 163, 83, 231, 254, 212, 45, 45, 33, 83, 177, 220, 146, 63, 135, 205, 131, 2, 113, 90, 24, 142, 199, 42, 125, 88, 164, 205, 184, 140, 167, 168, 150, 17, 101, 27, 23, 118, 166, 157, 148, 144, 78, 31, 185, 153, 131, 232, 227, 163, 83, 231, 254, 212, 45, 45, 33, 83, 177, 220, 146, 63, 135, 205, 131, 2, 113, 90, 24, 142, 199, 42, 125, 88, 164, 205, 184, 140, 167, 168, 150, 17, 101, 27, 23, 118, 4, 0];
    pub const BLOCK_530508_PARENT_HASH: &str = "2f6566c7411edfeba04ebcd68c5ecdbc8e95bf65da47ef553d6d8537200b74fa";

    pub const BLOCK_530509_BLOCK_HASH: &str = "d9072ff5dce7b5f28e352f18c9b8970226b3a3ec95e2590d0b9fd95fbbc0a34a";
    pub const BLOCK_530509_HEADER: [u8; 326] = [160, 195, 98, 125, 232, 107, 227, 232, 132, 63, 195, 108, 80, 142, 140, 117, 128, 169, 147, 164, 22, 189, 175, 40, 167, 182, 95, 91, 115, 64, 97, 205, 54, 97, 32, 0, 102, 20, 109, 108, 3, 105, 214, 125, 138, 237, 126, 36, 31, 19, 215, 199, 234, 135, 59, 195, 161, 143, 126, 141, 218, 96, 213, 61, 141, 209, 152, 88, 143, 9, 176, 93, 41, 147, 81, 48, 37, 170, 176, 124, 128, 24, 244, 86, 146, 81, 0, 82, 27, 141, 166, 17, 6, 142, 40, 39, 72, 99, 141, 200, 8, 6, 66, 65, 66, 69, 52, 2, 4, 0, 0, 0, 70, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 210, 28, 24, 114, 125, 49, 37, 169, 40, 64, 171, 175, 136, 240, 78, 13, 200, 166, 21, 59, 255, 112, 47, 80, 126, 101, 173, 252, 233, 104, 105, 124, 66, 143, 207, 126, 142, 25, 109, 156, 66, 7, 90, 71, 255, 88, 18, 27, 33, 7, 74, 199, 78, 98, 237, 246, 154, 102, 155, 218, 189, 40, 79, 135, 0, 4, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 176, 19, 157, 230, 234, 66, 142, 167, 143, 76, 124, 163, 163, 45, 159, 45, 168, 34, 187, 23, 247, 136, 49, 102, 251, 25, 167, 61, 146, 39, 200, 75, 225, 172, 166, 112, 61, 195, 209, 159, 106, 65, 20, 126, 130, 114, 148, 28, 176, 19, 157, 230, 234, 66, 142, 167, 143, 76, 124, 163, 163, 45, 159, 45, 168, 34, 187, 23, 247, 136, 49, 102, 251, 25, 167, 61, 146, 39, 200, 75, 225, 172, 166, 112, 61, 195, 209, 159, 106, 65, 20, 126, 130, 114, 148, 28, 48, 0];
    pub const BLOCK_530509_PARENT_HASH: &str = "a0c3627de86be3e8843fc36c508e8c7580a993a416bdaf28a7b65f5b734061cd";

    pub const BLOCK_530510_BLOCK_HASH: &str = "1b1482b3b3d7ec50d114b69db2f2eaa409720e242e4a73326b9a168d7f8ada33";
    pub const BLOCK_530510_HEADER: [u8; 326] = [217, 7, 47, 245, 220, 231, 181, 242, 142, 53, 47, 24, 201, 184, 151, 2, 38, 179, 163, 236, 149, 226, 89, 13, 11, 159, 217, 95, 187, 192, 163, 74, 58, 97, 32, 0, 245, 217, 189, 6, 186, 209, 230, 94, 10, 158, 123, 1, 199, 242, 177, 38, 224, 42, 2, 2, 60, 205, 29, 196, 59, 220, 108, 112, 55, 104, 250, 157, 32, 105, 142, 30, 87, 9, 124, 13, 5, 27, 83, 132, 209, 236, 100, 250, 198, 210, 66, 19, 93, 132, 134, 161, 78, 247, 153, 75, 206, 253, 31, 52, 8, 6, 66, 65, 66, 69, 52, 2, 8, 0, 0, 0, 71, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 56, 140, 167, 193, 35, 184, 31, 6, 95, 226, 191, 157, 215, 237, 145, 242, 210, 81, 131, 157, 187, 53, 20, 45, 217, 74, 115, 102, 141, 184, 7, 98, 29, 38, 151, 92, 67, 43, 92, 6, 110, 142, 176, 248, 163, 157, 151, 36, 197, 214, 21, 114, 172, 180, 206, 14, 67, 242, 197, 35, 183, 236, 211, 135, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 133, 236, 197, 104, 213, 125, 116, 34, 5, 50, 240, 32, 114, 67, 130, 37, 167, 75, 131, 135, 184, 49, 4, 78, 158, 37, 66, 218, 227, 119, 243, 51, 201, 220, 88, 209, 216, 52, 28, 91, 77, 167, 243, 236, 48, 77, 143, 166, 133, 236, 197, 104, 213, 125, 116, 34, 5, 50, 240, 32, 114, 67, 130, 37, 167, 75, 131, 135, 184, 49, 4, 78, 158, 37, 66, 218, 227, 119, 243, 51, 201, 220, 88, 209, 216, 52, 28, 91, 77, 167, 243, 236, 48, 77, 143, 166, 4, 0];
    pub const BLOCK_530510_PARENT_HASH: &str = "d9072ff5dce7b5f28e352f18c9b8970226b3a3ec95e2590d0b9fd95fbbc0a34a";

    pub const BLOCK_530511_BLOCK_HASH: &str = "ac52248b0788099073640dc35ffb80d0a587abc32b1ba13dbefa45e2831208d4";
    pub const BLOCK_530511_HEADER: [u8; 423] = [27, 20, 130, 179, 179, 215, 236, 80, 209, 20, 182, 157, 178, 242, 234, 164, 9, 114, 14, 36, 46, 74, 115, 50, 107, 154, 22, 141, 127, 138, 218, 51, 62, 97, 32, 0, 60, 146, 140, 142, 177, 188, 232, 133, 59, 89, 93, 132, 150, 12, 32, 181, 45, 108, 95, 68, 141, 240, 88, 235, 7, 34, 140, 87, 19, 161, 104, 88, 121, 29, 186, 215, 138, 61, 92, 174, 13, 94, 204, 197, 212, 56, 159, 11, 195, 83, 53, 62, 221, 28, 50, 162, 59, 51, 164, 197, 161, 166, 193, 223, 8, 6, 66, 65, 66, 69, 181, 1, 1, 5, 0, 0, 0, 72, 246, 1, 5, 0, 0, 0, 0, 10, 147, 181, 149, 239, 55, 138, 176, 16, 37, 167, 236, 58, 108, 2, 245, 148, 119, 111, 209, 50, 163, 112, 172, 156, 239, 73, 140, 21, 225, 38, 99, 255, 45, 198, 58, 81, 86, 234, 132, 56, 160, 105, 251, 58, 72, 254, 109, 235, 164, 238, 101, 103, 31, 121, 4, 237, 105, 47, 154, 17, 38, 209, 5, 19, 250, 238, 20, 229, 157, 3, 220, 21, 31, 213, 139, 251, 160, 235, 35, 51, 12, 176, 82, 15, 77, 12, 135, 104, 34, 131, 121, 127, 243, 90, 3, 5, 66, 65, 66, 69, 1, 1, 196, 20, 223, 225, 131, 12, 114, 25, 125, 53, 255, 89, 34, 69, 29, 23, 34, 175, 117, 47, 64, 235, 26, 184, 215, 254, 49, 94, 123, 201, 179, 26, 118, 116, 205, 124, 240, 69, 194, 124, 5, 141, 39, 251, 195, 173, 26, 217, 217, 236, 222, 247, 180, 164, 80, 27, 172, 227, 29, 90, 164, 153, 198, 138, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 160, 132, 201, 233, 59, 249, 121, 245, 79, 220, 110, 136, 227, 200, 51, 108, 122, 126, 120, 141, 251, 33, 74, 32, 73, 235, 30, 107, 23, 160, 7, 24, 31, 7, 12, 164, 36, 247, 28, 186, 57, 1, 204, 94, 201, 197, 166, 88, 160, 132, 201, 233, 59, 249, 121, 245, 79, 220, 110, 136, 227, 200, 51, 108, 122, 126, 120, 141, 251, 33, 74, 32, 73, 235, 30, 107, 23, 160, 7, 24, 31, 7, 12, 164, 36, 247, 28, 186, 57, 1, 204, 94, 201, 197, 166, 88, 4, 0];
    pub const BLOCK_530511_PARENT_HASH: &str = "1b1482b3b3d7ec50d114b69db2f2eaa409720e242e4a73326b9a168d7f8ada33";

    pub const BLOCK_530512_BLOCK_HASH: &str = "833b68adb116bbdec44012a87341a504374b9786e705cadc456aac243d9844b9";
    pub const BLOCK_530512_HEADER: [u8; 423] = [172, 82, 36, 139, 7, 136, 9, 144, 115, 100, 13, 195, 95, 251, 128, 208, 165, 135, 171, 195, 43, 27, 161, 61, 190, 250, 69, 226, 131, 18, 8, 212, 66, 97, 32, 0, 22, 124, 147, 11, 114, 93, 208, 203, 29, 14, 132, 196, 95, 91, 11, 62, 190, 60, 19, 174, 19, 57, 218, 71, 204, 106, 45, 89, 103, 12, 68, 97, 157, 7, 99, 114, 210, 67, 15, 69, 63, 46, 126, 111, 96, 113, 253, 4, 93, 3, 163, 45, 29, 107, 118, 133, 44, 85, 77, 6, 240, 2, 131, 221, 8, 6, 66, 65, 66, 69, 181, 1, 1, 3, 0, 0, 0, 73, 246, 1, 5, 0, 0, 0, 0, 138, 86, 196, 121, 118, 104, 189, 215, 187, 33, 46, 121, 144, 175, 137, 90, 193, 156, 88, 78, 78, 79, 31, 68, 217, 32, 19, 227, 110, 6, 17, 76, 99, 212, 172, 178, 150, 186, 73, 68, 83, 71, 79, 147, 29, 175, 16, 227, 74, 42, 116, 87, 173, 53, 155, 15, 22, 129, 28, 131, 232, 170, 64, 14, 229, 97, 169, 140, 171, 176, 232, 226, 237, 181, 210, 15, 52, 211, 178, 254, 7, 135, 118, 34, 36, 213, 10, 162, 142, 135, 19, 184, 103, 23, 37, 13, 5, 66, 65, 66, 69, 1, 1, 24, 68, 191, 33, 114, 74, 250, 108, 241, 7, 49, 8, 223, 7, 82, 73, 149, 28, 198, 184, 95, 56, 78, 253, 197, 249, 77, 55, 60, 99, 195, 74, 234, 137, 116, 153, 138, 54, 161, 223, 235, 113, 3, 31, 80, 217, 153, 26, 133, 113, 103, 212, 14, 204, 172, 44, 152, 215, 254, 244, 32, 238, 196, 131, 0, 4, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 175, 192, 67, 220, 237, 168, 212, 180, 47, 219, 51, 31, 141, 220, 22, 130, 28, 102, 93, 195, 172, 174, 32, 198, 92, 112, 63, 15, 158, 248, 51, 240, 39, 229, 143, 241, 112, 94, 107, 220, 26, 146, 61, 133, 167, 158, 231, 172, 175, 192, 67, 220, 237, 168, 212, 180, 47, 219, 51, 31, 141, 220, 22, 130, 28, 102, 93, 195, 172, 174, 32, 198, 92, 112, 63, 15, 158, 248, 51, 240, 39, 229, 143, 241, 112, 94, 107, 220, 26, 146, 61, 133, 167, 158, 231, 172, 48, 0];
    pub const BLOCK_530512_PARENT_HASH: &str = "ac52248b0788099073640dc35ffb80d0a587abc32b1ba13dbefa45e2831208d4";

    pub const BLOCK_530513_BLOCK_HASH: &str = "965301c1a2a097a4f2625838e60079a8131aaf1897f2ab449f06a3da151cbac7";
    pub const BLOCK_530513_HEADER: [u8; 326] = [131, 59, 104, 173, 177, 22, 187, 222, 196, 64, 18, 168, 115, 65, 165, 4, 55, 75, 151, 134, 231, 5, 202, 220, 69, 106, 172, 36, 61, 152, 68, 185, 70, 97, 32, 0, 231, 113, 132, 139, 184, 243, 86, 175, 149, 8, 70, 157, 185, 218, 236, 106, 139, 91, 214, 24, 153, 153, 239, 5, 12, 7, 25, 39, 229, 2, 11, 192, 85, 81, 28, 103, 233, 239, 35, 231, 194, 168, 61, 239, 67, 156, 30, 79, 20, 66, 7, 128, 25, 122, 232, 17, 14, 243, 130, 130, 116, 187, 38, 67, 8, 6, 66, 65, 66, 69, 52, 2, 7, 0, 0, 0, 74, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 110, 154, 93, 200, 93, 125, 112, 239, 171, 148, 98, 220, 58, 52, 58, 92, 212, 156, 197, 131, 135, 64, 106, 16, 98, 251, 179, 193, 253, 250, 127, 58, 88, 198, 106, 130, 39, 15, 113, 199, 35, 25, 21, 161, 130, 34, 52, 5, 170, 220, 107, 60, 156, 155, 98, 218, 61, 250, 242, 135, 198, 15, 155, 137, 0, 4, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 151, 126, 70, 157, 97, 168, 165, 22, 79, 179, 43, 58, 45, 149, 156, 121, 145, 62, 17, 242, 5, 25, 73, 27, 213, 31, 113, 198, 146, 25, 136, 250, 253, 77, 203, 224, 250, 237, 20, 138, 136, 6, 19, 173, 147, 90, 149, 27, 151, 126, 70, 157, 97, 168, 165, 22, 79, 179, 43, 58, 45, 149, 156, 121, 145, 62, 17, 242, 5, 25, 73, 27, 213, 31, 113, 198, 146, 25, 136, 250, 253, 77, 203, 224, 250, 237, 20, 138, 136, 6, 19, 173, 147, 90, 149, 27, 48, 0];
    pub const BLOCK_530513_PARENT_HASH: &str = "833b68adb116bbdec44012a87341a504374b9786e705cadc456aac243d9844b9";

    pub const BLOCK_530514_BLOCK_HASH: &str = "76a1996b3853333069f43745637f033f15211e6e1df14754c5285cad5fd60206";
    pub const BLOCK_530514_HEADER: [u8; 326] = [150, 83, 1, 193, 162, 160, 151, 164, 242, 98, 88, 56, 230, 0, 121, 168, 19, 26, 175, 24, 151, 242, 171, 68, 159, 6, 163, 218, 21, 28, 186, 199, 74, 97, 32, 0, 45, 237, 152, 87, 134, 249, 150, 235, 182, 215, 167, 67, 194, 200, 55, 35, 206, 169, 241, 120, 3, 153, 221, 210, 158, 190, 70, 172, 10, 190, 167, 189, 252, 184, 120, 251, 219, 200, 0, 20, 221, 169, 200, 125, 41, 176, 162, 133, 221, 95, 149, 219, 137, 41, 12, 3, 221, 235, 31, 244, 145, 216, 33, 116, 8, 6, 66, 65, 66, 69, 52, 2, 1, 0, 0, 0, 75, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 84, 135, 213, 229, 136, 129, 139, 144, 182, 21, 94, 4, 16, 161, 88, 53, 255, 205, 147, 157, 108, 49, 93, 160, 45, 207, 114, 8, 104, 186, 11, 34, 216, 195, 190, 33, 22, 225, 31, 79, 82, 59, 41, 251, 12, 94, 104, 179, 204, 49, 169, 153, 130, 53, 147, 232, 247, 73, 255, 66, 60, 78, 0, 142, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 131, 241, 193, 107, 23, 132, 106, 97, 180, 75, 200, 210, 34, 172, 124, 9, 169, 191, 0, 172, 167, 78, 247, 23, 215, 70, 208, 121, 222, 132, 78, 139, 200, 69, 112, 113, 124, 192, 67, 49, 154, 55, 89, 224, 0, 199, 93, 112, 131, 241, 193, 107, 23, 132, 106, 97, 180, 75, 200, 210, 34, 172, 124, 9, 169, 191, 0, 172, 167, 78, 247, 23, 215, 70, 208, 121, 222, 132, 78, 139, 200, 69, 112, 113, 124, 192, 67, 49, 154, 55, 89, 224, 0, 199, 93, 112, 4, 0];
    pub const BLOCK_530514_PARENT_HASH: &str = "965301c1a2a097a4f2625838e60079a8131aaf1897f2ab449f06a3da151cbac7";

    pub const BLOCK_530515_BLOCK_HASH: &str = "f959eb503df7965b11f2180f51daea2f708157d838a360a91f1ed48e6d7ea2c3";
    pub const BLOCK_530515_HEADER: [u8; 423] = [118, 161, 153, 107, 56, 83, 51, 48, 105, 244, 55, 69, 99, 127, 3, 63, 21, 33, 30, 110, 29, 241, 71, 84, 197, 40, 92, 173, 95, 214, 2, 6, 78, 97, 32, 0, 236, 202, 62, 80, 249, 89, 237, 35, 36, 250, 231, 8, 128, 83, 4, 171, 182, 163, 244, 187, 57, 211, 5, 208, 20, 142, 153, 254, 1, 167, 137, 143, 171, 159, 35, 78, 63, 42, 92, 26, 241, 185, 134, 52, 134, 234, 76, 42, 174, 160, 52, 23, 160, 114, 49, 6, 20, 95, 45, 22, 22, 123, 247, 195, 8, 6, 66, 65, 66, 69, 181, 1, 1, 2, 0, 0, 0, 76, 246, 1, 5, 0, 0, 0, 0, 90, 0, 57, 106, 139, 11, 94, 188, 152, 121, 231, 232, 225, 60, 212, 117, 129, 151, 198, 31, 146, 187, 202, 114, 164, 232, 182, 109, 48, 101, 60, 96, 195, 79, 104, 162, 245, 202, 181, 15, 190, 203, 128, 33, 179, 199, 213, 187, 184, 99, 36, 191, 178, 239, 226, 22, 26, 103, 212, 178, 78, 79, 175, 11, 60, 189, 135, 75, 208, 32, 191, 248, 186, 40, 210, 46, 46, 135, 126, 223, 187, 106, 244, 173, 124, 136, 118, 228, 110, 221, 245, 142, 90, 80, 233, 5, 5, 66, 65, 66, 69, 1, 1, 174, 183, 14, 115, 17, 222, 191, 210, 206, 223, 216, 129, 123, 170, 191, 54, 42, 95, 216, 108, 244, 51, 225, 124, 45, 181, 115, 235, 121, 176, 38, 12, 93, 63, 84, 137, 110, 146, 211, 193, 217, 224, 210, 250, 151, 130, 42, 250, 70, 148, 218, 178, 70, 231, 161, 36, 157, 9, 5, 204, 44, 24, 114, 135, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 141, 1, 36, 90, 146, 77, 227, 189, 12, 37, 60, 253, 134, 238, 239, 112, 220, 236, 212, 206, 211, 91, 254, 234, 85, 243, 121, 139, 76, 169, 131, 90, 202, 26, 170, 196, 24, 125, 131, 239, 228, 104, 134, 213, 124, 203, 97, 11, 141, 1, 36, 90, 146, 77, 227, 189, 12, 37, 60, 253, 134, 238, 239, 112, 220, 236, 212, 206, 211, 91, 254, 234, 85, 243, 121, 139, 76, 169, 131, 90, 202, 26, 170, 196, 24, 125, 131, 239, 228, 104, 134, 213, 124, 203, 97, 11, 4, 0];
    pub const BLOCK_530515_PARENT_HASH: &str = "76a1996b3853333069f43745637f033f15211e6e1df14754c5285cad5fd60206";

    pub const BLOCK_530516_BLOCK_HASH: &str = "866f658d57c7c983948acf778b8f384f5f98e5330d3dc048410b8c8ea501de1e";
    pub const BLOCK_530516_HEADER: [u8; 326] = [249, 89, 235, 80, 61, 247, 150, 91, 17, 242, 24, 15, 81, 218, 234, 47, 112, 129, 87, 216, 56, 163, 96, 169, 31, 30, 212, 142, 109, 126, 162, 195, 82, 97, 32, 0, 174, 91, 220, 75, 121, 40, 138, 106, 236, 24, 62, 179, 80, 20, 230, 214, 33, 240, 64, 72, 172, 153, 9, 186, 166, 23, 4, 73, 175, 43, 123, 236, 232, 234, 159, 68, 160, 82, 76, 205, 190, 207, 187, 150, 0, 212, 120, 237, 106, 202, 226, 228, 156, 42, 216, 117, 104, 69, 37, 131, 24, 173, 51, 252, 8, 6, 66, 65, 66, 69, 52, 2, 8, 0, 0, 0, 77, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 162, 220, 196, 180, 10, 248, 244, 237, 218, 63, 191, 218, 124, 81, 250, 3, 231, 107, 113, 236, 133, 15, 29, 170, 125, 70, 113, 108, 49, 77, 33, 126, 132, 229, 228, 83, 78, 114, 254, 69, 49, 148, 253, 125, 8, 192, 95, 3, 35, 226, 134, 178, 242, 122, 75, 75, 235, 131, 138, 109, 122, 226, 82, 130, 0, 4, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 182, 145, 176, 38, 173, 246, 186, 141, 130, 110, 10, 11, 184, 120, 227, 57, 115, 39, 203, 159, 131, 118, 89, 190, 74, 85, 72, 112, 129, 15, 161, 122, 186, 111, 229, 145, 15, 193, 60, 156, 9, 48, 245, 151, 88, 99, 184, 167, 182, 145, 176, 38, 173, 246, 186, 141, 130, 110, 10, 11, 184, 120, 227, 57, 115, 39, 203, 159, 131, 118, 89, 190, 74, 85, 72, 112, 129, 15, 161, 122, 186, 111, 229, 145, 15, 193, 60, 156, 9, 48, 245, 151, 88, 99, 184, 167, 48, 0];
    pub const BLOCK_530516_PARENT_HASH: &str = "f959eb503df7965b11f2180f51daea2f708157d838a360a91f1ed48e6d7ea2c3";

    pub const BLOCK_530517_BLOCK_HASH: &str = "0b9bd60ec9d7990c98e5215fe5e69b3d6f808bf705159634985692aa6e55ae4d";
    pub const BLOCK_530517_HEADER: [u8; 326] = [134, 111, 101, 141, 87, 199, 201, 131, 148, 138, 207, 119, 139, 143, 56, 79, 95, 152, 229, 51, 13, 61, 192, 72, 65, 11, 140, 142, 165, 1, 222, 30, 86, 97, 32, 0, 132, 128, 77, 246, 121, 54, 200, 248, 234, 145, 206, 173, 141, 173, 228, 76, 86, 72, 74, 236, 195, 211, 88, 119, 228, 113, 83, 40, 246, 233, 151, 220, 20, 223, 84, 225, 98, 31, 151, 178, 221, 185, 227, 104, 202, 90, 154, 183, 95, 253, 129, 165, 86, 63, 238, 211, 33, 25, 62, 161, 179, 11, 248, 116, 8, 6, 66, 65, 66, 69, 52, 2, 2, 0, 0, 0, 78, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 14, 1, 120, 89, 114, 137, 64, 144, 244, 201, 32, 193, 86, 147, 1, 103, 40, 71, 30, 240, 151, 151, 157, 251, 251, 223, 96, 246, 14, 102, 156, 28, 11, 162, 151, 94, 37, 98, 168, 60, 148, 181, 180, 232, 77, 75, 137, 167, 23, 170, 34, 177, 75, 153, 56, 119, 217, 230, 208, 159, 141, 6, 255, 135, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 133, 28, 56, 80, 9, 195, 18, 30, 255, 251, 214, 255, 115, 13, 97, 21, 216, 202, 226, 123, 250, 66, 112, 33, 244, 171, 23, 72, 9, 16, 103, 150, 184, 33, 229, 49, 189, 28, 77, 21, 61, 197, 167, 223, 17, 21, 170, 47, 133, 28, 56, 80, 9, 195, 18, 30, 255, 251, 214, 255, 115, 13, 97, 21, 216, 202, 226, 123, 250, 66, 112, 33, 244, 171, 23, 72, 9, 16, 103, 150, 184, 33, 229, 49, 189, 28, 77, 21, 61, 197, 167, 223, 17, 21, 170, 47, 4, 0];
    pub const BLOCK_530517_PARENT_HASH: &str = "866f658d57c7c983948acf778b8f384f5f98e5330d3dc048410b8c8ea501de1e";

    pub const BLOCK_530518_BLOCK_HASH: &str = "5560cbee8ff367a5f1c4d44d502b700c6c4eb159813e5f49401f7d9874ea64bd";
    pub const BLOCK_530518_HEADER: [u8; 326] = [11, 155, 214, 14, 201, 215, 153, 12, 152, 229, 33, 95, 229, 230, 155, 61, 111, 128, 139, 247, 5, 21, 150, 52, 152, 86, 146, 170, 110, 85, 174, 77, 90, 97, 32, 0, 8, 95, 166, 134, 2, 158, 161, 155, 29, 139, 181, 79, 199, 203, 252, 170, 237, 150, 234, 208, 100, 59, 136, 213, 70, 16, 113, 171, 80, 33, 120, 97, 111, 111, 141, 86, 229, 25, 156, 120, 245, 45, 51, 76, 109, 227, 251, 152, 136, 21, 45, 69, 123, 37, 59, 230, 175, 117, 149, 207, 19, 53, 248, 61, 8, 6, 66, 65, 66, 69, 52, 2, 1, 0, 0, 0, 79, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 78, 117, 27, 156, 198, 182, 144, 80, 108, 175, 229, 204, 13, 248, 192, 79, 26, 200, 13, 227, 1, 4, 182, 48, 157, 120, 206, 147, 123, 220, 120, 110, 87, 182, 2, 110, 181, 22, 133, 94, 84, 50, 248, 103, 82, 251, 40, 18, 9, 151, 33, 145, 52, 33, 146, 211, 105, 154, 86, 176, 197, 215, 24, 140, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 143, 189, 196, 55, 75, 29, 88, 7, 131, 72, 0, 60, 39, 192, 228, 244, 236, 37, 159, 131, 156, 133, 208, 62, 75, 148, 193, 36, 109, 135, 115, 96, 198, 86, 44, 49, 153, 114, 190, 190, 107, 37, 140, 6, 192, 189, 251, 89, 143, 189, 196, 55, 75, 29, 88, 7, 131, 72, 0, 60, 39, 192, 228, 244, 236, 37, 159, 131, 156, 133, 208, 62, 75, 148, 193, 36, 109, 135, 115, 96, 198, 86, 44, 49, 153, 114, 190, 190, 107, 37, 140, 6, 192, 189, 251, 89, 4, 0];
    pub const BLOCK_530518_PARENT_HASH: &str = "0b9bd60ec9d7990c98e5215fe5e69b3d6f808bf705159634985692aa6e55ae4d";

    pub const BLOCK_530519_BLOCK_HASH: &str = "ad0534358ace1b7c70321e7d72a165f96778c34763ddb007a2e5f42a199f605d";
    pub const BLOCK_530519_HEADER: [u8; 326] = [85, 96, 203, 238, 143, 243, 103, 165, 241, 196, 212, 77, 80, 43, 112, 12, 108, 78, 177, 89, 129, 62, 95, 73, 64, 31, 125, 152, 116, 234, 100, 189, 94, 97, 32, 0, 149, 143, 13, 72, 12, 27, 45, 15, 178, 77, 1, 83, 191, 221, 37, 193, 185, 19, 44, 33, 195, 102, 107, 43, 113, 202, 211, 77, 49, 74, 84, 39, 146, 157, 0, 43, 2, 4, 40, 34, 207, 251, 83, 159, 51, 96, 59, 60, 221, 131, 181, 174, 116, 188, 237, 46, 82, 43, 12, 28, 251, 14, 4, 41, 8, 6, 66, 65, 66, 69, 52, 2, 1, 0, 0, 0, 80, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 78, 110, 141, 227, 104, 243, 253, 129, 240, 96, 218, 177, 194, 54, 178, 230, 216, 23, 228, 179, 168, 30, 90, 58, 57, 173, 222, 68, 215, 40, 247, 94, 224, 91, 139, 177, 97, 99, 37, 26, 7, 72, 89, 144, 255, 10, 183, 146, 12, 128, 214, 244, 244, 19, 237, 86, 136, 104, 187, 97, 104, 117, 114, 143, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 151, 187, 125, 69, 135, 27, 217, 73, 109, 30, 238, 202, 120, 198, 212, 101, 55, 231, 33, 132, 166, 191, 129, 31, 185, 138, 163, 19, 8, 160, 29, 188, 114, 253, 47, 130, 252, 14, 235, 146, 139, 16, 218, 232, 104, 125, 147, 175, 151, 187, 125, 69, 135, 27, 217, 73, 109, 30, 238, 202, 120, 198, 212, 101, 55, 231, 33, 132, 166, 191, 129, 31, 185, 138, 163, 19, 8, 160, 29, 188, 114, 253, 47, 130, 252, 14, 235, 146, 139, 16, 218, 232, 104, 125, 147, 175, 4, 0];
    pub const BLOCK_530519_PARENT_HASH: &str = "5560cbee8ff367a5f1c4d44d502b700c6c4eb159813e5f49401f7d9874ea64bd";

    pub const BLOCK_530520_BLOCK_HASH: &str = "f860aad824b53db4b8c524b0a3cb0fecff631f2aedc4bd88411304bd4d9b088b";
    pub const BLOCK_530520_HEADER: [u8; 326] = [173, 5, 52, 53, 138, 206, 27, 124, 112, 50, 30, 125, 114, 161, 101, 249, 103, 120, 195, 71, 99, 221, 176, 7, 162, 229, 244, 42, 25, 159, 96, 93, 98, 97, 32, 0, 152, 63, 117, 180, 94, 203, 243, 68, 15, 184, 112, 21, 171, 17, 121, 117, 219, 84, 146, 63, 1, 118, 93, 60, 109, 198, 180, 113, 139, 53, 73, 60, 116, 101, 106, 136, 251, 245, 165, 189, 210, 22, 10, 202, 160, 49, 113, 94, 51, 102, 231, 231, 26, 114, 39, 95, 142, 254, 205, 1, 70, 181, 82, 130, 8, 6, 66, 65, 66, 69, 52, 2, 7, 0, 0, 0, 81, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 50, 214, 176, 220, 207, 233, 76, 117, 175, 5, 12, 61, 3, 58, 124, 21, 217, 14, 136, 230, 68, 67, 19, 13, 253, 130, 104, 121, 63, 127, 203, 11, 166, 243, 75, 39, 239, 147, 96, 8, 200, 206, 131, 93, 197, 238, 162, 15, 116, 47, 255, 114, 23, 133, 39, 41, 174, 75, 221, 23, 77, 137, 145, 139, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 140, 219, 193, 159, 198, 193, 94, 135, 17, 8, 100, 66, 233, 198, 102, 69, 98, 132, 184, 53, 214, 196, 20, 67, 16, 103, 143, 25, 25, 178, 86, 143, 181, 165, 248, 55, 124, 233, 65, 131, 225, 71, 194, 52, 30, 199, 18, 140, 140, 219, 193, 159, 198, 193, 94, 135, 17, 8, 100, 66, 233, 198, 102, 69, 98, 132, 184, 53, 214, 196, 20, 67, 16, 103, 143, 25, 25, 178, 86, 143, 181, 165, 248, 55, 124, 233, 65, 131, 225, 71, 194, 52, 30, 199, 18, 140, 4, 0];
    pub const BLOCK_530520_PARENT_HASH: &str = "ad0534358ace1b7c70321e7d72a165f96778c34763ddb007a2e5f42a199f605d";

    pub const BLOCK_530521_BLOCK_HASH: &str = "d81e4e99b8aa7adc87f8a5c412c6b4c0fc114d2e02ededa0818cf2316c614177";
    pub const BLOCK_530521_HEADER: [u8; 326] = [248, 96, 170, 216, 36, 181, 61, 180, 184, 197, 36, 176, 163, 203, 15, 236, 255, 99, 31, 42, 237, 196, 189, 136, 65, 19, 4, 189, 77, 155, 8, 139, 102, 97, 32, 0, 201, 3, 46, 69, 89, 223, 52, 166, 176, 235, 188, 12, 237, 32, 90, 140, 57, 122, 222, 4, 228, 87, 43, 218, 215, 132, 189, 17, 151, 85, 185, 130, 131, 46, 73, 148, 147, 19, 165, 227, 201, 186, 181, 14, 4, 184, 251, 205, 30, 173, 136, 11, 255, 193, 50, 43, 243, 184, 173, 119, 5, 124, 157, 199, 8, 6, 66, 65, 66, 69, 52, 2, 7, 0, 0, 0, 82, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 150, 175, 143, 71, 203, 226, 170, 43, 166, 108, 222, 130, 140, 189, 28, 185, 248, 155, 87, 36, 96, 50, 121, 159, 249, 127, 112, 220, 250, 140, 61, 105, 191, 45, 172, 56, 38, 158, 207, 27, 42, 11, 119, 209, 149, 200, 183, 94, 159, 6, 204, 234, 51, 147, 154, 22, 17, 151, 30, 233, 203, 117, 79, 135, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 171, 140, 214, 133, 219, 125, 182, 177, 223, 9, 163, 189, 102, 120, 209, 92, 132, 19, 48, 129, 179, 231, 143, 24, 13, 123, 211, 5, 91, 124, 234, 25, 60, 62, 105, 202, 189, 235, 198, 222, 4, 177, 225, 62, 243, 152, 101, 123, 171, 140, 214, 133, 219, 125, 182, 177, 223, 9, 163, 189, 102, 120, 209, 92, 132, 19, 48, 129, 179, 231, 143, 24, 13, 123, 211, 5, 91, 124, 234, 25, 60, 62, 105, 202, 189, 235, 198, 222, 4, 177, 225, 62, 243, 152, 101, 123, 4, 0];
    pub const BLOCK_530521_PARENT_HASH: &str = "f860aad824b53db4b8c524b0a3cb0fecff631f2aedc4bd88411304bd4d9b088b";

    pub const BLOCK_530522_BLOCK_HASH: &str = "21fb057c79a04d5028cee34bb0cf45b67979df73726616a8cc16183031a2db4b";
    pub const BLOCK_530522_HEADER: [u8; 326] = [216, 30, 78, 153, 184, 170, 122, 220, 135, 248, 165, 196, 18, 198, 180, 192, 252, 17, 77, 46, 2, 237, 237, 160, 129, 140, 242, 49, 108, 97, 65, 119, 106, 97, 32, 0, 200, 121, 53, 155, 146, 190, 124, 118, 57, 209, 227, 73, 37, 43, 51, 12, 44, 99, 156, 57, 106, 217, 192, 32, 39, 112, 209, 56, 169, 156, 39, 247, 190, 57, 78, 41, 70, 190, 166, 26, 108, 47, 63, 47, 186, 164, 123, 61, 17, 215, 141, 20, 26, 40, 47, 148, 27, 57, 133, 92, 66, 135, 188, 34, 8, 6, 66, 65, 66, 69, 52, 2, 9, 0, 0, 0, 83, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 158, 53, 15, 126, 57, 55, 165, 91, 35, 111, 180, 185, 63, 16, 115, 2, 26, 227, 55, 196, 81, 157, 178, 126, 126, 22, 74, 37, 102, 35, 232, 14, 137, 190, 199, 161, 64, 26, 187, 95, 130, 17, 85, 85, 185, 97, 114, 249, 183, 140, 39, 172, 203, 245, 156, 6, 80, 41, 191, 200, 136, 59, 188, 135, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 176, 155, 248, 6, 211, 101, 184, 95, 19, 129, 104, 155, 46, 231, 42, 126, 188, 24, 233, 179, 81, 250, 204, 95, 24, 16, 126, 190, 156, 218, 104, 19, 241, 7, 5, 143, 28, 75, 222, 34, 2, 131, 99, 63, 87, 51, 30, 19, 176, 155, 248, 6, 211, 101, 184, 95, 19, 129, 104, 155, 46, 231, 42, 126, 188, 24, 233, 179, 81, 250, 204, 95, 24, 16, 126, 190, 156, 218, 104, 19, 241, 7, 5, 143, 28, 75, 222, 34, 2, 131, 99, 63, 87, 51, 30, 19, 4, 0];
    pub const BLOCK_530522_PARENT_HASH: &str = "d81e4e99b8aa7adc87f8a5c412c6b4c0fc114d2e02ededa0818cf2316c614177";

    pub const BLOCK_530523_BLOCK_HASH: &str = "39495b7127dea0a7544b6a2be5efb2e8b98d0d0a6633354f9532fd83a1023180";
    pub const BLOCK_530523_HEADER: [u8; 326] = [33, 251, 5, 124, 121, 160, 77, 80, 40, 206, 227, 75, 176, 207, 69, 182, 121, 121, 223, 115, 114, 102, 22, 168, 204, 22, 24, 48, 49, 162, 219, 75, 110, 97, 32, 0, 146, 88, 180, 41, 184, 209, 73, 28, 7, 42, 33, 77, 35, 192, 56, 240, 99, 224, 239, 31, 238, 52, 20, 174, 185, 187, 93, 2, 16, 240, 117, 238, 139, 225, 166, 147, 12, 77, 183, 81, 153, 22, 77, 57, 73, 193, 96, 145, 126, 222, 117, 160, 97, 242, 16, 19, 57, 92, 108, 197, 73, 220, 22, 87, 8, 6, 66, 65, 66, 69, 52, 2, 0, 0, 0, 0, 84, 246, 1, 5, 0, 0, 0, 0, 5, 66, 65, 66, 69, 1, 1, 110, 127, 183, 200, 168, 12, 163, 60, 241, 140, 228, 71, 5, 239, 119, 214, 166, 47, 209, 109, 58, 146, 181, 113, 200, 245, 108, 121, 146, 189, 246, 83, 116, 77, 108, 161, 74, 181, 59, 178, 41, 164, 98, 58, 191, 139, 36, 129, 77, 78, 36, 104, 52, 121, 115, 76, 172, 139, 236, 225, 89, 168, 110, 140, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 138, 75, 67, 169, 57, 170, 113, 233, 87, 76, 152, 35, 15, 144, 24, 228, 165, 148, 170, 171, 108, 252, 241, 134, 25, 144, 217, 53, 26, 141, 241, 164, 137, 183, 114, 213, 164, 149, 211, 254, 141, 131, 113, 206, 55, 175, 207, 245, 138, 75, 67, 169, 57, 170, 113, 233, 87, 76, 152, 35, 15, 144, 24, 228, 165, 148, 170, 171, 108, 252, 241, 134, 25, 144, 217, 53, 26, 141, 241, 164, 137, 183, 114, 213, 164, 149, 211, 254, 141, 131, 113, 206, 55, 175, 207, 245, 4, 0];
    pub const BLOCK_530523_PARENT_HASH: &str = "21fb057c79a04d5028cee34bb0cf45b67979df73726616a8cc16183031a2db4b";

    pub const BLOCK_530524_BLOCK_HASH: &str = "4bfd5754a45daef37080d5de175737473e209bc26772ebc81485ef7892926a9a";
    pub const BLOCK_530524_HEADER: [u8; 423] = [57, 73, 91, 113, 39, 222, 160, 167, 84, 75, 106, 43, 229, 239, 178, 232, 185, 141, 13, 10, 102, 51, 53, 79, 149, 50, 253, 131, 161, 2, 49, 128, 114, 97, 32, 0, 22, 196, 164, 88, 29, 53, 123, 203, 140, 252, 0, 255, 231, 108, 155, 64, 36, 21, 115, 125, 211, 51, 133, 61, 50, 26, 171, 195, 12, 101, 45, 237, 68, 131, 79, 10, 196, 191, 102, 173, 42, 104, 27, 217, 174, 50, 118, 125, 190, 5, 123, 82, 139, 43, 117, 234, 195, 86, 132, 130, 63, 254, 30, 34, 8, 6, 66, 65, 66, 69, 181, 1, 1, 6, 0, 0, 0, 85, 246, 1, 5, 0, 0, 0, 0, 202, 232, 36, 72, 160, 22, 45, 97, 113, 94, 242, 7, 224, 191, 196, 83, 171, 63, 98, 2, 21, 167, 102, 172, 106, 182, 90, 68, 177, 247, 136, 49, 152, 43, 224, 157, 63, 169, 54, 153, 164, 221, 82, 139, 254, 13, 160, 194, 201, 59, 10, 43, 129, 174, 61, 246, 5, 208, 196, 26, 54, 124, 6, 0, 209, 31, 122, 203, 92, 95, 37, 233, 29, 125, 95, 146, 121, 45, 99, 78, 211, 238, 35, 49, 151, 23, 28, 158, 28, 53, 37, 73, 88, 64, 29, 4, 5, 66, 65, 66, 69, 1, 1, 152, 22, 49, 129, 180, 93, 133, 46, 245, 65, 155, 144, 71, 162, 110, 204, 70, 250, 159, 75, 249, 130, 237, 22, 161, 171, 136, 63, 188, 50, 132, 68, 6, 120, 155, 114, 21, 17, 88, 83, 52, 103, 122, 90, 106, 123, 35, 245, 163, 161, 2, 39, 48, 136, 137, 77, 101, 249, 95, 78, 146, 147, 142, 130, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 185, 64, 70, 252, 214, 210, 93, 108, 180, 199, 173, 39, 204, 44, 139, 28, 233, 58, 65, 71, 10, 28, 209, 10, 47, 234, 63, 214, 8, 157, 182, 187, 114, 26, 126, 9, 134, 131, 163, 116, 36, 108, 63, 203, 16, 92, 148, 3, 185, 64, 70, 252, 214, 210, 93, 108, 180, 199, 173, 39, 204, 44, 139, 28, 233, 58, 65, 71, 10, 28, 209, 10, 47, 234, 63, 214, 8, 157, 182, 187, 114, 26, 126, 9, 134, 131, 163, 116, 36, 108, 63, 203, 16, 92, 148, 3, 4, 0];
    pub const BLOCK_530524_PARENT_HASH: &str = "39495b7127dea0a7544b6a2be5efb2e8b98d0d0a6633354f9532fd83a1023180";

    pub const BLOCK_530525_BLOCK_HASH: &str = "bd3b346e93e94e1c8395b9a3fbb3c72d3a162de803709a2b49802fec6f803271";
    pub const BLOCK_530525_HEADER: [u8; 423] = [75, 253, 87, 84, 164, 93, 174, 243, 112, 128, 213, 222, 23, 87, 55, 71, 62, 32, 155, 194, 103, 114, 235, 200, 20, 133, 239, 120, 146, 146, 106, 154, 118, 97, 32, 0, 146, 4, 82, 246, 154, 146, 68, 111, 2, 88, 170, 132, 7, 161, 112, 251, 109, 113, 91, 65, 23, 63, 118, 54, 156, 247, 161, 140, 97, 99, 176, 176, 229, 133, 86, 151, 2, 218, 143, 35, 241, 55, 170, 107, 123, 179, 65, 163, 99, 36, 200, 73, 15, 73, 157, 64, 78, 219, 79, 107, 231, 17, 82, 232, 8, 6, 66, 65, 66, 69, 181, 1, 1, 6, 0, 0, 0, 86, 246, 1, 5, 0, 0, 0, 0, 54, 227, 94, 126, 182, 174, 73, 177, 220, 47, 25, 131, 237, 206, 0, 67, 215, 119, 43, 58, 225, 47, 218, 231, 45, 26, 73, 152, 242, 130, 101, 115, 192, 177, 212, 56, 143, 56, 138, 63, 130, 134, 2, 62, 157, 120, 115, 196, 250, 31, 136, 41, 138, 93, 155, 32, 207, 221, 103, 164, 126, 13, 88, 6, 130, 84, 51, 50, 170, 65, 150, 248, 110, 234, 225, 224, 120, 21, 106, 14, 229, 62, 238, 201, 224, 7, 187, 98, 121, 205, 88, 55, 55, 250, 44, 11, 5, 66, 65, 66, 69, 1, 1, 128, 222, 206, 14, 221, 149, 122, 116, 170, 27, 131, 188, 146, 244, 91, 213, 6, 16, 195, 68, 136, 208, 177, 102, 187, 236, 65, 74, 32, 58, 241, 64, 140, 215, 55, 157, 146, 124, 173, 154, 169, 204, 240, 253, 124, 126, 123, 245, 77, 39, 211, 66, 145, 35, 188, 83, 106, 15, 199, 103, 250, 86, 29, 140, 0, 4, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 136, 156, 99, 231, 220, 188, 7, 141, 55, 187, 205, 27, 169, 206, 156, 245, 132, 146, 176, 214, 140, 203, 149, 216, 156, 247, 50, 242, 207, 191, 13, 128, 199, 45, 237, 209, 246, 247, 223, 167, 182, 173, 229, 61, 171, 104, 31, 195, 136, 156, 99, 231, 220, 188, 7, 141, 55, 187, 205, 27, 169, 206, 156, 245, 132, 146, 176, 214, 140, 203, 149, 216, 156, 247, 50, 242, 207, 191, 13, 128, 199, 45, 237, 209, 246, 247, 223, 167, 182, 173, 229, 61, 171, 104, 31, 195, 48, 0];
    pub const BLOCK_530525_PARENT_HASH: &str = "4bfd5754a45daef37080d5de175737473e209bc26772ebc81485ef7892926a9a";

    pub const BLOCK_530526_BLOCK_HASH: &str = "60ee3874d6f79ba54bb4f9b62972e905a0cc4288e442879717bd62035fccf489";
    pub const BLOCK_530526_HEADER: [u8; 423] = [189, 59, 52, 110, 147, 233, 78, 28, 131, 149, 185, 163, 251, 179, 199, 45, 58, 22, 45, 232, 3, 112, 154, 43, 73, 128, 47, 236, 111, 128, 50, 113, 122, 97, 32, 0, 125, 88, 216, 106, 78, 15, 223, 129, 208, 221, 82, 33, 23, 7, 114, 152, 215, 15, 89, 196, 120, 191, 146, 61, 43, 28, 242, 215, 27, 174, 144, 5, 159, 181, 61, 184, 49, 49, 241, 40, 61, 216, 252, 230, 229, 97, 72, 65, 60, 208, 229, 23, 161, 209, 187, 22, 177, 64, 132, 195, 232, 41, 27, 156, 8, 6, 66, 65, 66, 69, 181, 1, 1, 5, 0, 0, 0, 87, 246, 1, 5, 0, 0, 0, 0, 208, 37, 173, 168, 177, 237, 96, 227, 63, 185, 51, 11, 22, 194, 246, 179, 137, 240, 114, 18, 242, 24, 243, 33, 223, 65, 133, 170, 209, 150, 110, 20, 70, 170, 204, 180, 177, 137, 238, 198, 152, 188, 19, 55, 15, 89, 81, 42, 190, 228, 199, 169, 107, 28, 114, 88, 230, 235, 10, 19, 168, 172, 130, 6, 91, 73, 146, 91, 151, 200, 100, 32, 139, 8, 216, 184, 221, 191, 175, 118, 41, 254, 41, 238, 163, 33, 122, 66, 109, 61, 237, 177, 230, 145, 66, 4, 5, 66, 65, 66, 69, 1, 1, 226, 243, 40, 252, 187, 22, 38, 36, 184, 227, 93, 208, 252, 234, 147, 124, 181, 5, 242, 180, 180, 9, 148, 27, 173, 18, 210, 144, 253, 32, 193, 16, 197, 38, 37, 4, 46, 144, 73, 77, 145, 93, 241, 39, 145, 88, 155, 250, 232, 235, 104, 109, 144, 168, 129, 60, 218, 183, 37, 44, 208, 75, 168, 142, 0, 4, 64, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 171, 84, 213, 115, 238, 211, 84, 39, 200, 191, 146, 137, 124, 72, 221, 18, 87, 79, 183, 67, 76, 135, 16, 187, 166, 24, 64, 10, 200, 213, 118, 176, 234, 209, 7, 223, 225, 29, 13, 147, 99, 228, 18, 66, 75, 82, 1, 155, 171, 84, 213, 115, 238, 211, 84, 39, 200, 191, 146, 137, 124, 72, 221, 18, 87, 79, 183, 67, 76, 135, 16, 187, 166, 24, 64, 10, 200, 213, 118, 176, 234, 209, 7, 223, 225, 29, 13, 147, 99, 228, 18, 66, 75, 82, 1, 155, 48, 0];
    pub const BLOCK_530526_PARENT_HASH: &str = "bd3b346e93e94e1c8395b9a3fbb3c72d3a162de803709a2b49802fec6f803271";

    pub const BLOCK_530527_BLOCK_HASH: &str = "62f1aaf6297b86b3749448d66cc43deada49940c3912a4ec4916344058e8f065";
    pub const BLOCK_530527_HEADER: [u8; 423] = [96, 238, 56, 116, 214, 247, 155, 165, 75, 180, 249, 182, 41, 114, 233, 5, 160, 204, 66, 136, 228, 66, 135, 151, 23, 189, 98, 3, 95, 204, 244, 137, 126, 97, 32, 0, 190, 164, 124, 198, 148, 159, 37, 236, 27, 38, 47, 14, 45, 255, 92, 44, 150, 159, 222, 131, 77, 242, 223, 82, 2, 83, 60, 239, 59, 240, 100, 159, 96, 27, 46, 203, 51, 252, 140, 150, 191, 182, 131, 236, 137, 101, 220, 13, 234, 16, 27, 244, 228, 172, 234, 8, 48, 159, 13, 152, 219, 155, 247, 203, 8, 6, 66, 65, 66, 69, 181, 1, 1, 3, 0, 0, 0, 88, 246, 1, 5, 0, 0, 0, 0, 190, 158, 92, 111, 241, 162, 35, 126, 93, 188, 37, 82, 192, 193, 97, 255, 220, 78, 2, 221, 136, 16, 40, 209, 176, 62, 99, 123, 241, 3, 171, 74, 153, 142, 189, 223, 96, 214, 248, 72, 130, 18, 61, 206, 59, 90, 74, 134, 243, 152, 158, 147, 17, 32, 50, 17, 123, 252, 105, 201, 101, 221, 52, 12, 228, 84, 40, 97, 46, 24, 29, 44, 193, 195, 239, 85, 92, 11, 209, 116, 131, 131, 112, 141, 93, 248, 119, 31, 223, 87, 113, 197, 199, 248, 153, 15, 5, 66, 65, 66, 69, 1, 1, 100, 140, 107, 114, 139, 31, 108, 148, 6, 14, 44, 185, 118, 71, 240, 246, 107, 200, 22, 228, 193, 191, 22, 86, 191, 215, 245, 172, 170, 103, 161, 33, 231, 34, 30, 180, 220, 197, 119, 187, 76, 252, 234, 86, 161, 209, 72, 150, 67, 111, 29, 80, 105, 21, 27, 224, 76, 214, 153, 24, 99, 218, 93, 133, 0, 4, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 129, 1, 150, 25, 213, 166, 9, 223, 191, 127, 80, 148, 89, 6, 174, 43, 165, 238, 147, 157, 55, 253, 47, 218, 243, 92, 25, 160, 242, 210, 121, 252, 26, 22, 232, 127, 72, 52, 89, 70, 79, 112, 217, 198, 184, 97, 175, 188, 82, 217, 150, 25, 213, 166, 9, 223, 191, 127, 80, 148, 89, 6, 174, 43, 165, 238, 147, 157, 55, 253, 47, 218, 243, 92, 25, 160, 242, 210, 121, 252, 26, 22, 232, 127, 72, 52, 89, 70, 79, 112, 217, 198, 184, 97, 175, 188, 82, 217, 4, 0];
    pub const BLOCK_530527_PARENT_HASH: &str = "60ee3874d6f79ba54bb4f9b62972e905a0cc4288e442879717bd62035fccf489";
    pub const BLOCK_530527_PRECOMMIT_MESSAGE: [u8; ENCODED_PRECOMMIT_LENGTH] = [
                1,
                98, 241, 170, 246, 41, 123, 134, 179, 116, 148, 72, 214, 108, 196, 61, 234, 218, 73, 148, 12, 57, 18, 164, 236, 73, 22, 52, 64,
                88, 232, 240, 101,
                95, 24, 8, 0,
                104, 11, 0, 0, 0, 0, 0, 0,
                240, 1, 0, 0, 0, 0, 0, 0];
    pub const BLOCK_530527_AUTHORITY_SIGS: [&str; QUORUM_SIZE] = [
        "3ebc508daaf5edd7a4b4779743ce9241519aa8940264c2be4f39dfd0f7a4f2c4c587752fbc35d6d34b8ecd494dfe101e49e6c1ccb0e41ff2aa52bc481fcd3e0c",
        "48f851a4cb99db770461b3b42e7a055fb4801a2a4d2627691e52d0bb955bc8c6c490b0d04d97365e39b7cffeb4489318f28deddbc0710a57f4d94a726a98df01",
        "cbc199cf5754103a3a52d51795596c1535a8766ea84073d6c064db28fa0a357521dd912516d694813e21d279a72f11b59029bed7671db6b0d2ee0cd68d0ebb0f",
        "8f006a2ac7cd3396d20d2230204e2742fd413bde5c4ad6ad688f01def90ae2b80bcfee0507aedbcc01a389c74f7c5315eadedff800f3ff8d7482c2d8afe47500",
        "d5b234c6268f1d822217ac2a88358d31ec14f8f975b0f5d3f87ada7dd88e87400f11e9aac94cab3c2d1e8d38088cc505e9426f35d07a5ae9f7bb5c33244f160a",
        "da57013e372c8cd4aa7bc6c6112d9404325e8d48fcc02c51ad915a725ee0424c3a54cee03dfe315d91f3e6a576f8134a17b28717485340c9ac1ebfe7fc72360f",
        "b22b809b0249ee4e8d43d3aee1a2f40bd529f9eaaa6493d7ec8198b5c93a15ce1e7d653d2aaf710ebfef4ff5aec8e120faf22776417b3621bf6b9de4af540805"
    ];
    pub const BLOCK_530527_PUB_KEY_INDICES: [usize; QUORUM_SIZE] = [
        2,
        3,
        6,
        1,
        0,
        9,
        5,
    ];
    pub const BLOCK_530527_AUTHORITY_SET_ID: u64 = 496;
    pub const BLOCK_530527_AUTHORITY_SET: [&str; NUM_AUTHORITIES_PADDED] = [
        "8e9edb840fcf9ce51b9d2e65dcae423aafd03ab5973da8d806207395a26af66e",
        "8d9b15ea8335270510135b7f7c5ef94e0df70e751d3c5f95fd1aa6d7766929b6",
        "0e0945b2628f5c3b4e2a6b53df997fc693344af985b11e3054f36a384cc4114b",
        "5568a33085a85e1680b83823c6b4b8a0b51d506748b5d5266dd536e258e18a9d",
        "cc6de644a35f4b205603fa125612df211d4f9d75e07c84d85cd35ea32a6b1ced",
        "e4c08a068e72a466e2f377e862b5b2ed473c4f0e58d7d265a123ad11fef2a797",
        "8916179559464bd193d94b053b250a0edf3da5b61d1f2bf2bf2640930dfd2c0e",
        "079590df34cd1fa2f83cb1ef770b3e254abb00fa7dbfb2f7f21b383a7a726bb2",
        "cc068bf6c1e467be8e2fdafb1d42ddafe8e66a0d05ea036c3d766cb6a0360797",
        "ba76ee41deca67a1d69113f89e233df3a63e6722ca988163848770f4659eb150",
        "ba76ee41deca67a1d69113f89e233df3a63e6722ca988163848770f4659eb150",    // Can be any valid pubkey
        "ba76ee41deca67a1d69113f89e233df3a63e6722ca988163848770f4659eb150",    // Can be any valid pubkey
        "ba76ee41deca67a1d69113f89e233df3a63e6722ca988163848770f4659eb150",    // Can be any valid pubkey
        "ba76ee41deca67a1d69113f89e233df3a63e6722ca988163848770f4659eb150",    // Can be any valid pubkey
        "ba76ee41deca67a1d69113f89e233df3a63e6722ca988163848770f4659eb150",    // Can be any valid pubkey
        "ba76ee41deca67a1d69113f89e233df3a63e6722ca988163848770f4659eb150",    // Can be any valid pubkey
    ];
    pub const BLOCK_530527_AUTHORITY_SET_COMMITMENT: &str = "0c076c231c5a3e15b03288bafbfe10ee86bd0ad23f9fecc86ee03fb439e045f6";

    pub const BLOCK_530527_PUBLIC_INPUTS_HASH: &str = "e9a1c86ce12af878fcf5953acf98233207ac109e3ab381b3306de42b399de41a";
}
