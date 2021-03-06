// Copyright 2018 The Exonum Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![feature(test)]
#![allow(dead_code)]

extern crate test;
extern crate rand;
extern crate tempdir;
extern crate exonum;

#[cfg(all(test, feature = "long_benchmarks"))]
mod tests {
    use std::collections::HashSet;

    use test::Bencher;
    use rand::{Rng, XorShiftRng, SeedableRng};
    use tempdir::TempDir;
    use exonum::storage::{Database, MemoryDB, RocksDB, DbOptions, ProofMapIndex, ProofListIndex};
    use exonum::storage::proof_map_index::PROOF_MAP_KEY_SIZE as KEY_SIZE;

    const NAME: &str = "name";

    fn generate_random_kv(len: usize) -> Vec<([u8; KEY_SIZE], Vec<u8>)> {
        let mut rng = XorShiftRng::from_seed([1, 56, 168, 192]);
        let mut exists_keys = HashSet::new();
        let mut base = [0; KEY_SIZE];
        rng.fill_bytes(&mut base);
        let base = base;

        let kv_generator = |_| {
            let mut v = vec![0; 8];

            // Generate only unique keys
            let mut k = base;
            let byte: usize = rng.gen_range(0, 31);
            k[byte] = rng.gen::<u8>();

            rng.fill_bytes(&mut v);
            while exists_keys.contains(&k) {
                rng.fill_bytes(&mut k);
            }
            exists_keys.insert(k);
            (k, v)
        };

        (0..len).map(kv_generator).collect::<Vec<_>>()
    }

    fn merkle_table_insertion<T: Database>(b: &mut Bencher, db: &T) {
        let mut rng = XorShiftRng::from_seed([192, 168, 56, 1]);
        let mut storage = db.fork();
        let mut table = ProofListIndex::new(NAME, &mut storage);

        b.iter(|| {
            let v_generator = |_| {
                let mut chunk = vec![0; 10];
                rng.fill_bytes(&mut chunk);
                chunk
            };

            for item in (0..1000).map(v_generator) {
                table.push(item);
            }
        });
    }

    fn merkle_patricia_table_insertion<T: Database>(b: &mut Bencher, db: &T) {
        let data = generate_random_kv(200);
        let mut storage = db.fork();
        let mut table = ProofMapIndex::new(NAME, &mut storage);

        b.iter(|| for item in &data {
            table.put(&item.0, item.1.clone());
        });
    }

    fn merkle_patricia_table_insertion_fork<T: Database>(b: &mut Bencher, db: &T) {
        let data = generate_random_kv(200);

        b.iter(|| {
            let patch;
            {
                let mut fork = db.fork();
                {
                    let mut table = ProofMapIndex::new(NAME, &mut fork);
                    for item in &data {
                        table.put(&item.0, item.1.clone());
                    }
                }
                patch = fork.into_patch();
            }
            db.fork().merge(patch);
        });
    }

    fn merkle_patricia_table_insertion_large_map<T: Database>(b: &mut Bencher, db: &T) {
        let data = generate_random_kv(200);
        let mut storage = db.fork();
        let mut table = ProofMapIndex::new(NAME, &mut storage);

        for item in &data {
            table.put(&item.0, item.1.clone());
        }

        b.iter(|| for item in &data {
            table.put(&item.0, item.1.clone());
        });
    }

    fn proof_map_index_build_proofs<T: Database>(b: &mut Bencher, db: &T) {
        let data = generate_random_kv(512);
        let mut storage = db.fork();
        let mut table = ProofMapIndex::new(NAME, &mut storage);

        for item in &data {
            table.put(&item.0, item.1.clone());
        }
        let table_merkle_root = table.merkle_root();
        let mut proofs = Vec::with_capacity(data.len());

        b.iter(|| {
            proofs.clear();
            proofs.extend(data.iter().map(|item| table.get_proof(item.0)));
        });

        for (i, proof) in proofs.into_iter().enumerate() {
            let checked_proof = proof.check().unwrap();
            assert_eq!(*checked_proof.entries()[0].1, data[i].1);
            assert_eq!(checked_proof.merkle_root(), table_merkle_root);
        }
    }

