#[cfg(test)]
#[path = "../../tests/unit/population/rosomaxa_test.rs"]
mod rosomaxa_test;

use super::*;
use crate::algorithms::gsom::*;
use crate::algorithms::math::relative_distance;
use crate::population::elitism::{DominanceOrdered, Shuffled};
use crate::utils::{Environment, Random};
use rand::prelude::SliceRandom;
use std::convert::TryInto;
use std::fmt::Formatter;
use std::ops::RangeBounds;
use std::sync::Arc;

/// Specifies rosomaxa configuration settings.
pub struct RosomaxaConfig {
    /// Selection size.
    pub selection_size: usize,
    /// Elite population size.
    pub elite_size: usize,
    /// Node population size.
    pub node_size: usize,
    /// Spread factor of GSOM.
    pub spread_factor: f64,
    /// Distribution factor of GSOM.
    pub distribution_factor: f64,
    /// Objective reshuffling probability.
    pub objective_reshuffling: f64,
    /// Learning rate of GSOM.
    pub learning_rate: f64,
    /// A node rebalance memory of GSOM.
    pub rebalance_memory: usize,
    /// A rebalance count.
    pub rebalance_count: usize,
    /// A ratio of exploration phase.
    pub exploration_ratio: f64,
}

impl RosomaxaConfig {
    /// Creates an instance of `RosomaxaConfig` using default parameters, but taking into
    /// account data parallelism settings.
    pub fn new_with_defaults(selection_size: usize) -> Self {
        Self {
            selection_size,
            elite_size: 2,
            node_size: 2,
            spread_factor: 0.25,
            distribution_factor: 0.25,
            objective_reshuffling: 0.01,
            learning_rate: 0.1,
            rebalance_memory: 100,
            rebalance_count: 2,
            exploration_ratio: 0.9,
        }
    }
}

/// Specifies behavior which returns a weights used to distinguish different solutions.
pub trait RosomaxaWeighted {
    /// Returns a weights used to distinguish different solutions.
    fn weights(&self) -> Vec<f64>;
}

/// Implements custom algorithm, code name Routing Optimizations with Self Organizing
/// MAps and eXtrAs (pronounced as "rosomaha", from russian "росомаха" - "wolverine").
pub struct Rosomaxa<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    objective: Arc<O>,
    environment: Arc<Environment>,
    config: RosomaxaConfig,
    elite: Elitism<O, S>,
    phase: RosomaxaPhases<O, S>,
}

