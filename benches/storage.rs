use std::hint::black_box;

use bonsai_trie::{
    databases::HashMapDb,
    id::{BasicId, BasicIdBuilder},
    BitVec, BonsaiStorage, BonsaiStorageConfig,
};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use rand::{prelude::*, thread_rng};
use starknet_types_core::{
    felt::Felt,
    hash::{Pedersen, Poseidon, StarkHash},
};

// Key height for benchmarks: 6 bytes = 48 bits
const BENCH_KEY_HEIGHT: u8 = 48;

mod flamegraph;

fn drop_storage(c: &mut Criterion) {
    c.bench_function("drop storage", move |b| {
        b.iter_batched(
            || {
                let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
                    HashMapDb::<BasicId>::default(),
                    BonsaiStorageConfig::default(),
                    BENCH_KEY_HEIGHT,
                );

                let mut rng = SmallRng::seed_from_u64(42);
                let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
                for _ in 0..4000 {
                    let bitvec = BitVec::from_vec(vec![
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                    ]);
                    bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
                }

                let mut id_builder = BasicIdBuilder::new();
                let id1 = id_builder.new_id();
                bonsai_storage.commit(id1).unwrap();

                bonsai_storage
            },
            std::mem::drop,
            BatchSize::LargeInput,
        );
    });
}

fn storage_with_insert(c: &mut Criterion) {
    c.bench_function("storage commit with insert", move |b| {
        let mut rng = thread_rng();
        b.iter_batched_ref(
            || {
                let bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
                    HashMapDb::<BasicId>::default(),
                    BonsaiStorageConfig::default(),
                    BENCH_KEY_HEIGHT,
                );
                bonsai_storage
            },
            |bonsai_storage| {
                let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
                for _ in 0..40000 {
                    let bitvec = BitVec::from_vec(vec![
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                    ]);
                    bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
                }
                // let mut id_builder = BasicIdBuilder::new();
                // bonsai_storage.commit(id_builder.new_id()).unwrap();
            },
            BatchSize::LargeInput,
        );
    });
}

fn storage(c: &mut Criterion) {
    c.bench_function("storage commit", move |b| {
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            BENCH_KEY_HEIGHT,
        );
        let mut rng = SmallRng::seed_from_u64(42);

        let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
        for _ in 0..1000 {
            let bitvec = BitVec::from_vec(vec![
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
            ]);
            bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
        }

        let mut id_builder = BasicIdBuilder::new();
        b.iter_batched_ref(
            || bonsai_storage.clone(),
            |bonsai_storage| {
                bonsai_storage.commit(id_builder.new_id()).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn one_update(c: &mut Criterion) {
    c.bench_function("one update", move |b| {
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            BENCH_KEY_HEIGHT,
        );
        let mut rng = SmallRng::seed_from_u64(42);

        let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
        for _ in 0..1000 {
            let bitvec = BitVec::from_vec(vec![
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
            ]);
            bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
        }

        let mut id_builder = BasicIdBuilder::new();
        bonsai_storage.commit(id_builder.new_id()).unwrap();

        b.iter_batched_ref(
            || bonsai_storage.clone(),
            |bonsai_storage| {
                let bitvec = BitVec::from_vec(vec![0, 1, 2, 3, 4, 5]);
                bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
                bonsai_storage.commit(id_builder.new_id()).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn five_updates(c: &mut Criterion) {
    c.bench_function("five updates", move |b| {
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            BENCH_KEY_HEIGHT,
        );
        let mut rng = SmallRng::seed_from_u64(42);

        let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
        for _ in 0..1000 {
            let bitvec = BitVec::from_vec(vec![
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
                rng.gen(),
            ]);
            bonsai_storage.insert(&[], &bitvec, &felt).unwrap();
        }

        let mut id_builder = BasicIdBuilder::new();
        bonsai_storage.commit(id_builder.new_id()).unwrap();

        b.iter_batched_ref(
            || bonsai_storage.clone(),
            |bonsai_storage| {
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 1, 2, 3, 4, 5]), &felt)
                    .unwrap();
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 2, 2, 5, 4, 5]), &felt)
                    .unwrap();
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 1, 2, 3, 3, 5]), &felt)
                    .unwrap();
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 1, 1, 3, 99, 3]), &felt)
                    .unwrap();
                bonsai_storage
                    .insert(&[], &BitVec::from_vec(vec![0, 1, 2, 3, 4, 6]), &felt)
                    .unwrap();
                bonsai_storage.commit(id_builder.new_id()).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn multiple_contracts(c: &mut Criterion) {
    c.bench_function("multiple contracts", move |b| {
        let mut bonsai_storage: BonsaiStorage<BasicId, _, Pedersen> = BonsaiStorage::new(
            HashMapDb::<BasicId>::default(),
            BonsaiStorageConfig::default(),
            32, // 4 bytes = 32 bits for the key in this benchmark
        );
        let mut rng = thread_rng();

        let felt = Felt::from_hex("0x66342762FDD54D033c195fec3ce2568b62052e").unwrap();
        for _ in 0..1000 {
            let bitvec = BitVec::from_vec(vec![rng.gen(), rng.gen(), rng.gen(), rng.gen()]);
            bonsai_storage
                .insert(
                    &[
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                        rng.gen(),
                    ],
                    &bitvec,
                    &felt,
                )
                .unwrap();
        }

        let mut id_builder = BasicIdBuilder::new();

        b.iter_batched_ref(
            || bonsai_storage.clone(),
            |bonsai_storage| {
                bonsai_storage.commit(id_builder.new_id()).unwrap();
            },
            criterion::BatchSize::LargeInput,
        );
    });
}

fn pedersen_hash(c: &mut Criterion) {
    c.bench_function("pedersen hash", move |b| {
        let felt0 =
            Felt::from_hex("0x100bd6fbfced88ded1b34bd1a55b747ce3a9fde9a914bca75571e4496b56443")
                .unwrap();
        let felt1 =
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f")
                .unwrap();
        b.iter(|| {
            black_box(Pedersen::hash(&felt0, &felt1));
        })
    });
}

fn poseidon_hash(c: &mut Criterion) {
    c.bench_function("poseidon hash", move |b| {
        let felt0 =
            Felt::from_hex("0x100bd6fbfced88ded1b34bd1a55b747ce3a9fde9a914bca75571e4496b56443")
                .unwrap();
        let felt1 =
            Felt::from_hex("0x00a038cda302fedbc4f6117648c6d3faca3cda924cb9c517b46232c6316b152f")
                .unwrap();
        b.iter(|| {
            black_box(Poseidon::hash(&felt0, &felt1));
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default(); // .with_profiler(flamegraph::FlamegraphProfiler::new(100));
    targets = storage, one_update, five_updates, pedersen_hash, poseidon_hash, drop_storage, storage_with_insert, multiple_contracts
}
criterion_main!(benches);
