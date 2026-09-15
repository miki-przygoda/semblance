# Resources

Every link below was checked as resolving during preparation (September 2026) via
live search/fetch. Things move; if one 404s later, the curated
[awesome-fly](https://github.com/cobanov/awesome-fly) index is the hub to
re-find it.

Grouped by how they map onto this project.

---

## Getting the data (the real connectome)

- **awesome-fly** — curated index of fly-connectome projects, models, and tools.
  Start here. <https://github.com/cobanov/awesome-fly>
- **Codex** — browser-based FlyWire / MaleCNS data explorer (no download needed to
  poke around). <https://codex.flywire.ai/>
- **MaleCNS project site** — male brain + nerve cord, cell types, releases.
  <https://male-cns.janelia.org/>
- **MaleCNS bulk download** — the actual connectivity tables (segment-to-segment
  weights, synapse partners) as downloadable files. This is the concrete on-ramp
  to an edge list you can feed `Connectome::from_edge_list`.
  <https://male-cns.janelia.org/download/>
- **MaleCNS at Janelia** — overview + neuroglancer scenes.
  <https://www.janelia.org/project-team/flyem/male-cns-connectome>
- **FlyConnectome CAVE tutorial** — how to pull the FlyWire female-brain connectome
  programmatically. <https://github.com/seung-lab/FlyConnectome>
- **fly connectome data tutorial** — accessing all the major fly datasets in one
  place. <https://github.com/sjcabs/fly_connectome_data_tutorial>

### Tooling for wrangling it (Python/R)

- **CAVEclient** — Python client for live FlyWire connectome access.
  <https://github.com/CAVEconnectome/CAVEclient>
- **navis** — neuron analysis & visualisation, works across datasets.
  <https://github.com/navis-org/navis>
- **fafbseg-py** — FlyWire/FAFB-specific tools, interoperable with navis.
  <https://github.com/navis-org/fafbseg-py>
- **flywire_annotations** — cross-checked cell types / annotations for FlyWire.
  <https://github.com/flyconnectome/flywire_annotations>
- **natverse/malecns** — R access to the MaleCNS dataset.
  <https://github.com/natverse/malecns>

> Practical path: use one of the Python tools to export a `pre,post,weight` edge
> list for the subgraph you care about, then load it in Rust via
> `Connectome::from_edge_list`. Start with a region/subgraph, not the whole brain.

---

## Compression & low-dimensional structure

Why compressing a connectome without destroying function is plausible.

- **Compression supports low-dimensional representations of behavior across neural
  circuits** — dimensionality reduction as lossy compression, tied to behaviour.
  <https://arxiv.org/pdf/2211.16599>
- **Circuit design in biology and machine learning (I): random networks and
  dimensional reduction** — the "hourglass" waist; circuits self-compress to a
  low-dimensional core. <https://arxiv.org/pdf/2408.09604>
- **Predicting Neural Activity from Connectome Embedding Spaces** — explicitly a
  *lossy* low-dimensional embedding of connectomic structure.
  <https://openreview.net/pdf/0f9f4d51650e755458a3e6e4cc3ef88b11c81bc1.pdf>
- **Variational autoencoder-based neural network model compression** — compressing
  network *parameters* through a latent space (directly relevant to `ReducedCodec`
  if you go the autoencoder route). <https://arxiv.org/pdf/2408.14513>
- **Latent space of a small genetic network (Drosophila gap genes)** — an
  autoencoder compressing a real Drosophila system to a 2D latent with minimal
  information loss; a clean worked example of the encode-inspect idea in biology.
  <https://www.pnas.org/doi/10.1073/pnas.2113651119>

---

## Indirect / generative encoding (small genome → big brain)

This is the family your compress→decode step belongs to.

- **HyperNEAT — a hypercube-based encoding for evolving large-scale neural
  networks** — the foundational "compact CPPN generates a big network".
  <https://pubmed.ncbi.nlm.nih.gov/19199382/>
- **Deep HyperNEAT** — evolving the size/depth of the generated substrate.
  <https://web.mit.edu/fsosa/www/papers/dhn18.pdf>
- **GENE — Searching Search Spaces: meta-evolving a geometric encoding** — projects
  neurons into a small latent and evolves coordinates; ~order-of-magnitude smaller
  genome for the same network. The closest thing to "evolve compact coordinates,
  expand to full wiring". <https://arxiv.org/pdf/2403.14019>
- **HyperNCA — growing developmental networks with neural cellular automata** —
  another compact-generator route to full connectivity.
  <https://arxiv.org/pdf/2204.11674>
- **Meta-Learning an Evolvable Developmental Encoding** — learning an encoding that
  is itself good to evolve in. <https://arxiv.org/pdf/2406.09020>

---

## Evolving in a latent space

- **Evolutionary Planning in Latent Space (EPLS)** — evolve in a learned compressed
  space rather than the full space, for speed. Same move as your step 3.
  <https://arxiv.org/pdf/2011.11293>

---

## Fly connectome as a model constraint (the adjacent prior art)

What the field currently does with the fly connectome — useful to know so you can
say precisely how your project differs (they fix the wiring; you compress/evolve
it and measure behaviour).

- **Connectome-constrained networks predict neural activity across the fly visual
  system (flyvis, Nature 2024)** — fixed connectome, tuned weights, predicts
  recorded activity. Paper: <https://www.nature.com/articles/s41586-024-07939-3> ·
  Code: <https://github.com/TuragaLab/flyvis>
- **Whole-body / embodied fruit fly (flybody, Nature 2025)** — a MuJoCo fly body +
  RL; the "give the brain a body and a world" direction.
  Paper: <https://www.nature.com/articles/s41586-025-09029-4> ·
  Code: <https://github.com/TuragaLab/flybody>

---

## How these map to the crates

- Data section → `connectome` (loaders).
- Indirect-encoding + compression sections → `codec` (`ReducedCodec`).
- Latent-evolution section → `evo`.
- flyvis/flybody → contrast for `DESIGN.md`'s gap argument, and a reference for
  designing `sim` stimuli/readouts and `fidelity` probes.
