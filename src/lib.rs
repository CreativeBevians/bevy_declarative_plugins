use core::any::Any;

pub(crate) mod approval;
pub(crate) mod erased;
pub(crate) mod graph;
pub(crate) mod metadata_ptr;
/// Declarative Plugin public API.
pub mod plugin_data;
use bevy_app::App;
pub use plugin_data::PluginOutput;

use crate::graph::{PluginList, PluginRegistrationGraph};

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
    fn apply_declarative(&mut self, entry_point_list: PluginList) -> Result<(), ()>;
}

impl DeclrAppExt for App {
    fn apply_declarative(&mut self, entry_point_list: PluginList) -> Result<(), ()> {
        let graph = entry_point_list.expand()?;
        if graph.can_build() {
            match graph.try_build() {
                Ok(mut graph_2) => {
                    let _ = graph_2.compute_resources();
                    let final_order = graph_2.final_items_order()?;
                    for item in final_order.0 {
                        item.apply_to_app(self);
                    }
                }
                Err(_) => todo!(),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::assert_matches;

    use bevy_app::{Startup, Update};
    use bevy_ecs::resource::Resource;

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
                .with_approval(|e| e.num % 2 == 0)
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
                .with_approval(|e| e.num > 30)
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
