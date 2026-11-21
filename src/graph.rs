//! Module containing the process of turning the "Entry point" plugins
//! into a dependency graph, solving that dependency graph, then turning the
//! solved graph into a list of items to add to a Bevy world.
//!
//! The core types and transforms for this process are:
//!
//! 1. [`EntryPoints`] -> [`PluginList`], turning a (potentially nested) tuple
//!    of plugins into the list of root plugins we care most about.
//! 2. [`PluginList`] -> [`PluginRegistrationGraph`], expanding that list of
//!    plugins into a graph.
//! 3. [`PluginRegistrationGraph`] -> [`DependenciesSolvedGraph`], solving
//!    dependencies and constraints from the registration graph to choose the
//!    right output to use.
//! 4. [`DependenciesSolvedGraph`] -> [`OrderedPluginItems`], turning that
//!    solved graph into an ordered list of items to register with a world.
//!
//!
use bevy_ecs::observer::Observer;
use bevy_platform::collections::HashMap;
use core::{convert::identity, hash::Hash};
use std::{
    boxed::Box,
    collections::{BTreeSet, VecDeque},
    fmt::Debug,
    vec::Vec,
};

use crate::{
    DeclarativePlugin, PluginOutput,
    approval::Approval,
    entry_points::EntryPoints,
    erased::{
        resource::{ResourceTypeId, StagedResource},
        schedule::{StagedScheduleLabel, StagedSystem},
    },
    plugin_data::{MessageRegistration, PluginDependency, PluginTypeId},
};

/// Error type for problems during the breadth-first "Expansion" phase.
#[derive(Debug)]
pub enum PluginListExpansionError {
    /// Too many instances of a plugin were found. This can be caused by a very
    /// deep dependency graph, or it can more likely be caused by a nontrivial
    /// cycle of plugins which register conflicting and ever-changing
    /// configurations. Consider the following:
    ///
    /// ```
    /// use bevy_declarative_plugins::{DeclarativePlugin, PluginOutput};
    /// use bevy_declarative_plugins::entry_points::*;
    ///
    /// pub struct InfiniteA {
    ///     num: usize,
    /// }
    ///
    /// impl DeclarativePlugin for InfiniteA {
    ///     fn build(&self, output: &mut PluginOutput) {
    ///         let num = self.num;
    ///         output.add_plugins(InfiniteB { num: self.num + 1 }.with_approval(move |inf_b| inf_b.num > num));
    ///     }
    /// }
    ///
    /// pub struct InfiniteB {
    ///     num: usize,
    /// }
    ///
    /// impl DeclarativePlugin for InfiniteB {
    ///     fn build(&self, output: &mut PluginOutput) {
    ///         let num = self.num;
    ///         output.add_plugins(InfiniteA { num: self.num + 1 }.with_approval(move |inf_a| inf_a.num > num));
    ///     }
    ///}
    /// ```
    ///
    /// The registering either of the above plugins could cause the graph to
    /// expand forever. Instead of letting this OOM the machine it's running
    /// on, we have a maximum number of instances that can be found before
    /// erroring.
    TooManyPluginInstances(PluginTypeId),
}

/// A list of "entry point" plugins and their outputs. This gets expanded into a graph.
pub struct PluginList {
    pub(crate) nodes: Vec<(Box<dyn DeclarativePlugin>, PluginOutput)>,
}

impl PluginList {
    /// Add a set of entry points to the existing list.
    pub(crate) fn extend_with<P: EntryPoints>(&mut self, entry_point: P) -> &mut Self {
        entry_point.add_to_list(self);
        self
    }

