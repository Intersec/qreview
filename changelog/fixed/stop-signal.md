- The server removes its address and its socket when it stops on `SIGTERM`
  or `SIGHUP`, as on Ctrl-C. An agent that ends the background task of the
  server, or a terminal that closes, stops it cleanly.
