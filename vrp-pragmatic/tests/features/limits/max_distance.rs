use crate::format::problem::*;
use crate::format::solution::*;
use crate::helpers::*;

#[test]
fn can_limit_by_max_distance() {
    let problem = Problem {
        plan: Plan { jobs: vec![create_delivery_job("job1", vec![100., 0.])], ..create_empty_plan() },
        fleet: Fleet {
            vehicles: vec![VehicleType {
                limits: Some(VehicleLimits {
                    max_distance: Some(99.),
                    shift_time: None,
                    tour_size: None,
                    allowed_areas: None,
                }),
                ..create_default_vehicle_type()
            }],
            profiles: create_default_matrix_profiles(),
        },
        ..create_empty_problem()
    };
    let matrix = Matrix {
        profile: Some("car".to_owned()),
        timestamp: None,
        travel_times: vec![1, 1, 1, 1],
        distances: vec![1, 100, 100, 1],
        error_codes: Option::None,
    };

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(
        solution,
        Solution {
            statistic: Statistic {
                cost: 0.,
                distance: 0,
                duration: 0,
                times: Timing { driving: 0, serving: 0, ..Timing::default() },
            },
            tours: vec![],
            unassigned: Some(vec![UnassignedJob {
                job_id: "job1".to_string(),
                reasons: vec![UnassignedJobReason {
                    code: "MAX_DISTANCE_CONSTRAINT".to_string(),
                    description: "cannot be assigned due to max distance constraint of vehicle".to_string()
                }]
            }]),
            ..create_empty_solution()
        }
    );
}
