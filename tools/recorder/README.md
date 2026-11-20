# dereth-recorder

Records a play session so that what the server really sent can be looked at afterwards. It sits
between your client and the server on your own machine, passes everything on, and writes every
datagram to a file. Its reader then lists the session's messages, or only those about one object:
when something in the world looked wrong, you can see what the server told your client about it,
and when.

It works with both games: Auberean or Horizon against Yalain, and Dereth against Empyrean (or
any server of the same protocol). It listens on loopback only, and the recording stays on your
machine. A recording holds your account name and everything your character saw and said; share it
only with people you would show that to.

```text
cargo build --release -p dereth-recorder
```

## Recording a session

1. Start the recorder, naming the file and the server:

   ```text
   dereth-recorder record --out wasp.pcap --server 127.0.0.1:9000
   ```

   It listens on `127.0.0.1:9100` and `9101` and passes everything on to the server's port and
   the one above it. `--listen 127.0.0.1:<port>` listens elsewhere on loopback; `--server` takes
   a name or an address, `127.0.0.1:9000` by default. It never writes over a file.

2. Point the client at the recorder instead of the server:

   ```text
   auberean --server 127.0.0.1:9100 ...
   dereth-client -h 127.0.0.1 -p 9100 ...
   ```

3. Play. When Yalain sends you on to its world service, the recorder comes along: it changes the
   address in that one message to a second pair of its own ports, which pass the world's traffic
   on. The file keeps the message as the server sent it.

4. Press Ctrl-C to stop. The file is a standard packet capture (pcap), which a capture viewer can
   open too.

## Reading one

```text
dereth-recorder read wasp.pcap
dereth-recorder read wasp.pcap --object 0x0000000180000A5B
```

The first lists every message; the second only those that name the object, in order:

```text
session 1: 127.0.0.1:50311 to 127.0.0.1:9000, from 0.000 s, 41 messages
session 2: 127.0.0.1:50311 to 127.0.0.1:9002, from 1.204 s, 5873 messages
    93.412  #2 s2c  0x10058  CreateObject  its subject
    93.412  #2 s2c  0x1007F  PlayBehavior  its subject
   101.870  #2 s2c  0x10086  Position  its subject
```

Each line is the time in seconds since the recording began, the session, the direction (`s2c`
from the server, `c2s` from the client), the message's type and its name, and how it names the
object: as the object it is about (`its subject`), in a named field (`$.object`), or only by its
id somewhere in its bytes (`its id at byte 16`), which finds a target or a parent too. A message
the protocol could not read is marked `(unread)`.

An object id is typed in hexadecimal after `0x`, or in decimal. The reader finds the server's
ports from the logins in the recording; for a file that holds none, give them with
`--server-ports 9000-9003`.

The recording is also an ordinary capture for [`dereth-pcap`](../pcap/README.md):
`dereth-pcap ingest --roots <folder> --server-ports 9000-9003` indexes it with the others.
