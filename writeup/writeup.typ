#import "@preview/fletcher:0.5.8": diagram, edge, node, as-label

#set par(justify: true)
#set heading(numbering: "1.")
// #set page(margin: 4em)
#show link: underline

#let erased = math.italic("Erased")
#let peek = $"visit"$
#let bool = math.italic("bool")
#let option = math.italic("Option")
#let output = math.italic("Output")
#let dependencies = math.italic("Dependencies")
#let plugins = (
  framework: `FrameworkPlugins`,
  windowing: `Windowing`,
  graphics: `Graphics`,
  audio: `Audio`,
  user: (
    a: `UserPluginA`,
    b: `UserPluginB`,
    input: `UserInputHandler`,
    game_systems: `UserGameSystems`,
    ui: `UserCommonGameUI`,
  )
)

#let if_html(body) = context {
  if target() == "html" {
    html.frame(body)
  } else {
    body
  }
}

#let cols = 1

#title()[Draft: Declarative Plugins for Bevy (and other ECS Worlds)]

= Intro

Plugins are how we meaningfully group sets of types and behaviour together in Bevy and other ECSs. The Bevy engine is composed of plugins, 3rd party crates make themselves available to developers as plugins, and we are encouraged to structure our code as plugins. The current plugin implementation has sharp edges, with registering the same plugin twice crashing the program. Dependencies have been on the wishlist since very early on#footnote[Issue: #link("https://github.com/bevyengine/bevy/issues/69")], and would enable "deeper" 3rd party plugins that build upon each other in ways we've not yet seen in the Bevy ecosystem.

This article discusses the implementation details for the `bevy_declarative_plugins`#footnote[Source code: #link("https://github.com/CreativeBevians/bevy_declarative_plugins")] crate, which has acted as a reference implementation. The API shape is not discussed in detail here, but can be viewed at in the source code or viewed via the docs.rs#footnote[Documentation: #link("https://docs.rs/bevy_declarative_plugins/latest/bevy_declarative_plugins/")].

= Motivation

Every plugin registration function in Bevy has access to `&mut App`. This also gives every plugin registration function access to `&mut World`, the "ECS" world that stores all the ECS-related data and runs all the systems.

Because every plugin registration function has full access, Bevy plugins can't have "dependencies" without the author needing to reason about the details of what those dependencies add and rely on themselves. This works fine if you own the entirety of what you're registering or if your 3rd party bevy dependencies do not register any dependencies you register yourself. If either of these conditions are not upheld then the program crashes, or non-crashing behaviour is hard to design.

We can imagine easy non-crashing behaviour that still means a developer's preferences get overwritten because a prior instance of a plugin added something and when Bevy sees the a new instance of something that already exists, it skips it. Or we can imagine giving a similar mechanism _recency bias_, but this would still require that developers be aware of the call tree of plugin registration.

This design approaches this complexity with three main goals:

1. Separate plugin registration from modifying `&mut World` or `&mut App` directly.
2. Respect the practical needs of developers building Bevy apps, including how their app-authorship-level decisions should be prioritised over those of plugin or engine authors.
3. Match the current API shape as much as possible.

This comes with an intent to enable the following in future:

1. Make hot-patching be able to account for adding or removing systems, other Bevy items, or even entire plugins.
2. Provide static/semi-static analysis tools on Bevy projects.

=== Changes in Plugin API

Plugins are used by everything, so it will be necessary to make migrating to a new system as painless as possible. For adding plugins to an App and running it, the method now return a `Result<FinalizedApp, _>` where `FinalizedApp` is a newtype over `App` to dissuade calling declarative plugin registration twice, and exposes a `run` method. in @init-compare we can see how this amounts to a change in method name and an `unwrap` call between adding plugins and running the program.

For defining plugins, beside the change in trait name the only visible change to the end user is replacing `&mut App` with `&mut PluginOutput`. At time of writing this doesn't replicate all additive API behaviour of `&mut App`, but as discussed later the paths for adding more behaviour are fairly clear.

