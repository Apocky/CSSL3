# Graph Report - Handover pipeline audit kickoff  (2026-07-28)

## Corpus Check
- 5 files · ~55,976 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 147 nodes · 305 edges · 11 communities
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 3 edges (avg confidence: 0.5)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `1b10cf87`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- support.js
- walk
- site.js
- getReact
- createRuntime
- boot
- loadReactUmd
- compileTemplate
- rootNameForDocument
- site.js
- room-v3.js

## God Nodes (most connected - your core abstractions)
1. `activateRoom()` - 12 edges
2. `element()` - 9 edges
3. `createRuntime()` - 9 edges
4. `walkChildren()` - 8 edges
5. `walk()` - 8 edges
6. `setText()` - 7 edges
7. `setText()` - 7 edges
8. `setText()` - 7 edges
9. `getReact()` - 7 edges
10. `boot()` - 7 edges

## Surprising Connections (you probably didn't know these)
- `boot()` --calls--> `getReact()`  [EXTRACTED]
  support.js → support.js  _Bridges community 3 → community 5_
- `walkFor()` --calls--> `getReact()`  [EXTRACTED]
  support.js → support.js  _Bridges community 3 → community 1_
- `init()` --calls--> `boot()`  [EXTRACTED]
  support.js → support.js  _Bridges community 5 → community 6_
- `compileAttr()` --calls--> `resolve()`  [EXTRACTED]
  support.js → support.js  _Bridges community 7 → community 1_
- `createRuntime()` --calls--> `resolve()`  [EXTRACTED]
  support.js → support.js  _Bridges community 7 → community 6_

## Import Cycles
- None detected.

## Communities (11 total, 0 thin omitted)

### Community 0 - "support.js"
Cohesion: 0.12
Nodes (8): cdnScriptFor(), compileTemplate(), encodeCamelAttrs(), encodeCase(), isElementClass(), isRenderableType(), loadReactUmd(), loadScript()

### Community 1 - "walk"
Cohesion: 0.33
Nodes (13): collectProps(), compileAttr(), contentKey(), cssToObj(), hostPositionStyle(), kebabToCamel(), walk(), walkChildren() (+5 more)

### Community 2 - "site.js"
Cohesion: 0.18
Nodes (19): applyRoomContent(), clearTilt(), createMessage(), motionIsOff(), renderAxisList(), renderHubLineage(), renderHubMessages(), renderHubRoom() (+11 more)

### Community 3 - "getReact"
Cohesion: 0.40
Nodes (5): createComponentFactory(), evalDcLogic(), getReact(), walkText(), warnUnresolved()

### Community 4 - "createRuntime"
Cohesion: 0.15
Nodes (28): activateNode(), activateRoom(), announce(), createAttachment(), createChatMessage(), createCrosscutStrip(), createDivider(), createEvent() (+20 more)

### Community 5 - "boot"
Cohesion: 0.29
Nodes (8): boot(), dcNameFromPath(), getReactDOM(), parseDataProps(), parseDcDocument(), parseDcText(), rootNameForDocument(), safeDecode()

### Community 6 - "loadReactUmd"
Cohesion: 0.29
Nodes (7): createExternalModules(), createHelmetManager(), createRegistry(), createRuntime(), createStreamTracker(), init(), Placeholder()

### Community 7 - "compileTemplate"
Cohesion: 0.50
Nodes (4): findTopLevelEquality(), parensWrapWhole(), resolve(), resolvePath()

### Community 8 - "rootNameForDocument"
Cohesion: 0.67
Nodes (4): createPseudoSheet(), importantify(), scanUnquotedUrl(), stripComments()

### Community 9 - "site.js"
Cohesion: 0.19
Nodes (18): applyRoomContent(), clearTilt(), createMessage(), motionIsOff(), renderAxisList(), renderHubLineage(), renderHubMessages(), renderHubRoom() (+10 more)

### Community 10 - "room-v3.js"
Cohesion: 0.39
Nodes (5): announce(), setAxis(), setContext(), setMotionOff(), setRoomIndex()

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `createRuntime()` connect `loadReactUmd` to `support.js`, `rootNameForDocument`, `getReact`, `compileTemplate`?**
  _High betweenness centrality (0.001) - this node is a cross-community bridge._
- **Should `support.js` be split into smaller, more focused modules?**
  _Cohesion score 0.11904761904761904 - nodes in this community are weakly interconnected._