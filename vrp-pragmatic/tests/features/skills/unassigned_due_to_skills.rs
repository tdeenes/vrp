use crate::format::problem::*;
use crate::format::solution::*;
use crate::helpers::*;

#[test]
fn can_have_unassigned_due_to_missing_vehicle_skill() {
    let problem = Problem {
        plan: Plan {
            jobs: vec![create_delivery_job_with_skills(
                "job1",
                vec![1., 0.],
                all_of_skills(vec!["unique_skill".to_string()]),
            )],
            ..create_empty_plan()
        },
        fleet: Fleet {
            vehicles: vec![create_default_vehicle("vehicle_without_skill")],
            profiles: create_default_matrix_profiles(),
        },
        ..create_empty_problem()
    };
    let matrix = create_matrix_from_problem(&problem);

    let solution = solve_with_metaheuristic(problem, Some(vec![matrix]));

    assert_eq!(
        solution,
        Solution {
            statistic: Statistic {
                cost: 0.,
                distance: 0,
                duration: 0,
                times: Timing { driving: 0, serving: 0, waiting: 0, break_time: 0 },
            },
            tours: vec![],
            unassigned: Some(vec![UnassignedJob {
                job_id: "job1".to_string(),
                reasons: vec![UnassignedJobReason {
                    code: "SKILL_CONSTRAINT".to_string(),
                    description: "cannot serve required skill".to_string()
                }]
            }]),
            ..create_empty_solution()
        }
    );
}