impl<O, S> HeuristicPopulation for Rosomaxa<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    type Objective = O;
    type Individual = S;

    fn add_all(&mut self, individuals: Vec<Self::Individual>) -> bool {
        // NOTE avoid extra deep copy
        let best_known = self.elite.ranked().map(|(i, _)| i).next();
        let elite = individuals
            .iter()
            .filter(|individual| self.is_comparable_with_best_known(individual, best_known))
            .map(|individual| individual.deep_copy())
            .collect::<Vec<_>>();
        let is_improved = self.elite.add_all(elite);

        match &mut self.phase {
            RosomaxaPhases::Initial { solutions: known_individuals } => {
                known_individuals.extend(individuals.into_iter())
            }
            RosomaxaPhases::Exploration { network, statistics, .. } => {
                network.store_batch(individuals, statistics.generation, IndividualInput::new);
            }
            RosomaxaPhases::Exploitation { .. } => {}
        }

        is_improved
    }

    fn add(&mut self, individual: Self::Individual) -> bool {
        let best_known = self.elite.ranked().map(|(i, _)| i).next();
        let is_improved = if self.is_comparable_with_best_known(&individual, best_known) {
            self.elite.add(individual.deep_copy())
        } else {
            false
        };

        match &mut self.phase {
            RosomaxaPhases::Initial { solutions: individuals } => individuals.push(individual),
            RosomaxaPhases::Exploration { network, statistics, .. } => {
                network.store(IndividualInput::new(individual), statistics.generation)
            }
            RosomaxaPhases::Exploitation { .. } => {}
        }

        is_improved
    }

    fn on_generation(&mut self, statistics: &HeuristicStatistics) {
        self.update_phase(statistics)
    }

    fn cmp(&self, a: &Self::Individual, b: &Self::Individual) -> Ordering {
        self.elite.cmp(a, b)
    }

    fn select<'a>(&'a self) -> Box<dyn Iterator<Item = &Self::Individual> + 'a> {
        match &self.phase {
            RosomaxaPhases::Exploration { network, coordinates, selection_size, .. } => {
                let (elite_explore_size, node_explore_size) = match *selection_size {
                    value if value > 6 => {
                        let elite_size = self.environment.random.uniform_int(2, 4) as usize;
                        (elite_size, 2)
                    }
                    value if value > 4 => (2, 2),
                    value if value > 2 => (2, 1),
                    _ => (1, 1),
                };

                Box::new(
                    self.elite
                        .select()
                        .take(elite_explore_size)
                        .chain(coordinates.iter().flat_map(move |(coordinate, _, _)| {
                            let explore_size = self.environment.random.uniform_int(1, node_explore_size) as usize;

                            network
                                .find(coordinate)
                                .map(|node| {
                                    let node = node.read().unwrap();
                                    // NOTE this is black magic to trick borrow checker, it should be safe to do
                                    // TODO is there better way to achieve similar result?
                                    unsafe { &*(&node.storage.population as *const Elitism<O, S>) as &Elitism<O, S> }
                                        .select()
                                        .take(explore_size)
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_else(Vec::new)
                                .into_iter()
                        }))
                        .take(*selection_size),
                )
            }
            RosomaxaPhases::Exploitation { selection_size } => Box::new(self.elite.select().take(*selection_size)),
            _ => Box::new(self.elite.select()),
        }
    }

    fn ranked<'a>(&'a self) -> Box<dyn Iterator<Item = (&Self::Individual, usize)> + 'a> {
        self.elite.ranked()
    }

    fn size(&self) -> usize {
        self.elite.size()
    }

    fn selection_phase(&self) -> SelectionPhase {
        match &self.phase {
            RosomaxaPhases::Initial { .. } => SelectionPhase::Initial,
            RosomaxaPhases::Exploration { .. } => SelectionPhase::Exploration,
            RosomaxaPhases::Exploitation { .. } => SelectionPhase::Exploitation,
        }
    }
}

type IndividualNetwork<O, S> = Network<IndividualInput<S>, IndividualStorage<O, S>, IndividualStorageFactory<O, S>>;

