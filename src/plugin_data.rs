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
use std::{any::type_name, boxed::Box, vec::Vec};

use crate::{
    App, DeclarativePlugin,
    approval::Approval,
    entry_points::AddDependency,
    erased::{
        resource::{ErasedResource, ResourceTypeId, StagedResource},
        schedule::{BeforeOrAfter, StagedScheduleLabel, StagedSystem},
    },
    graph::DeclrItem,
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
    pub(crate) plugin_name: &'static str,
    /// Observers registered by this plugin.
    pub(crate) observers: Vec<Observer>,
    /// The schedule graph for this plugin (to be merged with others later)
    pub(crate) schedules: MergeableSchedule,
    pub(crate) schedule_labels: Vec<StagedScheduleLabel>,
    /// Message storage
    pub(crate) messages: HashMap<TypeId, MessageRegistration>,
    /// Resource storage
    pub(crate) resource_staging: HashMap<ResourceTypeId, StagedResource>,
    pub(crate) plugin_approval: HashMap<PluginTypeId, Approval<Box<dyn DeclarativePlugin>>>,
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
            dependencies,
            plugin_approval,
            schedule_labels,
            plugin_name,
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
                dependencies: (),
                schedule_labels,
                plugin_name,
            },
            dependencies,
        )
    }

    pub(crate) fn get_resource(&self, resource_id: ResourceTypeId) -> Option<&StagedResource> {
        self.resource_staging.get(&resource_id)
    }

    pub(crate) fn extract_resource(
        &mut self,
        resource_id: ResourceTypeId,
    ) -> Option<StagedResource> {
        self.resource_staging.remove(&resource_id)
    }

    pub(crate) fn extract_resources_into_buffer(
        &mut self,
        buffer: &mut Vec<DeclrItem>,
        map: &HashMap<ResourceTypeId, PluginTypeId>,
    ) {
        let to_extract: Vec<_> = self
            .resource_staging
            .keys()
            .filter_map(|resource_id| {
                let plugin_id = map.get(resource_id)?;
                (*plugin_id == self.working_plugin).then_some(resource_id)
            })
            .cloned()
            .collect();
        for extract in to_extract {
            let Some(resource) = self.extract_resource(extract) else {
                continue;
            };
            buffer.push(DeclrItem::Resource(resource));
        }
    }

    pub(crate) fn copy_messages_to_hashmap(
        &self,
        messages: &mut HashMap<TypeId, MessageRegistration>,
    ) {
        messages.extend(
            self.messages
                .iter()
                .map(|(type_id, registration)| (*type_id, *registration)),
        );
    }

    pub(crate) fn extract_observers_into_buffer(&mut self, buffer: &mut Vec<DeclrItem>) {
        buffer.extend(self.observers.drain(..).map(DeclrItem::Observer));
    }

    pub(crate) fn extract_schedules_into_buffer(&mut self, buffer: &mut Vec<DeclrItem>) {
        buffer.extend(self.schedule_labels.drain(..).map(DeclrItem::ScheduleLabel));
    }

    pub(crate) fn extract_systems_into_buffer(&mut self, buffer: &mut Vec<DeclrItem>) {
        buffer.extend(self.schedules.schedules.drain(..).map(DeclrItem::System));
    }
}

impl PluginOutput {
    /// Create a plugin output structure for a given plugin type.
    pub(crate) fn new<P: DeclarativePlugin + 'static>() -> Self {
        Self {
            // This is set by App bookkeeping.
            is_entry_point: false,
            is_zero_sized_optimizable: size_of::<P>() == 0,
            working_plugin: PluginTypeId::of::<P>(),
            plugin_name: type_name::<P>(),
            observers: Vec::new(),
            schedules: MergeableSchedule {
                schedules: Vec::new(),
            },
            messages: HashMap::new(),
            resource_staging: HashMap::new(),
            dependencies: PluginDependencies(Vec::new()),
            plugin_approval: HashMap::new(),
            schedule_labels: Vec::new(),
        }
    }

    #[cfg(feature = "state")]
    pub fn init_state<T: States + Default>(&mut self) {
        use bevy_state::state::State;
        self.resource_builder::<State<T>>()
            .with_data(State::new(T::default()))
            .build();
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

    /// Alias for [`Self::add_dependencies`].
    pub fn add_plugins<P: AddDependency>(&mut self, dependencies: P) -> &mut Self {
        self.add_dependencies(dependencies)
    }

    /// Depend on plugins. The configuration you pass here may not necessarily
    /// be the final configuration that gets used. If you have requirements on
    /// the configuration of a plugin, consider calling the method
    /// [`AddDependency::with_approval`] on your dependency.
    pub fn add_dependencies<P: AddDependency>(&mut self, dependencies: P) -> &mut Self {
        dependencies.add_to_output(self);
        self
    }

    #[must_use]
    pub fn resource_builder<'a, R: Resource>(&'a mut self) -> ResourceBuilder<'a, R> {
        ResourceBuilder {
            resource: None,
            approval: None,
            output: self,
            is_default: true,
        }
    }

    pub fn insert_resource<R: Resource>(&mut self, resource: R) -> &mut Self {
        self.resource_builder().with_data(resource).build()
    }

    #[must_use]
    pub fn dependency_builder<'a, P>(&'a mut self) -> DependencyBuilder<'a, P> {
        DependencyBuilder {
            config: None,
            approval: None,
            output: self,
        }
    }

    pub fn add_schedule<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        &mut self,
        label: S,
        relative_to: BeforeOrAfter<O>,
    ) {
        self.schedule_labels
            .push(StagedScheduleLabel::new(label, relative_to).unwrap());
    }

    pub fn add_schedule_before<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        &mut self,
        label: S,
        before: O,
    ) {
        self.schedule_labels
            .push(StagedScheduleLabel::new(label, BeforeOrAfter::Before(before)).unwrap());
    }

    pub fn add_schedule_after<S: ScheduleLabel + 'static, O: ScheduleLabel + 'static>(
        &mut self,
        label: S,
        after: O,
    ) {
        self.schedule_labels
            .push(StagedScheduleLabel::new(label, BeforeOrAfter::After(after)).unwrap());
    }
}