    /// Expand the list of entry point plugins into a full graph of possible
    /// plugin candidates. Ignores recurring ZSTs and errors on too many
    /// instances of a given type to prevent infinite expansion. See:
    /// [`PluginListExpansionError::TooManyPluginInstances`].
    pub(crate) fn expand(mut self) -> Result<PluginRegistrationGraph, PluginListExpansionError> {
        let mut zst_already_expanded: HashMap<PluginTypeId, InstanceId> = HashMap::new();
        let mut graph = PluginRegistrationGraph::new(250);
        for (_, output) in &mut self.nodes {
            // Mark entry points.
            output.is_entry_point = true;
        }
        let mut dependency_queue = VecDeque::new();
        for (item, output) in self.nodes {
            let is_new_zst = output.is_zero_sized_optimizable
                && !zst_already_expanded.contains_key(&output.working_plugin);
            if is_new_zst || !output.is_zero_sized_optimizable {
                let type_id = output.working_plugin;
                let (reg_id, dependencies) = graph.insert_node(item, output, None);
                dependency_queue.extend(dependencies.0.into_iter().map(|d| (reg_id, d)));
                if is_new_zst {
                    // If it's a ZST, we need to add it to the list of known ones.
                    zst_already_expanded.insert(type_id, reg_id);
                }
            } else {
                // This should be "ZSTs already registered." Do nothing.
            }
        }
        // We avoid infinite expansion by having there be a limit to the number
        // of instances of a plugin in the graph.
        while let Some((added_by, dependency)) = dependency_queue.pop_front() {
            let plugin_id = dependency.type_id;
            if graph.count_instances(plugin_id) > graph.max_repeat_instances {
                return Err(PluginListExpansionError::TooManyPluginInstances(plugin_id));
            }
            if !zst_already_expanded.contains_key(&dependency.type_id)
                && let Some((dyn_plugin, output_fn)) = dependency.data
            {
                let Some(output) = output_fn(dyn_plugin.as_ref()) else {
                    continue;
                };
                let plugin_id = output.working_plugin;
                let can_zst_optimize = output.is_zero_sized_optimizable;
                let (reg_id, dependencies) = graph.insert_node(dyn_plugin, output, Some(added_by));
                graph.insert_edge(added_by, plugin_id);
                for dependency in dependencies.0 {
                    // If a dependency already has an existing instance
                    // registered in the graph that is suitable, we discard the
                    // plugin data that was generated from this instance.

                    // We can confidently skip adding that dependency because
                    // the semantics of plugin registration are that we're
                    // asking that a plugin exists and its config is
                    // acceptable, not that a plugin _must_ be a specific value
                    // (unless asked)

                    // This does cause a bit of a problem when it comes to
                    // thinking about how plugins become detached from the
                    // dependencies they do register.
                    if let Some(dependency_source) = graph.get(reg_id)
                        && graph
                            .instances_of_plugin(dependency.type_id)
                            .filter_map(|existing_instance| {
                                let source_approval = dependency_source
                                    .output
                                    .plugin_approval
                                    .get(&dependency.type_id)?;
                                Some(source_approval.approves(&existing_instance.plugin_data))
                            })
                            .any(identity)
                    {
                        // There already exists a suitable version of this
                        // plugin in the graph, so queue up a version of this
                        // dependency that doesn't contain any info other than
                        // the fact that a dependency to that plugin does exist.
                        dependency_queue.push_back((
                            reg_id,
                            PluginDependency {
                                data: None,
                                type_id: dependency.type_id,
                            },
                        ));
                    } else {
                        dependency_queue.push_back((reg_id, dependency));
                    }
                }
                if can_zst_optimize {
                    zst_already_expanded.insert(plugin_id, reg_id);
                }
            } else if dependency.data.is_none()
                && !zst_already_expanded.contains_key(&dependency.type_id)
            {
                // TODO: check if we ended up getting no instances at the end.
                graph.insert_edge(added_by, dependency.type_id);
            } else {
                // ZST, do nothing.
            }
        }
        Ok(graph)
    }
}

/// Sequential identifier for plugin registration.
#[derive(Debug, PartialEq, PartialOrd, Ord, Eq, Hash, Clone, Copy)]
pub(crate) struct InstanceId(usize, PluginTypeId);

impl InstanceId {
    pub(crate) fn plugin_id(&self) -> PluginTypeId {
        self.1
    }
}