impl<O, S> Rosomaxa<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    /// Creates a new instance of `Rosomaxa`.
    pub fn new(objective: Arc<O>, environment: Arc<Environment>, config: RosomaxaConfig) -> Result<Self, String> {
        if config.elite_size < 1 || config.node_size < 1 || config.selection_size < 2 {
            return Err("Rosomaxa algorithm requires some parameters to be above thresholds".to_string());
        }

        Ok(Self {
            objective: objective.clone(),
            environment: environment.clone(),
            elite: Elitism::new(objective, environment.random.clone(), config.elite_size, config.selection_size),
            phase: RosomaxaPhases::Initial { solutions: vec![] },
            config,
        })
    }

    fn update_phase(&mut self, statistics: &HeuristicStatistics) {
        let selection_size = match statistics.speed {
            HeuristicSpeed::Slow(ratio) => (self.config.selection_size as f64 * ratio).max(1.).round() as usize,
            HeuristicSpeed::Moderate => self.config.selection_size,
        };

        match &mut self.phase {
            RosomaxaPhases::Initial { solutions: individuals, .. } => {
                if individuals.len() >= 4 {
                    let mut network = Self::create_network(
                        self.objective.clone(),
                        self.environment.clone(),
                        &self.config,
                        individuals.drain(0..4).collect(),
                    );
                    individuals.drain(0..).for_each(|individual| network.store(IndividualInput::new(individual), 0));

                    self.phase = RosomaxaPhases::Exploration {
                        network,
                        coordinates: vec![],
                        statistics: statistics.clone(),
                        selection_size,
                    };
                }
            }
            RosomaxaPhases::Exploration {
                network,
                coordinates,
                statistics: old_statistics,
                selection_size: old_selection_size,
            } => {
                let exploration_ratio = match old_statistics.speed {
                    HeuristicSpeed::Slow(ratio) => self.config.exploration_ratio * ratio,
                    HeuristicSpeed::Moderate => self.config.exploration_ratio,
                };

                if statistics.termination_estimate < exploration_ratio {
                    *old_statistics = statistics.clone();
                    *old_selection_size = selection_size;

                    let best_individual = self.elite.select().next().expect("expected individuals in elite");
                    let best_fitness = best_individual.get_fitness().collect::<Vec<_>>();

                    Self::optimize_network(
                        network,
                        statistics,
                        best_fitness.as_slice(),
                        self.config.rebalance_memory,
                        self.config.rebalance_count,
                    );

                    Self::fill_populations(
                        network,
                        coordinates,
                        best_fitness.as_slice(),
                        statistics,
                        self.environment.random.as_ref(),
                    );
                } else {
                    self.phase = RosomaxaPhases::Exploitation { selection_size }
                }
            }
            RosomaxaPhases::Exploitation { selection_size: old_selection_size } => {
                *old_selection_size = selection_size;
            }
        }
    }

    fn is_comparable_with_best_known(&self, individual: &S, best_known: Option<&S>) -> bool {
        best_known.map_or(true, |best_known| self.objective.total_order(individual, best_known) != Ordering::Greater)
    }

    fn fill_populations<'a>(
        network: &'a IndividualNetwork<O, S>,
        coordinates: &mut Vec<(Coordinate, f64, usize)>,
        best_fitness: &[f64],
        statistics: &HeuristicStatistics,
        random: &(dyn Random + Send + Sync),
    ) {
        coordinates.clear();
        coordinates.extend(network.iter().filter_map(|(coordinate, node)| {
            let node = node.read().unwrap();
            let coordinate = node.storage.population.select().next().map(|individual| {
                (
                    coordinate.clone(),
                    relative_distance(best_fitness.iter().cloned(), individual.get_fitness()),
                    node.get_last_hits(network.get_current_time()),
                )
            });

            coordinate
        }));

        let shuffle_amount = Self::calculate_shuffle_amount(statistics, coordinates.len());
        if shuffle_amount != coordinates.len() {
            // partially randomize order
            if random.is_head_not_tails() {
                coordinates.sort_by(|(_, distance_a, _), (_, distance_b, _)| compare_floats(*distance_a, *distance_b));
            } else {
                coordinates.sort_by(|(_, _, last_hit_a), (_, _, last_hit_b)| last_hit_a.cmp(last_hit_b));
            }

            coordinates.partial_shuffle(&mut random.get_rng(), shuffle_amount);
        } else {
            coordinates.shuffle(&mut random.get_rng());
        }
    }

    fn calculate_shuffle_amount(statistics: &HeuristicStatistics, length: usize) -> usize {
        let ratio = match statistics.improvement_1000_ratio {
            v if v > 0.5 => {
                // https://www.wolframalpha.com/input/?i=plot+0.66+*+%281-+1%2F%281%2Be%5E%28-10+*%28x+-+0.5%29%29%29%29%2C+x%3D0+to+1
                let progress = statistics.termination_estimate;
                let ratio = 0.5 * (1. - 1. / (1. + std::f64::consts::E.powf(-10. * (progress - 0.5))));
                ratio.clamp(0.1, 0.5)
            }
            v if v > 0.2 => 0.5,
            _ => 1.,
        };

        (length as f64 * ratio).round() as usize
    }

    fn optimize_network(
        network: &mut IndividualNetwork<O, S>,
        statistics: &HeuristicStatistics,
        best_fitness: &[f64],
        rebalance_memory: usize,
        rebalance_count: usize,
    ) {
        let rebalance_memory = rebalance_memory as f64;
        let keep_size = match statistics.improvement_1000_ratio {
            v if v > 0.2 => {
                // https://www.wolframalpha.com/input/?i=plot+%281+-+1%2F%281%2Be%5E%28-10+*%28x+-+0.5%29%29%29%29%2C+x%3D0+to+1
                let x = statistics.termination_estimate.clamp(0., 1.);
                let ratio = 1. - 1. / (1. + std::f64::consts::E.powf(-10. * (x - 0.5)));
                rebalance_memory + rebalance_memory * ratio
            }
            v if v > 0.1 => 2. * rebalance_memory,
            v if v > 0.01 => 3. * rebalance_memory,
            _ => 4. * rebalance_memory,
        } as usize;

        if statistics.generation == 0 || network.size() <= keep_size {
            return;
        }

        let get_distance = |node: &NodeLink<IndividualInput<S>, IndividualStorage<O, S>>| {
            let node = node.read().unwrap();
            let individual = node.storage.population.select().next();

            individual.map(|individual| relative_distance(best_fitness.iter().cloned(), individual.get_fitness()))
        };

        // determine percentile value
        let mut distances = network.get_nodes().filter_map(get_distance).collect::<Vec<_>>();
        distances.sort_by(|a, b| compare_floats(*b, *a));
        let percentile_idx = if distances.len() > keep_size {
            distances.len() - keep_size
        } else {
            // NOTE remove 75% of nodes
            const PERCENTILE_THRESHOLD: f64 = 0.75;

            (distances.len() as f64 * PERCENTILE_THRESHOLD) as usize
        };

        if let Some(distance_threshold) = distances.get(percentile_idx).cloned() {
            network.retrain(rebalance_count, &|node| {
                get_distance(node).map_or(false, |distance| distance < distance_threshold)
            });
        }
    }

    fn create_network(
        objective: Arc<O>,
        environment: Arc<Environment>,
        config: &RosomaxaConfig,
        individuals: Vec<S>,
    ) -> IndividualNetwork<O, S> {
        let inputs_vec = individuals.into_iter().map(IndividualInput::new).collect::<Vec<_>>();

        let inputs_slice = inputs_vec.into_boxed_slice();
        let inputs_array: Box<[IndividualInput<S>; 4]> = match inputs_slice.try_into() {
            Ok(ba) => ba,
            Err(o) => panic!("expected individuals of length {} but it was {}", 4, o.len()),
        };

        let storage_factory = IndividualStorageFactory {
            node_size: config.node_size,
            reshuffling_probability: config.objective_reshuffling,
            random: environment.random.clone(),
            objective,
        };

        Network::new(
            *inputs_array,
            NetworkConfig {
                spread_factor: config.spread_factor,
                distribution_factor: config.distribution_factor,
                learning_rate: config.learning_rate,
                rebalance_memory: config.rebalance_memory,
                has_initial_error: true,
            },
            storage_factory,
        )
    }
}