#figure(caption: [Plugin registration comparison.])[
#table(columns: (auto, auto))[Old][New][
```rust
let mut app = App::new();
app.add_plugins((
    FrameworkPlugins::default(), 
    UserPluginA, 
    UserPluginB,
  ))
  .run();
```
][
```rust
let mut app = App::new();
// Adding plugins is a one-time
// thing, and can fail!
app.add_plugins_declarative((
    FrameworkPlugins::default(), 
    UserPluginA, 
    UserPluginB,
  ))
  .unwrap()
  .run();
```
]
]<init-compare>

#figure(caption: [Plugin definition comparison.])[
#table(columns: (auto, auto))[Old][New][
```rs
impl Plugin for UserPluginB {
  fn build(
      &self, 
      app: &mut App
    ) {
    app.add_plugins(UserGameSystems);
    app.add_systems(
      Update, (system_10, system_11)
    );
    app.add_message::<MyMessage>();
    // ...
    output.add_plugins(
      UserUiSystem::dark()
    );
  }
}
```
][
```rs
impl DeclarativePlugin for UserPluginB {
  fn build(
      &self, 
      output: &mut PluginOutput
    ) {
    output.add_plugins(UserGameSystems);
    output.add_systems(
      Update, (system_10, system_11)
    );
    output.add_message::<MyMessage>();
    // ...
    output.add_plugins(
      UserUiSystem::dark()
      .with_approval(
        // We *really* do not
        // want the light theme.
        // So lets prevent it
        // with a constraint.
        |ui_system: &UserUiSystem|
          ui_system.theme != "light"
      )
    );
  }
}
```
]
] <def-compare>

= Terminology

- *Item*: An additive change we want to make to `World`, usually representing "add this System" or "Register this message type."
- *Entry Points*: The plugins that an app author would add to the ECS `World`. These are at the "Root" of the dependency graph.
- *Unsolved Graph*: A graph whose nodes each contain a list of possible candidate plugins of the same type.
- *Solved Graph*: A graph whose nodes are single, unique candidates of each plugin type.
- *Staged Item*: Pairing of an erased Item and a function that can reverse the type erasure and add that item to an ECS `World`.
- *Plugin Instance*: A value of a type that implements `DeclarativePlugin`/`Plugin`.
- *Candidate Plugin*: A plugin instance that is approved of by plugins that depend on it.
- *Chosen Candidate*: A plugin instance that will be used by the final `App`.

= Data Structures

The transform we want to pursue is going from a list of "Entry Points," to an Unsolved Graph, to a Solved Graph, to a list of items we can register with an ECS world.

Here we'll work backwards, from answering "how do we erase and then register items with `World`" back through solved vs unsolved graphs. These do not represent the user-facing API, but the mechanisms that make the API and expected behaviour possible.

== Working Backwards: Items we'll Register

"Items" are the fundamental "Bevy things" we add during plugin registration. If it's something that introduces a piece of app-relevant information to `World`, it's an Item. An Item is an _additive_ piece of information, it introduces information and erases no existing information.

We want to be able to hand off item types to a final `World` in an order we decide once we've performed all our processing. To do this, we'll represent the items we're registering as an enum, and we'll store those items in a `Vec`:

#figure(caption: [Enum describing what we can register with an `World` during plugin registration.])[
```rust
enum DeclrItem {
    Message(MessageRegistration),
    System(StagedSystem),
    ScheduleLabel(StagedScheduleLabel),
    Observer(bevy_ecs::observer::Observer),
    Resource(StagedResource),
}
```]<registration-enum>

#figure(caption: [Ordered list of plugin items.])[
```rust
struct OrderedPluginItems(Vec<DeclrItem>);
```
]<registration-list>

Two of these Items are simple: `Observer` is a `bevy_ecs` type, and `MessageRegistration` is a pointer to `App::add_message::<T>` where `T` implements `Message`.

`StagedResource`, `StagedSystem`, and `StagedScheduleLabel` each exist for their own reasons. `StagedResource` gets around how `Resource` is not a _dyn-compatible_ trait. `StagedSystem` and `StagedScheduleLabel` simplify the registration process for highly-typed parts of the Bevy API.

