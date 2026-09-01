- The server reads the file list of the other changes of the series while
  you read the first one, so opening the next change is instant instead of
  a second of rename and copy detection. It only reads while nothing is
  being answered, so it never takes the git a click is waiting on.
