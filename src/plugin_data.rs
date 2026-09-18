#[cfg(feature = "state")]
use bevy_ecs::world::FromWorld;
use bevy_ecs::{
    message::Message,
    observer::{IntoObserver, Observer},
    resource::Resource,
    schedule::{IntoScheduleConfigs, ScheduleLabel},
    system::ScheduleSystem,
};
use bevy_platform::collections::HashMap;
#[cfg(feature = "state")]
use bevy_state::state::States;
use core::{
    any::{Any, TypeId},
    hash::Hash,
};
use std::{boxed::Box, marker::PhantomData, vec::Vec};

use crate::{
    App, DeclarativePlugin,
    approval::Approval,
    erased::{
        dataless::{ErasedDataless, OrderedDataless},
        resource::StagedResource,
        schedule::{StagedScheduleLabel, StagedSystem},
    },
};

/// A list of plugin dependencies for a particular [`PluginOutput`].
pub struct PluginDependencies(pub(crate) Vec<PluginDependency>);

/// Plugin output, opaque to end user.
///
/// This is designed to be a plugin data structure that end users don't need to
/// think about in terms of what it's "made of." Just something that stuff can be
/// added to.
///
/// TODO: docs that are user-facing, not reviewer-facing.
pub struct PluginOutput<D = PluginDependencies> {
    /// The plugin was added to an app, or part of a declarative bundle, rather
    /// than being inserted as a dependency.
    pub(crate) is_entry_point: bool,
    /// Is the plugin type zero-sized (most are). If a plugin is zero-sized we
    /// can make the assumption that all calls to [`DeclarativePlugin::build`]
    /// for that type give an identical [`PluginOutput`], as there is no
    /// configuration that can be done.
    pub(crate) is_zero_sized_optimizable: bool,
    /// Plugin type ID (used to build edges later)
    pub(crate) working_plugin: PluginTypeId,
    /// Observers registered by this plugin.
    pub(crate) observers: Vec<Observer>,
    /// The schedule graph for this plugin (to be merged with others later)
    pub(crate) schedules: MergeableSchedule,
    pub(crate) schedule_labels: Vec<StagedScheduleLabel>,
    /// Message storage
    pub(crate) messages: HashMap<TypeId, MessageRegistration>,
    /// Resource storage
    pub(crate) resource_staging: Vec<StagedResource>,
    pub(crate) plugin_approval: HashMap<PluginTypeId, Approval<Box<dyn DeclarativePlugin>>>,
    #[cfg(feature = "state")]
    pub(crate) states: Vec<ErasedDataless<()>>,
    /// Plugin dependencies, represented with a generic so we can erase them in
    /// plugin graph resolution.
    pub(crate) dependencies: D,
}

impl<D> PluginOutput<D> {
    pub(crate) fn extract_dependencies(self) -> (PluginOutput<()>, D) {
        let Self {
            is_entry_point,
            is_zero_sized_optimizable,
            working_plugin,
            observers,
            schedules,
            messages,
            resource_staging,
            states,
            dependencies,
            plugin_approval,
            schedule_labels,
        } = self;
        (
            PluginOutput {
                is_entry_point,
                is_zero_sized_optimizable,
                working_plugin,
                observers,
                schedules,
                messages,
                resource_staging,
                plugin_approval,
                states,
                dependencies: (),
                schedule_labels,
            },
            dependencies,
        )
    }
}

impl PluginOutput {
    /// Create a plugin output structure for a given plugin type.
    pub(crate) fn new<P: 'static>() -> Self {
        Self {
            // This is set by App bookkeeping.
            is_entry_point: false,
            is_zero_sized_optimizable: size_of::<P>() == 0,
            working_plugin: PluginTypeId(TypeId::of::<P>()),
            observers: Vec::new(),
            schedules: MergeableSchedule {
                schedules: Vec::new(),
            },
            messages: HashMap::new(),
            resource_staging: Vec::new(),
            dependencies: PluginDependencies(Vec::new()),
            plugin_approval: HashMap::new(),
            states: Vec::new(),
            schedule_labels: Vec::new(),
        }
    }

    #[cfg(feature = "state")]
    pub fn init_state<T: States + Default>(&mut self) {
        self.states.push(ErasedDataless {
            ordering: (),
            func: |world| {
                use bevy_state::state::State;
                world.get_resource_or_init::<State<T>>();
            },
        });
    }

    pub fn add_systems<M: 'static>(
        &mut self,
        schedule: impl ScheduleLabel + 'static,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M> + 'static,
    ) -> &mut Self {
        let _ = self.schedules.add_systems(schedule, systems);
        self
    }

    pub fn add_observer<M>(&mut self, observer: impl IntoObserver<M>) -> &mut Self {
        self.observers.push(observer.into_observer());
        self
    }

    pub fn add_message<M: Message + 'static>(&mut self) -> &mut Self {
        self.messages
            .insert(TypeId::of::<M>(), MessageRegistration::new::<M>());
        self
    }

    pub fn require_resource<R: Resource + Default>(&mut self) -> &mut Self {
        self.require_resource_with_approval(|_: &R| true)
    }

    pub fn require_exact_resource<R: Resource + Clone + PartialEq>(
        &mut self,
        resource: R,
    ) -> &mut Self {
        let cloned = resource.clone();
        self.require_resource_with_value_and_approval(resource, move |resource: &R| {
            *resource == cloned
        })
    }

    pub fn require_resource_with_approval<R: Resource + Default>(
        &mut self,
        approval: impl Fn(&R) -> bool + 'static,
    ) -> &mut Self {
        let Some(resource) = StagedResource::new(R::default(), true, |_| true) else {
            return self;
        };
        self.resource_staging.push(resource);
        self
    }

    pub fn require_resource_with_value<R: Resource>(&mut self, resource: R) -> &mut Self {
        let Some(resource) = StagedResource::new(resource, false, |_| true) else {
            return self;
        };
        self.resource_staging.push(resource);
        self
    }

    pub fn require_resource_with_value_and_approval<R: Resource>(
        &mut self,
        resource: R,
        approval: impl Fn(&R) -> bool + 'static,
    ) -> &mut Self {
        let Some(resource) = StagedResource::new(resource, false, approval) else {
            return self;
        };
        self.resource_staging.push(resource);
        self
    }

    #[deprecated]
    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> &mut Self {
        let Some(resource) = StagedResource::new(resource, false, |_| true) else {
            return self;
        };
        self.resource_staging.push(resource);
        self
    }
    pub fn dependency_builder<'a, P>(&'a mut self) -> DependencyBuilder<'a, P> {
        DependencyBuilder {
            config: None,
            approval: None,
            output: self,
        }
    }
}