This list does not enumerate all possible registrations one can make on an `App`, but extending it is trivial so long as new items do not introduce more complex ordering constraints.

== Erased and Staged Values

"Staged" values (see @staging-type) are a pairing between an owned, type erased value and a function that can reverse the type erasure of the value then add that value to `World`. The function pointer has "knowledge" of what the type of the erased value is because it was chosen when type information was still available.

#figure(caption: [A pair of an erased value, and a function that can reverse the erasure process and apply the value to a `World`])[
```rust
// "E" is a newtype over an erased 
// value, specific to each use case. 
// Erased Resources != Erased Systems.
struct Staged<E> {
    // The erased value.
    erased: E,
    // Reverse the erasure.
    apply: fn(&mut World, E),
}
```
]<staging-type>

Type erased values in this context have properties that mean we need to store a lot of extra information about them, extra metadata. I'm calling this a `MetadataPtr` (see @metadata-ptr), but this name doesn't matter much. The properties we need to account for are:

- There could be multiple instances of a plugin/resource of a type, and we will only use one.
- We need to be able to assume the type of the erased value, and visit / consume it if we're right.

Because we will not always end up using the erased value, and the erased value may have data stored on the heap, we need to store a drop function that we run from a `MetadataPtr`'s own Drop implementation. This drop function needs to be able to reverse the erasure of the pointer, rather than just deallocate. We also need to know if the drop function has already been run.

#figure(caption: [A type-erased value with the "Metadata" needed to properly manage it.])[
```rust
struct MetadataPtr {
    layout: Layout,
    ptr: NonNull<()>,
    drop_fn: fn(NonNull<()>, Layout),
    type_id: TypeId,
    already_dropped: bool,
}
```
] <metadata-ptr>

Layout has been stored here largely for debugging reasons during development #footnote[At one point I spent an hour asking myself why a value isn't reversing its erasure properly, only to realise I erased a `Box<Type>` instead of a `Type`.], and can be argued for removal. Everything else is necessary to be able to destroy or visit or reacquire ownership of the value.

Visiting a value also requires that the resulting value cannot contain a reference for the erasure-reversed value. To do this, the visit method has a `visit::<T, A>(self: &MetadataPtr, visitor: impl for<'a> Fn(&'a T) -> A) -> Option<A>` signature, where `T` is the type we're assuming while visiting and `A` is the resulting value.

Reversing erasure and retaking ownership has a more simple signature of `reverse_erasure::<T>(self: MetadataPtr) -> Result<T, MetadataPtr>`, where we use the original `MetadataPtr` as the "error" type to denote that the wrong type was "guessed."

With these mechanisms of type erasure and staging, or relying on existing Bevy/dyn-compatibility functionality, we have the ability to store erased values and insert them into `World` in a way consistent with the well-typed interfaces of Bevy.

== Unsolved Graph <unsolved-h>

Finding a subgraph can be a very expensive operation, and doesn't always express the behavior you want from a program. Consider the naive digraph in @naive-graph2:

#figure(caption:[A naively fully-featured digraph expressing unsolved dependencies between nodes.])[
  #import "diagrams/dependency-graphs.typ": plugins-ungrouped
  #if_html(plugins-ungrouped())
]<naive-graph2>

This expresses dependencies in a way that is fine-grained, but doesn't line up with how dependencies work in a build system, where you accept that a solver is going to do some of the work for you.

Instead, we put all instances of the same plugin type into a node together and let them depend on whole plugin types, not instances.

An unsolved graph has nodes that store each instance of a plugin of a given type, and edges between each _instance_ to each _plugin type_. That is, $A_1, A_2, A_3 in A$ are all _instances_ of a plugin type `A: Plugin`, and edges are pairs of `(InstanceId, PluginTypeId)` (expressed in @unsolved-graph as $A_n ->_"dep" C$). When expressing dependencies, they can only point towards a plugin type, not a plugin instance.

#figure(caption: [An Unsolved Graph representing the same graph as @naive-graph2. Instances are grouped together.])[
  #import "diagrams/dependency-graphs.typ": plugins-grouped
  #if_html(plugins-grouped())
] <unsolved-graph>

