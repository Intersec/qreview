#!/usr/bin/env bash
# Publish one release on the GitLab server that runs this pipeline.
#
# Two steps. The binary goes to the generic package registry, then the
# release is created and links to it. Everything talks to $CI_SERVER_URL, the
# server the job already belongs to. Nothing reaches any other host.
#
#   scripts/gitlab-release.sh <tag> <binary> <notes file>
#
# It reads the variables GitLab sets in a job: CI_API_V4_URL, CI_PROJECT_ID
# and CI_JOB_TOKEN.

set -euo pipefail

tag=${1:?usage: gitlab-release.sh <tag> <binary> <notes file>}
binary=${2:?usage: gitlab-release.sh <tag> <binary> <notes file>}
notes=${3:?usage: gitlab-release.sh <tag> <binary> <notes file>}

: "${CI_API_V4_URL:?not in a GitLab job}"
: "${CI_PROJECT_ID:?not in a GitLab job}"
: "${CI_JOB_TOKEN:?not in a GitLab job}"

api="${CI_API_V4_URL}/projects/${CI_PROJECT_ID}"
name=$(basename "$binary")
# The package registry wants a version without the leading v.
url="${api}/packages/generic/qreview/${tag#v}/${name}"

call() {
    curl --fail --silent --show-error --header "JOB-TOKEN: $CI_JOB_TOKEN" "$@"
}

echo "uploading $name to the package registry"
call --upload-file "$binary" "$url"
echo

# A job that is run again finds its release already there. Leave it alone:
# someone may have written on it since.
if call --output /dev/null "${api}/releases/${tag}" 2>/dev/null; then
    echo "the release $tag is already there, nothing to create"
    exit 0
fi

# node writes the JSON, because the notes are Markdown and quoting them by
# hand is how a release ends up with a broken description.
body=$(mktemp)
trap 'rm -f "$body"' EXIT
TAG="$tag" NAME="$name" URL="$url" NOTES="$notes" node -e '
const { readFileSync, writeFileSync } = require("node:fs");
const { TAG, NAME, URL, NOTES } = process.env;

writeFileSync(process.argv[1], JSON.stringify({
    name: `qreview ${TAG}`,
    tag_name: TAG,
    description: readFileSync(NOTES, "utf8"),
    assets: { links: [{ name: `${NAME}, static`, url: URL, link_type: "package" }] },
}));
' "$body"

echo "creating the release $tag"
call --header "Content-Type: application/json" --data "@$body" \
    --output /dev/null "${api}/releases"
echo "the release $tag is published"
