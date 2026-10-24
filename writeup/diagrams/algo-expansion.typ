#import "@preview/fletcher:0.5.8": diagram, edge, node, as-label
#import "../shared.typ": *
#let algo-expansion() = {
 diagram(
    node-stroke: .1em,
    spacing: 2em,
    node-shape: rect,
    node((0,0), [Populate Queue with Entry Point Plugins], name: <populate>),
    node((0,1), [Pop front of queue], name: <pop>),
    node((-0.5,2), `Some(StagedPlugin)`, name: <some>),
    node((0.5,2), `None`, name: <none>),
    node((0.5,3), [Finish], name: <finish>),
    node((-0.5,3), [Is a _ZST_?], name: <iszst>),
    node((-0,4), [Check for Vetos], name: <veto>),
    node((-1,4), [Check if\ already present], name: <zstpresent>),
    node((-0,7), [Compute $output$], name: <getoutput>),
    node((-0,8), [Extract Dependencies into\ ($output',dependencies)$], name: <extractdeps>),
    node((0,9), [Add $output'$ to Graph\ Push $dependencies$ to back of queue], name: <addpush>),
    edge(<populate>, <pop>, "-|>"),
    edge(<pop>, <some>, "-|>"),
    edge(<pop>, <none>, "-|>"),
    edge(<none>, <finish>, "-|>"),
    edge(<some>, <iszst>, "-|>"),
    edge(<iszst>, <veto>, label: "No", "-|>"),
    edge(<iszst>, <zstpresent.east>, label: "Yes", "-|>"),
    edge(<zstpresent>, (-1, 7), <getoutput>, "-|>"),
    edge(<zstpresent>, (-1, 1), <pop>, label-pos: 20%, label: "Already Present", "--|>"),
    edge(<veto.west>, (-0.5, 4), (-0.5,4.6), (-2, 4.6), (-2,1), <pop>, label: "Vetoed", label-pos: 12.5%, crossing: true, "--|>"),
    edge(<getoutput>, <extractdeps>, "-|>"),
    edge(<extractdeps>, <addpush>, "-|>"),
    edge(<addpush>, (-2, 9), (-2, 1), <pop>, "--|>"),
  ) 
}