This expresses how plugins can register dependencies, and place constraints on those dependencies, but cannot be overly exact without expressing something that might break during plugin resolution.

== Solved Graph

#figure(caption: [A possible solution for the graph in @unsolved-graph. Note that because $A_2$ was picked, there is no edge between $A$ and $B$.])[
  #import "diagrams/dependency-graphs.typ": plugins-solved
  #if_html(plugins-solved())
] <solved-graph>

A Solved Graph is one where we only have one instance of every plugin. It exists after the process of choosing which instance of a plugin to use. The only uncertain thing at this point in the process is which instances of a `Resource` to use, but most of the time resources will only be registered by a single plugin.

== Approval Functions <approval-defh>

In #link("https://doc.rust-lang.org/cargo/reference/resolver.html#semver-compatibility")[Cargo], and other modern package managers, versions of dependencies are usually chosen through some form of SAT solve on the version numbers of the dependencies.

Plugin configurations in Bevy are structs and enums. There isn't a simple domain specific language we can use to determine which configuration for a plugin is appropriate. Instead we use "Approval" functions, which for a plugin type $A$ is just a function of $\&A -> bool$ that can account for the type erasure in play.

Unlike with versioning, which cargo solves for us already, we only rarely need to care about plugin configuration. It's not expected that most plugins use approvals.

== Entry Points

"Entry Points" are the plugins that are added at the root of the dependency graph, by an app developer. These are the arguments to the function `DeclarativePlugin::add_plugins_declarative`. Because the app developer is putting these in manually, we treat them as "most important" during the graph solving algorithms as to best respect the intent of these developers.

```rs
fn main() {
    App::new()
        // A and B are entry points.
        .add_declarative_plugins((A, B))
}

impl DeclarativePlugin for A {
    fn build(&self, output: &mut PluginOutput) {
        // C is not an entry point.
        output.add_dependencies(C)
        // An alias for `add_dependencies`, so
        // D is still not an entry point.
        output.add_plugins(D)
    }
}
```

== `PluginOutput`

How do we store all the information we need from a plugin to begin with? With a struct that stores all the erased data in individual containers alongside some extra information for solving the graph:

#figure(caption: [The datatype that is modified by `DeclarativePlugins::build`, in lieu of `&mut App`.])[
```rs
// Users should not be given access to any of this, presented with an additive
// API only with no inspection of what the app has already added.
struct PluginOutput<D = PluginDependencies> {
    // Solving-relevant information.
    is_entry_point: bool,
    is_zero_sized_optimizable: bool,
    working_plugin: PluginTypeId,
    plugin_name: &'static str,
    
    // Dependencies
    dependencies: D,

    // Items and Approvals
    observers: Vec<Observer>,
    schedules: StagedSchedules,
    schedule_labels: Vec<StagedScheduleLabel>,
    messages: HashMap<TypeId, MessageRegistration>,
    resource_staging: HashMap<ResourceTypeId, StagedResource>,
    plugin_approval: HashMap<PluginTypeId, Approval<Box<dyn DeclarativePlugin>>>,
}
```
]

An important part of this data structure is that the fields for these are *_private_* and not the basis of the public API. Another is that the public API is *_additive-only_*, there are no methods exposed on this data structure that a plugin author or app developer can use to _remove_ something.

Other than information necessary for our graph solving and the erased storage fields, the dependencies of a `PluginOutput<D>` are a generic parameter. This is for two reasons: so that we can mark dependencies as "used" when we've populated the unsolved graph, and so we can define subapp registration functionality, such as with `RenderApp`. If dependencies are `()`, then you can't register dependencies and we don't have to worry about multiple graphs, just items that are meant for a different `App`/`World`.

= Algorithms

With data structures established, we now need to consider how we transform between those structures.

== Ordering of Bevy Items

The current ordering algorithm seeks to group all Item Kinds (i.e. Systems, Resources, Observers etc.) together. 
This ordering behaviour may change over time as new Item Kinds are supported or ordering constraints change, but with the current supported Item Kinds this functions well. At time of writing the order we want is as follows:

