# Creem CLI setup and safe test commands

Research scope: official Creem primary sources only. No API keys were requested, read, logged, or persisted.

## Findings

- `https://creem.io/SKILL.md` identifies separate production and test API bases: `https://api.creem.io` and `https://test-api.creem.io`; test keys use the `creem_test_` prefix and live keys use `creem_`. It explicitly says to start in test mode because resources and data are separate.
- Official CLI installation is documented as either Homebrew (`brew tap armitage-labs/creem && brew install creem`) or npm (`npm install -g @creem_io/cli`); `npx @creem_io/cli --help` is also shown in the official CLI docs. Verify installation with `creem --version`.
- Authentication is documented as `creem login --api-key <test-key>` followed by `creem whoami`; logout is `creem logout`. The SKILL says the CLI stores configuration locally at `~/.creem/config.json`. Do not put a real key in shell history, Markdown, CI logs, or chat; use an interactive/secret-manager mechanism when available.
- Safe, read-only smoke tests after authentication: `creem whoami`; `creem config get environment`; `creem products list --json`; `creem customers list --json`; `creem subscriptions list --json`; and `creem transactions list --limit 1 --json`. Use only a test key and confirm the environment before any write.
- For machine-readable output, official docs recommend `--json`; the agent CLI docs also show resource listing commands such as `creem products list --json` and `creem subscriptions list --all --json`.
- Potentially mutating commands include product creation, checkout creation, cancellations, refunds, credits, and migration. Treat them as unsafe by default. The official SKILL specifically says to ask before destructive actions, prefer `--mode scheduled` for subscription cancellation, and never guess resource IDs.

## Ambiguities and inconsistencies

- The SKILL's CLI section documents `creem login --api-key ...`, while the agent-oriented official docs say authentication may use `CREEM_API_KEY` or `creem login`; the exact precedence and whether environment variables are supported by every CLI version are not fully specified. Prefer `creem login` only with a test key, or consult the installed CLI's `creem --help` without exposing the key.
- The SKILL shows `creem config set environment test/live`, but also says the environment auto-detects from the key prefix. Do not assume changing config alone changes the API target; verify with `creem config get environment` and `creem whoami`.
- Official examples differ between `creem products` (interactive browser) and `creem products list` (non-interactive). For automation, use explicit subcommands plus `--json`.
- The SKILL says `creem transactions list` and `creem subscriptions list`; the agent docs additionally show `--all` and other flags. Flag availability may be version-dependent, so check `creem <resource> list --help` before relying on a flag.
- The official SKILL includes example placeholders such as `creem_test_YOUR_KEY_HERE`; these are documentation placeholders, not safe credentials or a command to execute verbatim.

## Recommended no-key validation sequence

```bash
creem --version
creem --help
creem products list --help
creem whoami                 # only if already authenticated locally
creem config get environment # only if already configured locally
```

If a test credential is intentionally supplied by an authorized human, authenticate it only through the CLI's documented login path, then run the read-only smoke tests above. Never print or persist the credential.

## Primary sources

- [Creem SKILL.md](https://creem.io/SKILL.md)
- [Creem CLI documentation](https://docs.creem.io/code/cli)
- [Creem CLI for agents](https://docs.creem.io/ai/for-agents/cli)
- [Creem CLI for humans](https://docs.creem.io/ai/for-humans/cli)