pub struct DependencyBuilder<'a, P> {
    config: Option<P>,
    approval: Option<Approval<Box<dyn DeclarativePlugin>>>,
    output: &'a mut PluginOutput,
}

impl<'a, P> DependencyBuilder<'a, P> {
    pub fn build(self) -> &'a mut PluginOutput
    where
        P: DeclarativePlugin,
    {
        let type_id = PluginTypeId(TypeId::of::<P>());
        let Self {
            config,
            approval,
            output,
        } = self;
        if let Some(approval) = approval {
            output.plugin_approval.insert(type_id, approval);
        }
        let dependency = if let Some(config) = config {
            PluginDependency::new_with_config(config)
        } else {
            PluginDependency::new::<P>()
        };
        output.dependencies.0.push(dependency);
        output
    }

    pub fn build_and_next<P2>(self) -> DependencyBuilder<'a, P2>
    where
        P: DeclarativePlugin,
        P2: DeclarativePlugin,
    {
        self.build().dependency_builder()
    }

    pub fn with_config(&mut self, config: P) -> &mut Self
    where
        P: DeclarativePlugin,
    {
        self.config = Some(config);
        self
    }

    pub fn with_approval<F: for<'b> Fn(&'b P) -> bool + 'static>(
        &mut self,
        approval: F,
    ) -> &mut Self
    where
        P: DeclarativePlugin,
    {
        self.approval = Some(Approval::new(move |dyn_plugin| {
            let Some(plugin) = <dyn Any>::downcast_ref::<P>(dyn_plugin) else {
                return false;
            };
            approval(plugin)
        }));
        self
    }
}

pub(crate) struct PluginDependency {
    pub(crate) type_id: PluginTypeId,
    /// An optional pairing of a plugin's data (as initialized by the plugin depending on it) and an erased function that builds the plugin output for that dependency.
    pub(crate) data: Option<(
        Box<dyn DeclarativePlugin>,
        fn(&dyn DeclarativePlugin) -> Option<PluginOutput>,
    )>,
}

impl PluginDependency {
    pub(crate) fn new_with_config<P: DeclarativePlugin + 'static>(plugin: P) -> Self {
        let data: Option<(
            Box<dyn DeclarativePlugin>,
            fn(&dyn DeclarativePlugin) -> Option<PluginOutput>,
        )> = Some((Box::new(plugin), |plugin: &dyn DeclarativePlugin| {
            <dyn Any>::downcast_ref::<P>(plugin).map(|plugin| {
                let mut output = PluginOutput::new::<P>();
                plugin.build(&mut output);
                output
            })
        }));
        PluginDependency {
            type_id: PluginTypeId(TypeId::of::<P>()),
            data,
        }
    }

    pub fn new<P: DeclarativePlugin + 'static>() -> Self {
        Self {
            type_id: PluginTypeId(TypeId::of::<P>()),
            data: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub(crate) struct PluginTypeId(TypeId);

pub(crate) struct MessageRegistration {
    // A function that actually registers the message type with the App/World.
    registration_func: fn(&mut App),
}

impl MessageRegistration {
    pub(crate) fn new<T: Message>() -> Self {
        Self {
            registration_func: |app| {
                app.add_message::<T>();
            },
        }
    }
}

pub(crate) struct MergeableSchedule {
    // This isn't ideal, as it does put the 'static requirements on systems,
    // but it will do for now.
    schedules: Vec<StagedSystem>,
}

impl MergeableSchedule {
    pub(crate) fn add_systems<M>(
        &mut self,
        schedule: impl ScheduleLabel + 'static,
        systems: impl IntoScheduleConfigs<ScheduleSystem, M> + 'static,
    ) -> Result<(), ()> {
        self.schedules
            .push(StagedSystem::new(schedule, systems).ok_or(())?);
        Ok(())
    }

    pub(crate) fn merge_simple(&mut self, other: Self) {
        self.schedules.extend(other.schedules);
    }
}
