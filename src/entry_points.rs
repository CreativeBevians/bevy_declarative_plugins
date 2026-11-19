//! Traits and types for registering plugins as "entry points" or collections
//! of dependencies.

use std::marker::PhantomData;

use crate::{
    DeclarativePlugin, PluginOutput, approval::WithApproval, graph::PluginList,
    plugin_data::PluginDependencies,
};

/// A trait for types that contain one or more Plugin.
pub trait EntryPoints: Sized {
    /// "Flatten" the plugins into a normalized list of plugins.
    fn get_output(self) -> PluginList {
        let mut list = PluginList { nodes: Vec::new() };
        self.add_to_list(&mut list);
        list
    }

    /// Add the internal plugins to an existing list of plugins.
    fn add_to_list(self, list: &mut PluginList);
}

/// All plugins are, themselves, potential entry points.
impl<A: DeclarativePlugin> EntryPoints for A {
    fn add_to_list(self, list: &mut PluginList) {
        let mut output = PluginOutput::new::<A>(PluginDependencies(Vec::new()));
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

/// A trait for types that can be added as dependencies. Done this way so users
/// can call builder-like configuration functions on plugins the same way they
/// do when adding systems to an app, like so:
/// ```
/// output.add_plugins((
///     // Regular, bare plugin.
///     MyPlugin,
///     // Plugin with an approval function tied to it.
///     3rdPartyPlugin { value: "Nifty" }
///         .with_approval(|party: &3rdPartyPlugin| party.value != "Terrible")
/// ));
/// ```
///
/// See the writeup or the top-level module docs for reasoning and background.
pub trait AddDependency: Sized {
    /// The underlying plugin for the type, for groups of plugins this should
    /// be the never type ([`!`])
    type Plugin;

    /// Add `self` to the [`PluginOutput`] of a plugin.
    fn add_to_output(self, output: &mut PluginOutput<PluginDependencies>);

    /// Create a version of this dependency with an approval function.
    fn with_approval(
        self,
        f: impl for<'a> Fn(&'a Self::Plugin) -> bool + 'static,
    ) -> WithApproval<Self, Self::Plugin>
    where
        Self: 'static,
    {
        WithApproval {
            subject: self,
            approval: Box::new(f),
        }
    }

    /// Get the underlying plugin value, if it exists. Some dependency
    /// representations will not have a real value associated with them, and
    /// plugin groups will need to
    fn get_value(self) -> Option<Self::Plugin>;
}

/// All declarative plugins can be added as a dependency.
impl<A: DeclarativePlugin> AddDependency for A {
    type Plugin = A;

    fn add_to_output(self, output: &mut PluginOutput) {
        output.dependency_builder().with_config(self).build();
    }

    fn get_value(self) -> Option<Self::Plugin> {
        Some(self)
    }
}

/// A [`PhantomData`] of a [`DeclarativePlugin`] represents a dependency
/// without a configuration. This should maybe be replaced with a specific
/// "`WithoutValue`" type in future, but this is what it is for now.
impl<A: DeclarativePlugin> AddDependency for PhantomData<A> {
    type Plugin = A;

    fn add_to_output(self, output: &mut PluginOutput) {
        output.dependency_builder::<A>().build();
    }

    fn get_value(self) -> Option<Self::Plugin> {
        None
    }
}

impl<A: AddDependency> AddDependency for WithApproval<A, A::Plugin>
where
    A::Plugin: DeclarativePlugin,
{
    type Plugin = A::Plugin;

    fn add_to_output(self, output: &mut PluginOutput) {
        let WithApproval {
            subject: plugin,
            approval,
        } = self;
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
        self.subject.get_value()
    }

    /// Calling `with_approval` on a [`WithApproval`] type will AND the two
    /// approval functions, "nested" [`WithApproval`] instances will have their
    /// functions replaced with panicking approval functions as only the
    /// topmost approval should be used.
    fn with_approval(
        self,
        f: impl for<'a> Fn(&'a Self::Plugin) -> bool + 'static,
    ) -> WithApproval<Self, Self::Plugin>
    where
        Self: 'static,
    {
        let WithApproval {
            subject: plugin,
            approval: prev_approval,
        } = self;

        WithApproval {
            subject: WithApproval {
                subject: plugin,
                // We avoid using the inner one, because we instead AND the
                // previous approval with the new approval.
                approval: Box::new(|_value: &A::Plugin| {
                    panic!("Should not use inner approval function")
                }),
            },
            approval: Box::new(move |value: &A::Plugin| f(value) && prev_approval(value)),
        }
    }
}

macro_rules! impl_add_dependency {
    () => {};
    ($($i:tt $t2:tt),+) => {
        impl <$($t2,)+> AddDependency for ($($t2 ),+) where $($t2: AddDependency,)+ {
            // For groups there isn't a canonical "plugin" to drill down to, so
            // we use the never type for it.
            type Plugin = !;
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
