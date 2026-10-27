# Specs and plans

These are the working records behind each piece of the workspace, kept because
they explain decisions the code alone does not.

- `specs/` — design documents, one per piece of work: what was built, why, and
  what was ruled out. The tolerances in the flyvis acceptance gates, for
  instance, are argued for here, and code and CI comments cite these files.
- `plans/` — step-by-step implementation plans written from a spec, with the
  task list, the code each step adds, and the commands that verify it.

Both were written with an AI coding-agent workflow: the
[superpowers](https://github.com/obra/superpowers) skills framework with
Claude Code, which is where the directory name and the "For agentic
workers" header at the top of each plan come from. `CLAUDE.md` at the repository root is that
workflow's standing instructions. Plans record what was intended at the time;
git history records what was built, and where the two differ, the code and its
tests are authoritative.

Several documents cite `docs/project-outline/`. That is the author's private
roadmap — epics, version plan, research notes — and it is not in this
repository, so those links do not resolve here.
