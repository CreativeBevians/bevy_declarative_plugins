/// A reference implementation for a bevy plugin system that supports
/// dependencies.
use core::any::Any;
pub(crate) mod approval;
pub(crate) mod erased;
pub(crate) mod graph;
pub(crate) mod metadata_ptr;
/// Declarative Plugin public API.
pub mod plugin_data;
pub use crate::graph::PluginListExpansionError;
use crate::graph::PluginResolutionError;
use crate::graph::{ItemGraphError, PluginList};
#[allow(unused_imports)]
use bevy_app::{App, Plugin};
pub use erased::resource::ResourceTypeId;
pub use erased::schedule::BeforeOrAfter;
pub use plugin_data::PluginOutput;
pub use plugin_data::PluginTypeId;
pub mod entry_points;

pub struct EditorTooling;

/// A declarative alternative to [`Plugin`].
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
    fn build_render(&self, output: &mut PluginOutput<()>) {}

    /// Registration method for editor tools. This is here largely for
    /// conceptual reasons, [`EditorTooling`] does not do anything currently.
    ///
    /// TODO: Have this be used at all.
    #[allow(unused_variables)]
    fn build_editor(&self, output: &mut EditorTooling) {}

    /// When this is a zero-sized type, it will give the same [`PluginOutput`]
    /// every time [`DeclarativePlugin::build`] is called.
    fn zero_sized_instances_are_identical(&self) -> bool {
        true
    }
}

pub trait DeclrAppExt {
    fn apply_declarative(
        &mut self,
        entry_point_list: PluginList,
    ) -> Result<(), DeclarativePluginError>;
}

#[derive(Debug)]
pub enum DeclarativePluginError {
    ExpansionError(PluginListExpansionError),
    ResolutionError(PluginResolutionError),
    ItemGraphError(ItemGraphError),
}

impl DeclrAppExt for App {
    fn apply_declarative(
        &mut self,
        entry_point_list: PluginList,
    ) -> Result<(), DeclarativePluginError> {
        let graph = entry_point_list
            .expand()
            .map_err(DeclarativePluginError::ExpansionError)?;
        let mut graph = graph
            .build()
            .map_err(|(err, _)| DeclarativePluginError::ItemGraphError(err))?;
        let final_order = graph
            .build()
            .map_err(DeclarativePluginError::ResolutionError)?;
        final_order.apply_to_app(self);
        Ok(())
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
        DeclarativePlugin, DeclrAppExt, entry_points::EntryPoints, graph::PluginListExpansionError,
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
        let order = expanded.build().unwrap().build().unwrap();
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
        let order = expanded.build().unwrap().build().unwrap();
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
        let order = expanded.build().unwrap().build().unwrap();
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
        let order = expanded.build().unwrap().build().unwrap();
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
            output
                .resource_builder()
                .with_data(TestResourceA(67))
                .build();
        }
    }

    struct ResourceCheckL;

    impl DeclarativePlugin for ResourceCheckL {
        fn build(&self, output: &mut crate::PluginOutput) {
            output
                .resource_builder()
                .with_data(TestResourceA(20))
                .with_approval(|data| data.0 % 2 == 1)
                .build();
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
            output
                .resource_builder()
                .with_data(TestResourceA(100))
                .with_approval(|_| false)
                .build();
        }
    }

    struct ResourceFailureCheckO;

    impl DeclarativePlugin for ResourceFailureCheckO {
        fn build(&self, output: &mut crate::PluginOutput) {
            output
                .resource_builder()
                .with_data(TestResourceA(100))
                .with_approval(|_| false)
                .build();
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
            output
                .dependency_builder()
                .with_config(InfiniteB { num: self.num + 1 })
                .with_approval(move |inf_b| inf_b.num > num)
                .build();
        }
    }

    pub struct InfiniteB {
        num: usize,
    }

    impl DeclarativePlugin for InfiniteB {
        fn build(&self, output: &mut crate::PluginOutput) {
            let num = self.num;
            output
                .dependency_builder()
                .with_config(InfiniteA { num: self.num + 1 })
                .with_approval(move |inf_a| inf_a.num > num)
                .build();
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
        let order = expanded.build().unwrap().build().unwrap();
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
        let list = LabelsPlugin.get_output();
        let mut app = App::new();
        let result = app.apply_declarative(list);
        assert_matches!(result, Ok(_));
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
        let order = expanded.build().unwrap().build().unwrap();
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
        let list = ObserverPluginA.get_output();
        let mut app = App::new();
        let result = app.apply_declarative(list);
        assert_matches!(result, Ok(_));
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
        let order = expanded.build().unwrap().build().unwrap();
        assert_eq!(order.observers().count(), 1);
    }
}
