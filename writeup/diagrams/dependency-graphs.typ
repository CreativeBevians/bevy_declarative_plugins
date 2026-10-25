#import "@preview/fletcher:0.5.8": diagram, edge, node, as-label
#import "../shared.typ": *

#let conflict-req() = {diagram(
	node-stroke: .1em,
	spacing: 2em,
  node((-1, 0), $A$, name: <A>),
  node((1, 0), $B$, name: <B>),
  node((-1, 1), $C$, name: <C>),
  node((-1, 2), $D_1$, name: <D1>),
  node((-1, 3), $..._1$, name: <Dot1>),
  node((1, 1), $D_2$, name: <D2>),
  node((1, 2), $..._2$, name: <Dot2>),
  edge(<A>, <C>, "-|>"),
  edge(<C>, <D1>, "-|>"),
  edge(<D1>, <Dot1>, "-|>"),
  edge(<B>, <D2>, "-|>"),
  edge(<D2>, <Dot2>, "-|>"),
)}

#let plugins-ungrouped() = {
  diagram(
    node-stroke: 0.1em,
    spacing: 2em,
    node-shape: circle,
    node((-1, 1), $A_1$, name: <A1>),
    node((0, 0.25), $A_2$, name: <A2>),
    node((1, 1), $A_3$, name: <A3>),
    node((-1, 2), $B_1$, name: <B1>),
    node((1, 2), $C_1$, name: <C1>),
    node((0, 2.75), $C_2$, name: <C2>),
    edge(<A1>, <C2>, "-|>"),
    edge(<A2>, <C1>, "-|>"),
    edge(<A3>, <C1>, "-|>"),
    edge(<A1>, <B1>, "-|>"),
    edge(<C1>, <B1>, "-|>"),
    edge(<C2>, <B1>, "-|>"),
  )
}

#let plugins-grouped() = {
  diagram(
    node-stroke: 0.1em,
    node-shape: rect,
    spacing: 4em,
    node((0, 0), [
      $A$\
      ${A_1,A_2,A_3}$
      ], name: <A>),
    node((-1, 1), [$B$\ ${B_1}$], name: <B>),
    node((1, 1), [
      $C$\
      ${C_1, C_2}$
      ], name: <C>),
    edge((0, 0.2), <C>, "-|>"),
    edge((0.2, 0.2), <C>, label: [$A_1,A_2,A_3 ->_"dep" C$], "-|>"),
    edge((-0.2, 0.2), <C>, "-|>"),
    edge((-0.2, 0.2), <B>, label: [$A_1 ->_"dep" B$], "-|>"),
  
    edge((1,1), <B>, label: [$C_1,C_2 ->_"dep" B$], "-|>"),
    edge((1,1.2), <B>, "-|>")
  )
}

#let conflict() = {
  diagram(
    node-stroke: .1em,
    spacing: 2em,
    node((-1, 1), [$C, f : option(D -> bool)$], name: <C>),
    node((1, 1), [$B, g : option(D -> bool)$], name: <B>),
    node((-1, 6), $D_1$, name: <D1>),
    node((1, 6), $D_2$, name: <D2>),
    edge((-1, 0), <C>, "--|>"),
    edge(<C>, <D1>, "-|>"),
    edge(<B>, <D2>, "-|>"),
    edge(<B>, <D1>, "-|>-", stroke: green, label: [$g(D_1) = "true"$], bend: -15deg),
    edge(<C>, <D1>, "-|>-", stroke: green, label: [$f(D_1) = "true"$], bend: -45deg),
    edge(<C>, <D2>, "-|>-", stroke: red, label: [$f(D_2) = "false"$], bend: 15deg),
    edge(<B>, <D2>, "-|>-", stroke: green, label: [$g(D_2) = "true"$], bend: 45deg),
  )
}

#let distance() = {
  diagram(
    node-stroke: .1em,
    spacing: 1em,
    node((-1, 0), $A$, name: <A>),
    node((0, 0), $B$, name: <B>),
    node((1, 0), $C$, name: <C>),
    node((2, 0), [$D_4$\ distance = 0], name: <D4>),
    node((-1, 1), [$D_1$\ distance = 1], name: <D1>),
    node((0, 3), [$D_2$\ distance = 3], name: <D2>),
    node((1, 2), [$D_3$\ distance = 2], name: <D3>),
    node((0, 1), $E$, name: <bdots1>),
    node((0, 2), $G$, name: <bdots2>),
    node((1, 1), $F$, name: <cdots1>),
    edge(<A>, <D1>, "-|>"),
    edge(<B>, <bdots1>, "-|>"),
    edge(<bdots1>, <bdots2>, "-|>"),
    edge(<bdots2>, <D2>, "-|>"),
    edge(<C>, <cdots1>, "-|>"),
    edge(<cdots1>, <D3>, "-|>"),
  )
}