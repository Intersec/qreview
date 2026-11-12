- **`gerrit.auth.command`** — a command of yours gives the authorization of
  the REST API of Gerrit: a token of OIDC, or an HTTP password. It runs at
  the first call to Gerrit, in the terminal of `qreview`, where it may ask
  you to log in. `examples/gerrit-auth-basic.sh` asks the HTTP password.