1. Messages / Events (Type registration, Zero Dependencies.)
2. Schedule Labels (Zero Dependencies but might want to depend on States, but states are Resources.)
3. Observers (Wants to know Events.)
4. Systems (Wants to know Schedule Labels.)
5. Resources (Resources are Entities, and so we want to know any global lifetime hooks.)

Bevy's use of Interning makes some of the type information dependencies (such as a System requiring a Schedule that requires a State, which is a Resource) between either irrelevant, or at least presents a path to make those dependencies irrelevant.

== Unsolved Graph Construction Algorithm

Constructing the unsolved graph has its own nuances we want to consider. The algorithm described in @graph-construction is a breadth-first expansion, as we want to prioritise the instances we find closest to the Entry Points a developer specifies. The size of the graph is constrained by a program only having a finite number of types as well as a limit on the number of instances of a specific plugin that can occur in the graph.

As shown in @unsolved-h, the shape of an unsolved graph is that where nodes are comprised of many plugin instances, and edges are from instance to node to represent the "lossy" nature of depending on something.

For zero-sized types, which comprise most plugin types in practice, we can make some assumptions that simplify the expansion process. These are accounted for in @graph-construction, and covered more in @zst-h.

If we come across a plugin config that fails approval by anything that depends on it, then we can decide to avoid evaluating it at all. We eagerly remove these, so even if one instance of a plugin disapproves of it and the rest of those plugins don't then the instance that is being evaluated will be discarded.

#figure(caption: [Breadth-first Graph Construction Algorithm with Proactive Culling.])[
  // #set edge(crossing: true)
  #import "diagrams/algo-expansion.typ": algo-expansion
  #if_html(algo-expansion())
]<graph-construction>

=== State Machine Behaviour and Plugin Instance Limits <infinite-h>

We can construct a plugin approval function for two plugins that, left unchecked, would consume all resources of a machine it runs on:

1. A plugin `struct A(usize)`, shown here as $A(n)$ and its inner value as $A_0$
2. A plugin `struct B(usize)`, shown here as $B(m)$ and its inner value as $B_0$
3. $A(n)$ requires $B(n + 1)$, approves $B$ only if $B_0 > n$
4. $B(m)$ requires $A(m + 1)$, approves $A$ only if $A_0 > m$

The cause of the infinite expansion is that all smaller instances will approve of all larger instances, but all larger instances disapprove of all smaller instances. We can see this expansion in @inf where the arrows above the nodes represent how each instance approves of later plugins, and the arrows below the nodes represent how later plugins disapprove of earlier plugin instances:

#figure(caption: [Infinitely expanding dependency, where $A(0)$ is the entry point.])[
  #diagram(
    node-stroke: .1em,
    spacing: 1em,
    node-shape: circle,
    node((-2, 0), [$A(0)$], stroke: (dash: "dashed"), name: <A0>),
    node((-1, 0), [$B(1)$], name: <B1>),
    node((0, 0), [$A(2)$], name: <A2>),
    node((1, 0), [$B(3)$], name: <B3>),
    node((2, 0), [$A(4)$], name: <A4>),
    node((3, 0), [...], name: <inf>, stroke: (paint: white.transparentize(100%))),
    edge(<A0>, <B1>, "-|>"),
    edge(<B1>, <A2>, "-|>"),
    edge(<A2>, <B3>, "-|>"),
    edge(<B3>, <A4>, "-|>"),
    edge(<A4>, <inf>, "-|>"),
    // Non-approvals
    edge(<B1>, <A0>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<A2>, <B1>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<B3>, <A0>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<B3>, <A2>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<B1>, <A0>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<A4>, <B1>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<A4>, <B3>, bend: 90deg, "-|>", stroke: (paint: red)),
    edge(<inf>, <A4>, bend: 90deg, "-|>", stroke: (paint: red, dash: "dashed")),
    // Approvals
    edge(<A0>, <B1>, bend: 90deg, "-|>", stroke: (paint: green)),
    edge(<B1>, <A2>, bend: 90deg, "-|>", stroke: (paint: green)),
    edge(<A0>, <B3>, bend: 90deg, "-|>", stroke: (paint: green)),
    edge(<A2>, <B3>, bend: 90deg, "-|>", stroke: (paint: green)),
    edge(<A0>, <B1>, bend: 90deg, "-|>", stroke: (paint: green)),
    edge(<B1>, <A4>, bend: 90deg, "-|>", stroke: (paint: green)),
    edge(<B3>, <A4>, bend: 90deg, "-|>", stroke: (paint: green)),
    edge(<A4>, <inf>, bend: 90deg, "-|>", stroke: (paint: green, dash: "dashed")),
    
  )
] <inf>

