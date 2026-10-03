# FTP

An FTP connection kind for Ice Commander. A saved connection mounts the server
in a panel: listing, reading, writing, new folders, delete and rename.

## What it registers

- a connection kind, `ftp`, described by `src/ftp-fs/documents/ftp.json`; the
  host builds the form from that description;
- one asset, `ftp.svg`, under the owner `ic-ftp-fs`, shown in the connections
  list and the drives list.

It claims no file extensions and adds no columns, toolbar buttons or shell.

## The form

`name`, `folder`, `host`, `port`, `user`, `pass`, `remote_path`.

- `name`, `host` and `user` are required; `name` identifies the record.
- `port` is an integer from 1 to 65535; empty or unparsable input becomes 21.
- `pass` is masked and marked `secret`.
- `remote_path` is declared as `opens_at`: the panel opens there. It is not a
  prefix; the mount is the whole server.
- A connection is summarised as `user@host`, or `host` without a user.

Placeholders are `conn_manager.*` keys from the application's catalogues, with
English fallbacks in the description; the plugin ships no translations.

## Behaviour

- Paths are taken as absolute on the server: slashes and backslashes at either
  end are trimmed, backslashes become `/`, and the result is prefixed with `/`.
- One control connection per mount, opened on the first call and kept. Before
  each call it is checked with `PWD`; a call that fails is retried once on a new
  connection, and the second failure is reported. A failed connect or login is
  not retried.
- Settings without a user log in as `anonymous`; without a password, an empty
  one is sent. Settings without a host are refused.
- Listings come from `LIST`. Unix `ls -l` lines of nine or more fields starting
  with `d` or `-` and DOS `<DIR>` lines are read as such; any other line of four
  or more fields is read as a DOS file line; shorter lines are skipped. `.` and
  `..` are dropped.
- Delete sends `DELE`, then `RMD` if that fails.
- The reason for a failure is kept on the mount and returned by `last_error`.

## Known limitations

- Plain FTP only: no FTPS, so credentials travel in clear. Passive mode only.
- No timeout on connect or on the sockets; a server that stops answering
  blocks the call.
- No `TYPE` command is sent; transfers use the server's default type.
- Listings carry no modification time and no permissions.
- Unix symlinks and other special entries (`l`, `b`, `c`, `p`, `s`) fall into
  the DOS branch and show up under a mangled name. Runs of spaces inside a
  name collapse to one.
- Delete does not recurse: a folder goes only if the server's `RMD` accepts it,
  which on most servers means empty.
- A file is read and written whole, in memory.
- A failed listing shows as an empty folder; the reason is still kept in
  `last_error`.

## Building

```sh
./build.sh          # release build; libraries are copied into bin/
./test.sh           # cargo test --workspace
./deploy-local.sh   # copies bin/ libraries into the plugin folder
```

Cargo builds into `bin/target` (`.cargo/config.toml`). `ic-plugin-api` comes
from `https://github.com/ice-commander/plugin-api.git`, branch `main`.

`deploy-local.sh` installs into `~/Library/Application Support/ice-commander/plugins`
on macOS, `%APPDATA%/ice-commander/plugins` on Windows and
`${XDG_DATA_HOME:-~/.local/share}/ice-commander/plugins` elsewhere;
`IC_PLUGIN_DIR` overrides the target. It removes only libraries it deployed
earlier, as recorded in `bin/.deployed`. Then switch the plugin on in
**Settings → Plugins** and restart.

The reported version comes from `package.json`; `npm run gen-version` writes it
into `version.rs`.

## Licence

MIT or Apache-2.0, at your option, except the icon in `src/ftp-fs/assets/`,
which is from Icons8 — see [THIRD-PARTY-LICENSES.md](THIRD-PARTY-LICENSES.md).
Contributions are taken under the [DCO](DCO); sign off with `git commit -s`.
