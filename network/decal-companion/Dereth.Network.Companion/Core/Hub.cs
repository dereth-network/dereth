using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Net;
using System.Text;
using System.Threading;

namespace Dereth.Network.Companion
{
    /// <summary>An HTTP answer: its status and body, or why there was none.</summary>
    public sealed class HttpReply
    {
        public int Status { get; set; }
        public string Body { get; set; } = "";
        /// <summary>Why no answer came (the hub unreachable, a timeout); empty when one did.</summary>
        public string Error { get; set; } = "";
        /// <summary>The seconds a <c>429</c> asks to wait, from its <c>Retry-After</c>.</summary>
        public int RetryAfter { get; set; }
    }

    /// <summary>GET requests, answered later on the caller's own thread.</summary>
    public interface IHttp
    {
        void Get(string url, Action<HttpReply> done);
    }

    /// <summary>
    /// GET requests made on the thread pool, so the game never waits for one; each answer is handed
    /// back at the next <see cref="Pump"/>, which the plugin calls from the game's own thread.
    /// Redirects are not followed and a body past 2 MiB is refused.
    /// </summary>
    public sealed class WebHttp : IHttp
    {
        private const int MaxBody = 2 << 20;
        private readonly ConcurrentQueue<Action> finished = new ConcurrentQueue<Action>();
        private readonly string userAgent;

        public WebHttp(string userAgent)
        {
            this.userAgent = userAgent;
            // The game's process predates TLS 1.2 as a default; the hub needs it.
            ServicePointManager.SecurityProtocol |= SecurityProtocolType.Tls12;
        }

        public void Get(string url, Action<HttpReply> done)
        {
            ThreadPool.QueueUserWorkItem(_ =>
            {
                var reply = Fetch(url);
                finished.Enqueue(() => done(reply));
            });
        }

        /// <summary>Hand every answer that has arrived to its caller.</summary>
        public void Pump()
        {
            while (finished.TryDequeue(out var a))
                a();
        }

        private HttpReply Fetch(string url)
        {
            try
            {
                var req = (HttpWebRequest)WebRequest.Create(url);
                req.Method = "GET";
                req.AllowAutoRedirect = false;
                req.Timeout = 15000;
                req.ReadWriteTimeout = 15000;
                req.UserAgent = userAgent;
                req.Accept = "application/json, text/plain";
                try
                {
                    using (var resp = (HttpWebResponse)req.GetResponse())
                        return Read(resp);
                }
                catch (WebException e) when (e.Response is HttpWebResponse resp)
                {
                    using (resp)
                        return Read(resp);
                }
            }
            catch (Exception e)
            {
                return new HttpReply { Error = e.Message };
            }
        }

        private static HttpReply Read(HttpWebResponse resp)
        {
            var reply = new HttpReply { Status = (int)resp.StatusCode };
            if (int.TryParse(resp.Headers["Retry-After"], NumberStyles.Integer, CultureInfo.InvariantCulture, out var wait))
                reply.RetryAfter = wait;
            using (var s = resp.GetResponseStream())
            using (var buf = new MemoryStream())
            {
                var chunk = new byte[16384];
                int n;
                while (s != null && (n = s.Read(chunk, 0, chunk.Length)) > 0)
                {
                    buf.Write(chunk, 0, n);
                    if (buf.Length > MaxBody)
                        return new HttpReply { Error = "the answer is too large" };
                }
                reply.Body = Encoding.UTF8.GetString(buf.ToArray());
            }
            return reply;
        }
    }

    /// <summary>A world as the hub lists it.</summary>
    public sealed class WorldRow
    {
        public string Slug { get; set; } = "";
        public string Name { get; set; } = "";
        public string Ruleset { get; set; } = "";
        /// <summary><c>on</c>, <c>off</c> or <c>unset</c>: whether the world is on the network.</summary>
        public string Federation { get; set; } = "";
        /// <summary><c>online</c>, <c>degraded</c>, <c>maintenance</c>, <c>offline</c>, <c>stale</c> or <c>unknown</c>.</summary>
        public string State { get; set; } = "";
        public int? Players { get; set; }
        /// <summary>The world's plugin policy as its entry gives it (<c>plugins.policy</c> or <c>plugin_policy</c>): the document, or the URL it is at; empty when it publishes none.</summary>
        public string Policy { get; set; } = "";
    }

