# Deploying the web build and its relay

One image holds the browser build and `papercraft-relay`. The relay serves the web app, keeps the
shared library on the data volume and, with `--agents`, runs Claude Code and opencode for a tab so a
phone can design by chatting. `k8s.yaml` is the whole manifest. Tailscale and the Logto/OIDC sign-in
sit in front of it (add the Tailnet ingress with `expose papercraft/papercraft papercraft`); the
relay itself has no accounts.

## What the agents may do

The tab sends chat messages over its relay socket; the relay starts the agent in an empty folder
under `/data/agents` and points it at that tab's own link (`http://127.0.0.1:8787/s/<token>/mcp`),
so its tool calls drive the person's open tab.

* Claude Code gets only the PaperCraft MCP tools. Its built-in tools (Bash, Edit, Write, Read, web,
  sub-agents) are removed and denied, and nothing is ever prompted for.
* opencode gets its built-in tools switched off and every permission denied, so only the MCP server
  runs.
* At most 2 agent processes run at once (over every tab); one that says nothing for 10 minutes is
  stopped, and so is every agent of a tab that leaves.
* Credentials come from the pod's environment only (Secret `papercraft-agents`). The tab never sends
  any, and the tab token is never logged.

## Build

`craft-libs` is a private Gitea dependency, so the image cannot fetch it. Vendor the crates on the
host, where you are signed in to Gitea, and hand them to the build:

```sh
mkdir -p /tmp/papercraft-vendor
cargo vendor --locked /tmp/papercraft-vendor/crates > /dev/null

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
TAG=$VERSION-$(git rev-parse --short HEAD)
docker build -f deploy/Dockerfile --build-context vendor=/tmp/papercraft-vendor \
  -t localhost:32000/papercraft-web:$TAG .
docker push localhost:32000/papercraft-web:$TAG
```

Never reuse a tag: the node caches images by tag and the pod would keep running the old one. A
rebuild of the same commit needs a new suffix (for example `-r2`). The agent CLIs are pinned by the
`CLAUDE_CODE` and `OPENCODE` build args in the Dockerfile; change them on purpose, not by rebuilding.

## Credentials

Make the Secret once. It is optional in the manifest, so the pod also starts without it, but the
agents then answer that they are not signed in.

```sh
kubectl -n papercraft create secret generic papercraft-agents \
  --from-literal=CLAUDE_CODE_OAUTH_TOKEN=...
```

Get the token with `claude setup-token` on a machine that is signed in to a Claude plan. Use
`--from-literal=ANTHROPIC_API_KEY=...` instead to bill an API account. opencode reads its own
provider variables from the same Secret (for example `OPENROUTER_API_KEY`); add them as more
`--from-literal` pairs. Changing the Secret needs a restart:
`kubectl -n papercraft rollout restart deploy/papercraft`.

## Roll out

1. Set `image:` in `deploy/k8s.yaml` to `localhost:32000/papercraft-web:$TAG`.
2. `kubectl apply -f deploy/k8s.yaml`
3. `kubectl -n papercraft rollout status deploy/papercraft`
4. Check the pod: `kubectl -n papercraft logs deploy/papercraft` shows the relay listening and no
   error, and `kubectl -n papercraft exec deploy/papercraft -- claude --version` prints the pinned
   version.
5. Open the site on a phone, open Chat, and pick Claude Code or OpenCode. The picker only appears
   when the relay was started with `--agents`; without it the panel says the server runs no agents.

The container runs as uid 10001 with a read-only root filesystem. The agents keep their config and
sessions under `HOME=/data/agent-home` on the data volume, and scratch files in `/tmp` (an
`emptyDir`). The resource limits (2 CPU, 2Gi) are sized for two agent processes at once.

## Roll back

Set `image:` back to the previous tag and `kubectl apply -f deploy/k8s.yaml`. Projects live on the
volume and are untouched.
