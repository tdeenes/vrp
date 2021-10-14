use crate::construction::constraints::ConstraintPipeline;
use crate::construction::heuristics::*;
use crate::models::common::*;
use crate::models::problem::{Actor, Job, Place, Single, TransportCost};
use crate::models::Problem;
use crate::utils::*;
use hashbrown::{HashMap, HashSet};
use std::cmp::Ordering;
use std::ops::Deref;
use std::sync::Arc;

const CLUSTER_DIMENSION_KEY: &str = "cls";

/// A trait to get or set cluster info.
pub trait ClusterDimension {
    /// Sets cluster.
    fn set_cluster(&mut self, jobs: Vec<(Job, VisitInfo)>) -> &mut Self;
    /// Gets cluster.
    fn get_cluster(&self) -> Option<&Vec<(Job, VisitInfo)>>;
}

impl ClusterDimension for Dimensions {
    fn set_cluster(&mut self, jobs: Vec<(Job, VisitInfo)>) -> &mut Self {
        self.set_value(CLUSTER_DIMENSION_KEY, jobs);
        self
    }

    fn get_cluster(&self) -> Option<&Vec<(Job, VisitInfo)>> {
        self.get_value(CLUSTER_DIMENSION_KEY)
    }
}

/// Specifies clustering algorithm configuration.
pub struct ClusterConfig {
    /// A thresholds for job clustering.
    threshold: ThresholdPolicy,
    /// Job visiting policy
    visiting: VisitPolicy,
    /// Job service time policy.
    service_time: ServiceTimePolicy,
    /// Specifies filtering policy.
    filtering: FilterPolicy,
    /// Specifies building policy.
    building: BuilderPolicy,
}

/// Defines a various thresholds to control cluster size.
pub struct ThresholdPolicy {
    /// Moving duration limit.
    moving_duration: Duration,
    /// Moving distance limit.
    moving_distance: Distance,
    /// Minimum shared time for jobs.
    min_shared_time: Option<Duration>,
}

/// Specifies cluster visiting policy.
pub enum VisitPolicy {
    /// It is required to return to the first job's location (cluster center) before visiting a next job.
    Repetition,
    /// Clustered jobs are visited one by one from the cluster center finishing in the end at the
    /// first job's location.
    ClosedContinuation,
    /// Clustered jobs are visited one by one starting from the cluster center and finishing in the
    /// end at the last job's location.
    OpenContinuation,
}

/// Specifies filtering policy.
pub struct FilterPolicy {
    /// Job filter.
    job_filter: Arc<dyn Fn(&Job) -> bool + Send + Sync>,
    /// Actor filter.
    actor_filter: Arc<dyn Fn(&Actor) -> bool + Send + Sync>,
}

/// Specifies service time policy.
pub enum ServiceTimePolicy {
    /// Keep original service time.
    Original,
    /// Correct service time by some multiplier.
    Multiplier(f64),
    /// Use fixed value for all clustered jobs.
    Fixed(f64),
}

/// Allows to control how clusters are built.
pub struct BuilderPolicy {
    /// The smallest time window of the cluster.
    smallest_time_window: Option<f64>,
    /// Checks whether given cluster can get more.
    size_filter: Arc<dyn Fn(&[Job]) -> bool + Send + Sync>,
    /// Orders visiting jobs based on their visit info.
    ordering: Arc<dyn Fn(&VisitInfo, &VisitInfo) -> Ordering + Send + Sync>,
}

/// Keeps track of information specific for job visiting.
#[derive(Clone)]
pub struct VisitInfo {
    /// An activity's service time.
    service_time: Duration,
    /// Movement info in forward direction.
    forward: (Distance, Duration),
    /// Movement info in backward direction.
    backward: (Distance, Duration),
}

/// Creates clusters of jobs trying to minimize their radius.
/// Limitations:
/// - only single jobs are clustered
/// - time offset is not supported
pub fn create_job_clusters(
    problem: Arc<Problem>,
    environment: Arc<Environment>,
    profile: &Profile,
    config: &ClusterConfig,
) -> Vec<(Job, Vec<Job>)> {
    let insertion_ctx = InsertionContext::new_empty(problem.clone(), environment);
    let constraint = insertion_ctx.problem.constraint.clone();
    let check_job = get_check_insertion_fn(insertion_ctx, config.filtering.actor_filter.as_ref());
    let estimates = get_estimates(problem.as_ref(), profile, config);

    get_clusters(&constraint, estimates, config, &check_job)
}

