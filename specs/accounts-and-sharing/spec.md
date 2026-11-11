# Accounts, sharing and the web build's URLs

Asked for on 2026-11-11: links that open a design, sign-in for more than one person, designs shared
between people and across the desktop, web and phone apps, a model picker people can see in the
web chat, settings on the home screen, per-project visibility, and agents that run on each
person's own credentials instead of the server owner's.

Order: each step is its own PR, CI run and deploy.

## 1. URLs (this change)

* `#/d/<project id>/<page id>` opens a design and its page. The home screen has no hash. Ids come
  from the manifest (`new_id`), so a link survives a rename.
* Opening a design or a page pushes a history entry, and back and forward move through them. A
  pasted link opens what it names. A link to a design that has not synced to this browser yet
  waits up to 15 s, then says so.
* A hash route needs no server rewrite and stays clear of the relay's `/p/`, `/s/` and `/tab/`
  paths. `#demo` keeps working.
* Code: `src/route.rs` (parse and format, unit tests), `src/web/routing.rs` (URL ↔ app state).

## 2. Model picker in the web chat

The hosted chat already gets the agent's `Models` event. Show the current model and let people
pick one the way the desktop chat does (`src/ui/chat_models.rs`). The tab sends the pick as
`model`, and the relay already passes it to the agent.

## 3. The user's own credentials (BYOK)

* Turn on the API-endpoint chat (`src/chat/anthropic.rs`, Settings → Chat → API endpoints) in the
  browser build. The browser calls the provider directly: Anthropic with
  `anthropic-dangerous-direct-browser-access`, or OpenRouter and other OpenAI-format hosts that
  allow CORS.
* Keys stay in that browser's storage. They never reach the relay, and the server never pays
  for them.
* The server's hosted agents become an optional fallback that the owner can switch off.

## 4. Settings on the home screen

The home screen gets a Settings entry. Theme, chat and the endpoint keys from step 3 live there
too, so people can set things up before they open a design.

## 5. Accounts and visibility

* **Identity:** Logto through oauth2-proxy (already in front of the public site). The relay
  reads the signed-in email that oauth2-proxy forwards, and trusts it only from the proxy.
* **Storage:** each account gets its own library under `--data/users/<id>/`, instead of one
  shared `--library`.
* **Roles:** `admin` (the owner, from a config list) and `member`.
* **Visibility per project:**
  * `private`, the default: only the owner.
  * `shared`: people with the link, or named accounts, who can edit together. This uses the
    existing merge and presence code (roadmap 34).
  * `public`: anyone can view it read-only. Only an admin can set it.
* **Desktop, phone and MCP clients:** they cannot pass the browser login. They get a per-account
  token from the web Settings, used the way share links are today. The phone app is the web
  build; a native mobile app is out of scope.

Open questions for step 5: whether members can invite others; whether public designs are listed
on a page or only reachable by link; and how existing shared-library projects are moved to the
owner's account.
