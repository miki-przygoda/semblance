# Expanding the flyvis generative spec into a typed network

**Date:** 2026-09-25
**Status:** approved, not yet implemented
**Epic:** 0.2.0 flyvis import, tasks 6 and 7 — `docs/project-outline/0.2.0-flyvis-import/`

## Why

`semblance-flyvis` reads the published 188 KB spec into typed structs and stops
there. That spec is *generative*: 65 node types and 605 connection types, tiled
over a hexagonal lattice, produce 45,669 neurons and 1,513,231 synapses. Nothing
yet performs the tiling, so nothing in the workspace can hold the actual network.

This is the piece everything downstream stands on. 0.3.0's integrator consumes
it. 0.4.0 scores it. 0.5.0 compresses the parameters that address it. And the
generative spec **is** the compressed representation the project exists to
study — so the expansion is not a loading convenience, it is the decode half of
the encoding under investigation.

Tasks 6 and 7 are specified together because task 7's assertions are task 6's
acceptance criteria. Splitting them would mean building the expansion against no
test, then writing the test that should have driven it.

## Scope

**In:**

1. `semblance-connectome` gains a new type holding an expanded, typed network.
2. `semblance-flyvis` gains the expansion: tiling, hull filling, edge
   construction — and a dependency on `semblance-connectome`.
3. `tools/export_build.py`: flyvis's own built arrays, as test fixtures.
4. The two-stage acceptance tests, plus the structural smoke test.

**Out — deliberately, and not to be added by an implementer:**

- **Any change to `Connectome`**, or to `codec`, `fidelity`, `sim`, `evo`, `app`
  signatures. This is purely additive. If a change to an existing signature seems
  necessary, stop — that is a scoping error, not a detail.
- **Dynamics.** No forward pass, no integration. 0.3.0.
- **Stimuli** and the hexal-to-cell mapping. Task 8 and 0.3.0 respectively.
- **Performance work.** Build it correctly; 0.6.0's sweep is where speed matters.
- **Closing the CI gap.** Tracked separately in `chore-and-fixes/`.

## Placement, and why

`connectome` is the root that `codec`, `sim` and `fidelity` all depend on.
`flyvis` is a leaf that depends on nothing. So:

- **The type lives in `connectome`.** If it lived in `flyvis`, then `codec` and
  `fidelity` would have to depend on the *importer* to talk about the thing they
  compress and score. That inverts the layering.
- **The expansion lives in `flyvis`.** It is flyvis's build semantics — a
  particular hull rule, a particular dropout behaviour, a particular ordering.
  Those quirks do not belong in a general graph type.

`flyvis` gains `semblance-connectome` as a dependency. Workspace-internal, so the
zero-dependency constraint in `CLAUDE.md` is untouched and `Cargo.lock` stays at
8 packages.

## The build algorithm

Read from flyvis 1.2.0, `connectome/connectome.py`, not from any summary.
Parameters as flyvis defaults them: `extent = 15`, `n_syn_fill = 1`.

### Nodes

```
for each node spec entry, in file order:
    (u_stride, v_stride), extent =
        stride(a, b) -> (a, b), 15
        tile(n)      -> (n, n), 15
        single       -> (1, 1), 0      # note: extent 0, not stride-based
    for u in -extent ..= extent:
        for v in max(-extent, -extent - u) ..= min(extent, extent - u):
            if u % u_stride == 0 and v % v_stride == 0:
                push Node { id: len(nodes), type, u, v }
```

The `v` bounds are what make the region a hexagon rather than a rhombus. At
`extent = 15` an unstrided type yields `3n^2 + 3n + 1 = 721` columns.

Node id is position in the list. Ordering is therefore: **spec entry order, then
`u` ascending, then `v` ascending.**

### Edges

```
index: (type, u, v) -> node id
for each edge spec entry e, in file order:
    offsets = fill_hull(e.offsets, n_syn_fill)
              if n_syn_fill > 0 and len(e.offsets) >= 3
              else e.offsets
    for (du, dv), n_syn in offsets:
        for src in nodes of type e.src, in node order:
            if let Some(tgt) = index[(e.tar, src.u + du, src.v + dv)]:
                push Edge { source: src, target: tgt, sign: e.alpha, n_syn }
```