type PlaceInfo = (PlaceIndex, Location, Duration, Vec<TimeWindow>);
type PlaceIndex = usize;
type DissimilarityInfo = (PlaceIndex, PlaceIndex, VisitInfo);
type DissimilarityIndex = HashMap<Job, Vec<DissimilarityInfo>>;
type CheckInsertionFn = (dyn Fn(&Job) -> bool + Send + Sync);

/// Estimates ability of each job to build a cluster.
fn get_estimates(problem: &Problem, profile: &Profile, config: &ClusterConfig) -> HashMap<Job, DissimilarityIndex> {
    let transport = problem.transport.as_ref();
    let jobs = problem
        .jobs
        .all()
        .filter(&*config.filtering.job_filter)
        // NOTE multi-job is not supported
        .filter(|job| job.as_single().is_some())
        .collect::<Vec<_>>();

    jobs.iter()
        .map(|outer| {
            let dissimilarities = jobs
                .iter()
                .filter(|inner| outer != *inner)
                .filter_map(|inner| {
                    let dissimilarities = get_dissimilarities(&outer, inner, profile, config, transport);
                    if dissimilarities.is_empty() {
                        None
                    } else {
                        Some((inner.clone(), dissimilarities))
                    }
                })
                .collect::<HashMap<_, _>>();
            (outer.clone(), dissimilarities)
        })
        .collect::<HashMap<_, _>>()
}

fn get_dissimilarities(
    outer: &Job,
    inner: &Job,
    profile: &Profile,
    config: &ClusterConfig,
    transport: &(dyn TransportCost + Send + Sync),
) -> Vec<DissimilarityInfo> {
    let departure = Default::default();
    outer
        .to_single()
        .places
        .iter()
        .enumerate()
        .filter_map(map_place)
        .flat_map(|(outer_place_idx, outer_loc, _, outer_times)| {
            inner.to_single().places.iter().enumerate().filter_map(map_place).filter_map(
                move |(inner_place_idx, inner_loc, inner_duration, inner_times)| {
                    let shared_time = outer_times
                        .iter()
                        .flat_map(|outer_time| {
                            inner_times.iter().filter_map(move |inner_time| {
                                outer_time.overlapping(inner_time).map(|tw| tw.duration())
                            })
                        })
                        .max_by(|a, b| compare_floats(*a, *b))
                        .unwrap_or(0.);

                    if shared_time > config.threshold.min_shared_time.unwrap_or(0.) {
                        let fwd_distance = transport.distance(profile, outer_loc, inner_loc, departure);
                        let fwd_duration = transport.duration(profile, outer_loc, inner_loc, departure);

                        let bck_distance = transport.distance(profile, inner_loc, outer_loc, departure);
                        let bck_duration = transport.duration(profile, inner_loc, outer_loc, departure);

                        match (
                            (fwd_duration - config.threshold.moving_duration < 0.),
                            (fwd_distance - config.threshold.moving_distance < 0.),
                            (bck_duration - config.threshold.moving_duration < 0.),
                            (bck_distance - config.threshold.moving_distance < 0.),
                        ) {
                            (true, true, true, true) => {
                                let service_time = match &config.service_time {
                                    ServiceTimePolicy::Original => inner_duration,
                                    ServiceTimePolicy::Multiplier(multiplier) => inner_duration * *multiplier,
                                    ServiceTimePolicy::Fixed(service_time) => *service_time,
                                };

                                let info = VisitInfo {
                                    service_time,
                                    forward: (fwd_distance, fwd_duration),
                                    backward: (bck_distance, bck_duration),
                                };

                                Some((outer_place_idx, inner_place_idx, shared_time, info))
                            }
                            _ => None,
                        }
                    } else {
                        None
                    }
                },
            )
        })
        .map(|(outer_place_idx, inner_place_idx, _, info)| (outer_place_idx, inner_place_idx, info))
        .collect()
}

