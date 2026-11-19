#![warn(missing_docs)]
//! A reference implementation for a bevy plugin system that supports
//! dependencies.
//!
//! The primary change you need to make to integrate this library into your
//! existing bevy code is to implement [`DeclarativePlugin`] rather than
//! [`Plugin`] for your plugin types, then call
//! [`DeclrAppExt::add_plugins_declarative`] to register your plugins.
//!
//! I.e. If you have the following plugin implementation of a plugin:
//!
//! ```ignore
//! pub struct InputManager;
//!
//! impl Plugin for InputManager {
//!     fn build(&self, app: &mut App) {
//!         app.add_systems(PreUpdate, my_input_handler);
//!     }
//! }
//!
//! fn my_input_handler(world: &mut World) {...}
//!
//! fn main() {
//!     let mut app = App::new();
//!     app.add_plugins(InputManager).run();
//! }
//! ```
//!
//! This should be rewritten as the following for a declarative plugin:
//!
//! ```ignore
//! pub struct InputManager;
//!
//! impl DeclarativePlugin for InputManager {
//!     fn build(&self, output: &mut PluginOutput) {
//!         output.add_systems(PreUpdate, my_input_handler);
//!     }
//! }
//!
//! fn my_input_handler(world: &mut World) {...}
//!
//! fn main() {
//!     let mut app = App::new();
//!     app.add_plugins_declarative(InputManager).unwrap().run();
//! }
//! ```
//!
//! Relatively simple change. Most of the item-registration API is the same,
//! with the exception of Schedules and States. See
//! [`PluginOutput::add_schedule_label`] and [`PluginOutput::init_state`] for
//! details there.
//!
//! # Dependencies
//!
//! Right now, in vanilla Bevy, if you want a plugin you author to rely on
//! another plugin, you need to be certain it's only added from a single place.
//!
//! If the plugin you depend on is added a second time, the program crashes.
//! Even if it didn't crash and you were adding a plugin with fields then your
//! configuration might not be respect by an earlier or later registration. Or,
//! a plugin being registered twice would mean that all its items are
//! registered twice too.
//!
//! This implementation aims to look as similar to the previous way of doing
//! plugins as possible while making dependencies behave as you'd expect them
//! to, just working out of the box. To that end, adding a dependency looks
//! like the following:
//!
//! ```ignore
//! use bevy_3rd_party_crate::3rdPartyDeclarativePlugin;
//!
//! pub struct MyPlugin;
//!
//! impl DeclarativePlugin for MyPlugin {
//!     fn build(&self, output: &mut PluginOutput) {
//!         output.add_plugins(3rdPartyDeclarativePlugin);
//!     }
//! }
//! ```
//!
//! ## Approvals
//!
//! Some level of control over which instance of a plugin is used is moved
//! onto the solver. This is necessary to "solve" which instance of a plugin
//! to use when a plugin is not a zero-sized type (even if in practice most
//! plugins are ZSTs).
//!
//! In the case where plugin configuration is critical, you can specify what
//! you _need_ as a plugin / app author like so:
//!
//! ```ignore
//! output.add_plugins(
//!     // Non-zero-sized-type plugin with config we care about.
//!     PhysicsEnginePlugin::with_solver(Solver::Xpbd)
//!         // This plugin only approves of other instances of the physics engine
//!         // plugin if the correct solver is being used.
//!         .with_approval(|engine| engine.solver == Solver::Xpbd)
//! );
//! ```
//!
//! Because not all plugins are zero-sized types, multiple plugins can register
//! conflicting configurations of the same plugin. We need a way for plugin
//! authors and game developers to specify what constraints they need to put on
//! the configuration of a plugin (if any).
//!
//! "Approval" functions is this framework's way of doing this. These are
//! `Fn(&T) -> bool` functions that decide if the plugin that requires a
//! dependency "approves" of the configuration from another source.
//!
//! This serves a similar purpose to how we introduce version requirements to
//! dependencies in `Cargo.toml` files, like `bevy = "0.20"` being a function
//! for choosing which bevy version to use: should match 0.20, patch versions
//! don't matter. We can translate this to the way we do things here as:
//!
//! ```ignore
//! // How `Cargo.toml` treats `bevy = "0.20"` in its constraint solving.
//! // Illustrative of why we use approval functions to fit constraints in
//! // plugin management here.
//! |bevy_crate| bevy_crate.version.major == 0 && bevy_crate.version.minor == 20;
//!
//! // A more constrained `bevy = "=0.15.0"`
//! |bevy_crate| bevy_crate.version.major == 0
//!     && bevy_crate.version.minor == 15
//!     && bevy_crate.version.patch == 0;
//! ```
//!
//! # Resources
//!
//! Plugins can also register resources. Resources are much more likely to be
//! modified during runtime, but there can still be cases where the starting
//! value of a resource is critical and there are disagreements between plugins
//! registering the same resource.
//!
//! You can use this same Approval feature to put constraints on what you
//! need from the initial state of a resource as a plugin author, just like
//! with a plugin dependency you rely on:
//!
//! ```ignore
//! #[derive(Debug, Resource)]
//! struct MyResource { num: usize }
//! // ...
//! output.add_resource(MyResource { num: 7 }.with_approval(|res| res.num >= 6));
//! ```
//!
//! # Supported Items
//!
//! "Items" are the pieces of functionality and data we want to introduce to a
//! Bevy `World`. The currently supported items are:
//!
//! - Systems
//! - Resources
//!     - Resources, like plugins, can be registered by multiple sources and
//!       need to be "solved" in a similar way.
//! - Schedule Labels
//!     - These are slightly different than usual! Using [`BeforeOrAfter`] to
//!       specify where these labels should be placed in the schedule graph.
//!       See [`PluginOutput::add_schedule_label`] for details.
//! - States (under the `state` flag, using resources)
//! - Messages
//! - Observers
//! - Dependencies on other Declarative Plugins
//!
//! This project has largely been developed while reflecting on my own use of
//! Bevy and the way my project has sprawled over time. I'm eager to hear the
//! requirements and constraints of other users and plugin authors of the Bevy
//! engine and how they aren't met by this design yet.
//!
//! # `RenderApp` and Editor tooling
//!
//! [`DeclarativePlugin`] has two methods that are marked deprecated that
//! represent a possible way to expose both the `RenderApp` and future editor
//! extension APIs.
//!
//! For `RenderApp` / Render world, we expose a [`PluginOutput<()>`]: a version
//! of [`PluginOutput`] that cannot register dependencies. This communicates
//! that the render world is a secondary concern for plugin developers, and
//! that operations being performed on it are not happening to the main world.
//!
//! The main issue with exposing these endpoints as trait methods is that they
//! are not _extensible_. One can imagine something built atop bevy that needs
//! access to information during or after solving time that is neither about
//! [`RenderApp`] or engine tooling, and this rigidity does not allow that
//! case.
//!
//! An alternative to this could be that each plugin stores some kind list of
//! dyn compatible "`PluginContext`" type with the ability to register
//! post-solve hooks to determine what to do. This a minor detail, though.
//!
//! # Future work
//!
//! Having a space between "Items registered" and "Items added to world" lets
//! us do things like:
//!
//! - Choose to be a language server for the scene format.
//! - Expose information to tooling.
//! - Intercept hot-patching signals and modify the running world based on
//!   what has changed, allowing new systems, observers, plugins, messages etc.
//!   to be introduced or modified at runtime.
//!
//! None of this is currently implemented, but the infrastructure here makes
//! this kind of thing possible.
//!
//! # How Candidate Plugins are Chosen
//!
//! The algorithm for choosing which instance of conflicting, non-zst plugins
//! gets used is (loosely) as follows:
//!
//! 1. A plugin must pass all approval functions that exist for it to be
//!    picked.
//! 2. The plugin closest to the "root" of the tree that passes the approval
//!    functions will be chosen.
//!
//! The "root" of the dependency tree is whatever is added at
//! [`DeclrAppExt::add_plugins_declarative`], as this is the single entry point
//! an app developer has more control over than a plugin developer.
//!
//! # Upstreaming
//!
//! This project aims to be upstreamed by Bevy, largely because without
//! compatibility with the broader ecosystem it serves as an internal
//! dependencies tool instead of an enabler of better ecosystem tooling.
//! If you want to develop a 3rd party crate with "deep" dependencies, reach
//! out to crate authors to put support under a flag.
use core::any::Any;
pub(crate) mod approval;
pub(crate) mod erased;
pub(crate) mod graph;
pub(crate) mod metadata_ptr;
/// Declarative Plugin public API.
pub mod plugin_data;
use crate::entry_points::EntryPoints;
use crate::graph::ItemGraphError;
pub use crate::graph::PluginListExpansionError;
use crate::graph::PluginResolutionError;
use bevy_app::AppExit;
#[allow(unused_imports)]
use bevy_app::{App, Plugin};
// Need to re-export this for error types.
pub use erased::resource::ResourceTypeId;
pub use erased::schedule::BeforeOrAfter;
pub use plugin_data::PluginOutput;
pub use plugin_data::PluginTypeId;
pub mod entry_points;