    /// <summary>A character as the directory lists it.</summary>
    public sealed class DirectoryRow
    {
        public string Address { get; set; } = "";
        public string WorldName { get; set; } = "";
        public int? Level { get; set; }
        /// <summary>The hour the profile last changed, as the hub gives it.</summary>
        public string Updated { get; set; } = "";
    }

    /// <summary>A hub's answer: a value, or why there is none.</summary>
    public sealed class HubResult<T> where T : class
    {
        private HubResult(T? value, string error)
        {
            Value = value;
            Error = error;
        }

        public static HubResult<T> Of(T value) => new HubResult<T>(value, "");

        public static HubResult<T> Failed(string error) => new HubResult<T>(null, error);

        public T? Value { get; }
        public string Error { get; } = "";
        public bool Ok => Value != null;
    }

    /// <summary>
    /// The hub's public API: the worlds and their live status, one world's entry, and the character
    /// directory. Only <c>https://</c>, or <c>http://</c> to this machine (a hub run locally).
    /// </summary>
    public sealed class HubClient
    {
        public const string DefaultHub = "https://hub.dereth.network";
        private const int PageSize = 100;
        private const int MaxPages = 5;
        private readonly IHttp http;

        public HubClient(string baseUrl, IHttp http)
        {
            if (!IsAllowed(baseUrl))
                throw new ArgumentException("a hub is reached over https://, or http:// on this machine", nameof(baseUrl));
            BaseUrl = baseUrl.TrimEnd('/');
            this.http = http;
        }

        public string BaseUrl { get; }

        /// <summary>Whether the companion may fetch from <paramref name="url"/>.</summary>
        public static bool IsAllowed(string url)
        {
            if (!Uri.TryCreate(url, UriKind.Absolute, out var u) || u.UserInfo.Length > 0)
                return false;
            if (u.Scheme == Uri.UriSchemeHttps)
                return true;
            return u.Scheme == Uri.UriSchemeHttp && (u.IsLoopback || u.Host == "localhost");
        }

        /// <summary>Every listed world, reporting online first, as the hub orders them.</summary>
        public void Worlds(Action<HubResult<List<WorldRow>>> done)
        {
            var all = new List<WorldRow>();
            void Page(int offset, int pages)
            {
                var url = BaseUrl + "/v1/servers?limit=" + PageSize + "&offset=" + offset.ToString(CultureInfo.InvariantCulture);
                Fetch(url, body =>
                {
                    var data = Json.List(Json.Get(body, "data"));
                    foreach (var row in data)
                        all.Add(World(row));
                    var total = Json.Int(Json.Get(body, "total")) ?? all.Count;
                    if (data.Count > 0 && all.Count < total && pages + 1 < MaxPages)
                        Page(offset + data.Count, pages + 1);
                    else
                        done(HubResult<List<WorldRow>>.Of(all));
                }, e => done(HubResult<List<WorldRow>>.Failed(e)));
            }
            Page(0, 0);
        }

        /// <summary>One world's entry.</summary>
        public void World(string slug, Action<HubResult<WorldRow>> done)
        {
            if (!NetAddress.IsSlug(slug))
            {
                done(HubResult<WorldRow>.Failed("That is not a world's slug."));
                return;
            }
            Fetch(BaseUrl + "/v1/servers/" + slug, body => done(HubResult<WorldRow>.Of(World(body))),
                e => done(HubResult<WorldRow>.Failed(e)));
        }