We avoid this consuming all machine resources by having a limit for the number of instances of a single plugin. The breadth-first approach to expansion lets us, in the case of a less-infinite version of this, bias towards instances closer to the app developer's intent.

In the case of infinite expansion, detecting repeating sub-cycles in a graph may be desirable from a point of completeness. This approach isn't necessarily practical for developers. Having a low ceiling on how many instances of a plugin can be registered before the resolution process returns an error lets us keep startup times in player/user-friendly timescales and keeps errors simple to interpret.

// Todo: Make it so that error types include the plugins with the most instances.

=== Zero-Sized Plugin Types <zst-h>

Most plugin types are _Zero-Sized Types_ (ZSTs) in that they are a struct of the form in @zst-fig, equivalent to `()`.

#figure(caption: [A Zero-Sized Type.])[
```rust
pub struct MyFeaturePlugin;
```
] <zst-fig>

A ZST only has one valid state. This allows us to make the following assumptions during graph expansion:

1. Any two instances of a ZST are the same semantically. $A = A$.
2. The output of any two instances of a ZST are the same.

Using these assumptions, we only need to store the first `PluginOutput` we see for a ZST plugin.

(1) holds when the ZST implements `'static`, as `Any` has been introduced as a supertrait for plugins this holds for all plugins. 

(2) can be violated by users running code dependent on information available to the machine it is running on (i.e. random information, clock time). We don't have to respect this, and choose to only use the first output from a ZST plugin anyway rather than try to respect a plugin configuration whose input type we can't meaningfully inspect.

== Solving the Graph

Solving an Unsolved Graph takes the form:

1. Find all plugins that have an approval function for another plugin.
2. Filter out all plugin instances that do not pass approval functions.
3. Pick the remaining instances that are closest to the entry points.

=== Resolving Conflicts with Approval Functions <approvalh>

When two plugins register separate instances of a plugin type as a dependency, and that dependency is not a ZST, we have to make a choice between which instance to use. In @plugin-conflict we can see a simple version of this:

#figure(caption: [A plugin dependency graph with a conflict between two non-_ZST_ plugins.])[
  #import "diagrams/dependency-graphs.typ": conflict-req
  #if_html(conflict-req())
]<plugin-conflict>

To do this, we make use of the approval functions described in @approval-defh. As `DeclarativePlugin` is both dyn compatible and has `Any` as a supertrait, our well-typed approval functions#footnote[See: `src/approval.rs` in the reference implementation.] are erased over a `dyn DeclarativePlugin` and cast down.

#figure(caption:[Expansion of the conflict in @plugin-conflict, showing the results of the approval functions.])[
  #import "diagrams/dependency-graphs.typ": conflict
  #if_html(conflict())
] <conflict-expansion>

#figure(caption: [Matrix formulation of @conflict-expansion.])[
  #table(
    columns: (auto, auto, auto),
    align: center + horizon,
    table.header([], [$D_1$], [$D_2$]),
    $f in C$, "true", table.cell(fill: red.lighten(67%))[false],
    $g in B$, "true", "true",
    [Valid Candidate], "yes", "no"
  )
]<table-expansion>

Plugin Developers have full control over what happens in `DeclarativePlugin::build`, and they can decide that the value of `&self` affects the approval functions it registers for other plugins. in @value-different $F_1$ and $F_2$ have different approval functions, and because of this $G_3$ will not be chosen as a valid instance of $G$ despite us not knowing which instance of $F$ will be the chosen instance. This short-circuiting is overly eager, but means we do not need to find a subgraph.

