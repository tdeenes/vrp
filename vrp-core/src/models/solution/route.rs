use crate::models::common::{Distance, Duration, Location, Schedule, TimeWindow};
use crate::models::problem::{Actor, Job, Multi, Single};
use crate::models::solution::Tour;
use crate::utils::{compare_floats, compare_shared};
use std::cmp::Ordering;
use std::sync::Arc;

/// Specifies an extra commute information to reach the actual place.
#[derive(Clone)]
pub struct Commute {
    /// An commute information to reach place.
    pub forward: (Distance, Duration),

    /// An commute information to get out from the place.
    pub backward: (Distance, Duration),
}

/// Specifies activity place.
#[derive(Clone, Debug)]
pub struct Place {
    /// Location where activity is performed.
    pub location: Location,

    /// Specifies activity's duration.
    pub duration: Duration,

    /// Specifies activity's time window: an interval when job is allowed to be started.
    pub time: TimeWindow,
}

/// Represents activity which is needed to be performed.
pub struct Activity {
    /// Specifies activity details.
    pub place: Place,

    /// Specifies activity's schedule including commute time.
    pub schedule: Schedule,

    /// Specifies associated job. Empty if it has no association with a single job (e.g. tour start or end).
    /// If single job is part of multi job, then original job can be received via `retrieve_job` method.
    pub job: Option<Arc<Single>>,

    /// An extra commute time to the place.
    pub commute: Option<Commute>,
}

/// Represents a tour performing jobs.
pub struct Route {
    /// An actor associated within route.
    pub actor: Arc<Actor>,

    /// Specifies job tour assigned to this route.
    pub tour: Tour,
}

impl Route {
    /// Returns a deep copy of `Route`.
    pub fn deep_copy(&self) -> Self {
        Self { actor: self.actor.clone(), tour: self.tour.deep_copy() }
    }
}

impl Activity {
    /// Creates an activity with a job.
    pub fn new_with_job(job: Arc<Single>) -> Self {
        Activity {
            place: Place { location: 0, duration: 0.0, time: TimeWindow { start: 0.0, end: f64::MAX } },
            schedule: Schedule { arrival: 0.0, departure: 0.0 },
            job: Some(job),
            commute: None,
        }
    }

    /// Creates a deep copy of `Activity`.
    pub fn deep_copy(&self) -> Self {
        Self {
            place: Place {
                location: self.place.location,
                duration: self.place.duration,
                time: self.place.time.clone(),
            },
            schedule: self.schedule.clone(),
            job: self.job.clone(),
            commute: self.commute.clone(),
        }
    }

    /// Checks whether activity has given job.
    pub fn has_same_job(&self, job: &Job) -> bool {
        match self.retrieve_job() {
            Some(j) => match (&j, job) {
                (Job::Multi(lhs), Job::Multi(rhs)) => compare_shared(lhs, rhs),
                (Job::Single(lhs), Job::Single(rhs)) => compare_shared(lhs, rhs),
                _ => false,
            },
            _ => false,
        }
    }

    /// Returns job if activity has it.
    pub fn retrieve_job(&self) -> Option<Job> {
        match self.job.as_ref() {
            Some(single) => Multi::roots(single).map(Job::Multi).or_else(|| Some(Job::Single(single.clone()))),
            _ => None,
        }
    }
}

impl Commute {
    /// Checks whether zero is no time costs for commute.
    pub fn is_zero_time(&self) -> bool {
        compare_floats(self.forward.1, 0.) == Ordering::Equal && compare_floats(self.backward.1, 0.) == Ordering::Equal
    }
}
