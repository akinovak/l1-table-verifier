use halo2::{
    arithmetic::FieldExt,
    circuit::{Chip, Layouter, Region},
    plonk::{Advice, Column, ConstraintSystem, Error, TableColumn},
    poly::Rotation,
};

use std::marker::PhantomData;

#[derive(Copy, Clone, Debug)]
pub  struct TableRow {
    pub x: u8,
    pub y: u8,
}

impl TableRow {
    pub fn new(x: u8, y: u8) -> Self {
        TableRow {
            x,
            y,
        }
    }
}

#[derive(Clone, Debug)]
pub struct InputRow {
    pub x: Option<u8>,
    pub y: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct Inputs {
    pub x: Column<Advice>,
    pub y: Column<Advice>,
}

#[derive(Clone, Debug)]
pub struct Table {
    pub x: TableColumn,
    pub y: TableColumn,
}

#[derive(Clone, Debug)]
pub struct TableConfig {
    pub input: Inputs,
    pub table: Table,
}

#[derive(Clone, Debug)]
pub struct TableChip<F: FieldExt> {
    config: TableConfig,
    _marker: PhantomData<F>,
}

impl<F: FieldExt> Chip<F> for TableChip<F> {
    type Config = TableConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}

impl<F: FieldExt> TableChip<F> {
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        input_x: Column<Advice>,
        input_y: Column<Advice>,
    ) -> <Self as Chip<F>>::Config {
        let table_x = meta.lookup_table_column();
        let table_y = meta.lookup_table_column();

        meta.lookup(|meta| {
            let x_cur = meta.query_advice(input_x, Rotation::cur());
            let y_cur = meta.query_advice(input_y, Rotation::cur());

            vec![
                (x_cur, table_x),
                (y_cur, table_y),
            ]
        });

        TableConfig {
            input: Inputs {
                x: input_x,
                y: input_y,
            },
            table: Table {
                x: table_x,
                y: table_y,
            },
        }
    }

    pub fn construct(config: TableConfig) -> Self {
        TableChip {
            config, 
            _marker: PhantomData
        }
    }

    pub fn load(
        config: TableConfig,
        layouter: &mut impl Layouter<F>,
        xs: &Vec<u8>,
        ys: &Vec<u8>
    ) -> Result<<Self as Chip<F>>::Loaded, Error> {
        let config = config.clone();
        layouter.assign_table(
            || "some public table table",
            |mut table| {
                let mut row_offset = 0;
                for (&x, &y) in xs.iter().zip(ys.iter()) {
                    table.assign_cell(
                        || format!("xor_l_col row {}", row_offset),
                        config.table.x,
                        row_offset,
                        || Ok(F::from(x as u64)),
                    )?;
                    table.assign_cell(
                        || format!("xor_r_col row {}", row_offset),
                        config.table.y,
                        row_offset,
                        || Ok(F::from(y as u64)),
                    )?;
                    row_offset += 1;
                }
                Ok(())
            },
        )
    }

    pub fn add_row(
        &self,
        region: &mut Region<'_, F>,
        row: usize,
        x: Option<u8>,
        y: Option<u8>,
    ) -> Result<(), Error> {
        let config = self.config();

        region.assign_advice(
            || format!("x: {}", row), 
            config.input.x, 
            row, 
            || { 
                x.map(|x| F::from(x as u64))
                .ok_or(Error::Synthesis)
            }
        )?;

        region.assign_advice(
            || format!("y: {}", row), 
            config.input.y, 
            row, 
            || { 
                y.map(|y| F::from(y as u64))
                .ok_or(Error::Synthesis)
            }
        )?;

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::{TableChip, TableConfig};

    use halo2::{
        circuit::{Layouter, SimpleFloorPlanner},
        dev::MockProver,
        pasta::Fp,
        plonk::{Circuit, ConstraintSystem, Error},
    };

    use pasta_curves::pallas;

    #[test]
    fn lookup_table() {
        #[derive(Clone, Debug, Default)]
        struct MyCircuit {
            // public inputs
            xs: Vec<u8>,
            ys: Vec<u8>,
        }

        impl Circuit<pallas::Base> for MyCircuit {
            type Config = TableConfig;
            type FloorPlanner = SimpleFloorPlanner;

            fn without_witnesses(&self) -> Self {
                Self::default()
            }

            fn configure(meta: &mut ConstraintSystem<pallas::Base>) -> Self::Config {
                let input_x = meta.advice_column();
                let input_y = meta.advice_column();

                TableChip::configure(meta, input_x, input_y)
            }

            fn synthesize(
                &self,
                config: Self::Config,
                mut layouter: impl Layouter<pallas::Base>,
            ) -> Result<(), Error> {


                TableChip::load(config.clone(), &mut layouter, &self.xs, &self.ys)?;

                let table_chip = TableChip::construct(config);

                layouter.assign_region(
                    || "compress",
                    |mut region| {
                        table_chip.add_row(
                            &mut region, 
                            0, 
                            Some(0), 
                            Some(1), 
                        )?;

                        table_chip.add_row(
                            &mut region, 
                            1, 
                            Some(1), 
                            Some(125), 
                        )?;

                        table_chip.add_row(
                            &mut region, 
                            2, 
                            Some(2), 
                            Some(126), 
                        )?;
                        Ok(())
                    },
                )?;

                assert_eq!(self.xs[1], 1);
                assert_eq!(self.ys[1], 125);

                // Some value dependent on row_1
                let _z = 1 + self.ys[2];

                Ok(())
            }
        }

        let circuit: MyCircuit = MyCircuit {
            xs: vec![0, 1, 2],
            ys: vec![1, 125, 126]
        };

        let prover = match MockProver::<Fp>::run(4, &circuit, vec![]) {
            Ok(prover) => prover,
            Err(e) => panic!("{:?}", e),
        };
        assert_eq!(prover.verify(), Ok(()));
    }
}