#figure(caption: [An approval matrix where two instances of the same plugin type $F$ disagree over an instance of $G$])[
  #let f(s) = {
    if s == false {
      return table.cell(fill: red.lighten(67%))[false]
    } else {
      return [true]
    }
  }
  #table(
    columns: (auto,auto,auto,auto,),
    align: center + horizon,
    table.header([], [$G_1$], [$G_2$], $G_3$),
    $f_1 in F_1$, f(true), f(false), f(true),
    $f_2 in F_2$, f(true), f(false), f(false),
    [Valid Candidate], "yes", "no", "no",
  )
] <value-different>

=== No Valid Candidates

When there is no valid candidate, this is considered a valid case to error and not produce a valid configuration. This is analogous to the way Cargo will error when there is no valid dependency candidate.

=== Choosing Between Candidates

In the case where we have more than one valid candidate, we choose based on distance from Entry Points where a lower distance is more preferred. If a plugin is an entry point, its distance will be 0 and will be chosen over any other valid candidate. In @distances we would choose $D_4$, as its distance would be 0. This represents how the developer, adding $D_4$ as an entry point plugin, has stated a preference for that particular configuration.

#figure(caption: [Entry point distances for instances of a plugin type $D$ in a dependency graph.])[
  #import "diagrams/dependency-graphs.typ": distance
  #if_html(distance())
] <distances>

Distances are a piece of information used to break stalemates between valid configurations _after_ approval functions are run. This is shown in @distances-table, where distance is only considered after $G_1$ and $G_5$ have been established as valid instances of $G$ for the program.

#figure(caption: [A set of dependencies and constraints where there is more than one valid candidate.])[
  #let f(s) = {
    if s == false {
      return table.cell(fill: red.lighten(67%))[false]
    } else {
      return [true]
    }
  }
  #table(
    columns: (auto,auto,auto,auto,auto,auto,),
    align: center + horizon,
    table.header([], [$G_1$], [$G_2$], $G_3$, $G_4$, $G_5$),
    $f_1 in D$, f(true), f(false), f(false), f(false), f(true),
    $f_2 in E$, f(true), f(false), f(false), f(true), f(true),
    $f_3 in F$, f(true), f(false), f(true), f(true), f(true),
    [Valid Candidate], "yes", "no", "no", "no", "yes",
    [Distance], "4", "3", "2", "1", "0",
    [Chosen Candidate], "no", "", "", "", "yes"
  )
] <distances-table>

== Choosing Resources <choose-res>

Resources are chosen after the graph is solved, but before item ordering is performed. This can be summarized as:

1. Find all resources that need approval from another plugin.
2. Filter out all resource instances that do not pass approval functions.
3. Pick the remaining resource instances that are closest to the entry points, preferring non-default values.

The non-default preference is a divergence from the candidate plugin search. The design justification from me here is that it is worse for a default value to be chosen over a user-intended value, especially if two entry points register a resource of the same type but one of them registers a non-default value. This is a debatable behaviour, and not load-bearing.

== Plugins that touch the Rendering World

`RenderApp` items should make up a separate list of items from the main list of plugins. Additionally, `RenderApp` should be prevented from adding dependencies on the basis that `DeclarativePlugin::build` is the source of truth for where dependencies are registered.

#figure(caption: [Potential "Add to render world" API.])[
```rust
trait DeclarativePlugin {
  // PluginOutput has the default type param `PluginDependencies`
  fn build(&self, output: &mut PluginOutput);

  fn build_render(&self, output: &mut PluginOutput<()>) {
    // Empty default impl.
  }
}
```
]<rendering>

= Future Work

Firstly, this is a draft document and those of you reading it should tell me what developer needs remain unmet by this design.

The inspiration for this approach was first thinking about how build systems like Cargo or npm manage dependencies, as well as asking myself "how could hot-patching support the addition/removal of systems." The latter has not been addressed here, but the API surface of plugins has been made more restrictive in ways that make it possible to start reasoning about "diffing" plugin registration in future.

Aside from hot-patching, it would be good to expose an API for registering functions that can be known to the asset BSN. This may need namespacing, but this document doesn't aim to go into the details of that here.