/// A declarative alternative to [`Plugin`]. Rather than modifying an [`App`],
/// you modify a [`PluginOutput`] which is then processed later down the line.
///
/// The methods of [`PluginOutput`] mirror much of what people access during
/// plugin registration, but doesn't allow direct access to `World` before
/// plugins have been resolved.
///
/// For applying things to the Render app, use the
/// [`DeclarativePlugin::build_render`] method. This method cannot add
/// dependencies but can otherwise still add most items.
pub trait DeclarativePlugin: Any {
    /// Plugin registration function. Intentionally close to [`Plugin::build`].
    fn build(&self, output: &mut PluginOutput);

    /// Registration method for information relevant to the render world.
    ///
    /// This version of [`PluginOutput`] does not have any dependencies, but can
    /// otherwise still do everything else that a plugin output can.
    ///
    /// If you need dependencies, register them in the
    /// [`DeclarativePlugin::build`] method for your plugin.
    ///
    /// TODO: Have this be used at all.
    #[allow(unused_variables)]
    #[deprecated(
        since = "0.0.1",
        note = "Not actually used yet. Another design may be preferable."
    )]
    fn build_render(&self, output: &mut PluginOutput<()>) {}

    /// Registration method for editor tools. This is here largely for
    /// conceptual reasons, [`EditorTooling`] does not do anything currently.
    ///
    /// TODO: Have this be used at all.
    #[allow(unused_variables)]
    #[deprecated(
        since = "0.0.1",
        note = "Not actually used yet. Another design may be preferable."
    )]
    fn build_editor(&self, output: &mut EditorTooling) {}

    /// When this is a zero-sized type, it will give the same [`PluginOutput`]
    /// every time [`DeclarativePlugin::build`] is called.
    fn zero_sized_instances_are_identical(&self) -> bool {
        true
    }
}

