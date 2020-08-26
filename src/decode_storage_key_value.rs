use std::collections::HashMap;

use crate::metadata::{Metadata, StorageMetadata};
use codec::{Decode, Encode};
use frame_metadata::{DecodeDifferent, StorageEntryType, StorageHasher};

////////////////////////////////////////////////////////////////////////
//    Storage Key/Value decode
////////////////////////////////////////////////////////////////////////

// storage prefix in hex string to the StorageMetadata
pub struct StoragePrefixLookupTable(pub HashMap<String, StorageMetadata>);

impl From<Metadata> for StoragePrefixLookupTable {
    fn from(metadata: Metadata) -> Self {
        Self(
            metadata
                .modules
                .into_iter()
                .map(|(_, module_metadata)| {
                    module_metadata
                        .storage
                        .into_iter()
                        .map(|(_, storage_metadata)| {
                            let storage_prefix = storage_metadata.prefix();
                            (hex::encode(storage_prefix.0), storage_metadata)
                        })
                })
                .flatten()
                .collect(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransparentStorageType {
    Plain,
    Map { key: String, value_ty: String },
    DoubleMap { key1: String, key2: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransparentStorageKey {
    pub module_prefix: String,
    pub storage_prefix: String,
    pub ty: TransparentStorageType,
}

// in hex
// module twox_128, 32 chars
// storage prefix twox_128, 32 chars
// module_prefix + storage_prefix = 32 * 2
pub const PREFIX_LENGTH: usize = 32 * 2;

impl StoragePrefixLookupTable {
    pub fn lookup(&self, prefix: &str) -> Option<&StorageMetadata> {
        self.0.get(prefix)
    }

    // Parse the final storage key and return the _readable_ key.
    pub fn parse_storage_key(&self, storage_key: String) -> Option<TransparentStorageKey> {
        let storage_prefix = &storage_key[..PREFIX_LENGTH];

        if let Some(storage_metadata) = self.lookup(storage_prefix) {
            match &storage_metadata.ty {
                StorageEntryType::Plain(value) => Some(TransparentStorageKey {
                    module_prefix: String::from(&storage_metadata.module_prefix),
                    storage_prefix: String::from(&storage_metadata.storage_prefix),
                    ty: TransparentStorageType::Plain,
                }),
                StorageEntryType::Map {
                    hasher,
                    key,
                    value,
                    unused,
                } => match hasher {
                    StorageHasher::Twox64Concat | StorageHasher::Blake2_128Concat => {
                        let hashed_key_concat = &storage_key[PREFIX_LENGTH..];
                        let hash_length = hash_length_of(hasher);
                        let _hashed_key = &hashed_key_concat[..hash_length];
                        let key = &hashed_key_concat[hash_length..];

                        let value_ty: String = match value {
                            DecodeDifferent::Encode(b) => unreachable!("TODO: really unreachable?"),
                            DecodeDifferent::Decoded(o) => o.into(),
                        };

                        Some(TransparentStorageKey {
                            module_prefix: String::from(&storage_metadata.module_prefix),
                            storage_prefix: String::from(&storage_metadata.storage_prefix),
                            ty: TransparentStorageType::Map {
                                key: key.into(),
                                value_ty,
                            },
                        })
                    }
                    _ => unreachable!("All Map storage should use foo_concat hasher"),
                },
                StorageEntryType::DoubleMap {
                    hasher,
                    key1,
                    key2,
                    value,
                    key2_hasher,
                } => {
                    // hashed_key1 ++ key1 ++ hashed_key2 ++ key2
                    let hashed_key_concat = &storage_key[PREFIX_LENGTH..];
                    match hasher {
                        StorageHasher::Twox64Concat | StorageHasher::Blake2_128Concat => {
                            let key1_hash_length = hash_length_of(hasher);

                            match key2_hasher {
                                StorageHasher::Twox64Concat | StorageHasher::Blake2_128Concat => {
                                    let key2_hash_length = hash_length_of(key2_hasher);
                                }
                                _ => unreachable!(
                                    "All DoubleMap storage should use foo_concat hasher for key2"
                                ),
                            }
                            todo!()
                        }
                        _ => unreachable!(
                            "All DoubleMap storage should use foo_concat hasher for key1"
                        ),
                    }
                }
            }
        } else {
            println!("ERROR: can not find the StorageMetadata from lookup table");
            None
        }
    }
}

// Returns the length of this hasher in hex.
fn hash_length_of(hasher: &StorageHasher) -> usize {
    match hasher {
        StorageHasher::Blake2_128 => 32,
        StorageHasher::Blake2_256 => 32 * 2,
        StorageHasher::Blake2_128Concat => 32,
        StorageHasher::Twox128 => 32,
        StorageHasher::Twox256 => 32 * 2,
        StorageHasher::Twox64Concat => 16,
        StorageHasher::Identity => unreachable!(),
    }
}

fn generic_decode<T: codec::Decode>(encoded: Vec<u8>) -> Result<T, codec::Error> {
    Decode::decode(&mut encoded.as_slice())
}

#[cfg(test)]
mod tests {
    use super::*;
    use frame_metadata::RuntimeMetadataPrefixed;
    use frame_system::AccountInfo;
    use pallet_balances::AccountData;
    use polkadot_primitives::v1::{AccountIndex, Balance};
    use std::convert::TryInto;

    fn get_metadata() -> Metadata {
        let s = include_str!("../test_data/metadata.txt");
        let s = s.trim();
        // string hex
        // decode hex string without 0x prefix
        let data = hex::decode(s).unwrap();
        let meta: RuntimeMetadataPrefixed =
            Decode::decode(&mut data.as_slice()).expect("failed to decode metadata prefixed");
        meta.try_into().expect("failed to convert to metadata")
    }

    // hex(encoded): 010000000864000000000000000000000000000000c80000000000000000000000000000002c01000000000000000000000000000090010000000000000000000000000000
    fn mock_account_info_data() -> (Vec<u8>, AccountInfo<AccountIndex, AccountData<Balance>>) {
        let mock_account_data: AccountData<Balance> = AccountData {
            free: 100,
            reserved: 200,
            misc_frozen: 300,
            fee_frozen: 400,
        };

        let mock_account_info: AccountInfo<AccountIndex, AccountData<Balance>> = AccountInfo {
            nonce: 1,
            refcount: 8,
            data: mock_account_data,
        };

        (mock_account_info.encode(), mock_account_info)
    }

    #[test]
    fn prase_storage_map_should_work() {
        //  twox_128("System"): 0x26aa394eea5630e07c48ae0c9558cef7
        // twox_128("Account"): 0xb99d880ec681799c0cf30e8886371da9
        //
        //      Account ID: 0xbe5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25f
        // Blake2 128 Hash: 0x32a5935f6edc617ae178fef9eb1e211f
        let metadata = get_metadata();
        let table: StoragePrefixLookupTable = metadata.into();

        let storage_key = "26aa394eea5630e07c48ae0c9558cef7b99d880ec681799c0cf30e8886371da932a5935f6edc617ae178fef9eb1e211fbe5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25f";
        let storage_value = "010000000864000000000000000000000000000000c80000000000000000000000000000002c01000000000000000000000000000090010000000000000000000000000000";

        let expected = TransparentStorageKey {
            module_prefix: "System".into(),
            storage_prefix: "Account".into(),
            ty: TransparentStorageType::Map {
                key: "be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25f".into(),
                value_ty: "AccountInfo<T::Index, T::AccountData>".into(),
            },
        };

        assert_eq!(
            table.parse_storage_key(storage_key.into()).unwrap(),
            expected
        );

        // Firstly, we need to build Storage Value decode function table.
        let mut storage_value_decode_fn_map = HashMap::new();

        let try_decode_account_info = |encoded: Vec<u8>| {
            generic_decode::<AccountInfo<AccountIndex, AccountData<Balance>>>(encoded)
        };

        storage_value_decode_fn_map.insert(
            String::from("AccountInfo<T::Index, T::AccountData>"),
            try_decode_account_info,
        );

        if let TransparentStorageType::Map { key, value_ty } = expected.ty {
            let decode_fn = storage_value_decode_fn_map.get(&value_ty).unwrap();
            let decoded_value = decode_fn(hex::decode(storage_value).unwrap()).unwrap();
            let expected_decoded_value = mock_account_info_data().1;
            assert_eq!(decoded_value, expected_decoded_value);
        } else {
            panic!("Not Map")
        }
    }

    #[test]
    fn test_decode_storage_value() {
        use codec::Encode;
        use frame_system::AccountInfo;
        use pallet_balances::AccountData;
        use polkadot_primitives::v1::{AccountIndex, Balance};
        use std::collections::HashMap;

        // Firstly, we need to build Storage Value decode function table by hand.
        let mut storage_value_decode_fn_map = HashMap::new();

        let try_decode_account_info = |encoded: Vec<u8>| {
            generic_decode::<AccountInfo<AccountIndex, AccountData<Balance>>>(encoded)
        };
        storage_value_decode_fn_map.insert(
            String::from("AccountInfo<T::Index, T::AccountData>"),
            try_decode_account_info,
        );

        let mock_account_data: AccountData<Balance> = AccountData {
            free: 100,
            reserved: 200,
            misc_frozen: 300,
            fee_frozen: 400,
        };

        let mock_account_info: AccountInfo<AccountIndex, AccountData<Balance>> = AccountInfo {
            nonce: 1,
            refcount: 8,
            data: mock_account_data,
        };

        let encoded_account_info = mock_account_info.encode();
        if let Some(decode_fn) =
            storage_value_decode_fn_map.get("AccountInfo<T::Index, T::AccountData>")
        {
            assert_eq!(decode_fn(encoded_account_info).unwrap(), mock_account_info);
        }
    }
}
