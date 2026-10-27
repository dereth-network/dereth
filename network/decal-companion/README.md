# The Dereth Network companion for Decal

> **Very experimental. Not recommended for general use.** Nothing builds or releases it: it is
> built by hand from this folder, and it may change or go away.

A Decal plugin for the retail client: one window over the Dereth Network's `@dn` commands and the
hub's public list of worlds. It works on any world on the network, Empyrean or ACE with the
[ACE mod](../ace-mod/README.md).

| page | what it shows | what it types |
|---|---|---|
| Status | on or off the network, your address, the hub's link, your directory page, the Discord link code, the world's rules for plugins | `@dn`, `@dn on`, `@dn off`, `@dn link`, `@dn unlink` |
| Friends | your friends on other worlds, online first, kept up to date by the world's online and offline lines | `@dn friends`, `@dn friends add Name@world`, `@dn friends remove Name@world` |
| Channels | your channels, a channel's members, invitations, the last lines said in them | `@dn channel list`, `members`, `create`, `join`, `leave`, `invite` |
| Global | the last lines from other worlds in the Global room | `@dng <text>` |
| Directory | a search of the network's character directory | nothing: it asks the hub |
| Worlds | every listed world with its live status and players, refreshed each minute while shown | nothing: it asks the hub |

**How it works.** It types each command through the game's own chat parser, exactly as you could,
and reads the world's answers in the chat window. It types only when you click: a button, a row,
or a page shown (each page asks at most once in 10 seconds). The answers to its own questions are
kept out of the chat window; everything else, the answers to commands you type included, stays.
It reads no game memory and calls nothing inside the client. The hub is reached over HTTPS
(`https://hub.dereth.network`, or `hub = <url>` in a `settings.txt` beside the plugin).

**A world's rules for plugins.** Once the companion knows your world (from the `@dn` status line),
it reads the world's entry on the hub and shows its plugin policy (`plugins.policy`, the document
or an `https://` address of it, as the launcher reads it). Nothing in a policy stops the companion,
which automates nothing. No hub publishes a policy yet: the hub's registration has no field for
one, so today the page says the world publishes none.

## Building and testing

The plugin targets .NET Framework 4.8, 32-bit, and compiles against your Decal install:

```sh
dotnet build Dereth.Network.Companion/Dereth.Network.Companion.csproj -c Release -p:DecalDirectory="C:\Games\Decal 3.0"
dotnet test tests/Companion.Tests.csproj
```

The tests need no Decal and no game. They read the world's answers as the ACE mod and Empyrean
write them, ask a stand-in hub over loopback HTTP, and drive the window through a stand-in for
Decal's controls.

## Installing

With Decal installed and the game closed, in an elevated PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File install\install.ps1 -From <the folder the build wrote Dereth.Network.Companion.dll to>
```

It copies the plugin to `C:\Games\Dereth\Companion` (`-To` names another folder) and adds it to
Decal's plugins. `install\uninstall.ps1` takes it out again.

## Trying it by hand

1. Install it, start the game with Decal, and log in on a world on the network.
2. Click Dereth Network on Decal's bar. The Status page asks `@dn`; the answer shows on the page,
   not in chat.
3. Join and Leave type `@dn on` and `@dn off`. Their answers show in chat and on the page.
4. On Friends, type `Name@world` and click Add; on another client, log that character in and out
   and watch its row change.
5. On Channels, make a channel, invite a second character, accept on the other client from its
   Channels page, and pick the channel to see its members.
6. On Global, say a line and see it on another world.
7. Click Link to Discord; the link and its code show on the Status page.
8. On Directory, search for a listed character; on Worlds, see every world's status.