/// A stand-in for an API that can expose information and behavior to an editor
/// or equivalent tooling.
pub struct EditorTooling;

/// A "finalized" app, used here to deter calling
/// [`DeclrAppExt::add_plugins_declarative`] more than once.
#[derive(Debug)]
pub struct FinalizedApp(App);

impl FinalizedApp {
    /// Run the underlying app.
    pub fn run(&mut self) -> AppExit {
        self.0.run()
    }

    /// Get the [`App`] back. This is marked unsafe as an app that has already
    /// had declarative plugins applied to it will run into undefined behavior
    /// if the [`DeclrAppExt::add_plugins_declarative`] is called a second
    /// time. If you need to add more plugins at the entry point site, either
    /// nest tuples (i.e. `((A, B, C), (D, E, F))`) or move things to
    /// dependencies of plugins you manage.
    ///
    /// SAFETY: Do not run [`DeclrAppExt::add_plugins_declarative`] a second
    /// time after calling this function.
    pub unsafe fn extract_app(self) -> App {
        self.0
    }
}

/// Extension trait for [`App`].
pub trait DeclrAppExt {
    /// Add a declarative set of plugins to the app, solving for dependencies
    /// etc.
    ///
    /// NOTE: This should only be called once per instance of [`App`].
    #[must_use]
    fn add_plugins_declarative(
        self,
        entry_point_list: impl EntryPoints,
    ) -> Result<FinalizedApp, DeclarativePluginError>;
}