fn get_check_insertion_fn(
    insertion_ctx: InsertionContext,
    actor_filter: &(dyn Fn(&Actor) -> bool + Send + Sync),
) -> impl Fn(&Job) -> bool {
    let result_selector = BestResultSelector::default();
    let routes = insertion_ctx
        .solution
        .registry
        .next()
        .filter(|route_ctx| actor_filter.deref()(&route_ctx.route.actor))
        .collect::<Vec<_>>();

    move |job: &Job| -> bool {
        unwrap_from_result(routes.iter().try_fold(false, |_, route_ctx| {
            let result = evaluate_job_insertion_in_route(
                &insertion_ctx,
                route_ctx,
                job,
                InsertionPosition::Any,
                InsertionResult::make_failure(),
                &result_selector,
            );

            match result {
                InsertionResult::Success(_) => Err(true),
                InsertionResult::Failure(_) => Ok(false),
            }
        }))
    }
}

fn get_clusters(
    constraint: &ConstraintPipeline,
    estimates: HashMap<Job, DissimilarityIndex>,
    config: &ClusterConfig,
    check_insertion: &CheckInsertionFn,
) -> Vec<(Job, Vec<Job>)> {
    let mut used_jobs = HashSet::new();
    let mut clusters = Vec::new();
    let mut cluster_estimates = estimates
        .iter()
        .map(|(job, estimate)| (job.clone(), (None, estimate.clone())))
        .collect::<Vec<(_, (Option<Job>, HashMap<_, _>))>>();

    loop {
        // build clusters
        parallel_foreach_mut(cluster_estimates.as_mut_slice(), |(center, (cluster, _))| {
            if cluster.is_none() {
                *cluster = build_job_cluster(constraint, &estimates, center, config, check_insertion)
            }
        });

        // sort trying to prioritize clusters with more jobs
        cluster_estimates.sort_by(|(_, (a_job, a_dis)), (_, (b_job, b_dis))| match (a_job, b_job) {
            (Some(_), Some(_)) => b_dis.len().cmp(&a_dis.len()),
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (None, None) => Ordering::Equal,
        });

        let new_cluster = cluster_estimates.first().and_then(|(_, (cluster, _))| cluster.as_ref()).cloned();

        if let Some(new_cluster) = new_cluster {
            let new_cluster_jobs = new_cluster
                .dimens()
                .get_cluster()
                .expect("expected to have jobs in a cluster")
                .iter()
                .map(|(job, _)| job.clone())
                .collect::<Vec<_>>();

            clusters.push((new_cluster.clone(), new_cluster_jobs.clone()));
            used_jobs.extend(new_cluster_jobs.iter().cloned());

            let new_cluster_jobs = new_cluster_jobs.iter().collect::<HashSet<_>>();

            // remove used jobs from analysis
            cluster_estimates.retain(|(center, _)| !new_cluster_jobs.contains(center));
            cluster_estimates.iter_mut().for_each(|(_, (cluster, candidates))| {
                candidates.retain(|job, _| !new_cluster_jobs.contains(job));

                let is_cluster_affected = cluster
                    .as_ref()
                    .and_then(|cluster| cluster.dimens().get_cluster())
                    .map_or(false, |cluster_jobs| cluster_jobs.iter().any(|(job, _)| new_cluster_jobs.contains(job)));

                if is_cluster_affected {
                    // NOTE force to rebuild cluster on next iteration
                    *cluster = None;
                }
            });
            cluster_estimates.retain(|(_, (_, candidates))| !candidates.is_empty());
        } else {
            break;
        }
    }

    clusters
}

