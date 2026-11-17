- **An agent in a devcontainer** — the server also listens on a socket in
  `.git/qreview/`, with a copy of its address, so an agent in a container
  that shares the project reaches the server on your machine with nothing
  to configure. The directory is private to you and goes away when the
  server stops.
