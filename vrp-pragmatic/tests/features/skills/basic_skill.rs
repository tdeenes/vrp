use crate::helpers::*;
use crate::json::problem::*;
use crate::json::solution::*;

#[test]
fn can_wait_for_job_start() {
    let problem = Problem {
        id: "my_problem".to_string(),
        plan: Plan {
            jobs: vec![create_delivery_job_with_skills("job1", vec![1., 0.], vec!["unique_skill".to_string()])],
            relations: Option::None,
        },
        fleet: Fleet {
            types: vec![
                create_default_vehicle("vehicle_without_skill"),
                VehicleType {
                    id: "vehicle_with_skill".to_string(),
                    profile: "car".to_string(),
                    costs: create_default_vehicle_costs(),
                    shifts: vec![create_default_vehicle_shift_with_locations((10., 0.), (10., 0.))],
                    capacity: vec![10],
                    amount: 1,
                    skills: Some(vec!["unique_skill".to_string()]),
                    limits: None,
                },
            ],
            profiles: create_default_profiles(),
        },
        config: None,
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, vec![matrix]);

    assert_eq!(
        solution,
        Solution {
            problem_id: "my_problem".to_string(),
            statistic: Statistic {
                cost: 47.,
                distance: 18,
                duration: 19,
                times: Timing { driving: 18, serving: 1, waiting: 0, break_time: 0 },
            },
            tours: vec![Tour {
                vehicle_id: "vehicle_with_skill_1".to_string(),
                type_id: "vehicle_with_skill".to_string(),
                shift_index: 0,
                stops: vec![
                    create_stop_with_activity(
                        "departure",
                        "departure",
                        (10., 0.),
                        1,
                        ("1970-01-01T00:00:00Z", "1970-01-01T00:00:00Z"),
                    ),
                    create_stop_with_activity(
                        "job1",
                        "delivery",
                        (1., 0.),
                        0,
                        ("1970-01-01T00:00:09Z", "1970-01-01T00:00:10Z"),
                    ),
                    create_stop_with_activity(
                        "arrival",
                        "arrival",
                        (10., 0.),
                        0,
                        ("1970-01-01T00:00:19Z", "1970-01-01T00:00:19Z"),
                    )
                ],
                statistic: Statistic {
                    cost: 47.,
                    distance: 18,
                    duration: 19,
                    times: Timing { driving: 18, serving: 1, waiting: 0, break_time: 0 },
                },
            }],
            unassigned: vec![],
            extras: None,
        }
    );
}