fn build_job_cluster(
    constraint: &ConstraintPipeline,
    estimates: &HashMap<Job, DissimilarityIndex>,
    center_job: &Job,
    config: &ClusterConfig,
    check_insertion: &CheckInsertionFn,
) -> Option<Job> {
    let ordering = config.building.ordering.as_ref();
    let center = center_job.to_single();
    let center_estimates = estimates.get(center_job).expect("missing job in estimates");

    // iterate through all places and choose the one with most jobs clustered
    center.places.iter().enumerate().filter_map(map_place).fold(
        Option::<(Job, usize)>::None,
        |best_cluster, center_place_info| {
            let (center_place_idx, center_location, center_duration, center_times) = center_place_info;
            let new_center_job =
                create_single_job(Some(center_location), center_duration, &center_times, &center.dimens);
            let new_visit_info = VisitInfo { service_time: center_duration, forward: (0., 0.), backward: (0., 0.) };

            // allow jobs only from candidates
            let mut cluster_candidates =
                center_estimates.iter().map(|(candidate, _)| candidate.clone()).collect::<HashSet<_>>();

            let mut cluster = with_cluster_dimension(new_center_job.clone(), &new_center_job, new_visit_info);
            let mut last_job = center_job.clone();
            let mut last_place_idx = center_place_idx;
            let mut count = 1_usize;

            loop {
                if cluster_candidates.is_empty() {
                    break;
                }

                // get job estimates specific for the last visited place
                let mut job_estimates = estimates
                    .get(&last_job)
                    .iter()
                    .flat_map(|index| index.iter().filter(|(job, _)| cluster_candidates.contains(job)))
                    .flat_map(|estimate| {
                        // embed the first visit info to sort estimates of all candidate jobs later
                        get_sorted_dissimilarities(last_place_idx, estimate, ordering)
                            .into_iter()
                            .next()
                            .map(|(_, _, visit_info)| (estimate.0, estimate.1, visit_info))
                    })
                    .collect::<Vec<_>>();
                job_estimates.sort_by(|(_, _, a_info), (_, _, b_info)| ordering.deref()(a_info, b_info));

                // try to find the first successful addition to the cluster from job estimates
                let addition_result = unwrap_from_result(job_estimates.iter().try_fold(None, |_, candidate| {
                    if let Some((new_cluster, used_place_idx, used_info)) = try_add_job(
                        constraint,
                        last_place_idx,
                        &cluster,
                        (candidate.0, candidate.1),
                        config,
                        check_insertion,
                    ) {
                        Err(Some((new_cluster, candidate.0, used_place_idx, used_info)))
                    } else {
                        Ok(None)
                    }
                }));

                match addition_result {
                    Some((new_cluster, added_job, place_idx, visit_info)) => {
                        cluster = with_cluster_dimension(new_cluster, added_job, visit_info);
                        last_job = added_job.clone();
                        last_place_idx = place_idx;
                        count += 1;
                    }
                    None => {
                        cluster = finish_cluster(cluster, config);
                        cluster_candidates.clear();
                    }
                }
            }

            match &best_cluster {
                Some((_, best_count)) if *best_count > count => Some((cluster, count)),
                None => Some((cluster, count)),
                _ => best_cluster,
            }
        },
    );

    unimplemented!()
}

fn try_add_job(
    constraint: &ConstraintPipeline,
    center_place_idx: usize,
    cluster: &Job,
    candidate: (&Job, &Vec<DissimilarityInfo>),
    config: &ClusterConfig,
    check_insertion: &CheckInsertionFn,
) -> Option<(Job, usize, VisitInfo)> {
    let time_window_threshold = config.building.smallest_time_window.unwrap_or(0.);

    let cluster = cluster.to_single();
    let cluster_place = cluster.places.first().expect("expect one place in cluster");
    let cluster_times = filter_times(cluster_place.times.as_slice());
    let cluster_last_duration = cluster
        .dimens
        .get_cluster()
        .and_then(|jobs| jobs.last())
        .and_then(|(job, _)| job.as_single())
        .and_then(|job| job.places.first())
        .map_or(cluster_place.duration, |place| place.duration);

    let job = candidate.0.to_single();
    let dissimilarities = get_sorted_dissimilarities(center_place_idx, candidate, config.building.ordering.as_ref());

    unwrap_from_result(dissimilarities.into_iter().try_fold(None, |_, (_, place_idx, info)| {
        let place = job.places.get(place_idx).expect("wrong place index");
        let place_times = filter_times(place.times.as_slice());

        let new_cluster_times = cluster_times
            .iter()
            .flat_map(|cluster_time| {
                place_times.iter().filter_map(move |place_time| place_time.overlapping(cluster_time))
            })
            .filter_map(|time| {
                // adapt service time from last cluster job to avoid time window violation of
                // a next job in case of last time arrival
                let end = time.end - cluster_last_duration;
                if end - time.start < time_window_threshold {
                    None
                } else {
                    Some(TimeWindow::new(time.start, end))
                }
            })
            .collect::<Vec<_>>();

        // no time window intersection: cannot be clustered
        if new_cluster_times.is_empty() {
            return Ok(None);
        }

        let movement = match config.visiting {
            VisitPolicy::Repetition => info.forward.1 + info.backward.1,
            VisitPolicy::ClosedContinuation | VisitPolicy::OpenContinuation => info.forward.1,
        };

        let new_cluster_duration = cluster_place.duration + movement + info.service_time;

        let updated_cluster = create_single_job(
            cluster_place.location.clone(),
            new_cluster_duration,
            &new_cluster_times,
            &cluster.dimens,
        );
        let updated_candidate =
            create_single_job(place.location, new_cluster_duration, &new_cluster_times, &job.dimens);

        // stop on first successful cluster
        constraint
            .merge_constrained(updated_cluster, updated_candidate)
            .map(|job| if check_insertion.deref()(&job) { Some((job, place_idx, info)) } else { None })
            .map_or_else(|_| Ok(None), |data| Err(data))
    }))
}

