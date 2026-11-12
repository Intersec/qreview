#!/bin/sh
# A gerrit.auth.command that gives the HTTP password of a Gerrit account.
# Generate the password in the settings of Gerrit, under HTTP Credentials.
#
#   "gerrit": { "auth": { "command": ["/path/to/gerrit-auth-basic.sh"] } }
#
# qreview runs it at its first call to Gerrit, in the terminal where you
# started qreview, with QREVIEW_INTERACTIVE=1. It asks the user name and
# the password there, and keeps neither. QREVIEW_GERRIT_URL names the
# server that asks.
set -eu

if [ "${QREVIEW_INTERACTIVE:-}" != 1 ]; then
    echo "run qreview in a terminal: this command asks the Gerrit password there" >&2
    exit 1
fi

printf 'Gerrit at %s\n' "${QREVIEW_GERRIT_URL:-}" >&2
printf 'User name: ' >&2
read -r user
printf 'HTTP password: ' >&2
if [ -t 0 ]; then
    stty -echo
    # Ctrl-C must not leave the terminal with no echo.
    trap 'stty echo' EXIT
    trap 'exit 130' INT TERM
fi
read -r password
if [ -t 0 ]; then
    stty echo
fi
printf '\n' >&2

# printf is a builtin of the shell: the password appears in no command line.
basic=$(printf '%s:%s' "$user" "$password" | base64 | tr -d '\n')
printf '{"authorization":"Basic %s"}\n' "$basic"
