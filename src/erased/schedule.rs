use std::any::{TypeId, type_name};

use bevy_app::MainScheduleOrder;
use bevy_ecs::{
    schedule::{IntoScheduleConfigs, Schedule, ScheduleLabel, Schedules},
    system::ScheduleSystem,
    world::World,
};

use crate::metadata_ptr::MetadataPtr;

/// Helper enum for describing where a schedule label should be inserted
/// relative to another schedule label.
#[derive(Debug, Clone, Copy)]
pub enum BeforeOrAfter<S> {
    /// Insert a schedule before this one.
    Before(S),
    /// Insert a schedule after this one.
    After(S),
}

impl<S> BeforeOrAfter<S> {
    /// Map over the underlying value, producing a new [`BeforeOrAfter`].
    pub fn map<T>(self, f: impl FnOnce(S) -> T) -> BeforeOrAfter<T> {
        match self {
            BeforeOrAfter::Before(label) => BeforeOrAfter::Before(f(label)),
            BeforeOrAfter::After(label) => BeforeOrAfter::After(f(label)),
        }
    }

    /// Get a reference to the underlying value.
    pub fn get(&self) -> &S {
        match self {
            BeforeOrAfter::Before(s) | BeforeOrAfter::After(s) => s,
        }
    }
}

/// Newtype over the [`MetadataPtr`] that stores the schedule ordering data.
pub(crate) struct ErasedScheduleLabel(MetadataPtr);

impl ErasedScheduleLabel {
    /// Create a new pairing of a schedule label and a what label it should be
    /// inserted before / after.
    pub(crate) fn new<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        label: S,
        relative_to: BeforeOrAfter<O>,
    ) -> Option<Self> {
        Some(Self(MetadataPtr::new((label, relative_to))?))
    }

    /// Initialize the schedule labels to world, without ordering them.
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
            })
    }
}

/// A [`Staged`] erased schedule label.
pub(crate) struct StagedScheduleLabel {
    pub(crate) staged: Staged<ErasedScheduleLabel>,
}

impl StagedScheduleLabel {
    ///
    pub(crate) fn new<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        label: S,
        relative_to: BeforeOrAfter<O>,
    ) -> Option<Self> {
        Some(Self {
            staged: Staged {
                erased: ErasedScheduleLabel::new(label, relative_to)?,
                unerase_and_apply_to_world: Self::reverse_erase_apply_to_world::<S, O>,
            },
        })
    }

    fn reverse_erase_apply_to_world<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        world: &mut World,
        erased: ErasedScheduleLabel,
    ) {
        let initialized = erased.init_to_world::<S, O>(world).is_some();
        if initialized {
            let mut schedule_order = world.get_resource_or_init::<MainScheduleOrder>();
            match erased.0.try_reverse_erase::<(S, BeforeOrAfter<O>)>() {
                Ok((label, BeforeOrAfter::Before(other_label))) => {
                    if schedule_order.labels.contains(&other_label.intern()) {
                        schedule_order.insert_before(other_label, label);
                    } else if schedule_order
                        .startup_labels
                        .contains(&other_label.intern())
                    {
                        schedule_order.insert_startup_before(other_label, label);
                    } else {
                        panic!("Expected {other_label:?} to exist.");
                    }
                }
                Ok((label, BeforeOrAfter::After(other_label))) => {
                    if schedule_order.labels.contains(&other_label.intern()) {
                        schedule_order.insert_after(other_label, label);
                    } else if schedule_order
                        .startup_labels
                        .contains(&other_label.intern())
                    {
                        schedule_order.insert_startup_after(other_label, label);
                    } else {
                        panic!("Expected {other_label:?} to exist.");
                    }
                }
                Err(_) => {
                    panic!("Incorrect label type.")
                }
            }
        }
    }
}

/// An erased pair of a [`ScheduleLabel`] type and an [`IntoScheduleConfigs`]
/// type.
pub(crate) struct ErasedScheduleSystemsPair(MetadataPtr);

impl ErasedScheduleSystemsPair {
    pub fn new<P: 'static>(pair: P) -> Option<Self> {
        Some(Self(MetadataPtr::new(pair)?))
    }
}

/// A staged
pub(crate) struct StagedSystem {
    pub(crate) staged: Staged<ErasedScheduleSystemsPair>,
}

impl StagedSystem {
    pub(crate) fn new<L, S, M>(label: L, systems: S) -> Option<Self>
    where
        L: ScheduleLabel + 'static,
        S: IntoScheduleConfigs<ScheduleSystem, M> + 'static,
    {
        let pair: (L, S) = (label, systems);
        Some(Self {
            staged: Staged {
                erased: ErasedScheduleSystemsPair::new(pair)?,
                unerase_and_apply_to_world: Self::reverse_erase_apply_to_world::<L, S, M>,
            },
        })
    }

    fn reverse_erase_apply_to_world<L, S, M>(world: &mut World, erased: ErasedScheduleSystemsPair)
    where
        L: ScheduleLabel + 'static,
        S: IntoScheduleConfigs<ScheduleSystem, M> + 'static,
    {
        match erased.0.try_reverse_erase::<(L, S)>() {
            Ok((label, systems)) => {
                world
                    .get_resource_or_init::<Schedules>()
                    .add_systems(label, systems);
            }
            Err(e) => {
                panic!(
                    "Could not apply system to world for {} {}. TypeIds are {:?}, {:?}",
                    type_name::<L>(),
                    type_name::<S>(),
                    e.erased_type_id(),
                    TypeId::of::<(L, S)>()
                )
            }
        }
    }
}

/// An erased value and a function that can add that value to a [`World`].
///
/// This pairing works by knowing what the erased type is before it gets erased
/// and using that information to construct a function whose signature takes
/// the erased value, but whose body reverses the type erasure (usually via
/// [`MetadataPtr::try_reverse_erase`].)
///
/// This lets us erase the value while also taking advantage of
pub(crate) struct Staged<E> {
    pub(crate) erased: E,
    pub(crate) unerase_and_apply_to_world: fn(&mut World, E),
}

impl<E> Staged<E> {
    /// Use the [`Self::unerase_and_apply_to_world`] fn on the erased value.
    pub(crate) fn apply_to_world(self, world: &mut World) {
        (self.unerase_and_apply_to_world)(world, self.erased)
    }
}
