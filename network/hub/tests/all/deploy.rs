//! The deployment guide and the container image hold to what a public hub needs: a service
//! confined as tightly as it can run, its secret handed over by systemd, one refresh of
//! Cloudflare's ranges that never leaves a bad list behind, a container that trusts its proxy's one
//! address, a restore that says what it brings back, and a daily import of the community list.
//! Fixture: `network/hub/DEPLOY.md` and `network/hub/Dockerfile`, read as text.

fn read(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The first fenced block of `lang` after `marker` in `text`.
fn block_after<'a>(text: &'a str, marker: &str, lang: &str) -> &'a str {
    let at = text
        .find(marker)
        .unwrap_or_else(|| panic!("{marker} is in the guide"));
    let fence = format!("```{lang}\n");
    let start = text[at..].find(&fence).expect("a block") + at + fence.len();
    let end = text[start..].find("```").expect("the block ends") + start;
    &text[start..end]
}

/// Spec: hub: the guide's service runs confined: no capabilities, no new namespaces, the system
/// calls a service makes, other processes hidden, files private, and the Discord secret handed over
/// by systemd from a file only root reads.
#[test]
fn the_service_is_confined_and_given_its_secret_by_systemd() {
    let guide = read("DEPLOY.md");
    let unit = block_after(
        &guide,
        "`/etc/systemd/system/dereth-network-hub.service`:",
        "ini",
    );
    for directive in [
        "UMask=0077",
        "CapabilityBoundingSet=\n",
        "RestrictNamespaces=true",
        "SystemCallFilter=@system-service",
        "ProtectProc=invisible",
        "NoNewPrivileges=true",
        "ProtectSystem=strict",
        "LoadCredential=discord-client-secret:/etc/dereth-network-hub/discord-client-secret",
        "--discord-client-secret-file %d/discord-client-secret",
    ] {
        assert!(unit.contains(directive), "{directive:?} in the unit");
    }
    assert!(
        !unit.contains("EnvironmentFile"),
        "no secret in the environment"
    );
    assert!(!guide.contains("DERETH_HUB_DISCORD_CLIENT_SECRET="));
    assert!(guide.contains("install -d -o root -g dereth-hub -m 0750 /etc/dereth-network-hub"));
    assert!(guide.contains(
        "install -o root -g root -m 0600 /dev/null /etc/dereth-network-hub/discord-client-secret"
    ));
    assert!(
        guide.contains("sudo -u dereth-hub DERETH_HUB_DB="),
        "the command line runs as the hub's user"
    );
}

/// Spec: hub: Cloudflare's ranges are refreshed by one script that replaces the list only when
/// the download reads, and makes the proxy's and the firewall's rules from that same list.
#[test]
fn the_cloudflare_ranges_are_replaced_whole_and_feed_proxy_and_firewall() {
    let guide = read("DEPLOY.md");
    let script = block_after(&guide, "`/usr/local/sbin/dereth-hub-cloudflare`:", "sh");
    let at = |needle: &str| {
        script
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} in the script"))
    };
    assert!(script.contains("set -eu"));
    assert!(at("mktemp") < at("curl"), "fetched into a new file");
    assert!(
        at("did not read") < at("mv -f"),
        "checked before it replaces the list"
    );
    assert!(at("mv -f \"$new\" \"$dir/cloudflare.txt\"") < at("/etc/caddy/cloudflare.caddy"));
    assert!(at("/etc/caddy/cloudflare.caddy") < at("ufw allow"));
    assert!(
        script.contains("< \"$dir/cloudflare.txt\""),
        "ufw from the same file"
    );
    let caddy = block_after(&guide, "`/etc/caddy/Caddyfile`:", "caddy");
    assert!(caddy.contains("import /etc/caddy/cloudflare.caddy"));
    assert!(
        !caddy.contains("173.245.48.0/20"),
        "no second copy of the list to drift"
    );
}

/// Spec: hub: a container trusts its proxy's one address, never a whole range of networks, and
/// the image trusts none until told.
#[test]
fn a_container_trusts_its_proxys_one_address() {
    let guide = read("DEPLOY.md");
    let image = read("Dockerfile");
    assert!(
        !guide.contains("172.16.0.0/12 \\"),
        "no whole range trusted"
    );
    let command = image
        .lines()
        .find(|l| l.starts_with("CMD "))
        .expect("the image's command");
    assert!(
        !command.contains("--trusted-proxy"),
        "the image trusts nothing by itself: {command}"
    );
    let run = block_after(&guide, "## 7. The container", "sh");
    assert!(run.contains("docker network create --subnet"));
    assert!(run.contains("--trusted-proxy 172.31.86.2 "));
}

/// Spec: hub: the guide's restore says what a backup brings back: keys, sessions, locks and
/// disabled operators as they were.
#[test]
fn a_restore_says_what_it_brings_back() {
    let guide = read("DEPLOY.md");
    let at = guide.find("- **Restoring:**").expect("restore notes");
    let notes = &guide[at..guide[at..].find("- **Upgrading:**").expect("next") + at];
    for word in [
        "keys",
        "sessions",
        "dereth-network-hub end-sessions",
        "locks",
        "disable-operator",
    ] {
        assert!(notes.contains(word), "{word:?} in the restore notes");
    }
    assert!(
        !notes.contains("sqlite3"),
        "the hub's own command, not the database by hand"
    );
}

/// Spec: hub: the guide imports the community list daily, as the hub's user against the hub's
/// registry, applying all but conflicts and logging one line.
#[test]
fn the_community_list_is_imported_daily_as_the_hubs_user() {
    let guide = read("DEPLOY.md");
    let unit = block_after(
        &guide,
        "`/etc/systemd/system/dereth-network-hub-community.service`:",
        "ini",
    );
    for line in [
        "User=dereth-hub",
        "Environment=DERETH_HUB_DB=/var/lib/dereth-network-hub/hub.sqlite",
        "Type=oneshot",
        "NoNewPrivileges=true",
    ] {
        assert!(unit.contains(line), "{line:?} in the unit");
    }
    let start = unit
        .lines()
        .find(|l| l.starts_with("ExecStart="))
        .expect("the command");
    for flag in [
        "import-community",
        "--fetch",
        "--apply",
        "--skip-conflicts",
        "--summary",
    ] {
        assert!(start.contains(flag), "{flag} in {start}");
    }
    let timer = block_after(
        &guide,
        "`/etc/systemd/system/dereth-network-hub-community.timer`:",
        "ini",
    );
    assert!(timer.contains("OnCalendar=daily") && timer.contains("Persistent=true"));
    let tunnel = guide
        .find("**A Cloudflare tunnel (simplest).**")
        .expect("the tunnel");
    let proxy = guide
        .find("**A reverse proxy of your own")
        .expect("the proxy");
    assert!(tunnel < proxy, "the tunnel comes first");
}
