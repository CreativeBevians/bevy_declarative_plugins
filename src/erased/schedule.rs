use bevy_app::MainScheduleOrder;
use bevy_ecs::{
    schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel, Schedules},
    system::ScheduleSystem,
    world::World,
};

use crate::metadata_ptr::MetadataPtr;

pub(crate) struct ErasedScheduleLabel(MetadataPtr);

#[derive(Debug, Clone, Copy)]
pub enum BeforeOrAfter<S> {
    Before(S),
    After(S),
}

impl<S> BeforeOrAfter<S> {
    pub fn map<T>(self, f: impl FnOnce(S) -> T) -> BeforeOrAfter<T> {
        match self {
            BeforeOrAfter::Before(label) => BeforeOrAfter::Before(f(label)),
            BeforeOrAfter::After(label) => BeforeOrAfter::After(f(label)),
        }
    }

    pub fn get(&self) -> &S {
        match self {
            BeforeOrAfter::Before(s) | BeforeOrAfter::After(s) => s,
        }
    }
}

impl ErasedScheduleLabel {
    pub(crate) fn new<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        label: S,
        relative_to: BeforeOrAfter<O>,
    ) -> Option<Self> {
        Some(Self(MetadataPtr::new((label, relative_to))?))
    }

    pub(crate) fn init_to_world<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        &self,
        world: &mut World,
    ) -> Option<()> {
        self.0
            .visit::<(S, BeforeOrAfter<O>), ()>(|(schedule, other)| {
                let mut schedules = world.resource_mut::<Schedules>();
                let label = schedule.intern();
                if !schedules.contains(label) {
                    schedules.insert(Schedule::new(label));
                }
                let other_label = other.get().intern();
                if !schedules.contains(other_label) {
                    schedules.insert(Schedule::new(other_label));
                }
                drop(schedules);
            })
    }
}

pub(crate) struct StagedScheduleLabel {
    pub(crate) staged: Staged<ErasedScheduleLabel>,
}

impl StagedScheduleLabel {
    pub(crate) fn new<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        label: S,
        relative_to: BeforeOrAfter<O>,
    ) -> Option<Self> {
        Some(Self {
            staged: Staged {
                erased: ErasedScheduleLabel::new(label, relative_to)?,
                unerase_and_apply_to_world: |world, erased| {
                    let initialized = erased.init_to_world::<S, O>(world).is_some();
                    if initialized {
                        let mut schedule_order = world.get_resource_or_init::<MainScheduleOrder>();
                        match erased.0.try_reverse_erase::<(S, BeforeOrAfter<O>)>() {
                            Ok((label, BeforeOrAfter::Before(other_label))) => {
                                schedule_order.insert_before(other_label, label);
                            }
                            Ok((label, BeforeOrAfter::After(other_label))) => {
                                schedule_order.insert_after(other_label, label);
                            }
                            Err(_) => {
                                panic!("Incorrect label type.")
                            }
                        }
                    }
                },
            },
        })
    }
}

pub(crate) struct ErasedScheduleSystemsPair(MetadataPtr);

impl ErasedScheduleSystemsPair {
    pub fn new<P: 'static>(pair: P) -> Option<Self> {
        Some(Self(MetadataPtr::new(pair)?))
    }
}

pub(crate) struct StagedSystem {
    pub(crate) staging: Staged<ErasedScheduleSystemsPair>,
}

impl StagedSystem {
    pub(crate) fn new<L, S, M>(label: L, systems: S) -> Option<Self>
    where
        L: ScheduleLabel + 'static,
        S: IntoScheduleConfigs<ScheduleSystem, M> + 'static,
    {
        let pair: (L, S) = (label, systems);
        Some(Self {
            staging: Staged {
                erased: ErasedScheduleSystemsPair::new(pair)?,
                unerase_and_apply_to_world: |world, erased| match erased
                    .0
                    .try_reverse_erase::<(L, S)>()
                {
                    Ok((label, systems)) => {
                        world
                            .get_resource_or_init::<Schedules>()
                            .add_systems(label, systems);
                    }
                    _ => {}
                },
            },
        })
    }
}

pub(crate) struct Staged<E> {
    pub(crate) erased: E,
    pub(crate) unerase_and_apply_to_world: fn(&mut World, E),
}

impl<E> Staged<E> {
    pub(crate) fn apply_to_world(self, world: &mut World) {
        (self.unerase_and_apply_to_world)(world, self.erased)
    }
}
