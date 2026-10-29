#!/bin/sh
# Build the landing page into a folder ready for GitHub Pages:
#   sh site/build.sh <out-dir> [<previous-build-dir>]
# Needs curl, jq and ffmpeg. GH_TOKEN, when set, authenticates the GitHub API call.
# RELEASE_JSON=<file> uses that file instead of asking the API. The demo video is re-encoded only
# when docs/demo.mp4 changed since <previous-build-dir>, so a release-only update stays cheap.
# <out-dir> must be new or empty.
set -eu
cd "$(dirname "$0")/.."

OUT="${1:?usage: sh site/build.sh <out-dir> [<previous-build-dir>]}"
PREV="${2:-}"
REPO="${GITHUB_REPOSITORY:-nxtime/papercraft}"

if [ -n "$(ls -A "$OUT" 2>/dev/null)" ]; then
    echo "$OUT is not empty; pass a new folder." >&2
    exit 1
fi
mkdir -p "$OUT/media/agents"

cp site/index.html site/site.css site/site.js "$OUT/"
cp assets/app-icon/papercraft.svg "$OUT/media/icon.svg"
for agent in claude openai cursor goose kimi mistral qwen cline githubcopilot nousresearch; do
    cp "assets/agents/$agent.svg" "$OUT/media/agents/"
done
for shot in hero chat annotations variables; do
    cp "docs/screenshots/readme/$shot.png" "$OUT/media/"
done
# GitHub Pages runs Jekyll otherwise, which is slower and has nothing to do here.
touch "$OUT/.nojekyll"

# The page reads this shape; site.js maps the API's own shape onto it the same way.
if [ -n "${RELEASE_JSON:-}" ]; then
    cp "$RELEASE_JSON" "$OUT/release.json"
else
    set -- -fsSL -H "Accept: application/vnd.github+json"
    if [ -n "${GH_TOKEN:-}" ]; then
        set -- "$@" -H "Authorization: Bearer $GH_TOKEN"
    fi
    curl "$@" "https://api.github.com/repos/$REPO/releases/latest" | jq '{
        tag: .tag_name,
        name: .name,
        publishedAt: .published_at,
        url: .html_url,
        assets: [.assets[] | {name, size, url: .browser_download_url, digest}]
    }' > "$OUT/release.json"
fi
jq -e '.tag and (.assets | length > 0)' "$OUT/release.json" > /dev/null

SRC_SUM="$(sha256sum docs/demo.mp4 | cut -d' ' -f1)"
if [ -n "$PREV" ] && [ -f "$PREV/media/demo.mp4" ] && [ "$(cat "$PREV/media/demo.source" 2>/dev/null)" = "$SRC_SUM" ]; then
    cp "$PREV/media/demo.mp4" "$PREV/media/poster.jpg" "$OUT/media/"
else
    # 720p30 without audio keeps the tour near 2 MB, against 26 MB for the 1080p60 source.
    ffmpeg -loglevel error -y -i docs/demo.mp4 -vf "scale=1280:-2,fps=30" -c:v libx264 -crf 28 \
        -preset slow -pix_fmt yuv420p -an -movflags +faststart "$OUT/media/demo.mp4"
    ffmpeg -loglevel error -y -ss 1.5 -i docs/demo.mp4 -frames:v 1 -vf scale=1280:-2 -q:v 4 \
        "$OUT/media/poster.jpg"
fi
echo "$SRC_SUM" > "$OUT/media/demo.source"

echo "Built $(jq -r .tag "$OUT/release.json") into $OUT"
