#[cfg(test)]
#[path = "../../../tests/unit/format/solution/writer_test.rs"]
mod writer_test;

use crate::format::coord_index::CoordIndex;
use crate::format::solution::activity_matcher::get_job_tag;
use crate::format::solution::model::Timing;
use crate::format::solution::*;
use crate::format::*;
use crate::format_time;
use std::io::{BufWriter, Write};
use vrp_core::construction::constraints::route_intervals;
use vrp_core::models::common::*;
use vrp_core::models::problem::Multi;
use vrp_core::models::solution::{Activity, Route};
use vrp_core::models::{Problem, Solution};
use vrp_core::solver::Metrics;

type ApiActivity = crate::format::solution::model::Activity;
type ApiSolution = crate::format::solution::model::Solution;
type ApiSchedule = crate::format::solution::model::Schedule;
type ApiMetrics = crate::format::solution::model::Metrics;
type ApiGeneration = crate::format::solution::model::Generation;
type AppPopulation = crate::format::solution::model::Population;
type ApiIndividual = crate::format::solution::model::Individual;
type DomainSchedule = vrp_core::models::common::Schedule;
type DomainLocation = vrp_core::models::common::Location;
type DomainExtras = vrp_core::models::Extras;
type DomainCommute = vrp_core::models::solution::Commute;

/// A trait to serialize solution in pragmatic format.
pub trait PragmaticSolution<W: Write> {
    /// Serializes solution in pragmatic json format.
    fn write_pragmatic_json(&self, problem: &Problem, writer: BufWriter<W>) -> Result<(), String>;

    /// Serializes solution in pragmatic geo json format.
    fn write_geo_json(&self, problem: &Problem, writer: BufWriter<W>) -> Result<(), String>;
}

impl<W: Write> PragmaticSolution<W> for (&Solution, f64) {
    fn write_pragmatic_json(&self, problem: &Problem, writer: BufWriter<W>) -> Result<(), String> {
        write_pragmatic_json(problem, self.0, None, writer)
    }

    fn write_geo_json(&self, problem: &Problem, writer: BufWriter<W>) -> Result<(), String> {
        write_geo_json(problem, self.0, writer)
    }
}

impl<W: Write> PragmaticSolution<W> for (&Solution, f64, &Metrics) {
    fn write_pragmatic_json(&self, problem: &Problem, writer: BufWriter<W>) -> Result<(), String> {
        write_pragmatic_json(problem, self.0, Some(self.2), writer)
    }

    fn write_geo_json(&self, problem: &Problem, writer: BufWriter<W>) -> Result<(), String> {
        write_geo_json(problem, self.0, writer)
    }
}

fn write_pragmatic_json<W: Write>(
    problem: &Problem,
    solution: &Solution,
    metrics: Option<&Metrics>,
    writer: BufWriter<W>,
) -> Result<(), String> {
    let solution = create_solution(problem, solution, metrics);
    serialize_solution(writer, &solution).map_err(|err| err.to_string())?;
    Ok(())
}

fn write_geo_json<W: Write>(problem: &Problem, solution: &Solution, writer: BufWriter<W>) -> Result<(), String> {
    let solution = create_solution(problem, solution, None);
    serialize_solution_as_geojson(writer, problem, &solution).map_err(|err| err.to_string())?;
    Ok(())
}

struct Leg {
    pub last_detail: Option<(DomainLocation, Timestamp)>,
    pub load: Option<MultiDimLoad>,
    pub statistic: Statistic,
}

impl Leg {
    fn new(last_detail: Option<(DomainLocation, Timestamp)>, load: Option<MultiDimLoad>, statistic: Statistic) -> Self {
        Self { last_detail, load, statistic }
    }

    fn empty() -> Self {
        Self { last_detail: None, load: None, statistic: Statistic::default() }
    }
}

/// Creates solution.
pub fn create_solution(problem: &Problem, solution: &Solution, metrics: Option<&Metrics>) -> ApiSolution {
    let coord_index = get_coord_index(problem);

    let tours = solution.routes.iter().map(|r| create_tour(problem, r, coord_index)).collect::<Vec<Tour>>();

    let statistic = tours.iter().fold(Statistic::default(), |acc, tour| acc + tour.statistic.clone());

    let unassigned = create_unassigned(solution);
    let violations = create_violations(solution);

    let extras = create_extras(solution, metrics);

    ApiSolution { statistic, tours, unassigned, violations, extras }
}

