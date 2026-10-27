using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Threading;
using Microsoft.VisualStudio.TestTools.UnitTesting;

namespace Dereth.Network.Companion.Tests
{
    /// <summary>The hub's public API, asked over HTTP of a hub that stands in for it on this machine.</summary>
    [TestClass]
    public sealed class HubTests
    {
        [TestMethod]
        public void The_world_list_reads_every_page_and_each_worlds_live_status()
        {
            using (var hub = new FakeHub(path =>
                   {
                       if (path == "/v1/servers?limit=100&offset=0")
                           return Answer.Ok("{\"data\":[" + World("frost", "Frost", "online", 12, "on") + "," +
                                            World("coldeve", "Coldeve", "stale", null, "off") + "],\"total\":3,\"limit\":2,\"offset\":0}");
                       if (path == "/v1/servers?limit=100&offset=2")
                           return Answer.Ok("{\"data\":[" + World("eulmore", "Eulmore", "offline", null, "unset") + "],\"total\":3}");
                       return Answer.Of(404, "{\"error\":{\"code\":\"NOT_FOUND\",\"message\":\"no\"}}");
                   }))
            {
                var r = Wait<List<WorldRow>>((http, done) => new HubClient(hub.Url, http).Worlds(done));
                Assert.IsTrue(r.Ok, r.Error);
                var worlds = r.Value!;
                CollectionAssert.AreEqual(new[] { "frost", "coldeve", "eulmore" }, worlds.Select(w => w.Slug).ToArray());
                Assert.AreEqual(("Frost", "online", (int?)12, "on"), (worlds[0].Name, worlds[0].State, worlds[0].Players, worlds[0].Federation));
                Assert.AreEqual(("stale", (int?)null), (worlds[1].State, worlds[1].Players));
                CollectionAssert.AreEqual(new[] { "/v1/servers?limit=100&offset=0", "/v1/servers?limit=100&offset=2" }, hub.Paths());
            }
        }

        [TestMethod]
        public void A_worlds_entry_carries_the_plugin_policy_it_publishes()
        {
            using (var hub = new FakeHub(path => path == "/v1/servers/frost"
                       ? Answer.Ok("{\"slug\":\"frost\",\"name\":\"Frost\",\"plugins\":{\"policy\":\"policy_version = 1\\n[automation]\\ncombat = \\\"deny\\\"\\n\"}}")
                       : Answer.Of(404, "{}")))
            {
                var r = Wait<WorldRow>((http, done) => new HubClient(hub.Url, http).World("frost", done));
                Assert.IsTrue(r.Ok, r.Error);
                StringAssert.StartsWith(r.Value!.Policy, "policy_version = 1\n[automation]");
            }
            using (var hub = new FakeHub(_ => Answer.Ok("{\"slug\":\"aa\",\"plugin_policy\":\"https://aa.example/policy.toml\"}")))
            {
                var r = Wait<WorldRow>((http, done) => new HubClient(hub.Url, http).World("aa", done));
                Assert.AreEqual("https://aa.example/policy.toml", r.Value!.Policy, "the entry's other spelling");
            }
        }

        [TestMethod]
        public void A_directory_search_is_sent_escaped_and_its_results_read()
        {
            using (var hub = new FakeHub(_ => Answer.Ok(
                       "{\"data\":[{\"address\":\"Sir Bob@coldeve\",\"name\":\"Sir Bob\",\"world\":{\"slug\":\"coldeve\",\"name\":\"Coldeve\"}," +
                       "\"updated\":\"2026-10-12T14:00:00.000Z\",\"level\":126}],\"total\":1,\"limit\":50,\"offset\":0,\"nextOffset\":null}")))
            {
                var r = Wait<List<DirectoryRow>>((http, done) => new HubClient(hub.Url, http).Search(" Sir B ", done));
                Assert.IsTrue(r.Ok, r.Error);
                var row = r.Value!.Single();
                Assert.AreEqual(("Sir Bob@coldeve", "Coldeve", (int?)126), (row.Address, row.WorldName, row.Level));
                CollectionAssert.AreEqual(new[] { "/v1/directory/characters?q=Sir%20B&limit=50" }, hub.Paths());
            }
        }

        [TestMethod]
        public void A_search_the_directory_would_refuse_is_never_sent()
        {
            using (var hub = new FakeHub(_ => Answer.Ok("{}")))
            {
                foreach (var q in new[] { "b", "b4", "Bob!", new string('a', 33) })
                {
                    var r = Wait<List<DirectoryRow>>((http, done) => new HubClient(hub.Url, http).Search(q, done));
                    Assert.IsFalse(r.Ok, q);
                }
                Assert.AreEqual(0, hub.Paths().Length);
            }
        }

        [TestMethod]
        public void The_hubs_refusals_are_told_in_its_own_words()
        {
            using (var hub = new FakeHub(path =>
                   {
                       if (path.StartsWith("/v1/directory", StringComparison.Ordinal))
                           return Answer.Of(429, "{\"error\":{\"code\":\"RATE_LIMITED\",\"message\":\"slow\",\"retryAfterSeconds\":12}}", "Retry-After: 12");
                       if (path == "/v1/servers/gone")
                           return Answer.Of(404, "{\"error\":{\"code\":\"NOT_FOUND\",\"message\":\"no world\"}}");
                       if (path == "/v1/servers/odd")
                           return Answer.Ok("{not json");
                       return Answer.Of(400, "{\"error\":{\"code\":\"BAD_REQUEST\",\"message\":\"limit: 1-100\"}}");
                   }))
            {
                Assert.AreEqual("The hub asks to wait 12 seconds.",
                    Wait<List<DirectoryRow>>((http, done) => new HubClient(hub.Url, http).Search("Bob", done)).Error);
                Assert.AreEqual("The hub has no such entry.",
                    Wait<WorldRow>((http, done) => new HubClient(hub.Url, http).World("gone", done)).Error);
                Assert.AreEqual("The hub's answer did not read.",
                    Wait<WorldRow>((http, done) => new HubClient(hub.Url, http).World("odd", done)).Error);
                Assert.AreEqual("limit: 1-100",
                    Wait<List<WorldRow>>((http, done) => new HubClient(hub.Url, http).Worlds(done)).Error);
            }
        }

