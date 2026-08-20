# Examples

Copy the one you need and change what you want.

| File | Where it goes |
|---|---|
| `config.json` | `~/.config/qreview/config.json`, your own settings |
| `languages.json` | `.qreview.json` at the top level of a repository |

A `.qreview.json` in a repository is read after your own file, so it wins.
Put the file types of a code base there and every reader of that repository
gets the map with no setup.

A grammar is data too. Drop a `.sublime-syntax` or `.tmLanguage` file in
`~/.config/qreview/grammars/` and name it in the map.
