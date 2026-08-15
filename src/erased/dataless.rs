use bevy_ecs::world::World;

use crate::erased::schedule::BeforeOrAfter;

pub type OrderedDataless = ErasedDataless<Order>;

pub type Order = Option<BeforeOrAfter<RegistrationOrder>>;

pub struct ErasedDataless<O> {
    pub(crate) ordering: O,
    pub(crate) func: fn(&mut World),
}

pub enum RegistrationOrder {
    Messages,
    Events,
    Observers,
    Schedules,
    Resources,
    Systems,
}