        /// <summary>Characters in the directory whose name, or a word of it, starts with <paramref name="text"/>.</summary>
        public void Search(string text, Action<HubResult<List<DirectoryRow>>> done)
        {
            var q = text.Trim();
            if (!IsSearch(q))
            {
                done(HubResult<List<DirectoryRow>>.Failed("Search for 2 to 32 letters of a name."));
                return;
            }
            var url = BaseUrl + "/v1/directory/characters?q=" + Uri.EscapeDataString(q) + "&limit=50";
            Fetch(url, body =>
            {
                var rows = new List<DirectoryRow>();
                foreach (var r in Json.List(Json.Get(body, "data")))
                    rows.Add(new DirectoryRow
                    {
                        Address = Json.Str(Json.Get(r, "address")),
                        WorldName = Json.Str(Json.At(r, "world", "name")),
                        Level = Json.Int(Json.Get(r, "level")),
                        Updated = Json.Str(Json.Get(r, "updated")),
                    });
                done(HubResult<List<DirectoryRow>>.Of(rows));
            }, e => done(HubResult<List<DirectoryRow>>.Failed(e)));
        }

        /// <summary>A text document at an <c>https://</c> URL: a world's published plugin policy.</summary>
        public void Text(string url, Action<HubResult<string>> done)
        {
            if (!IsAllowed(url))
            {
                done(HubResult<string>.Failed("Only an https:// address is read."));
                return;
            }
            http.Get(url, r =>
            {
                if (r.Error.Length > 0)
                    done(HubResult<string>.Failed("It could not be reached: " + r.Error));
                else if (r.Status != 200)
                    done(HubResult<string>.Failed("It answered " + r.Status.ToString(CultureInfo.InvariantCulture) + "."));
                else
                    done(HubResult<string>.Of(r.Body));
            });
        }

        /// <summary>Whether the directory takes <paramref name="q"/> as a search: 2 to 32 letters, spaces, apostrophes and hyphens, two of them letters.</summary>
        public static bool IsSearch(string q)
        {
            if (q.Length < 2 || q.Length > 32)
                return false;
            var letters = 0;
            foreach (var c in q)
            {
                if (char.IsLetter(c))
                    letters++;
                else if (c != ' ' && c != '\'' && c != '-')
                    return false;
            }
            return letters >= 2;
        }

        private static WorldRow World(object? row) => new WorldRow
        {
            Slug = Json.Str(Json.Get(row, "slug")),
            Name = Json.Str(Json.Get(row, "name")),
            Ruleset = Json.Str(Json.Get(row, "ruleset")),
            Federation = Json.Str(Json.Get(row, "federation")),
            State = Json.Str(Json.At(row, "status", "state")),
            Players = Json.Int(Json.At(row, "status", "playersOnline")),
            Policy = Json.Str(Json.At(row, "plugins", "policy")) is { Length: > 0 } p ? p : Json.Str(Json.Get(row, "plugin_policy")),
        };

        private void Fetch(string url, Action<object?> ok, Action<string> failed)
        {
            http.Get(url, r =>
            {
                if (r.Error.Length > 0)
                {
                    failed("The hub could not be reached: " + r.Error);
                    return;
                }
                if (r.Status == 429)
                {
                    failed(r.RetryAfter > 0
                        ? "The hub asks to wait " + r.RetryAfter.ToString(CultureInfo.InvariantCulture) + " seconds."
                        : "The hub asks to wait a while.");
                    return;
                }
                object? body;
                try
                {
                    body = Json.Parse(r.Body);
                }
                catch (FormatException)
                {
                    failed(r.Status == 200
                        ? "The hub's answer did not read."
                        : "The hub answered " + r.Status.ToString(CultureInfo.InvariantCulture) + ".");
                    return;
                }
                if (r.Status == 200)
                {
                    ok(body);
                    return;
                }
                if (r.Status == 404)
                {
                    failed("The hub has no such entry.");
                    return;
                }
                var message = Json.Str(Json.At(body, "error", "message"));
                failed(message.Length > 0 ? message : "The hub answered " + r.Status.ToString(CultureInfo.InvariantCulture) + ".");
            });
        }
    }
}
