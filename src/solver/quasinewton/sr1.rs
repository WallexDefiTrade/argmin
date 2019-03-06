// Copyright 2018 Stefan Kroboth
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! # References:
//!
//! [0] Jorge Nocedal and Stephen J. Wright (2006). Numerical Optimization.
//! Springer. ISBN 0-387-30303-0.

use crate::prelude::*;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// SR1 method
///
/// # Example
///
/// ```rust
/// # extern crate argmin;
/// # extern crate ndarray;
/// use argmin::prelude::*;
/// use argmin::solver::quasinewton::SR1;
/// use argmin::solver::linesearch::MoreThuenteLineSearch;
/// # use argmin::testfunctions::{rosenbrock_2d, rosenbrock_2d_derivative};
/// use ndarray::{array, Array1, Array2};
/// # use serde::{Deserialize, Serialize};
///
/// # #[derive(Clone, Default, Serialize, Deserialize)]
/// # struct MyProblem { }
/// #
/// #  impl ArgminOp for MyProblem {
/// #      type Param = Array1<f64>;
/// #      type Output = f64;
/// #      type Hessian = Array2<f64>;
/// #
/// #      fn apply(&self, p: &Self::Param) -> Result<Self::Output, Error> {
/// #          Ok(rosenbrock_2d(&p.to_vec(), 1.0, 100.0))
/// #      }
/// #
/// #      fn gradient(&self, p: &Self::Param) -> Result<Self::Param, Error> {
/// #          Ok(Array1::from_vec(rosenbrock_2d_derivative(
/// #              &p.to_vec(),
/// #              1.0,
/// #              100.0,
/// #          )))
/// #      }
/// #  }
/// #
/// #  fn run() -> Result<(), Error> {
/// // Define cost function
/// let cost = MyProblem {};
///
/// // Define initial parameter vector
/// // let init_param: Array1<f64> = Array1::from_vec(vec![1.2, 1.2]);
/// let init_param: Array1<f64> = array![-1.2, 1.0];
/// let init_hessian: Array2<f64> = Array2::eye(2);
///
/// // set up a line search
/// let linesearch = MoreThuenteLineSearch::new(cost.clone());
///
/// // Set up solver
/// let mut solver = SR1::new(cost, init_param, init_hessian, linesearch);
///
/// // Set maximum number of iterations
/// solver.set_max_iters(80);
///
/// // Attach a logger
/// solver.add_logger(ArgminSlogLogger::term());
///
/// // Run solver
/// solver.run()?;
///
/// // Wait a second (lets the logger flush everything before printing again)
/// std::thread::sleep(std::time::Duration::from_secs(1));
///
/// // Print result
/// println!("{:?}", solver.result());
/// # Ok(())
/// # }
/// #
/// # fn main() {
/// #     if let Err(ref e) = run() {
/// #         println!("{} {}", e.as_fail(), e.backtrace());
/// #         std::process::exit(1);
/// #     }
/// # }
/// ```
///
/// # References:
///
/// [0] Jorge Nocedal and Stephen J. Wright (2006). Numerical Optimization.
/// Springer. ISBN 0-387-30303-0.
#[derive(Serialize, Deserialize)]
pub struct SR1<L, H> {
    /// Inverse Hessian
    inv_hessian: H,
    /// line search
    linesearch: L,
}

impl<L, H> SR1<L, H> {
    /// Constructor
    pub fn new(init_inverse_hessian: H, linesearch: L) -> Self {
        SR1 {
            inv_hessian: init_inverse_hessian,
            linesearch: linesearch,
        }
    }
}

impl<O, L, H> Solver<O> for SR1<L, H>
where
    O: ArgminOp<Output = f64, Hessian = H>,
    O::Param: Debug
        + Clone
        + Default
        + Serialize
        + ArgminSub<O::Param, O::Param>
        + ArgminDot<O::Param, f64>
        + ArgminDot<O::Param, O::Hessian>
        + ArgminScaledAdd<O::Param, f64, O::Param>
        + ArgminNorm<f64>
        + ArgminMul<f64, O::Param>,
    O::Hessian: Debug
        + Clone
        + Default
        + Serialize
        + ArgminSub<O::Hessian, O::Hessian>
        + ArgminDot<O::Param, O::Param>
        + ArgminDot<O::Hessian, O::Hessian>
        + ArgminAdd<O::Hessian, O::Hessian>
        + ArgminMul<f64, O::Hessian>
        + ArgminTranspose
        + ArgminEye,
    L: Clone + ArgminLineSearch<O::Param> + Solver<OpWrapper<O>>,
{
    fn init(
        &mut self,
        op: &mut OpWrapper<O>,
        state: IterState<O::Param, O::Hessian>,
    ) -> Result<Option<ArgminIterData<O>>, Error> {
        let cost = op.apply(&state.cur_param)?;
        let grad = op.gradient(&state.cur_param)?;
        Ok(Some(
            ArgminIterData::new()
                .param(state.cur_param)
                .cost(cost)
                .grad(grad),
        ))
    }

    fn next_iter(
        &mut self,
        op: &mut OpWrapper<O>,
        state: IterState<O::Param, O::Hessian>,
    ) -> Result<ArgminIterData<O>, Error> {
        let prev_grad = state.cur_grad;
        let p = self.inv_hessian.dot(&prev_grad).mul(&(-1.0));

        self.linesearch.set_init_param(state.cur_param.clone());
        self.linesearch.set_init_grad(prev_grad.clone());
        self.linesearch.set_init_cost(state.cur_cost);
        // self.linesearch
        //     .set_search_direction(p.mul(&(1.0 / p.norm())));
        self.linesearch.set_search_direction(p);

        // Run solver
        let linesearch_result =
            Executor::new(op.clone(), self.linesearch.clone(), state.cur_param.clone())
                .run_fast()?;

        let xk1 = linesearch_result.param;

        let grad = op.gradient(&xk1)?;
        let yk = grad.sub(&prev_grad);

        let sk = xk1.sub(&state.cur_param);

        let skmhkyk = sk.sub(&self.inv_hessian.dot(&yk));
        let a: O::Hessian = skmhkyk.dot(&skmhkyk);
        let b: f64 = skmhkyk.dot(&yk);

        let sk_norm: f64 = sk.dot(&sk);
        let skmhkyk_norm: f64 = skmhkyk.dot(&skmhkyk);
        if b.abs() >= 10e-8 * sk_norm * skmhkyk_norm {
            self.inv_hessian = self.inv_hessian.add(&a.mul(&(1.0 / b)));
        }

        Ok(ArgminIterData::new()
            .param(xk1)
            .cost(linesearch_result.cost)
            .grad(grad))
    }

    fn terminate(&mut self, state: &IterState<O::Param, O::Hessian>) -> TerminationReason {
        if state.cur_grad.norm() < std::f64::EPSILON.sqrt() {
            return TerminationReason::TargetPrecisionReached;
        }
        if (state.prev_cost - state.cur_cost).abs() < std::f64::EPSILON {
            return TerminationReason::NoChangeInCost;
        }
        TerminationReason::NotTerminated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::send_sync_test;
    use crate::solver::linesearch::MoreThuenteLineSearch;

    type Operator = MinimalNoOperator;

    send_sync_test!(sr1, SR1<Operator, MoreThuenteLineSearch<Operator>>);
}