impl<O, S> Display for Rosomaxa<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.phase {
            RosomaxaPhases::Exploration { network, .. } => {
                let state = get_network_state(network);
                write!(f, "{}", state)
            }
            _ => write!(f, "{}", self.elite),
        }
    }
}

#[allow(clippy::large_enum_variant)]
enum RosomaxaPhases<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    Initial {
        solutions: Vec<S>,
    },
    Exploration {
        network: IndividualNetwork<O, S>,
        coordinates: Vec<(Coordinate, f64, usize)>,
        statistics: HeuristicStatistics,
        selection_size: usize,
    },
    Exploitation {
        selection_size: usize,
    },
}

struct IndividualInput<S>
where
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    weights: Vec<f64>,
    individual: S,
}

impl<S> IndividualInput<S>
where
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    pub fn new(individual: S) -> Self {
        Self { weights: individual.weights(), individual }
    }
}

impl<S> Input for IndividualInput<S>
where
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    fn weights(&self) -> &[f64] {
        self.weights.as_slice()
    }
}

struct IndividualStorageFactory<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    node_size: usize,
    reshuffling_probability: f64,
    random: Arc<dyn Random + Send + Sync>,
    objective: Arc<O>,
}

impl<O, S> StorageFactory<IndividualInput<S>, IndividualStorage<O, S>> for IndividualStorageFactory<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    fn eval(&self) -> IndividualStorage<O, S> {
        let mut elitism = Elitism::new(self.objective.clone(), self.random.clone(), self.node_size, self.node_size);
        if self.random.is_hit(self.reshuffling_probability) {
            elitism.shuffle_objective();
        }
        IndividualStorage { population: elitism }
    }
}

struct IndividualStorage<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    population: Elitism<O, S>,
}

impl<O, S> Storage for IndividualStorage<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    type Item = IndividualInput<S>;

    fn add(&mut self, input: Self::Item) {
        self.population.add(input.individual);
    }

    fn drain<R>(&mut self, range: R) -> Vec<Self::Item>
    where
        R: RangeBounds<usize>,
    {
        self.population.drain(range).into_iter().map(IndividualInput::new).collect()
    }

    fn distance(&self, a: &[f64], b: &[f64]) -> f64 {
        relative_distance(a.iter().cloned(), b.iter().cloned())
    }

    fn size(&self) -> usize {
        self.population.size()
    }
}

impl<O, S> Display for IndividualStorage<O, S>
where
    O: HeuristicObjective<Solution = S> + Shuffled,
    S: HeuristicSolution + RosomaxaWeighted + DominanceOrdered,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.population)
    }
}
