# Substrate &middot; [![GitHub license](https://img.shields.io/badge/license-GPL3%2FApache2-blue)](LICENSE) [![GitLab Status](https://gitlab.parity.io/parity/substrate/badges/master/pipeline.svg)](https://gitlab.parity.io/parity/substrate/pipelines) [![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](docs/CONTRIBUTING.adoc)

<p align="center">
  <img src="/docs/media/sub.gif">
</p>


Substrate is a next-generation framework for blockchain innovation 🚀.

## Polymath changelog

### [v2.0.1-2] - 2021-02-11
#### Changed
  - transaction-pool: drop unpropagable txs if local node can't author blocks, from [3c9b031 commit](https://github.com/paritytech/substrate/commit/3c9b031e449a6249dde07e00066848e0ee481ddf).
  - Check for equivocation report staleness on `validate_unsigned` as well, from [85c479f commit](https://github.com/paritytech/substrate/commit/85c479f2ed3a762de2629faffcdbb70a69e70a6b).

### [v2.0.1-1] - 2021-02-02
#### Changed
  - Transactions are now ordered by its `priority` and its `insertion_id`.
  - `CheckWeight` can be re-implemented external projects.

## Trying it out

Simply go to [substrate.dev](https://substrate.dev) and follow the 
[installation](https://substrate.dev/docs/en/knowledgebase/getting-started/) instructions. You can 
also try out one of the [tutorials](https://substrate.dev/en/tutorials).

## Contributions & Code of Conduct

Please follow the contributions guidelines as outlined in [`docs/CONTRIBUTING.adoc`](docs/CONTRIBUTING.adoc). In all communications and contributions, this project follows the [Contributor Covenant Code of Conduct](docs/CODE_OF_CONDUCT.md).

## Security

The security policy and procedures can be found in [`docs/SECURITY.md`](docs/SECURITY.md).

## License

- Substrate Primitives (`sp-*`), Frame (`frame-*`) and the pallets (`pallets-*`), binaries (`/bin`) and all other utilities are licensed under [Apache 2.0](LICENSE-APACHE2).
- Substrate Client (`/client/*` / `sc-*`) is licensed under [GPL v3.0 with a classpath linking exception](LICENSE-GPL3).

The reason for the split-licensing is to ensure that for the vast majority of teams using Substrate to create feature-chains, then all changes can be made entirely in Apache2-licensed code, allowing teams full freedom over what and how they release and giving licensing clarity to commercial teams.

In the interests of the community, we require any deeper improvements made to Substrate's core logic (e.g. Substrate's internal consensus, crypto or database code) to be contributed back so everyone can benefit.
