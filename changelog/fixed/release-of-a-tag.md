- Pushing a tag releases it. The tag is annotated, so `git push
  --follow-tags` carries it, and the pipeline that starts publishes the
  static Linux x86-64 binary through the GitLab server of the project, with
  no other host in the path.
