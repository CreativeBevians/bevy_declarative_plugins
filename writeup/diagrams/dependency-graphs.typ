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