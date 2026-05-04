use std::sync::Arc;

use halo2_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    dev::MockProver,
    pasta::Fp,
    plonk::{Advice, Circuit, Column, ConstraintSystem, Error, Selector},
    poly::Rotation,
};
use zkwire::prelude::*;

#[derive(Clone, Debug)]
struct FibonacciConfig {
    advice: [Column<Advice>; 3],
    selector: Selector,
}

#[derive(Default)]
struct BrokenFibonacciCircuit;

impl Circuit<Fp> for BrokenFibonacciCircuit {
    type Config = FibonacciConfig;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self
    }

    fn configure(meta: &mut ConstraintSystem<Fp>) -> Self::Config {
        let advice = [
            meta.advice_column(),
            meta.advice_column(),
            meta.advice_column(),
        ];
        let selector = meta.selector();

        meta.create_gate("fibonacci step", |meta| {
            let s = meta.query_selector(selector);
            let c = meta.query_advice(advice[2], Rotation::cur());
            let a = meta.query_advice(advice[0], Rotation::cur());
            let b = meta.query_advice(advice[1], Rotation::cur());
            vec![s * (c - a - b)]
        });

        FibonacciConfig { advice, selector }
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<Fp>,
    ) -> Result<(), Error> {
        layouter.assign_region(
            || "broken fibonacci",
            |mut region| {
                config.selector.enable(&mut region, 0)?;

                zkwire_assign!(
                    region,
                    "broken fibonacci",
                    "fib(a) intentionally wrong",
                    config.advice[0],
                    0,
                    Value::known(Fp::from(2)),
                )?;
                zkwire_assign!(
                    region,
                    "broken fibonacci",
                    "fib(b)",
                    config.advice[1],
                    0,
                    Value::known(Fp::from(1)),
                )?;
                zkwire_assign!(
                    region,
                    "broken fibonacci",
                    "fib(c)",
                    config.advice[2],
                    0,
                    Value::known(Fp::from(2)),
                )?;

                Ok(())
            },
        )
    }
}

fn main() {
    let tracker = Arc::new(ZkWireTracker::new());
    let _guard = ZkWireTracker::install(&tracker);

    let circuit = BrokenFibonacciCircuit;
    let prover = MockProver::run(4, &circuit, vec![]).unwrap();

    let _ = prover.verify_and_forge(&tracker);
}
