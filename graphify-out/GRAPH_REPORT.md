# Graph Report - .  (2026-06-26)

## Corpus Check
- cluster-only mode — file stats not available

## Summary
- 33 nodes · 54 edges · 6 communities (4 shown, 2 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `a3f03936`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]

## God Nodes (most connected - your core abstractions)
1. `PowerMonitor` - 17 edges
2. `Message` - 7 edges
3. `poll_battery()` - 4 edges
4. `BatteryData` - 3 edges
5. `main()` - 2 edges
6. `setup.sh script` - 1 edges

## Surprising Connections (you probably didn't know these)
- `PowerMonitor` --references--> `Id`  [EXTRACTED]
  src/app.rs →   _Bridges community 3 → community 1_
- `PowerMonitor` --references--> `String`  [EXTRACTED]
  src/app.rs →   _Bridges community 3 → community 0_

## Import Cycles
- None detected.

## Communities (6 total, 2 thin omitted)

### Community 0 - "Community 0"
Cohesion: 0.28
Nodes (5): Result, BatteryData, poll_battery(), main(), String

### Community 1 - "Community 1"
Cohesion: 0.33
Nodes (4): Id, Option, Message, Subscription

### Community 2 - "Community 2"
Cohesion: 0.40
Nodes (4): Action, Flags, Self, Task

### Community 3 - "Community 3"
Cohesion: 0.53
Nodes (3): Application, Core, PowerMonitor

## Knowledge Gaps
- **1 isolated node(s):** `setup.sh script`
  These have ≤1 connection - possible missing edges or undocumented components.
- **2 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `PowerMonitor` connect `Community 3` to `Community 0`, `Community 1`, `Community 2`, `Community 4`?**
  _High betweenness centrality (0.598) - this node is a cross-community bridge._
- **Why does `Message` connect `Community 1` to `Community 2`, `Community 3`, `Community 4`?**
  _High betweenness centrality (0.059) - this node is a cross-community bridge._
- **What connects `setup.sh script` to the rest of the system?**
  _1 weakly-connected nodes found - possible documentation gaps or missing edges._