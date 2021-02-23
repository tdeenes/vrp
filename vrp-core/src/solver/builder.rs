use crate::construction::heuristics::InsertionContext;
use crate::construction::Quota;
use crate::models::{Problem, Solution};
use crate::solver::evolution::EvolutionConfig;
use crate::solver::hyper::HyperHeuristic;
use crate::solver::mutation::*;
use crate::solver::population::Population;
use crate::solver::termination::*;
use crate::solver::{Solver, Telemetry};
use crate::utils::{Environment, TimeQuota};
use std::sync::Arc;

/// Provides configurable way to build Vehile Routing Problem [`Solver`] instance using fluent
/// interface style.
///
/// A newly created builder instance is pre-configured with some reasonable defaults for mid-size
/// problems (~200), so there is no need to call any of its methods.
///
/// [`Solver`]: ./struct.Solver.html
///
/// # Examples
///
/// This example shows how to override some of default metaheuristic parameters using fluent
/// interface methods:
///
/// ```
/// # use vrp_core::models::examples::create_example_problem;
/// # use std::sync::Arc;
/// use vrp_core::solver::Builder;
/// use vrp_core::models::Problem;
/// use vrp_core::utils::Environment;
///
/// // create your VRP problem
/// let problem: Arc<Problem> = create_example_problem();
/// let environment = Arc::new(Environment::default());
/// // build solver using builder with overridden parameters
/// let solver = Builder::new(problem, environment)
///     .with_max_time(Some(60))
///     .with_max_generations(Some(100))
///     .build()?;
/// // run solver and get the best known solution within its cost.
/// let (solution, cost, _) = solver.solve()?;
///
/// assert_eq!(cost, 42.);
/// assert_eq!(solution.routes.len(), 1);
/// assert_eq!(solution.unassigned.len(), 0);
/// # Ok::<(), String>(())
/// ```
pub struct Builder {
    /// A max amount generations in evolution.
    pub max_generations: Option<usize>,

    /// A max seconds to run evolution.
    pub max_time: Option<usize>,

    /// A cost variation parameters for termination criteria.
    pub cost_variation: Option<(usize, f64)>,

    /// An evolution configuration..
    pub config: EvolutionConfig,
}

impl Builder {
    /// Creates a new instance of `Builder`.
    pub fn new(problem: Arc<Problem>, environment: Arc<Environment>) -> Self {
        Self {
            max_generations: None,
            max_time: None,
            cost_variation: None,
            config: EvolutionConfig::new(problem, environment),
        }
    }
}

impl Builder {
    /// Sets telemetry. Default telemetry is set to do nothing.
    pub fn with_telemetry(mut self, telemetry: Telemetry) -> Self {
        self.config.telemetry = telemetry;
        self
    }

    /// Sets max generations to be run by evolution. Default is 3000.
    pub fn with_max_generations(mut self, limit: Option<usize>) -> Self {
        self.max_generations = limit;
        self
    }

    /// Sets cost variation termination criteria. Default is None.
    pub fn with_cost_variation(mut self, variation: Option<(usize, f64)>) -> Self {
        self.cost_variation = variation;
        self
    }

    /// Sets max running time limit for evolution. Default is 300 seconds.
    pub fn with_max_time(mut self, limit: Option<usize>) -> Self {
        self.max_time = limit;
        self
    }

    /// Sets initial parameters used to construct initial population.
    pub fn with_init_params(
        mut self,
        size: Option<usize>,
        initial_methods: Option<Vec<(Arc<dyn Recreate + Send + Sync>, usize)>>,
    ) -> Self {
        self.config.telemetry.log("configured to use custom initial population parameters");

        if let Some(size) = size {
            self.config.population.initial.size = size;
        }

        if let Some(initial_methods) = initial_methods {
            self.config.population.initial.methods = initial_methods;
        }

        self
    }

    /// Sets initial solutions in population. Default is no solutions in population.
    pub fn with_init_solutions(mut self, solutions: Vec<Solution>) -> Self {
        self.config.telemetry.log(format!("provided {} initial solutions to start with", solutions.len()).as_str());
        self.config.population.initial.individuals = solutions
            .into_iter()
            .map(|solution| {
                InsertionContext::new_from_solution(
                    self.config.problem.clone(),
                    (solution, None),
                    self.config.environment.clone(),
                )
            })
            .collect();
        self
    }

    /// Sets population algorithm. Default is rosomaxa.
    pub fn with_population(mut self, population: Box<dyn Population + Send + Sync>) -> Self {
        self.config.telemetry.log("configured to use custom population");
        self.config.population.variation = Some(population);
        self
    }

    /// Sets hyper heuristic algorithm. Default is simple selective.
    pub fn with_hyper(mut self, hyper: Box<dyn HyperHeuristic + Send + Sync>) -> Self {
        self.config.telemetry.log("configured to use custom hyper-heuristic");
        self.config.hyper = hyper;
        self
    }

    /// Sets termination algorithm. Default is max time and max generations.
    pub fn with_termination(mut self, termination: Arc<dyn Termination + Send + Sync>) -> Self {
        self.config.telemetry.log("configured to use custom termination parameters");
        self.config.termination = termination;
        self
    }

    /// Builds [`Solver`](./struct.Solver.html) instance.
    pub fn build(self) -> Result<Solver, String> {
        let problem = self.config.problem.clone();

        let (criterias, quota): (Vec<Box<dyn Termination + Send + Sync>>, _) =
            match (self.max_generations, self.max_time, self.cost_variation) {
                (None, None, None) => {
                    self.config
                        .telemetry
                        .log("configured to use default max-generations (3000) and max-time (300secs)");
                    (vec![Box::new(MaxGeneration::new(3000)), Box::new(MaxTime::new(300.))], None)
                }
                _ => {
                    let mut criterias: Vec<Box<dyn Termination + Send + Sync>> = vec![];

                    if let Some(limit) = self.max_generations {
                        self.config.telemetry.log(format!("configured to use max-generations: {}", limit).as_str());
                        criterias.push(Box::new(MaxGeneration::new(limit)))
                    }

                    let quota = if let Some(limit) = self.max_time {
                        self.config.telemetry.log(format!("configured to use max-time: {}s", limit).as_str());
                        criterias.push(Box::new(MaxTime::new(limit as f64)));
                        Some(create_time_quota(limit))
                    } else {
                        None
                    };

                    if let Some((sample, threshold)) = self.cost_variation {
                        self.config.telemetry.log(
                            format!(
                                "configured to use cost variation with sample: {}, threshold: {}",
                                sample, threshold
                            )
                            .as_str(),
                        );
                        criterias.push(Box::new(CostVariation::new(sample, threshold)))
                    }

                    (criterias, quota)
                }
            };

        let mut config = self.config;
        config.termination = Arc::new(CompositeTermination::new(criterias));
        config.quota = quota;

        Ok(Solver { problem, config })
    }
}

fn create_time_quota(limit: usize) -> Arc<dyn Quota + Sync + Send> {
    Arc::new(TimeQuota::new(limit as f64))
}