Note the loop nesting: **offset outer, source inner.** A missing target is
skipped silently (`suppress(KeyError)` upstream) — that skip is the entire
dropout mechanism that turns 605 declared connection types into 604 realised and
2,378 filled offset groups into 2,355.

`e.lambda_mult` is carried upstream as `n_syn_certainty`. It plays no part in the
dynamics (`weight = sign * n_syn * syn_strength`) and is not stored here.

### `fill_hull`

```
known = [(du, dv) for each offset]
hull  = ConvexHull((1 + 1e-6) * known, qhull_options="QJ")
verts = known[hull.vertices]          # UNSCALED points, by index
grid  = every integer (u, v) with min_u <= u <= max_u, min_v <= v <= max_v
inside = [p for p in grid if matplotlib.Path(verts).contains_points(p)]
return offsets + [((u, v), n_syn_fill) for (u, v) in inside if (u, v) not in known]
```

Four details that decide the edge count:

1. **The `1 + 1e-6` scaling affects only which points qhull selects as
   vertices.** The polygon is built from the *unscaled* points. It cannot inflate
   the hull. (An earlier note in the epic claimed it could; that was wrong.)
2. **`contains_points` is matplotlib's Haines "crossings multiply" algorithm
   with a half-open `>=` rule** — neither boundary-inclusive nor
   boundary-exclusive. This is the whole ballgame: boundary-inclusive yields
   1,517,786 edges, boundary-exclusive 1,453,484, Haines **1,513,231 exactly**.
3. **Original offsets come first, filled ones after**, in grid order — and the
   grid is `u`-major (`np.mgrid` then `dstack` then `concatenate`). Edge ordering
   depends on this.
4. Filling applies only when an edge type declares **3 or more** offsets.

## The type

```rust
pub struct TypedNetwork {
    // Node side
    cell_types: Vec<String>,      //        65, spec order
    node_cell_type: Vec<u16>,     //    45,669 -> index into cell_types
    node_u: Vec<i16>,
    node_v: Vec<i16>,

    // Edge side
    conn_types: Vec<(u16, u16)>,  //       604, (source cell type, target cell type)
    conn_sign: Vec<f32>,          //       604, +1 or -1
    edge_source: Vec<u32>,        // 1,513,231
    edge_target: Vec<u32>,
    edge_conn_type: Vec<u16>,     // 1,513,231 -> index into conn_types
    edge_n_syn: Vec<f32>,
}
```

Name: `TypedNetwork`. It sits beside `Connectome` and the distinguishing property
is that types, not edges, carry the parameters. Do not rename it unilaterally.

### Why this shape

Across 0.6.0's sweep, **topology is fixed and only the 734-vector changes** — a
codec perturbs parameters, never wiring. So the type stores topology only, and
weights are derived on demand:

```rust
/// weight[e] = sign[ct(e)] * n_syn[e] * syn_strength[ct(e)]
pub fn weights_into(&self, syn_strength: &[f32], out: &mut [f32]);
```

One pass over 1.5M edges, into a caller-owned buffer so the sweep allocates once.
A type that stored weights as primary would force a rebuild per evaluation, and
would quietly re-flatten the cell-type parameter sharing that is the subject of
the research.

`sign` is stored per connection type (604 floats), not per edge, because it is
constant within a type.

### Construction and validation

`connectome` does not know what a flyvis spec is, so it cannot do the expansion.
`flyvis` builds the vectors and hands them over:

```rust
TypedNetwork::new(...) -> Result<TypedNetwork, NetworkError>
```

`new` validates rather than trusts: every `node_cell_type` and `edge_conn_type`
in range, every `edge_source`/`edge_target` below the node count, all edge arrays
the same length, `conn_sign` entries exactly ±1. These are cheap and they catch
precisely the index-map corruption described below.

## Two traps

### The connection-type index must drop zero-realising types

`CONN_TYPES` in `pretrained.rs` was generated as first-appearance order over
flyvis's **built** edge array:

