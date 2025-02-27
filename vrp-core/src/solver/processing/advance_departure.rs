use super::*;
use crate::construction::constraints::TransportConstraintModule;
use crate::construction::heuristics::InsertionContext;
use rosomaxa::HeuristicSolution;

/// Provides way to reduce waiting time by advancing departure time.
#[derive(Default)]
pub struct AdvanceDeparture {}

impl HeuristicSolutionProcessing for AdvanceDeparture {
    type Solution = InsertionContext;

    fn post_process(&self, solution: Self::Solution) -> Self::Solution {
        let mut insertion_ctx = solution.deep_copy();

        let problem = insertion_ctx.problem.clone();

        let activity = problem.activity.as_ref();
        let transport = problem.transport.as_ref();

        insertion_ctx.solution.routes.iter_mut().for_each(|route_ctx| {
            TransportConstraintModule::advance_departure_time(route_ctx, activity, transport, true);
        });

        problem.constraint.accept_solution_state(&mut insertion_ctx.solution);

        insertion_ctx
    }
}