        [TestMethod]
        public void A_hub_that_does_not_answer_is_said_to_be_unreachable()
        {
            int port;
            var l = new TcpListener(IPAddress.Loopback, 0);
            l.Start();
            port = ((IPEndPoint)l.LocalEndpoint).Port;
            l.Stop();
            var r = Wait<List<WorldRow>>((http, done) => new HubClient("http://127.0.0.1:" + port, http).Worlds(done));
            StringAssert.StartsWith(r.Error, "The hub could not be reached: ");
        }

        [TestMethod]
        public void Only_https_or_a_hub_on_this_machine_is_reached()
        {
            Assert.IsTrue(HubClient.IsAllowed("https://hub.dereth.network"));
            Assert.IsTrue(HubClient.IsAllowed("http://127.0.0.1:8650"));
            Assert.IsTrue(HubClient.IsAllowed("http://localhost:8650/"));
            Assert.IsFalse(HubClient.IsAllowed("http://hub.dereth.network"));
            Assert.IsFalse(HubClient.IsAllowed("https://user:pw@hub.dereth.network"));
            Assert.IsFalse(HubClient.IsAllowed("ftp://hub.dereth.network"));
            Assert.ThrowsException<ArgumentException>(() => new HubClient("http://example.org", new WebHttp("t")));
        }

        private static string World(string slug, string name, string state, int? players, string federation) =>
            "{\"slug\":\"" + slug + "\",\"name\":\"" + name + "\",\"ruleset\":\"PvE\",\"federation\":\"" + federation +
            "\",\"status\":{\"state\":\"" + state + "\",\"playersOnline\":" + (players.HasValue ? players.Value.ToString() : "null") + "}}";

        /// <summary>Run one request through the real HTTP client and wait for its answer, which a stalled run gives up on.</summary>
        private static HubResult<T> Wait<T>(Action<WebHttp, Action<HubResult<T>>> start) where T : class
        {
            var http = new WebHttp("DerethNetworkCompanion/tests");
            HubResult<T>? result = null;
            start(http, r => result = r);
            var guard = Stopwatch.StartNew();
            while (result == null)
            {
                http.Pump();
                if (result != null)
                    break;
                if (guard.Elapsed > TimeSpan.FromMinutes(1))
                    Assert.Fail("no answer: the run stalled");
                Thread.Sleep(2);
            }
            return result;
        }
    }

    internal sealed class Answer
    {
        public int Status;
        public string Body = "";
        public string[] Headers = new string[0];

        public static Answer Ok(string body) => Of(200, body);

        public static Answer Of(int status, string body, params string[] headers) =>
            new Answer { Status = status, Body = body, Headers = headers };
    }

    /// <summary>A hub on this machine's loopback that answers each request from a table, and remembers what it was asked.</summary>
    internal sealed class FakeHub : IDisposable
    {
        private readonly TcpListener listener;
        private readonly Func<string, Answer> answer;
        private readonly ConcurrentQueue<string> paths = new ConcurrentQueue<string>();
        private readonly Thread thread;

        public FakeHub(Func<string, Answer> answer)
        {
            this.answer = answer;
            listener = new TcpListener(IPAddress.Loopback, 0);
            listener.Start();
            Url = "http://127.0.0.1:" + ((IPEndPoint)listener.LocalEndpoint).Port;
            thread = new Thread(Serve) { IsBackground = true };
            thread.Start();
        }

        public string Url { get; }

        public string[] Paths() => paths.ToArray();

        private void Serve()
        {
            while (true)
            {
                TcpClient client;
                try
                {
                    client = listener.AcceptTcpClient();
                }
                catch (SocketException)
                {
                    return;
                }
                catch (ObjectDisposedException)
                {
                    return;
                }
                using (client)
                using (var s = client.GetStream())
                {
                    var head = ReadHead(s);
                    var path = head.Split(' ').Skip(1).FirstOrDefault() ?? "/";
                    paths.Enqueue(path);
                    var a = answer(path);
                    var body = Encoding.UTF8.GetBytes(a.Body);
                    var sb = new StringBuilder();
                    sb.Append("HTTP/1.1 ").Append(a.Status).Append(" X\r\n");
                    sb.Append("Content-Type: application/json\r\n");
                    sb.Append("Content-Length: ").Append(body.Length).Append("\r\n");
                    foreach (var h in a.Headers)
                        sb.Append(h).Append("\r\n");
                    sb.Append("Connection: close\r\n\r\n");
                    var bytes = Encoding.ASCII.GetBytes(sb.ToString());
                    s.Write(bytes, 0, bytes.Length);
                    s.Write(body, 0, body.Length);
                }
            }
        }

        private static string ReadHead(Stream s)
        {
            var buf = new List<byte>();
            while (true)
            {
                var b = s.ReadByte();
                if (b < 0)
                    break;
                buf.Add((byte)b);
                var n = buf.Count;
                if (n >= 4 && buf[n - 4] == '\r' && buf[n - 3] == '\n' && buf[n - 2] == '\r' && buf[n - 1] == '\n')
                    break;
            }
            return Encoding.ASCII.GetString(buf.ToArray());
        }

        public void Dispose()
        {
            listener.Stop();
            thread.Join(TimeSpan.FromSeconds(30));
        }
    }
}