```python
for a, b in zip(dec(c.edges.source_type[:]), dec(c.edges.target_type[:])):
    if (a, b) not in seen: conn_types.append((a, b))
```

Since edges are appended in spec-entry order, that resolves to *spec order,
filtered to entries realising at least one edge*.

So dropping `Lawf1 -> Lawf1` is **not a cosmetic count correction.** It is what
keeps `SYN_STRENGTH[j]` aligned with connection type `j`. A build that retained
all 605 would shift every index past Lawf1 by one, and **every weight from that
point on would be attributed to the wrong connection type** — while producing a
network that runs, has a plausible edge count, and looks entirely fine.

Same failure family as the `syn_count` log-space trap in `task-5-dynamics.md`.
Both are silent, both are catastrophic, both are invisible to any test that only
checks totals.

`CELL_TYPES` has no equivalent hazard: first-appearance over nodes is plain spec
order, all 65 present.

### `%` on negatives differs between Python and Rust, and it does not matter

Node placement tests `u % u_stride == 0`. Python returns a non-negative
remainder (`-4 % 3 == 2`); Rust returns a signed one (`-4 % 3 == -1`). Different
values — but the test is `== 0`, and divisibility is sign-agnostic, so the two
agree on every input.

Recorded because it reads like a portability bug. **Do not "fix" it** with
`rem_euclid`; that would be a no-op dressed as a correction, and the next reader
would have to re-derive that it was safe.

## Testing

### Stage 1 — the filled offsets

`tools/export_build.py` dumps flyvis's post-`fill_hull` offsets per edge type
(2,378 groups). The Rust `fill_hull` is compared against them entry for entry.

This isolates the convex hull and the containment rule completely — the two
pieces that cannot be derived from first principles and must match an external
library's boundary conventions. It is small, human-readable, and when it fails it
names the exact offset group that disagrees.

### Stage 2 — the built network

The same script dumps `source_index`, `target_index`, `n_syn` and the realised
connection-type list. Compared element-wise.

Passing stage 1 first means a stage 2 failure can only be the tiling loops or the
ordering — a much smaller search than "somewhere in 1.5M edges".

**Why two stages rather than one end-to-end check:** a single "did we get
1,513,231" gate can pass on compensating errors, and when it fails it says
nothing about which half was wrong.

### Stage 3 — the structural smoke test (task 7)

Assertions on the built network, against the published figures. These need the
spec JSON but *not* the flyvis export, so they are the weakest-dependency gate of
the three — and the only one a reader can check against the paper.

| Assertion | Value |
|---|---|
| distinct `(u, v)` lattice positions | 721 |
| neurons | 45,669 |
| realised edges | 1,513,231 |
| realised offset groups | 2,355 |
| declared connection types | 605 |
| realised connection types | 604 |
| nodes in spec | 65 |
| biological cell types | 64 (CT1 counted twice) |

The last two matter: the CT1 doubling and the Lawf1 dropout are exactly where a
smoke test silently passes on the wrong invariant.

### Ordering is part of the contract

Node ordering must match flyvis exactly, because 0.3.0 compares per-neuron
activity against exported traces and a permuted network would fail with no
indication that ordering was the cause.

Edge ordering does not affect results mathematically, but matching it is free if
the loops are written in the order above — and it is what makes stage 2's
element-wise comparison possible at all. Match it.

## Risks

**The hull may not reproduce.** qhull with `QJ` joggling may include collinear
boundary points as vertices in ways a monotone-chain hull does not, and that
interacts with the half-open rule exactly at the boundary. Stage 1 exists to
surface this early and precisely. If it resists, the fallback is to compare
against the dumped offsets to learn the rule empirically — not to embed the
dumped offsets, which would make the "generative spec" non-generative and discard
the provenance argument that motivated reading the published file in the first
place.

**The fixtures are gitignored, so the acceptance tests will skip in a fresh
clone.** This is the existing CI gap recorded in `chore-and-fixes/`, not a new
one. Tests must skip loudly enough to be visible, following the existing pattern
in `spec.rs`.

**Scale is new.** Everything so far has run on a 500-neuron synthetic graph. 1.5M
edges will surface memory assumptions that have never been tested. Build it
simply first; do not pre-optimise.