    fn proof_map_index_verify_proofs<T: Database>(b: &mut Bencher, db: &T) {
        let data = generate_random_kv(512);
        let mut storage = db.fork();
        let mut table = ProofMapIndex::new(NAME, &mut storage);

        for item in &data {
            table.put(&item.0, item.1.clone());
        }
        let table_merkle_root = table.merkle_root();
        let proofs: Vec<_> = data.iter().map(|item| table.get_proof(item.0)).collect();

        b.iter(|| for (i, proof) in proofs.iter().enumerate() {
            let checked_proof = proof.clone().check().unwrap();
            assert_eq!(*checked_proof.entries()[0].1, data[i].1);
            assert_eq!(checked_proof.merkle_root(), table_merkle_root);
        });
    }

    fn create_rocksdb(tempdir: &TempDir) -> RocksDB {
        let options = DbOptions::default();
        RocksDB::open(tempdir.path(), &options).unwrap()
    }

    #[bench]
    fn bench_merkle_table_append_memorydb(b: &mut Bencher) {
        let db = MemoryDB::new();
        merkle_table_insertion(b, &db);
    }

    #[bench]
    fn bench_merkle_table_append_rocksdb(b: &mut Bencher) {
        let tempdir = TempDir::new("exonum").unwrap();
        let db = create_rocksdb(&tempdir);
        merkle_table_insertion(b, &db);
    }

    #[bench]
    fn bench_merkle_patricia_table_insertion_memorydb(b: &mut Bencher) {
        let db = MemoryDB::new();
        merkle_patricia_table_insertion(b, &db);
    }

    #[bench]
    fn bench_merkle_patricia_table_insertion_fork_memorydb(b: &mut Bencher) {
        let db = MemoryDB::new();
        merkle_patricia_table_insertion_fork(b, &db);
    }

    #[bench]
    fn bench_merkle_patricia_table_insertion_rocksdb(b: &mut Bencher) {
        let tempdir = TempDir::new("exonum").unwrap();
        let db = create_rocksdb(&tempdir);
        merkle_patricia_table_insertion(b, &db);
    }

    #[bench]
    fn bench_merkle_patricia_table_insertion_fork_rocksdb(b: &mut Bencher) {
        let tempdir = TempDir::new("exonum").unwrap();
        let db = create_rocksdb(&tempdir);
        merkle_patricia_table_insertion_fork(b, &db);
    }

    #[bench]
    fn long_bench_merkle_patricia_table_insertion_memorydb(b: &mut Bencher) {
        let db = MemoryDB::new();
        merkle_patricia_table_insertion_large_map(b, &db);
    }

    #[bench]
    fn long_bench_merkle_patricia_table_insertion_rocksdb(b: &mut Bencher) {
        let tempdir = TempDir::new("exonum").unwrap();
        let db = create_rocksdb(&tempdir);
        merkle_patricia_table_insertion_large_map(b, &db);
    }

    #[bench]
    fn long_bench_proof_map_index_build_proofs_memorydb(b: &mut Bencher) {
        let db = MemoryDB::new();
        proof_map_index_build_proofs(b, &db);
    }

    #[bench]
    fn long_bench_proof_map_index_build_proofs_rocksdb(b: &mut Bencher) {
        let tempdir = TempDir::new("exonum").unwrap();
        let db = create_rocksdb(&tempdir);
        proof_map_index_build_proofs(b, &db);
    }

    #[bench]
    fn long_bench_proof_map_index_validate_proofs_memorydb(b: &mut Bencher) {
        let db = MemoryDB::new();
        proof_map_index_verify_proofs(b, &db);
    }
}