pub struct ResourceBuilder<'a, R> {
    resource: Option<R>,
    approval: Option<Approval<ErasedResource>>,
    output: &'a mut PluginOutput,
    is_default: bool,
}

impl<'a, R> ResourceBuilder<'a, R> {
    pub fn build(self) -> &'a mut PluginOutput
    where
        R: Resource,
    {
        let type_id = ResourceTypeId::of::<R>();
        let Self {
            resource,
            approval,
            output,
            is_default,
        } = self;
        let approval = match approval {
            Some(f) => f,
            None => Approval::always_approve(),
        };
        output.resource_staging.insert(
            type_id,
            StagedResource::new(resource, is_default, approval).unwrap(),
        );
        output
    }

    #[must_use]
    pub fn with_data(self, resource: R) -> Self where {
        ResourceBuilder {
            resource: Some(resource),
            approval: None,
            output: self.output,
            is_default: false,
        }
    }

    #[must_use]
    pub fn with_default(mut self) -> Self
    where
        R: Default,
    {
        self.resource = Some(R::default());
        self.is_default = true;
        self
    }

    #[must_use]
    pub fn with_approval<F: Fn(&R) -> bool + 'static>(mut self, f: F) -> Self
    where
        R: 'static,
    {
        self.approval = Some(Approval::new(move |erased: &ErasedResource| {
            erased
                .0
                .visit::<R, bool>(|resource| f(resource))
                .unwrap_or(true)
        }));
        self
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
        let type_id = PluginTypeId::of::<P>();
        let Self {
            config,
            approval,
            output,
        } = self;
        output.plugin_approval.remove(&type_id);
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

    #[must_use]
    pub fn with_config(mut self, config: P) -> Self
    where
        P: DeclarativePlugin,
    {
        self.config = Some(config);
        self
    }

    #[must_use]
    pub fn with_approval<F: for<'b> Fn(&'b P) -> bool + 'static>(mut self, approval: F) -> Self
    where
        P: DeclarativePlugin,
    {
        self.approval = Some(Approval::new(
            move |dyn_plugin: &Box<dyn DeclarativePlugin>| {
                let Some(plugin) = <dyn Any>::downcast_ref::<P>(dyn_plugin.as_ref()) else {
                    return false;
                };
                approval(plugin)
            },
        ));
        self
    }
}

pub(crate) struct PluginDependency {
    pub(crate) type_id: PluginTypeId,
    /// An optional pairing of a plugin's data (as initialized by the plugin
    /// depending on it) and an erased function that builds the plugin output
    /// for that dependency.
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
            type_id: PluginTypeId::of::<P>(),
            data,
        }
    }

    pub fn new<P: DeclarativePlugin + 'static>() -> Self {
        Self {
            type_id: PluginTypeId::of::<P>(),
            data: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub struct PluginTypeId(TypeId);

impl PluginTypeId {
    pub(crate) fn of<P: DeclarativePlugin>() -> Self {
        Self(TypeId::of::<P>())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MessageRegistration {
    // A function that actually registers the message type with the App/World.
    pub(crate) registration_func: fn(&mut App) -> &mut App,
}

impl MessageRegistration {
    pub(crate) fn new<T: Message>() -> Self {
        Self {
            registration_func: App::add_message::<T>,
        }
    }
}

pub(crate) struct MergeableSchedule {
    // This isn't ideal, as it does put the 'static requirements on systems,
    // but it will do for now.
    pub(crate) schedules: Vec<StagedSystem>,
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
}