fn create_tour(problem: &Problem, route: &Route, coord_index: &CoordIndex) -> Tour {
    let is_multi_dimen = has_multi_dimensional_capacity(problem.extras.as_ref());

    let actor = route.actor.as_ref();
    let vehicle = actor.vehicle.as_ref();
    let profile = &vehicle.profile;
    let transport = problem.transport.as_ref();

    let mut tour = Tour {
        vehicle_id: vehicle.dimens.get_id().unwrap().clone(),
        type_id: vehicle.dimens.get_value::<String>("type_id").unwrap().to_string(),
        shift_index: *vehicle.dimens.get_value::<usize>("shift_index").unwrap(),
        stops: vec![],
        statistic: Statistic::default(),
    };

    let intervals = route_intervals(route, Box::new(|a| get_activity_type(a).map_or(false, |t| t == "reload")));

    let mut leg = intervals.into_iter().fold(Leg::empty(), |leg, (start_idx, end_idx)| {
        let (start_delivery, end_pickup) = route.tour.activities_slice(start_idx, end_idx).iter().fold(
            (leg.load.unwrap_or_else(MultiDimLoad::default), MultiDimLoad::default()),
            |acc, activity| {
                let (delivery, pickup) = activity
                    .job
                    .as_ref()
                    .and_then(|job| get_capacity(&job.dimens, is_multi_dimen).map(|d| (d.delivery.0, d.pickup.0)))
                    .unwrap_or((MultiDimLoad::default(), MultiDimLoad::default()));
                (acc.0 + delivery, acc.1 + pickup)
            },
        );

        let (start_idx, start) = if start_idx == 0 {
            let start = route.tour.start().unwrap();
            let (has_dispatch, is_same_location) = route.tour.get(1).map_or((false, false), |activity| {
                let has_dispatch = activity
                    .retrieve_job()
                    .and_then(|job| job.dimens().get_value::<String>("type").cloned())
                    .map_or(false, |job_type| job_type == "dispatch");

                let is_same_location = start.place.location == activity.place.location;

                (has_dispatch, is_same_location)
            });

            tour.stops.push(Stop {
                location: coord_index.get_by_idx(start.place.location).unwrap(),
                time: format_schedule(&start.schedule),
                load: if has_dispatch { vec![0] } else { start_delivery.as_vec() },
                distance: 0,
                activities: vec![ApiActivity {
                    job_id: "departure".to_string(),
                    activity_type: "departure".to_string(),
                    location: None,
                    time: if is_same_location {
                        Some(Interval {
                            start: format_time(start.schedule.arrival),
                            end: format_time(start.schedule.departure),
                        })
                    } else {
                        None
                    },
                    job_tag: None,
                    commute: None,
                }],
            });
            (start_idx + 1, start)
        } else {
            (start_idx, route.tour.get(start_idx - 1).unwrap())
        };

        let mut leg = route.tour.activities_slice(start_idx, end_idx).iter().fold(
            Leg::new(Some((start.place.location, start.schedule.departure)), Some(start_delivery), leg.statistic),
            |leg, act| {
                let activity_type = get_activity_type(act).cloned();
                let (prev_location, prev_departure) = leg.last_detail.unwrap();
                let prev_load = if activity_type.is_some() {
                    leg.load.unwrap()
                } else {
                    // NOTE arrival must have zero load
                    let dimen_size = leg.load.unwrap().size;
                    MultiDimLoad::new(vec![0; dimen_size])
                };

                let activity_type = activity_type.unwrap_or_else(|| "arrival".to_string());
                let is_break = activity_type == "break";

                let job_tag = act.job.as_ref().and_then(|single| {
                    get_job_tag(single, (act.place.location, (act.place.time.clone(), start.schedule.departure)))
                        .cloned()
                });
                let job_id = match activity_type.as_str() {
                    "pickup" | "delivery" | "replacement" | "service" => {
                        let single = act.job.as_ref().unwrap();
                        let id = single.dimens.get_id().cloned();
                        id.unwrap_or_else(|| Multi::roots(single).unwrap().dimens.get_id().unwrap().clone())
                    }
                    _ => activity_type.clone(),
                };

                let commute = act.commute.clone().unwrap_or(DomainCommute { forward: (0., 0.), backward: (0., 0.) });
                let driving = if commute.is_zero_time() {
                    transport.duration(profile, prev_location, act.place.location, prev_departure)
                } else {
                    // NOTE: no need to drive in case of non-zero commute, this goes to commuting time
                    0.
                };

                let activity_arrival = act.schedule.arrival + commute.forward.1;
                let service_start = activity_arrival.max(act.place.time.start);
                let waiting = service_start - activity_arrival;
                let serving = act.place.duration;
                let service_end = service_start + serving;
                let commuting = commute.forward.1 + commute.backward.1;
                let activity_departure = act.schedule.departure - commute.backward.1;

                debug_assert_eq!(service_end, activity_departure);

                // NOTE: use original cost traits to adapt time-based costs (except waiting/commuting)
                // TODO: add better support of time based activity costs
                let serving_cost = problem.activity.cost(actor, act, service_start);
                let transport_cost = if commute.is_zero_time() {
                    transport.cost(actor, prev_location, act.place.location, prev_departure)
                } else {
                    // no real driving costs in case of commute
                    commuting * vehicle.costs.per_service_time
                };
                let cost = serving_cost + transport_cost + waiting * vehicle.costs.per_waiting_time;

                let location_distance =
                    transport.distance(profile, prev_location, act.place.location, prev_departure) as i64;
                let distance = leg.statistic.distance + location_distance - commute.forward.0 as i64;

                let is_new_stop = match (act.commute.as_ref(), prev_location == act.place.location) {
                    (Some(commute), false) if commute.is_zero_time() => true,
                    (Some(_), _) => false,
                    (None, is_same_location) => !is_same_location,
                };

                if is_new_stop {
                    tour.stops.push(Stop {
                        location: coord_index.get_by_idx(act.place.location).unwrap(),
                        time: format_schedule(&act.schedule),
                        load: prev_load.as_vec(),
                        distance,
                        activities: vec![],
                    });
                }

                let load = calculate_load(prev_load, act, is_multi_dimen);

                let last = tour.stops.len() - 1;
                let mut last = tour.stops.get_mut(last).unwrap();

                last.time.departure = format_time(act.schedule.departure);
                last.load = load.as_vec();
                last.activities.push(ApiActivity {
                    job_id,
                    activity_type: activity_type.clone(),
                    location: if !is_new_stop && activity_type == "dispatch" {
                        None
                    } else {
                        Some(coord_index.get_by_idx(act.place.location).unwrap())
                    },
                    time: Some(Interval { start: format_time(activity_arrival), end: format_time(activity_departure) }),
                    job_tag,
                    commute: act
                        .commute
                        .as_ref()
                        .map(|commute| Commute::new(commute, act.schedule.arrival, activity_departure)),
                });

                // NOTE detect when vehicle returns after activity to stop point
                let end_location = if commute.backward.1 > 0. {
                    tour.stops
                        .last()
                        .and_then(|stop| coord_index.get_by_loc(&stop.location))
                        .expect("expect to have at least one stop")
                } else {
                    act.place.location
                };

                Leg {
                    last_detail: Some((end_location, act.schedule.departure)),
                    statistic: Statistic {
                        cost: leg.statistic.cost + cost,
                        distance,
                        duration: leg.statistic.duration + act.schedule.departure as i64 - prev_departure as i64,
                        times: Timing {
                            driving: leg.statistic.times.driving + driving as i64,
                            serving: leg.statistic.times.serving + (if is_break { 0 } else { serving as i64 }),
                            waiting: leg.statistic.times.waiting + waiting as i64,
                            break_time: leg.statistic.times.break_time + (if is_break { serving as i64 } else { 0 }),
                            commuting: leg.statistic.times.commuting + commuting as i64,
                        },
                    },
                    load: Some(load),
                }
            },
        );

        leg.load = Some(leg.load.unwrap() - end_pickup);

        leg
    });

    // NOTE remove redundant info
    tour.stops
        .iter_mut()
        .filter(|stop| stop.activities.len() == 1)
        .flat_map(|stop| stop.activities.iter_mut())
        .for_each(|activity| {
            activity.location = None;
            activity.time = None;
        });

    leg.statistic.cost += vehicle.costs.fixed;

    tour.vehicle_id = vehicle.dimens.get_id().unwrap().clone();
    tour.type_id = vehicle.dimens.get_value::<String>("type_id").unwrap().clone();
    tour.statistic = leg.statistic;

    tour
}