/// A node in the expanded graph representing the items a plugin wants to
/// register.
pub(crate) struct PluginInstance {
    /// Sequential registration identifier.
    pub(crate) registration_id: InstanceId,
    /// Type-erased plugin configuration data.
    pub(crate) plugin_data: Box<dyn DeclarativePlugin>,
    /// The items the plugin wants to add to the App.
    pub(crate) output: PluginOutput<()>,
    /// Distance from an entry point.
    pub(crate) distance_from_entry: usize,
}

impl Debug for PluginInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginInstance")
            .field("registration_id", &self.registration_id)
            .field("plugin_data", &"{...}")
            .field("output", &"{...}")
            .field("distance_from_entry", &self.distance_from_entry)
            .finish()
    }
}

/// The expanded, but not yet solved, graph store for plugins.
///
/// Instead of having a "full" node for each plugin instance, we have one node
/// per plugin type (i.e all instances of a plugin `A` will be in the same
/// node.) This reflects how a plugin type can register another plugin type as
/// a dependency but cannot depend on a specific instance. This lets us solve
/// dependencies when we have more than one candidate across the dependency
/// graph.
pub(crate) struct PluginRegistrationGraph {
    registration_counter: usize,
    nodes: HashMap<PluginTypeId, Vec<PluginInstance>>,
    dependency_edges: HashMap<InstanceId, Vec<PluginTypeId>>,
    max_repeat_instances: usize,
}

impl Debug for PluginRegistrationGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginRegistrationGraph")
            .field("registration_counter", &self.registration_counter)
            .field("nodes", &self.nodes.len())
            .field("dependency_edges", &self.dependency_edges)
            .finish()
    }
}

impl PluginRegistrationGraph {
    /// Generate a new, unique [`InstanceId`]. Increments an internal
    /// counter.
    fn new_id(&mut self, plugin_id: PluginTypeId) -> InstanceId {
        let id = InstanceId(self.registration_counter, plugin_id);
        self.registration_counter += 1;
        id
    }

    /// Create a new, empty graph. `max_repeat_instances` dictates how many
    /// instances the graph expansion algorithm will allow before erroring.
    /// See: [`PluginListExpansionError::TooManyPluginInstances`].
    pub(crate) fn new(max_repeat_instances: usize) -> Self {
        Self {
            registration_counter: 0,
            nodes: HashMap::new(),
            dependency_edges: HashMap::new(),
            max_repeat_instances,
        }
    }

    /// Count the number of instances of a given plugin type.
    pub(crate) fn count_instances(&self, plugin_id: PluginTypeId) -> usize {
        self.nodes
            .get(&plugin_id)
            .map(|instances| instances.len())
            .unwrap_or(0)
    }

    #[must_use]
    pub(crate) fn insert_node<D>(
        &mut self,
        plugin_data: Box<dyn DeclarativePlugin>,
        output: PluginOutput<D>,
        added_by: Option<InstanceId>,
    ) -> (InstanceId, D) {
        let plugin_id = output.working_plugin;
        let registration_id = self.new_id(plugin_id);
        let (output, dependencies) = output.extract_dependencies();
        let distance_from_entry = if output.is_entry_point {
            0
        } else if let Some(added_by) = added_by
            && let Some(source_instance) = self.get(added_by)
        {
            source_instance.distance_from_entry + 1
        } else {
            usize::MAX
        };
        let node = PluginInstance {
            registration_id,
            plugin_data,
            output,
            distance_from_entry,
        };
        self.nodes.entry(plugin_id).or_default().push(node);
        (registration_id, dependencies)
    }

    pub(crate) fn build(self) -> Result<DependenciesSolvedGraph, (ItemGraphError, Self)> {
        DependenciesSolvedGraph::new(self)
    }

    pub(crate) fn can_build(&self) -> bool {
        // We can't build the next graph unless each plugin that is a
        // dependency has a valid candidate.
        self.nodes
            .iter()
            .filter(|(plugin_id, _)| self.is_depended_on(**plugin_id))
            .all(|(plugin_id, _)| self.candidate_exists(*plugin_id).is_ok())
    }

