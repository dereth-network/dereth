# dereth-headless

The Dereth client with no window, no graphics device and no interface, driven by a text script of
actions. It replays recorded sessions for tests, logs on to a server, runs plugins, and, as a
**daemon**, stays in the world on its own: a buffbot, a trader, anything a plugin does unattended.

```text
dereth-headless --script <file>
dereth-headless --daemon --connect <host:port> --account <name> --password-file <file> --script <file>
```

`dereth-headless --help` lists every switch and script command.

## A script

One command per line; `#` starts a comment.

```text
enter Buffbel           # enter the world as Buffbel (created first, a soldier, when missing)
run 5                   # five seconds at the wall clock's pace
plugin load plugins/buffbot
say /buffbot on         # a line typed in the chat bar
run forever             # until stopped (Ctrl+C) or the connection is lost
```

`run <secs>` runs the client for so many seconds; `run forever` runs it until Ctrl+C (or a
termination request) stops it, or its connection is lost, which ends the script with an error.
Ctrl+C stops the client at its next frame, and it logs out as it shuts down.

## The daemon

`--daemon` keeps a client in the world until it is stopped. The script is each connection's
setup: the daemon runs it, then runs on as `run forever`. When the connection is lost (the server
restarted, the network failed, the character was logged off), that client is shut down, and after
a wait a new one connects and runs the script again. The wait is 5 seconds after a connection that
reached the world, and doubles with each attempt in a row that did not (10, 20, 40 seconds), up to
a minute. A client notices a server that went silent as the game's own client does, after about
two minutes and twenty seconds; a server that closes the connection is noticed at once.

What plugins keep (`--plugin-store <dir>`) survives a reconnect, so a plugin that keeps "on" with
the character (the Buffbot does) carries on where it was. A daemon given a settings folder
(`DERETH_SETTINGS_DIR`, as for the desktop client) keeps there what its journeys learn for each
world, a portal that refused it among them, so the next connection does not try it again.

A client stopped without logging off (its process killed) leaves its session on the server for a
while. The next logon to that account is refused (character error 1, "the account was still in
the world"), and the server logs the old session off as it refuses; the daemon's next attempt, 10
seconds later, gets in.

**The password** never goes on the command line, where other programs can read it: it is the
first line of the file `--password-file` names, else the `DERETH_PASSWORD` environment variable.

**The log file** (`--log-file <file>`, appended to) holds, each line with its time: the daemon's
connections and why each ended (`daemon`), every chat line (`chat`), every line a plugin prints
(`dereth::plugin`; the Buffbot's requests, casts, fizzles and failures among them),
the plugin host's own lines (`dereth::plugins`), and any warning.

```text
dereth-headless --daemon --connect play.example.org:9000 --account mybuffbot \
  --password-file buffbot-password.txt --world --plugin-store buffbot-store \
  --plugin-data buffbot-data --log-file buffbot.log --script buffbot.script
```

`--world` builds the world with no device, so the character stands in it and plugins that move it
(a buffbot asking a player to come closer does not; a macro does) can. Run several daemons on one
machine, each with its own account, store and log.

## Other switches

- `--dat-dir <dir>`: the retail dats (else the working directory, or the program's).
- `--plugins <dir>`: load the plugin package in a folder, or every package in it, at the start.
- `--plugin-data <dir>`, `--plugin-store <dir>`: the plugins' data folders and stores (in memory
  when absent).
- `--plugin-policy <file>`: the world's plugin policy, as its world entry publishes it.
- `--nav-published <file>`: the server's published portal table, for journeys.
- `--log <filter>`: the log on stderr (`info` by default).