fn get_sorted_dissimilarities(
    center_place_idx: usize,
    estimate: (&Job, &Vec<DissimilarityInfo>),
    ordering: &(dyn Fn(&VisitInfo, &VisitInfo) -> Ordering + Send + Sync),
) -> Vec<(Job, usize, VisitInfo)> {
    let (job, dissimilarities) = estimate;
    let mut dissimilarities = dissimilarities
        .iter()
        .filter(|(outer_place_idx, ..)| *outer_place_idx == center_place_idx)
        .map(|(_, place_idx, info)| (job.clone(), *place_idx, info.clone()))
        .collect::<Vec<_>>();

    // sort dissimilarities based on user provided ordering function
    dissimilarities.sort_by(|(_, _, a_info), (_, _, b_info)| ordering.deref()(a_info, b_info));

    dissimilarities
}

fn map_place(place_data: (PlaceIndex, &Place)) -> Option<PlaceInfo> {
    let (idx, place) = place_data;
    place.location.map(|location| (idx, location, place.duration, filter_times(place.times.as_slice())))
}

fn filter_times(times: &[TimeSpan]) -> Vec<TimeWindow> {
    times.iter().filter_map(|time| time.as_time_window()).collect::<Vec<_>>()
}

fn with_cluster_dimension(cluster: Job, added_job: &Job, visit_info: VisitInfo) -> Job {
    let cluster = cluster.to_single();

    let mut cluster = Single { places: cluster.places.clone(), dimens: cluster.dimens.clone() };

    let mut jobs = cluster.dimens.get_cluster().cloned().unwrap_or_else(|| Vec::new());
    jobs.push((added_job.clone(), visit_info));

    cluster.dimens.set_cluster(jobs);

    Job::Single(Arc::new(cluster))
}

fn finish_cluster(cluster: Job, config: &ClusterConfig) -> Job {
    let clustered_jobs = cluster.dimens().get_cluster();

    match (&config.visiting, clustered_jobs) {
        (VisitPolicy::ClosedContinuation, Some(clustered)) => {
            // add extra duration from last clustered job to finish cluster visiting

            let cluster = cluster.to_single();
            assert_eq!(cluster.places.len(), 1);
            let (_, visit_info) = clustered.last().expect("expect clustered jobs to be non-empty");

            let mut place = cluster.places.get(0).unwrap().clone();
            place.duration += visit_info.backward.1;

            Job::Single(Arc::new(Single { places: vec![place], dimens: cluster.dimens.clone() }))
        }
        _ => cluster,
    }
}

fn create_single_job(location: Option<Location>, duration: Duration, times: &[TimeWindow], dimens: &Dimensions) -> Job {
    Job::Single(Arc::new(Single {
        places: vec![Place {
            location,
            duration,
            times: times.into_iter().map(|time| TimeSpan::Window(time.clone())).collect(),
        }],
        dimens: dimens.clone(),
    }))
}