    pub(crate) fn is_depended_on(&self, plugin_id: PluginTypeId) -> bool {
        self.dependency_edges
            .values()
            .any(|ids| ids.contains(&plugin_id))
    }

    /// Removes the candidates from the graph.
    pub(crate) fn extract_candidates(
        &mut self,
    ) -> Result<
        (
            Vec<PluginInstance>,
            HashMap<PluginTypeId, Vec<PluginTypeId>>,
        ),
        (),
    > {
        let candidate_ids = self.naive_candidates()?;
        let mut extracted = Vec::with_capacity(candidate_ids.len());
        let mut edges = HashMap::new();
        for candidate in candidate_ids {
            let entry = self.nodes.entry(candidate.plugin_id()).or_default();
            let ix = entry
                .iter()
                .enumerate()
                .find_map(|(ix, node)| (node.registration_id == candidate).then_some(ix))
                .expect("Candidate finding function found complete list, but entries are missing.");
            extracted.push(entry.remove(ix));
            if let Some(dependencies) = self.dependency_edges.get(&candidate) {
                edges.insert(candidate.plugin_id(), dependencies.clone());
            }
        }
        Ok((extracted, edges))
    }

    /// Get the registration IDs of each valid candidate.
    /// Called naive here because it doesn't cull valid candidates that were
    /// added but aren't depended on by anything.
    pub(crate) fn naive_candidates(&self) -> Result<Vec<InstanceId>, ()> {
        let mut candidates = Vec::new();
        for plugin_id in self.plugin_ids() {
            // Expensive! There is almost definitely a faster way of doing this
            // while building the graph rather than at the end. Or maybe it's
            // fine.
            candidates.push(self.candidate_exists(plugin_id)?);
        }
        Ok(candidates)
    }

    pub(crate) fn candidate_exists(&self, plugin_id: PluginTypeId) -> Result<InstanceId, ()> {
        let mut working = None;
        // We have two loops here, the outer loop is to go through each candidate.
        // The inner loop is to find out if there is any version of this
        'outer: for candidate_instance in self.instances_of_plugin(plugin_id) {
            let candidate = candidate_instance.registration_id;
            // For each plugin with a dependency, see if each of those dependents agree on this configuration.
            for depends_on in self.has_approvals_for_plugin(plugin_id) {
                match depends_on.output.plugin_approval.get(&plugin_id) {
                    // If an approval function exists, and it approves, continue.
                    Some(approval) => {
                        if approval.approves(&candidate_instance.plugin_data) {
                            continue;
                        } else {
                            continue 'outer;
                        }
                    }
                    // A dependent *should* have an approval function, but if it doesn't
                    // then we assume it's fine.
                    None => continue,
                }
            }
            match &working.and_then(|working| self.get(working)) {
                Some(prev_candidate) => {
                    // Two cases where we write a new candidate when we already
                    // have one:
                    //
                    // 1. Previous instance wasn't an entry point, but this
                    //    instance was.
                    // 2. Both previous instance and current instance were
                    //    entry points, but the current entry point has a
                    //    higher registration id.
                    //
                    // This does mean we have a preference for later entry
                    // points over earlier ones. This could cause some
                    // unexpected behavior and maybe it's better to just
                    // highlight to the user what's gone on and prevent that.
                    if candidate_instance.output.is_entry_point
                        && (!prev_candidate.output.is_entry_point
                            || candidate_instance.registration_id > prev_candidate.registration_id)
                    {
                        working = Some(candidate_instance.registration_id)
                    }
                }
                None => {
                    working = Some(candidate);
                }
            }
        }
        working.ok_or(())
    }

    pub(crate) fn get(&self, registration_id: InstanceId) -> Option<&PluginInstance> {
        self.instances_of_plugin(registration_id.plugin_id())
            .find(|node| node.registration_id == registration_id)
    }

    /// Collects all nodes that have an approval function for their
    /// dependencies.
    pub(crate) fn has_approvals_for_plugin(
        &self,
        plugin_id: PluginTypeId,
    ) -> impl Iterator<Item = &PluginInstance> {
        self.all_instances().filter_map(move |node| {
            let item = node.output.plugin_approval.get(&plugin_id)?;
            if item.is_trivially_true() {
                return None;
            }
            Some(node)
        })
    }

    /// Iterate over every plugin instance in the graph.
    pub(crate) fn all_instances(&self) -> impl Iterator<Item = &PluginInstance> {
        self.nodes.iter().flat_map(|(_, nodes)| nodes.iter())
    }

    /// Iterate over the IDs for each plugin in the graph.
    pub(crate) fn plugin_ids(&self) -> impl Iterator<Item = PluginTypeId> {
        self.nodes.iter().map(|(k, _)| *k)
    }

    /// Iterate over every instance for a given plugin type.
    pub(crate) fn instances_of_plugin(
        &self,
        plugin_id: PluginTypeId,
    ) -> impl Iterator<Item = &PluginInstance> {
        self.nodes
            .get(&plugin_id)
            .into_iter()
            .flat_map(|instances| instances.iter())
    }

    /// All the dependency edges in the graph.
    ///
    /// Note that this is from a specific instance in the graph
    /// ([`InstanceId`]) to a more general plugin type. This reflects
    /// the way dependencies can only specify their needs with an approval
    /// function, rather than demanding a specific instance.
    pub(crate) fn insert_edge(&mut self, from: InstanceId, to: PluginTypeId) {
        self.dependency_edges.entry(from).or_default().push(to);
    }
}