fn format_schedule(schedule: &DomainSchedule) -> ApiSchedule {
    ApiSchedule { arrival: format_time(schedule.arrival), departure: format_time(schedule.departure) }
}

fn calculate_load(current: MultiDimLoad, act: &Activity, is_multi_dimen: bool) -> MultiDimLoad {
    let job = act.job.as_ref();
    let demand = job.and_then(|job| get_capacity(&job.dimens, is_multi_dimen)).unwrap_or_default();
    current - demand.delivery.0 - demand.delivery.1 + demand.pickup.0 + demand.pickup.1
}

fn create_unassigned(solution: &Solution) -> Option<Vec<UnassignedJob>> {
    let unassigned = solution
        .unassigned
        .iter()
        .filter(|(job, _)| job.dimens().get_value::<String>("vehicle_id").is_none())
        .map(|(job, code)| {
            let (code, reason) = map_code_reason(*code);
            UnassignedJob {
                job_id: job.dimens().get_id().expect("job id expected").clone(),
                reasons: vec![UnassignedJobReason { code: code.to_string(), description: reason.to_string() }],
            }
        })
        .collect::<Vec<_>>();

    if unassigned.is_empty() {
        None
    } else {
        Some(unassigned)
    }
}

fn create_violations(solution: &Solution) -> Option<Vec<Violation>> {
    // NOTE at the moment only break violation is mapped
    let violations = solution
        .unassigned
        .iter()
        .filter(|(job, _)| job.dimens().get_value::<String>("type").map_or(false, |t| t == "break"))
        .map(|(job, _)| Violation::Break {
            vehicle_id: job.dimens().get_value::<String>("vehicle_id").expect("vehicle id").clone(),
            shift_index: *job.dimens().get_value::<usize>("shift_index").expect("shift index"),
        })
        .collect::<Vec<_>>();

    if violations.is_empty() {
        None
    } else {
        Some(violations)
    }
}

