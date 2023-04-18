use avail_subxt::build_client;
use avail_subxt::primitives::Header;
use sp_state_machine::{read_proof_check, StorageProof};

use subxt::{
	ext::{
		sp_core::{storage::StorageKey, twox_128, H256},
		sp_runtime::traits::BlakeTwo256,
	},
};

#[tokio::main]
pub async fn main() {
    let url: &str = "wss://testnet.avail.tools:443/ws";

    let c = build_client(url).await.unwrap();

    // The block hash for block 576728 (https://testnet.avail.tools/#/explorer/query/576728)
    let block_hash_vec = hex::decode("b71429ef80257a25358e386e4ca1debe72c38ea69d833e23416a4225fabb1a78").unwrap();
    let mut block_hash_array: [u8; 32] = [0; 32];
    for i in 0..block_hash_vec.len() {
        block_hash_array[i] = block_hash_vec[i];
    }
    let block_hash = Some(H256(block_hash_array));

    // Get the header from the RPC
    let header: Header = c.rpc().header(block_hash).await.unwrap().unwrap();

    // Get the events from this block
    let events = c.events().at(block_hash).await.unwrap();

    for e in events.iter() {
        let event = e.unwrap();
        let name = event.variant_name();
        println!("event is {:?}\n\n\n", name);
    }

    // Construct the storage key for the events
    let mut events_storage_key = twox_128(b"System").to_vec();
    events_storage_key.extend(twox_128(b"Events").to_vec());
    let sk = StorageKey(events_storage_key);

    // Output the storage key in hex
    let sk_hex = hex::encode(sk.0.clone());
    println!("storage key is {:?}\n\n\n", sk_hex);

    let keys = [sk.0.as_slice()];

    // Retrieve the storage data for the event key
    let data = c.rpc().storage(keys[0], block_hash).await.unwrap().unwrap();
    println!("data is {:?}\n\n\n", data.0);

    // Retrieve the storage proof for the event key
    let proof = c.rpc().read_proof(keys, block_hash).await.unwrap();

    // Convert ReadProof type to StorageProof type (NOT WORKING!!!)
    // Sample conversion of ReadProof to StorageProof here: https://github.com/paritytech/substrate/blob/785115b3a13901b0c708af8166430bcc9c71f28f/client/rpc/src/state/state_full.rs#L365
    let mut sp_vec = Vec::new();
    for i in 0..proof.proof.len() {
        sp_vec.push(proof.proof[i].0.clone());
    }
    let sp = StorageProof::new(sp_vec);

    // Check that the proof is valid (NOT WORKING!!!)
    // Can also check proof here:  https://github.com/polytope-labs/solidity-merkle-trees/blob/main/src/MerklePatricia.sol#L31
    let local_result1 = read_proof_check::<BlakeTwo256, _>(header.state_root, sp, keys).unwrap();

    println!("local_result1 is {:?}\n\n\n", local_result1);
}