/// Errors that can occur during the process of choosing what plugins or
/// resource to use for a
#[derive(Debug)]
pub enum PluginResolutionError {
    NoResourceCandidate(Vec<(ResourceTypeId, &'static str)>),
    NoPluginCandidate(Vec<(PluginTypeId, &'static str)>),
}

#[derive(Debug)]
pub enum ItemGraphError {
    ///
    FailedBuildCheck,
}

/// A graph representing the post-plugin-choice stage of the solving process.
///
/// What remains to be done is choosing which resources to choose from the
/// remaining instances and then ordering the items for registration.
#[derive(Debug)]
pub(crate) struct DependenciesSolvedGraph {
    sources: HashMap<PluginTypeId, PluginInstance>,
    accepted_resource_sources: HashMap<ResourceTypeId, PluginTypeId>,
}

impl DependenciesSolvedGraph {
    /// Try to construct a [`DependenciesSolvedGraph`].
    pub(crate) fn new(
        mut graph: PluginRegistrationGraph,
    ) -> Result<Self, (ItemGraphError, PluginRegistrationGraph)> {
        if graph.can_build() {
            let (nodes, _) = graph.extract_candidates().unwrap();
            Ok(Self {
                sources: nodes
                    .into_iter()
                    .map(|node| (node.registration_id.plugin_id(), node))
                    .collect(),
                accepted_resource_sources: HashMap::new(),
            })
        } else {
            Err((ItemGraphError::FailedBuildCheck, graph))
        }
    }

    /// Mutably iterate over the instances of plugins in this graph.
    pub(crate) fn instances_mut(&mut self) -> impl Iterator<Item = &mut PluginInstance> {
        self.sources.values_mut()
    }

    /// Try to get an ordered list of bevy items to register. Only fails if
    /// the resources selection can't be computed.
    pub(crate) fn try_finalize(&mut self) -> Result<OrderedPluginItems, PluginResolutionError> {
        self.compute_resources()?;
        Ok(self.final_items_order())
    }

    /// Establish candidates for which resources to use. Uses similar approval
    /// and selection heuristics to plugin selection, but as resources are more
    /// simple than plugins we have less to worry about.
    pub(crate) fn compute_resources(&mut self) -> Result<(), PluginResolutionError> {
        self.accepted_resource_sources.drain();
        let mut conflicting_sources: HashMap<
            ResourceTypeId,
            (BTreeSet<PluginTypeId>, &'static str),
        > = HashMap::new();
        for (source, node) in &self.sources {
            for resource in &node.output.resource_staging {
                let resource_id = resource.0;
                let entry = conflicting_sources.entry(*resource_id).or_default();
                entry.0.insert(*source);
                entry.1 = resource.1.resource_name;
            }
        }
        // If there are any resources without valid candidates, we error.
        let mut no_candidates = Vec::new();
        for (resource_id, (mut source_ids, resource_name)) in conflicting_sources {
            // We use `source_ids` as a list of candidates, removing ones that have hard conflicts as we go.
            if source_ids.len() == 1
                && let Some(source_id) = source_ids.pop_first()
            {
                // At this moment in time, a single instance of a resource will always be accepted.
                // This doesn't make much sense, and should be changed.
                self.accepted_resource_sources
                    .insert(resource_id, source_id);
            } else {
                let approve_all = Approval::always_approve();
                let mut to_remove = BTreeSet::new();
                for (_, plugin_node) in self
                    .sources
                    .iter()
                    .filter(|(plugin_id, _)| source_ids.contains(*plugin_id))
                {
                    let approval = plugin_node
                        .output
                        .resource_staging
                        .get(&resource_id)
                        .map(|resource| &resource.approval_from_plugin)
                        .unwrap_or(&approve_all);
                    for source_inner in &source_ids {
                        if !to_remove.contains(source_inner)
                            && let Some(staged_resource) =
                                self.sources.get(source_inner).and_then(|node| {
                                    node.output
                                        .resource_staging
                                        .get(&resource_id)
                                })
                            // If this staged resource doesn't have an erased resource
                            // we can't establish it as a candidate anyway.
                            && let Some(erased) = staged_resource.get_erased()
                            && approval.approves(erased)
                        {
                            // We can keep this as the following has been met:
                            // 1. It's not already marked for removal
                            // 2. The resource type is present.
                            // 3. The approval function for the outer loop passes.
                            // Omission is acceptance.
                        } else {
                            // Otherwise, we add it to the removal list.
                            to_remove.insert(*source_inner);
                        }
                    }
                }
                if source_ids.len() == to_remove.len() || source_ids.is_empty() {
                    // There are no valid resources, accumulate an error.
                    no_candidates.push((resource_id, resource_name));
                    continue;
                }
                let mut candidates = source_ids.difference(&to_remove);
                // We can unwrap because of the length check just before here.
                let mut ongoing_candidate = candidates.next().unwrap();
                // Now  we have candidates, we choose one closest to the "root"
                // of the tree so that we respect user requirements, preferring non-defaults.
                for compare_candidate in candidates {
                    let (Some(ongoing), Some(compare)) = (
                        self.sources.get(ongoing_candidate),
                        self.sources.get(compare_candidate),
                    ) else {
                        continue;
                    };
                    let ongoing_distance = ongoing.distance_from_entry;
                    let compare_distance = compare.distance_from_entry;
                    if compare_distance < ongoing_distance {
                        ongoing_candidate = compare_candidate;
                    } else if ongoing_distance == compare_distance
                        && let Some(current_instance) = ongoing.output.get_resource(resource_id)
                        && current_instance.is_default
                    {
                        // Identical distance, but the current instance is a default instance.
                        ongoing_candidate = compare_candidate;
                    }
                }
                self.accepted_resource_sources
                    .insert(resource_id, *ongoing_candidate);
            }
        }
        if !no_candidates.is_empty() {
            return Err(PluginResolutionError::NoResourceCandidate(no_candidates));
        }
        Ok(())
    }

    /// Build out the final registration graph from the registration
    /// information we've constructed so far.
    pub(crate) fn final_items_order(&mut self) -> OrderedPluginItems {
        let mut buffer = Vec::new();
        // For messages, we build out a hashmap of values to reduce noisy
        // duplication of message registration if messages are
        let mut messages = HashMap::new();
        self.instances_mut().for_each(|node| {
            node.output.extract_messages_to_hashmap(&mut messages);
        });
        buffer.extend(
            messages
                .into_iter()
                .map(|(_, message)| DeclrItem::Message(message)),
        );
        // The rest are kinda normal.
        self.instances_mut().for_each(|node| {
            node.output.extract_schedules_into_buffer(&mut buffer);
        });
        self.instances_mut().for_each(|node| {
            node.output.extract_observers_into_buffer(&mut buffer);
        });
        self.instances_mut().for_each(|node| {
            node.output.extract_systems_into_buffer(&mut buffer);
        });
        let mut map = HashMap::new();
        std::mem::swap(&mut map, &mut self.accepted_resource_sources);
        self.instances_mut().for_each(|node| {
            node.output.extract_resources_into_buffer(&mut buffer, &map);
        });
        // Swap back in case we use this in debugging / analysis.
        std::mem::swap(&mut map, &mut self.accepted_resource_sources);
        OrderedPluginItems(buffer)
    }
}

/// Items that can be added to a world.
#[allow(unused)]
pub(crate) enum DeclrItem {
    Message(MessageRegistration),
    System(StagedSystem),
    ScheduleLabel(StagedScheduleLabel),
    Observer(Observer),
    Resource(StagedResource),
}

impl Debug for DeclrItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Message(_) => f.debug_tuple("Message").finish(),
            Self::System(_) => f.debug_tuple("System").finish(),
            Self::ScheduleLabel(_) => f.debug_tuple("ScheduleLabel").finish(),
            Self::Observer(_) => f.debug_tuple("Observer").finish(),
            Self::Resource(_) => f.debug_tuple("Resource").finish(),
        }
    }
}

impl DeclrItem {
    pub fn apply_to_app(self, app: &mut bevy_app::App) {
        match self {
            DeclrItem::Message(message_registration) => {
                (message_registration.registration_func)(app);
            }
            DeclrItem::System(staged_system) => {
                staged_system.staged.apply_to_world(app.world_mut());
            }
            DeclrItem::ScheduleLabel(staged_schedule_label) => {
                staged_schedule_label.staged.apply_to_world(app.world_mut());
            }
            DeclrItem::Observer(observer) => {
                app.add_observer(observer);
            }
            DeclrItem::Resource(staged_resource) => staged_resource.apply_to_world(app.world_mut()),
        }
    }
}

/// The final order for things to be registered in.
#[derive(Debug)]
pub(crate) struct OrderedPluginItems(pub(crate) Vec<DeclrItem>);

impl OrderedPluginItems {
    pub(crate) fn apply_to_app(self, app: &mut bevy_app::App) {
        for item in self.0 {
            item.apply_to_app(app);
        }
    }

    #[cfg(test)]
    pub(crate) fn messages(&self) -> impl Iterator<Item = &MessageRegistration> {
        self.0.iter().filter_map(|item| match item {
            DeclrItem::Message(message) => Some(message),
            _ => None,
        })
    }

    #[cfg(test)]
    pub(crate) fn systems(&self) -> impl Iterator<Item = &StagedSystem> {
        self.0.iter().filter_map(|item| match item {
            DeclrItem::System(system) => Some(system),
            _ => None,
        })
    }

    #[cfg(test)]
    pub(crate) fn labels(&self) -> impl Iterator<Item = &StagedScheduleLabel> {
        self.0.iter().filter_map(|item| match item {
            DeclrItem::ScheduleLabel(label) => Some(label),
            _ => None,
        })
    }

    #[cfg(test)]
    pub(crate) fn observers(&self) -> impl Iterator<Item = &Observer> {
        self.0.iter().filter_map(|item| match item {
            DeclrItem::Observer(observer) => Some(observer),
            _ => None,
        })
    }

    #[cfg(test)]
    pub(crate) fn resources(&self) -> impl Iterator<Item = &StagedResource> {
        self.0.iter().filter_map(|item| match item {
            DeclrItem::Resource(resource) => Some(resource),
            _ => None,
        })
    }
}
