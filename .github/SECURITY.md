# Security Policy

## Reporting a Vulnerability

If you believe you've found a security issue in errex, **do not open a
public issue**. Use GitHub's private vulnerability reporting:

1. Go to the [Security tab](https://github.com/TheHoltz/errex/security/advisories/new).
2. File a new advisory with reproduction steps and impact.

errex is a small project with no triage queue — direct contact is the
fastest path. Expect a response within a few days. If the issue is
confirmed, we'll work with you on a fix and coordinated disclosure
before publishing the advisory.

## Supported Versions

errex is **alpha**. Only the latest commit on `main` is supported. There
are no released versions yet.

| Branch | Supported |
|---|---|
| `main` | ✅ |
| anything else | ❌ |

## Out of Scope

- Issues that require physical access to the host running errex.
- DoS via unbounded ingest when the operator hasn't set
  `ERREXD_RATE_LIMIT_PER_MIN` (rate limiting is opt-in by default).
- Findings against deprecated / removed code paths in `git log`.
