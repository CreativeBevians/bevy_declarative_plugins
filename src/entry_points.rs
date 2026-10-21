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