fn get_activity_type(activity: &Activity) -> Option<&String> {
    activity.job.as_ref().and_then(|single| single.dimens.get_value::<String>("type"))
}

fn get_capacity(dimens: &Dimensions, is_multi_dimen: bool) -> Option<Demand<MultiDimLoad>> {
    if is_multi_dimen {
        dimens.get_demand().cloned()
    } else {
        let create_capacity = |capacity: SingleDimLoad| {
            if capacity.value == 0 {
                MultiDimLoad::default()
            } else {
                MultiDimLoad::new(vec![capacity.value])
            }
        };
        dimens.get_demand().map(|demand: &Demand<SingleDimLoad>| Demand {
            pickup: (create_capacity(demand.pickup.0), create_capacity(demand.pickup.1)),
            delivery: (create_capacity(demand.delivery.0), create_capacity(demand.delivery.1)),
        })
    }
}

fn has_multi_dimensional_capacity(extras: &DomainExtras) -> bool {
    let capacity_type = extras
        .get("capacity_type")
        .and_then(|s| s.downcast_ref::<String>())
        .unwrap_or_else(|| panic!("Cannot get capacity type!"));
    match capacity_type.as_str() {
        "multi" => true,
        "single" => false,
        _ => panic!("Unknown capacity type: '{}'", capacity_type),
    }
}

fn create_extras(_solution: &Solution, metrics: Option<&Metrics>) -> Option<Extras> {
    metrics.map(|metrics| Extras {
        metrics: Some(ApiMetrics {
            duration: metrics.duration,
            generations: metrics.generations,
            speed: metrics.speed,
            evolution: metrics
                .evolution
                .iter()
                .map(|g| ApiGeneration {
                    number: g.number,
                    timestamp: g.timestamp,
                    i_all_ratio: g.i_all_ratio,
                    i_1000_ratio: g.i_1000_ratio,
                    is_improvement: g.is_improvement,
                    population: AppPopulation {
                        individuals: g
                            .population
                            .individuals
                            .iter()
                            .map(|i| ApiIndividual {
                                tours: i.tours,
                                unassigned: i.unassigned,
                                cost: i.cost,
                                improvement: i.improvement,
                                fitness: i.fitness.clone(),
                            })
                            .collect(),
                    },
                })
                .collect(),
        }),
    })
}
