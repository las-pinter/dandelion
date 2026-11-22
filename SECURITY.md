# Security policy

Dandelion controls the machine it runs on, so a vulnerability in it can mean
full control of that machine. Please report security issues privately.

## Reporting

Use GitHub's private vulnerability reporting: **Security → Report a
vulnerability** on this repository. Do not open a public issue.

Include the affected version or commit, the steps to reproduce, and the
impact you expect. You will get an acknowledgement, and a fix or mitigation
plan once the report is confirmed.

## Supported versions

Only the latest release and `main` receive security fixes.

## Deployment model

Dandelion is designed to listen only on localhost and a Tailscale address,
behind its own login. Exposing it to the public internet is unsupported.
