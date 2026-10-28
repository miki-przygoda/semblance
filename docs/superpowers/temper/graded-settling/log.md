# graded-settling — temper log

Epic 0.3.0, piece 1 (epic tasks 2, 3, 4): the ensemble loader, the graded
integrator and the settling gates.

- Spec: `docs/superpowers/specs/2026-10-28-graded-settling-design.md`
- Plan: `docs/superpowers/plans/2026-10-28-graded-settling.md`
- Branch: `graded-settling`, from `main` at `ce1b079`. Host: local.
- Gates: `.temper/gate-quick.sh` (build, every test with fixtures required,
  clippy) and `.temper/gate-full.sh` (CI's steps plus the 1.85 build and the
  8-package check). Both are local scripts, not in git.

Entries below are appended by temper's stages and by the head.
