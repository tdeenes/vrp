use hashbrown::{HashMap, HashSet};
use std::sync::Arc;
use vrp_core::models::common::ValueDimension;
use vrp_core::models::problem::Actor;

/// An actor group key implementation which creates groups using "type" dimension.
pub fn create_typed_actor_groups(actors: &[Arc<Actor>]) -> Box<dyn Fn(&Arc<Actor>) -> usize + Send + Sync> {
    let unique_type_keys: HashSet<_> = actors
        .iter()
        .map(|a| (a.vehicle.dimens.get_value::<String>("type_id").cloned().unwrap(), a.detail.clone()))
        .collect();

    let type_key_map: HashMap<_, _> = unique_type_keys.into_iter().zip(0_usize..).collect();

    let groups: HashMap<_, _> = actors
        .iter()
        .map(|a| {
            (
                a.clone(),
                *type_key_map
                    .get(&(a.vehicle.dimens.get_value::<String>("type_id").cloned().unwrap(), a.detail.clone()))
                    .unwrap(),
            )
        })
        .collect();

    Box::new(move |a| *groups.get(a).unwrap())
}
