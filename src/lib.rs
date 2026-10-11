use core::any::Any;

pub(crate) mod approval;
pub(crate) mod erased;
pub(crate) mod graph;
pub(crate) mod metadata_ptr;
/// Declarative Plugin public API.
pub mod plugin_data;
use bevy_app::App;
pub use plugin_data::PluginOutput;

use crate::{
    DeclarativePluginError::CouldNotExpand,
    graph::{ItemGraphError, PluginList},
};

/// A declarative alternative to [`Plugin`]
pub trait DeclarativePlugin: Any {
    /// Plugin registration function.
    fn build(&self, output: &mut PluginOutput);

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
    ItemGraphError(ItemGraphError),
    CouldNotExpand,
    CouldNotFinalizeOrder,
}

impl DeclrAppExt for App {
    fn apply_declarative(
        &mut self,
        entry_point_list: PluginList,
    ) -> Result<(), DeclarativePluginError> {
        let graph = entry_point_list
            .expand()
            .map_err(|_err| DeclarativePluginError::CouldNotExpand)?;
        if graph.can_build() {
            match graph.try_build() {
                Ok(mut graph_2) => {
                    let _ = graph_2.compute_resources();
                    let final_order = graph_2
                        .final_items_order()
                        .map_err(|_err| DeclarativePluginError::CouldNotFinalizeOrder)?;
                    final_order.apply_to_app(self);
                }
                Err((graph_error, _)) => {
                    return Err(DeclarativePluginError::ItemGraphError(graph_error));
                }
            }
        } else {
            return Err(DeclarativePluginError::ItemGraphError(
                ItemGraphError::FailedBuildCheck,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::assert_matches;

    use bevy_app::{App, Startup, Update};
    use bevy_ecs::{resource::Resource, world::World};

    use crate::{DeclarativePlugin, EntryPoints, graph::PluginList};

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
        let mut graph = expanded.try_build().unwrap();
        let order = graph.final_items_order().unwrap();
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
        let mut graph = expanded.try_build().unwrap();
        let order = graph.final_items_order().unwrap();
        assert_eq!(order.systems().count(), 4);
    }

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
    struct NonZstPluginF {
        s: String,
    }

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
        let list = NonZstPluginF {
            s: "Lmao".to_string(),
        }
        .get_output();
        let expanded = list.expand();
        assert_matches!(expanded, Ok(_));
        let expanded = expanded.unwrap();
        let mut graph = expanded.try_build().unwrap();
        let order = graph.final_items_order().unwrap();
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
        let mut graph = expanded.try_build().unwrap();
        let order = graph.final_items_order().unwrap();
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
        let graph = expanded.try_build();
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
        let mut graph = expanded.unwrap().try_build().unwrap();
        let resource_compute_result = graph.compute_resources();
        assert_matches!(resource_compute_result, Ok(_));
        let order = graph.final_items_order();
        assert_matches!(order, Ok(_));
        let order = order.unwrap();
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
        let mut graph = expanded.unwrap().try_build().unwrap();
        let resource_compute_result = graph.compute_resources();
        assert_matches!(resource_compute_result, Ok(_));
        let order = graph.final_items_order();
        assert_matches!(order, Ok(_));
        let order = order.unwrap();
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
        let mut graph = expanded.unwrap().try_build().unwrap();
        let resource_compute_result = graph.compute_resources();
        assert_matches!(resource_compute_result, Err(_));
    }
}

pub trait EntryPoints {
    fn get_output(self) -> PluginList;
}

impl<A: DeclarativePlugin> EntryPoints for A {
    fn get_output(self) -> PluginList {
        let mut output = PluginOutput::new::<A>();
        self.build(&mut output);
        PluginList {
            nodes: vec![(Box::new(self), output)],
        }
    }
}

impl<A: EntryPoints, B: EntryPoints> EntryPoints for (A, B) {
    fn get_output(self) -> PluginList {
        let (a, b) = self;
        let mut list = PluginList { nodes: Vec::new() };
        list.extend(a.get_output()).extend(b.get_output());
        list
    }
}

impl<A: EntryPoints, B: EntryPoints, C: EntryPoints> EntryPoints for (A, B, C) {
    fn get_output(self) -> PluginList {
        let (a, b, c) = self;
        let mut list = PluginList { nodes: Vec::new() };
        list.extend(a.get_output())
            .extend(b.get_output())
            .extend(c.get_output());
        list
    }
}

impl<A: EntryPoints, B: EntryPoints, C: EntryPoints, D: EntryPoints> EntryPoints for (A, B, C, D) {
    fn get_output(self) -> PluginList {
        let (a, b, c, d) = self;
        let mut list = PluginList { nodes: Vec::new() };
        list.extend(a.get_output())
            .extend(b.get_output())
            .extend(c.get_output())
            .extend(d.get_output());
        list
    }
}

impl<A: EntryPoints, B: EntryPoints, C: EntryPoints, D: EntryPoints, E: EntryPoints> EntryPoints
    for (A, B, C, D, E)
{
    fn get_output(self) -> PluginList {
        let (a, b, c, d, e) = self;
        let mut list = PluginList { nodes: Vec::new() };
        list.extend(a.get_output())
            .extend(b.get_output())
            .extend(c.get_output())
            .extend(d.get_output())
            .extend(e.get_output());
        list
    }
}
