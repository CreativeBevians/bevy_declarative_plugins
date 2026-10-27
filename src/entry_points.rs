use std::marker::PhantomData;

use crate::{DeclarativePlugin, PluginOutput, graph::PluginList};

pub trait EntryPoints: Sized {
    fn get_output(self) -> PluginList {
        let mut list = PluginList { nodes: Vec::new() };
        self.add_to_list(&mut list);
        list
    }

    fn add_to_list(self, list: &mut PluginList);
}

impl<A: DeclarativePlugin> EntryPoints for A {
    fn add_to_list(self, list: &mut PluginList) {
        let mut output = PluginOutput::new::<A>();
        self.build(&mut output);
        list.nodes.push((Box::new(self), output));
    }
}

macro_rules! impl_entry_points {
    () => {};
    ($($i:tt $t2:tt),+) => {
        impl <$($t2,)+> EntryPoints for ($($t2 ),+) where $($t2: EntryPoints,)+ {
            fn add_to_list(self, list: &mut PluginList) {
                let ($($i),+) = self;
                $(PluginList::extend_with(list, $i);)+
            }
        }
    };
}

impl_entry_points!(a A, b B);
impl_entry_points!(a A, b B, c C);
impl_entry_points!(a A, b B, c C, d D);
impl_entry_points!(a A, b B, c C, d D, e E);
impl_entry_points!(a A, b B, c C, d D, e E, f F);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H, i I);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L, m M);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L, m M, n N);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L, m M, n N, o O);
impl_entry_points!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L, m M, n N, o O, p P);

pub trait AddDependency: Sized {
    type Plugin;

    fn add_to_output(self, output: &mut PluginOutput);

    fn with_approval(
        self,
        f: impl for<'a> Fn(&'a Self::Plugin) -> bool + 'static,
    ) -> WithApproval<Self>
    where
        Self: 'static,
    {
        WithApproval {
            plugin: self,
            approval: Box::new(f),
        }
    }

    fn get_value(self) -> Option<Self::Plugin>;
}

impl<A: DeclarativePlugin> AddDependency for A {
    type Plugin = A;

    fn add_to_output(self, output: &mut PluginOutput) {
        output.dependency_builder().with_config(self).build();
    }

    fn get_value(self) -> Option<Self::Plugin> {
        Some(self)
    }
}

impl<A: DeclarativePlugin> AddDependency for PhantomData<A> {
    type Plugin = A;

    fn add_to_output(self, output: &mut PluginOutput) {
        output.dependency_builder::<A>().build();
    }

    fn get_value(self) -> Option<Self::Plugin> {
        None
    }
}

pub struct WithApproval<A: AddDependency> {
    plugin: A,
    approval: Box<dyn for<'a> Fn(&'a A::Plugin) -> bool + 'static>,
}

impl<A: AddDependency> AddDependency for WithApproval<A>
where
    A::Plugin: DeclarativePlugin,
{
    type Plugin = A::Plugin;

    fn add_to_output(self, output: &mut PluginOutput) {
        let WithApproval { plugin, approval } = self;
        match plugin.get_value() {
            Some(value) => output
                .dependency_builder()
                .with_config(value)
                .with_approval(approval)
                .build(),
            None => output.dependency_builder().with_approval(approval).build(),
        };
    }

    fn get_value(self) -> Option<Self::Plugin> {
        self.plugin.get_value()
    }
}

macro_rules! impl_add_dependency {
    () => {};
    ($($i:tt $t2:tt),+) => {
        impl <$($t2,)+> AddDependency for ($($t2 ),+) where $($t2: AddDependency,)+ {
            type Plugin = ();
            fn add_to_output(self, output: &mut PluginOutput) {
                let ($($i),+) = self;
                $(AddDependency::add_to_output($i, output);)+
            }
            fn get_value(self) -> Option<Self::Plugin> {
                None
            }
        }
    };
}

impl_add_dependency!(a A, b B);
impl_add_dependency!(a A, b B, c C);
impl_add_dependency!(a A, b B, c C, d D);
impl_add_dependency!(a A, b B, c C, d D, e E);
impl_add_dependency!(a A, b B, c C, d D, e E, f F);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H, i I);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L, m M);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L, m M, n N);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L, m M, n N, o O);
impl_add_dependency!(a A, b B, c C, d D, e E, f F, g G, h H, i I, j J, k K, l L, m M, n N, o O, p P);
