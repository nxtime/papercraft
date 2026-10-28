# Security policy

## Supported versions

PaperCraft is an alpha. Security fixes go into the latest release and `main`.

## Reporting a vulnerability

**Do not open a public issue.** Use GitHub's private reporting: the repository's **Security** tab,
then **Report a vulnerability**. Include what you found, how to reproduce it, the version and
platform, and the impact you see.

A maintainer will acknowledge the report as soon as they can and keep you updated until it is
fixed. Please give us reasonable time to ship a fix before you disclose it.

## What counts

Examples of in-scope problems:

* the MCP server (`127.0.0.1:29980`) or the relay accepting requests it should reject (browser
  origins, DNS rebinding, another user);
* path traversal or code execution from a project folder, a design-system or library package, or
  an imported Figma, Excalidraw or SVG file;
* the updater or the agent integrations running something the person did not approve;
* secrets (an Anthropic key, relay tokens) written to disk unprotected or sent anywhere
  unexpected.

PaperCraft is local-first: it opens no network port beyond the loopback MCP server unless you
share a project through a relay you run.
