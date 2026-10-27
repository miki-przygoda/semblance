# Security

Please report a vulnerability privately, through GitHub's private vulnerability
reporting: the **Security** tab of this repository, then **Report a
vulnerability**. Do not open a public issue for it.

This is offline research code with no network service. The parts most worth a
look are the ones that handle downloaded files:

- `ci/fetch-fixtures.sh`, which downloads the fixture data and verifies every
  file against `ci/fixtures.sha256` before using it;
- the Python in `tools/`, which loads flyvis's PyTorch checkpoints. Loading a
  checkpoint can run code, so only point those scripts at checkpoints from
  flyvis's own published source.

Only the latest commit on `main` is supported.