/// Errors that can occur during declarative plugin registration.
#[derive(Debug)]
pub enum DeclarativePluginError {
    /// The plugin list could not be expanded from the list of plugins.
    ExpansionError(PluginListExpansionError),
    /// There was a missing candidate for a plugin or resource type.
    ResolutionError(PluginResolutionError),
    /// Plugin selection failed in some way.
    ItemGraphError(ItemGraphError),
}

impl DeclrAppExt for App {
    fn add_plugins_declarative(
        mut self,
        entry_point_list: impl EntryPoints,
    ) -> Result<FinalizedApp, DeclarativePluginError> {
        let graph = entry_point_list
            .get_output()
            .expand()
            .map_err(DeclarativePluginError::ExpansionError)?;
        let mut graph = graph
            .build()
            .map_err(|(err, _)| DeclarativePluginError::ItemGraphError(err))?;
        let final_order = graph
            .try_finalize()
            .map_err(DeclarativePluginError::ResolutionError)?;
        final_order.apply_to_app(&mut self);
        Ok(FinalizedApp(self))
    }
}

#[cfg(test)]
mod test {
    use std::assert_matches;

    use bevy_app::{App, Startup, Update};
    use bevy_ecs::{
        event::Event, message::Message, observer::On, resource::Resource, schedule::ScheduleLabel,
        system::Commands,
    };

    use crate::{
        DeclarativePlugin, DeclrAppExt,
        entry_points::{AddDependency, EntryPoints},
        erased::resource::AddResource,
        graph::PluginListExpansionError,
    };

    struct ShallowPluginA;

