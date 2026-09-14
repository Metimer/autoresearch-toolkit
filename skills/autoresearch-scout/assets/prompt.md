# Autoresearch session

Fill this template with observed evidence before starting; never invent values.

## Goal and budget
- Goal:
- Maximum iterations:
- Wall-clock deadline:
- Per-command timeout:
- Network/dependency restrictions:

## Baseline identity
- Repository, branch, HEAD:
- Environment and versions:
- Workload/input identity and cache policy:
- Primary metric, unit and direction:
- Three baseline rounds and noise spread:
- Checks commands, provenance and results:
- Behavioral invariants and test identities/counts where relevant:

## Execution
- From repository root: bash .auto/measure.sh
- From repository root: bash .auto/checks.sh
- Estimated iteration duration:
- METRIC output name:

## Scope
- Allowed paths:
- Protected paths (tests, fixtures, benchmark inputs, configs, dependencies):
- Candidate hot paths and evidence:
- Reference: .auto/context.md

## Acceptance
- Improvement must exceed observed measurement noise with green checks.
- No behavior/coverage/quality regression.
- Session methodology remains fixed; append-only result logs are allowed.
- Commit policy and author, only if commits are authorized:
