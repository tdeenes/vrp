use crate::construction::heuristics::InsertionContext;
use crate::construction::Quota;
use crate::models::common::SingleDimLoad;
use crate::models::Problem;
use crate::solver::evolution::{EvolutionStrategy, RunSimple};
use crate::solver::hyper::{HyperHeuristic, MultiSelective};
use crate::solver::mutation::*;
use crate::solver::population::*;
use crate::solver::processing::*;
use crate::solver::telemetry::Telemetry;
use crate::solver::termination::*;
use crate::solver::TelemetryMode;
use crate::utils::Environment;
use std::sync::Arc;

/// A configuration which controls evolution execution.
pub struct EvolutionConfig {
    /// An original problem.
    pub problem: Arc<Problem>,

    /// A processing configuration.
    pub processing: Option<Arc<dyn Processing + Send + Sync>>,

    /// A population configuration.
    pub population: PopulationConfig,

    /// A hyper heuristic.
    pub hyper: Box<dyn HyperHeuristic + Send + Sync>,

    /// A termination defines when evolution should stop.
    pub termination: Arc<dyn Termination + Send + Sync>,

    /// An evolution strategy.
    pub strategy: Arc<dyn EvolutionStrategy + Send + Sync>,

    /// A quota for evolution execution.
    pub quota: Option<Arc<dyn Quota + Send + Sync>>,

    /// An environmental context.
    pub environment: Arc<Environment>,

    /// A telemetry to be used.
    pub telemetry: Telemetry,
}

/// Contains population specific properties.
pub struct PopulationConfig {
    /// An initial solution config.
    pub initial: InitialConfig,

    /// Population algorithm variation.
    pub variation: Option<Box<dyn Population + Send + Sync>>,
}

/// An initial solutions configuration.
pub struct InitialConfig {
    /// Create methods to produce initial individuals.
    pub methods: Vec<(Arc<dyn Recreate + Send + Sync>, usize)>,
    /// Initial size of population to be generated.
    pub max_size: usize,
    /// Quota for initial solution generation.
    pub quota: f64,
    /// Initial individuals in population.
    pub individuals: Vec<InsertionContext>,
}

impl EvolutionConfig {
    /// Creates a new instance of `EvolutionConfig` using default settings.
    pub fn new(problem: Arc<Problem>, environment: Arc<Environment>) -> Self {
        Self {
            problem: problem.clone(),
            processing: Some(Arc::new(CompositeProcessing::new(vec![
                Arc::new(AdvanceDeparture::default()),
                Arc::new(UnassignmentReason::default()),
            ]))),
            population: PopulationConfig {
                initial: InitialConfig {
                    max_size: 4,
                    quota: 0.05,
                    methods: vec![
                        (Arc::new(RecreateWithCheapest::default()), 1),
                        (Arc::new(RecreateWithFarthest::default()), 1),
                        (Arc::new(RecreateWithNearestNeighbor::default()), 1),
                        (Arc::new(RecreateWithGaps::new(1, (problem.jobs.size() / 10).max(1))), 1),
                        (Arc::new(RecreateWithSkipBest::new(1, 2)), 1),
                        (Arc::new(RecreateWithRegret::new(2, 3)), 1),
                        (
                            Arc::new(RecreateWithBlinks::<SingleDimLoad>::new_with_defaults(
                                environment.random.clone(),
                            )),
                            1,
                        ),
                        (Arc::new(RecreateWithPerturbation::new_with_defaults(environment.random.clone())), 1),
                    ],
                    individuals: vec![],
                },
                variation: Some(get_default_population(problem.objective.clone(), environment.clone())),
            },
            hyper: Box::new(MultiSelective::new_with_defaults(problem, environment.clone())),
            termination: Arc::new(CompositeTermination::new(vec![
                Box::new(MaxTime::new(300.)),
                Box::new(MaxGeneration::new(3000)),
            ])),
            strategy: Arc::new(RunSimple::default()),
            quota: None,
            telemetry: Telemetry::new(TelemetryMode::None),
            environment,
        }
    }
}
