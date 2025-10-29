# Contributing to **INTMAX2**

> _We scale Ethereum without sacrificing privacy. You can help._

Thank you for your interest in improving **INTMAX2**, the zero‑knowledge roll‑up developed by the Internet Maximalism community. Whether you spot a typo, design a new feature, or help triage issues, your contribution makes the project stronger. This guide explains how to participate effectively and respectfully.

---

## Why read these guidelines?

Following the steps below shows respect for the maintainers’ time and helps us review your work quickly. In return, we will do our best to respond promptly, give constructive feedback, and merge high‑quality changes.

## What kinds of contributions are welcome?

- **Code** — Rust changes that improve functionality, performance, or security.
- **Documentation** — Tutorials, API references, FAQs, diagrams, or translations.
- **Testing & QA** — Bug reports, reproducible test cases, and adding automated tests.
- **Dev UX** — Build scripts, Dockerfiles, CI/CD, developer tools.
- **Community support** — Answering questions on Discord, writing blog posts, or recording demo videos.

## Contributions we are _not_ looking for

- **End‑user support requests** — Please open a ticket at [intmaxhelp.zendesk.com](https://intmaxhelp.zendesk.com/hc/en-gb/requests/new) instead of the GitHub issue tracker.
- **Exchange listing questions / price talk** — Off‑topic for this repository.
- **Security disclosures in public issues** — See the security section below.

---

## Ground rules

- Be respectful, inclusive, and patient — read and follow our [Code of Conduct](./CODE_OF_CONDUCT.md).
- Discuss large changes in an issue _before_ starting work.
- All code must pass pre-commit checks via Lefthook:
  - `rustfmt --edition 2021 --emit files {staged_files}` for formatting (`cargo-fmt`)
  - `cargo clippy --tests -- -D warnings -D clippy::perf -D clippy::correctness` for linting (`cargo-clippy`)
  - `typos {staged_files}` for typo detection (`check-typos`)
- Write tests for all new Rust crates (`cargo test --all-features`).
- Keep PRs focused: one feature or bug‑fix per pull request.
- Never commit secrets, private keys, or user data.

---

## Your first contribution

Newcomers are welcome! Look for issues labeled **`good first issue`** or **`help wanted`**:
[https://github.com/InternetMaximalism/intmax2/issues?q=is%3Aopen+label%3A%22good+first+issue%22](https://github.com/InternetMaximalism/intmax2/issues?q=is%3Aopen+label%3A%22good+first+issue%22)

If you’re unsure, ask in **#dev-general** on our [Discord](https://discord.gg/TGMctchPR6).

For a gentle introduction, consider improving docs or adding small tests.

---

## Getting started (Workflow)

1. **Fork** [https://github.com/InternetMaximalism/intmax2](https://github.com/InternetMaximalism/intmax2) and clone your fork.
2. **Create a branch**: `git checkout -b feat/short-description`.
3. **Set up the tool‑chain**

   ```bash
   find . -name ".env.example" -exec sh -c 'cp "$1" "${1%.example}"' _ {} \;
   docker compose up db -d
   (cd store-vault-server && sqlx database reset -y && \
   cd ../legacy-store-vault-server && sqlx database reset -y && sqlx database setup && \
   cd ../validity-prover && sqlx database reset -y && sqlx database setup && \
   cd ../withdrawal-server && sqlx database reset -y && sqlx database setup)
   ```

4. **Run the test suite** to ensure your environment is healthy:

   ```bash
   cargo fmt --all
   cargo clippy --all-targets --all-features
   cargo test --all-features
   ```

5. **Commit** using [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

   - feat: add deposit‑to‑intmax2 command
   - fix(wallet): handle zero‑gas meta‑tx edge‑case

6. **Push** and **open a Pull Request** against `dev` (not `main`).
7. Complete the PR checklist; a maintainer will review within **5 business days**.

### Obvious fixes

Typo or whitespace fixes that don’t change functionality can be submitted without opening an issue first.

---

## Bug reports

If something isn’t working:

1. Search existing issues first.
2. Open a **new issue** with the template and include:

   - INTMAX2 commit hash / Docker image tag
   - OS and architecture (e.g. Ubuntu 22.04 x86‑64)
   - Steps to reproduce (commands, transaction hashes, etc.)
   - Expected vs. actual behavior
   - Logs / screenshots when possible

### Security vulnerabilities

Do **not** open a public issue. Email **[support@intmax.io](mailto:support@intmax.io)** with details and we will coordinate a responsible disclosure.

---

## Feature requests & improvements

Open an issue describing:

- **What problem** you are solving and who benefits
- **Proposed solution** (API sketch, UX flow, or pseudocode)
- **Alternatives** you considered

A maintainer will discuss scope and alignment with the project roadmap before anyone starts coding.

---

## Code review process

- At least **two approvals** from core maintainers are required.
- CI must be green before merging.
- The PR author (or a maintainer) should **squash‑merge** once reviews pass.
- Inactive PRs with no activity for **30 days** may be closed — feel free to reopen when ready.

---

## Community & support

- **Discord**: [https://discord.gg/TGMctchPR6](https://discord.gg/TGMctchPR6) (`#dev-general`)
- **GitHub Discussions**: [https://github.com/InternetMaximalism/intmax2/discussions](https://github.com/InternetMaximalism/intmax2/discussions)
- **Support portal (private)**: [intmaxhelp.zendesk.com](https://intmaxhelp.zendesk.com/hc/en-gb/requests/new)

---

## Coding, commit, and label conventions

| Area         | Convention                                                                                |
| ------------ | ----------------------------------------------------------------------------------------- |
| Rust         | `cargo fmt`, forbid warnings in CI                                                        |
| Commits      | **Conventional Commits** (`feat:`, `fix:`, `chore:` …)                                    |
| Issue labels | `bug`, `enhancement`, `good first issue`, `help wanted`, `security`, `docs`, `discussion` |

---

### Thanks

INTMAX2 exists because of contributors like **you**. We appreciate your time and effort to make private, scalable blockchain infrastructure a reality.