    impl DeclarativePlugin for ShallowPluginA {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_systems(Update, || {});
        }
    }

    struct ShallowPluginB;

    impl DeclarativePlugin for ShallowPluginB {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_systems(Update, || {});
        }
    }

    #[test]
    fn count_systems() {
        let list = (ShallowPluginA, ShallowPluginB).get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let order = expanded.build().unwrap().try_finalize().unwrap();
        assert_eq!(order.systems().count(), 2);
    }

    #[derive(Debug)]
    struct DeeperPluginC;

    impl DeclarativePlugin for DeeperPluginC {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_systems(Startup, || println!("{:?}", DeeperPluginC));
            output
                .dependency_builder::<ShallowPluginA>()
                .with_config(ShallowPluginA)
                .build();
        }
    }

    #[derive(Debug)]
    struct DeeperPluginD;

    impl DeclarativePlugin for DeeperPluginD {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_systems(Startup, || println!("{:?}", DeeperPluginD));
            output
                .dependency_builder::<ShallowPluginB>()
                .with_config(ShallowPluginB)
                .build();
        }
    }

    #[test]
    fn count_systems_deep() {
        let list = (DeeperPluginC, DeeperPluginD).get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let order = expanded.build().unwrap().try_finalize().unwrap();
        assert_eq!(order.systems().count(), 4);
    }

    #[test]
    fn systems_actually_run() {}

    #[derive(Debug)]
    struct NonZstPluginE {
        num: usize,
    }

    impl DeclarativePlugin for NonZstPluginE {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_systems(Update, || {});
        }
    }

    #[derive(Debug)]
    struct NonZstPluginF;

    impl DeclarativePlugin for NonZstPluginF {
        fn build(&self, output: &mut crate::PluginOutput) {
            output
                .dependency_builder()
                .with_config(NonZstPluginE { num: 7 })
                .build();
        }
    }

    #[test]
    fn non_zst_plugins() {
        let list = NonZstPluginF.get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let order = expanded.build().unwrap().try_finalize().unwrap();
        assert_eq!(order.systems().count(), 1);
    }

    #[derive(Debug)]
    struct MessierDependenciesG;

    impl DeclarativePlugin for MessierDependenciesG {
        fn build(&self, output: &mut crate::PluginOutput) {
            output
                .dependency_builder()
                .with_config(NonZstPluginE { num: 4 })
                .with_approval(|e| dbg!(e.num % 2 == 0))
                .build();
        }
    }

    #[derive(Debug)]
    struct MessierDependenciesH;

    impl DeclarativePlugin for MessierDependenciesH {
        fn build(&self, output: &mut crate::PluginOutput) {
            output
                .dependency_builder()
                .with_config(NonZstPluginE { num: 40 })
                .with_approval(|e| dbg!(e.num > 30))
                .build();
        }
    }

    struct MessierDependenciesI;

    impl DeclarativePlugin for MessierDependenciesI {
        fn build(&self, output: &mut crate::PluginOutput) {
            output
                .dependency_builder()
                .with_config(NonZstPluginE { num: 1 })
                .build();
        }
    }

    #[test]
    fn messier_dependencies() {
        let list = (
            MessierDependenciesG,
            MessierDependenciesH,
            MessierDependenciesI,
        )
            .get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let order = expanded.build().unwrap().try_finalize().unwrap();
        assert_eq!(order.systems().count(), 1);
    }

    struct ContraDependencyJ;

    impl DeclarativePlugin for ContraDependencyJ {
        fn build(&self, output: &mut crate::PluginOutput) {
            output
                .dependency_builder()
                .with_config(NonZstPluginE { num: 2 })
                .with_approval(|j| j.num == 2)
                .build();
        }
    }

    #[test]
    fn should_fail_dependency() {
        let list = (MessierDependenciesH, ContraDependencyJ).get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let graph = expanded.build();
        assert_matches!(graph, Err(_))
    }

    #[derive(Debug, Resource, Default)]
    struct TestResourceA(usize);

    struct ResourceCheckK;

    impl DeclarativePlugin for ResourceCheckK {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_resource(TestResourceA(67));
        }
    }

    struct ResourceCheckL;

    impl DeclarativePlugin for ResourceCheckL {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_resource(TestResourceA(20).with_approval(|data| data.0 % 2 == 1));
        }
    }

    #[test]
    fn resource_selection() {
        let list = (ResourceCheckK, ResourceCheckL).get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let mut graph = expanded.unwrap().build().unwrap();
        let resource_compute_result = graph.compute_resources();
        assert_matches!(resource_compute_result, Ok(_));
        let order = graph.final_items_order();
        assert_eq!(order.resources().count(), 1);
        let mut app = App::new();
        order.apply_to_app(&mut app);
        let resource = app
            .world_mut()
            .get_resource_or_insert_with(|| TestResourceA(0));
        assert_eq!(resource.0, 67)
    }

    struct ResourceCheckM;

    impl DeclarativePlugin for ResourceCheckM {
        fn build(&self, output: &mut crate::PluginOutput) {
            output
                .dependency_builder()
                .with_config(ResourceCheckK)
                .build();
        }
    }

    #[test]
    fn resource_dependency() {
        let list = ResourceCheckM.get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let mut graph = expanded.unwrap().build().unwrap();
        let resource_compute_result = graph.compute_resources();
        assert_matches!(resource_compute_result, Ok(_));
        let order = graph.final_items_order();
        assert_eq!(order.resources().count(), 1);
        let mut app = App::new();
        order.apply_to_app(&mut app);
        let resource = app
            .world_mut()
            .get_resource_or_insert_with(|| TestResourceA(0));
        assert_eq!(resource.0, 67)
    }

    struct ResourceFailureCheckN;

    impl DeclarativePlugin for ResourceFailureCheckN {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_resource(TestResourceA(100).with_approval(|_| false));
        }
    }

    struct ResourceFailureCheckO;

    impl DeclarativePlugin for ResourceFailureCheckO {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_resource(TestResourceA(100).with_approval(|_| false));
        }
    }

    #[test]
    fn resource_trivial_failure() {
        let list = (ResourceFailureCheckN, ResourceFailureCheckO).get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let mut graph = expanded.unwrap().build().unwrap();
        let resource_compute_result = graph.compute_resources();
        assert_matches!(resource_compute_result, Err(_));
    }

    pub struct InfiniteA {
        num: usize,
    }

    impl DeclarativePlugin for InfiniteA {
        fn build(&self, output: &mut crate::PluginOutput) {
            let num = self.num;
            output.add_plugins(
                InfiniteB { num: self.num + 1 }.with_approval(move |inf_b| inf_b.num > num),
            );
        }
    }

    pub struct InfiniteB {
        num: usize,
    }

    impl DeclarativePlugin for InfiniteB {
        fn build(&self, output: &mut crate::PluginOutput) {
            let num = self.num;
            output.add_plugins(
                InfiniteA { num: self.num + 1 }.with_approval(move |inf_a| inf_a.num > num),
            );
        }
    }

    #[test]
    fn infinite_dependency_fails() {
        let list = InfiniteA { num: 0 }.get_output();
        let expanded = list.expand();
        assert_matches!(
            expanded,
            Err(PluginListExpansionError::TooManyPluginInstances(_))
        );
    }

    #[derive(Debug, Message)]
    struct MessageA;

    #[derive(Debug, Message)]
    struct MessageB;

    #[derive(Debug, Message)]
    struct MessageC;

    #[derive(Debug)]
    struct MessagesPluginA;

    impl DeclarativePlugin for MessagesPluginA {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_message::<MessageA>().add_message::<MessageB>();
        }
    }

    #[derive(Debug)]
    struct MessagesPluginB;

    impl DeclarativePlugin for MessagesPluginB {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_message::<MessageB>().add_message::<MessageC>();
        }
    }

    #[test]
    fn messages() {
        let list = (MessagesPluginA, MessagesPluginB).get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let order = expanded.build().unwrap().try_finalize().unwrap();
        assert_eq!(order.messages().count(), 3);
    }

    #[derive(Debug, ScheduleLabel, Hash, PartialEq, PartialOrd, Ord, Eq, Clone)]
    struct MyLabelA;

    #[derive(Debug, ScheduleLabel, Hash, PartialEq, PartialOrd, Ord, Eq, Clone)]
    struct MyLabelB;

    #[derive(Debug, Resource, Default)]
    struct LabelTestResource(usize);

    #[derive(Debug)]
    struct LabelsPlugin;

    impl DeclarativePlugin for LabelsPlugin {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_schedule_after(MyLabelA, Startup);
            output.add_schedule_after(MyLabelB, MyLabelA);
            output.add_systems(MyLabelB, |mut commands: Commands| {
                commands.insert_resource(LabelTestResource(4));
            });
        }
    }

    #[test]
    fn labels() {
        let app = App::new();
        let result = app.add_plugins_declarative(LabelsPlugin);
        assert_matches!(result, Ok(_));
        let mut app = result.unwrap().0;
        app.world_mut().run_schedule(MyLabelB);
        app.world_mut().flush();
        let result = app.world_mut().get_resource_or_init::<LabelTestResource>();
        assert_eq!(result.0, 4);
    }

    #[test]
    fn count_labels() {
        let list = LabelsPlugin.get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let order = expanded.build().unwrap().try_finalize().unwrap();
        assert_eq!(order.labels().count(), 2);
    }

    #[derive(Event)]
    struct ObserverEntityEvent;

    struct ObserverPluginA;

    impl DeclarativePlugin for ObserverPluginA {
        fn build(&self, output: &mut crate::PluginOutput) {
            output.add_systems(Startup, |mut commands: Commands| {
                commands.trigger(ObserverEntityEvent);
            });
            output.add_observer(|_on: On<ObserverEntityEvent>, mut commands: Commands| {
                commands.insert_resource(LabelTestResource(4));
            });
        }
    }

    #[test]
    fn observers() {
        let app = App::new();
        let result = app.add_plugins_declarative(ObserverPluginA);
        assert_matches!(result, Ok(_));
        let mut app = result.unwrap().0;
        app.world_mut().run_schedule(Startup);
        app.world_mut().flush();
        let result = app.world_mut().get_resource_or_init::<LabelTestResource>();
        assert_eq!(result.0, 4);
    }

    #[test]
    fn count_observers() {
        let list = ObserverPluginA.get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let order = expanded.build().unwrap().try_finalize().unwrap();
        assert_eq!(order.observers().count(), 1);
    }
}
