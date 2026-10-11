// `/dn verify`: the world proves to the hub that it is the world its operator registered or
// claimed on the site. It sends the operator's one-time token to the hub's verification route with
// its key, from whatever address its outgoing calls leave by. The hub checks the key and the token,
// then sends a challenge to the world's endpoint at its game port and holds the call until the world
// posts the challenge's nonce back to the answer route, with the same key, from any address: so the
// world proves it answers at its registered address, wherever its own calls come from.
//
// Both calls run off the world thread; the hub's answer to `/dn verify` is shown on a later pump
// (NetworkWorld.Verify), and the answer to a challenge is only logged.

using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace Dereth.Network;

/// <summary>A `/dn verify` to send: the token, who typed it, and whom to show the answer to.</summary>
public sealed record VerifyRequest(string Token, string By, uint? Player);

public static class HubVerify
{
    /// <summary>Whether `token` has the shape of the hub's verification token: `dnv_` and 32 characters of base64url.</summary>
    public static bool IsToken(string? token)
    {
        if (token is null || token.Length != 36 || !token.StartsWith("dnv_", StringComparison.Ordinal))
            return false;
        for (var i = 4; i < token.Length; i++)
        {
            var c = token[i];
            if (!(c is >= 'a' and <= 'z' || c is >= 'A' and <= 'Z' || c is >= '0' and <= '9' || c is '-' or '_'))
                return false;
        }
        return true;
    }

    /// <summary>
    /// The verification route for the link's address: the same host and prefix over `https://`
    /// (`http://` for a loopback hub on `ws://`).
    /// </summary>
    public static Uri RouteFor(Uri link)
    {
        var scheme = link.Scheme == "wss" ? "https" : "http";
        var path = link.AbsolutePath;
        if (path.EndsWith("/link", StringComparison.Ordinal))
            path = path[..^"/link".Length];
        var b = new UriBuilder(link) { Scheme = scheme, Path = path + "/verify" };
        if (link.IsDefaultPort)
            b.Port = -1;
        return b.Uri;
    }

    /// <summary>The route a challenge's nonce is posted to: the verification route's `/answer`.</summary>
    public static Uri AnswerRouteFor(Uri verifyRoute)
    {
        var b = new UriBuilder(verifyRoute) { Path = verifyRoute.AbsolutePath.TrimEnd('/') + "/answer" };
        if (verifyRoute.IsDefaultPort)
            b.Port = -1;
        return b.Uri;
    }

    /// <summary>The line a `/dn verify` answers with, from the hub's answer: its status and its JSON body.</summary>
    public static string Line(int status, string body)
    {
        JsonNode? v = null;
        try
        {
            v = JsonNode.Parse(body);
        }
        catch (JsonException)
        {
        }
        if (status == 200)
        {
            return (v?["approval"]?.GetValueKind() == JsonValueKind.String ? v["approval"]!.GetValue<string>() : null) switch
            {
                "approved" => "The Dereth Network verified this world. It is on the network now; the link comes up within a minute.",
                "rate_limited" => "The Dereth Network verified this world. Today's allowance of new worlds is spent: turn federation on from the site later.",
                "not_network_slug" => "The Dereth Network verified this world. Its slug is too long, or reserved, to join the network.",
                "locked" => "The Dereth Network verified this world, but the hub's operator has locked it.",
                _ => "The Dereth Network verified this world.",
            };
        }
        string? message = null;
        if (v?["error"]?["message"] is JsonValue m && m.GetValueKind() == JsonValueKind.String)
            message = new string(m.GetValue<string>().Where(c => !char.IsControl(c)).Take(200).ToArray());
        return (status, message) switch
        {
            (401, _) => "The Dereth Network did not verify this world: the hub does not know this world's key. Check the key in the variable KeyEnvironmentVariable names against the key the site gave you.",
            (429, _) => "The Dereth Network did not verify this world: too many attempts. Wait a while and try again.",
            (_, { } msg) => $"The Dereth Network did not verify this world: {msg}{(EndsSentence(msg) ? "" : ".")}",
            _ => $"The Dereth Network did not verify this world: the hub answered {status}.",
        };
    }

    /// <summary>Reads at most 64 KiB of an answer's body.</summary>
    private static async Task<string> ReadBody(HttpResponseMessage response, CancellationToken ct)
    {
        await using var stream = await response.Content.ReadAsStreamAsync(ct).ConfigureAwait(false);
        var buffer = new byte[64 * 1024];
        var read = 0;
        while (read < buffer.Length)
        {
            var n = await stream.ReadAsync(buffer.AsMemory(read), ct).ConfigureAwait(false);
            if (n == 0)
                break;
            read += n;
        }
        return Encoding.UTF8.GetString(buffer, 0, read);
    }

    /// <summary>
    /// Posts a challenge's nonce (`nonceHex`) to the answer route with `key`, and says how it went,
    /// for the log.
    /// </summary>
    public static async Task<string> AnswerAsync(HttpClient http, Uri answerRoute, string key, string nonceHex, CancellationToken ct)
    {
        var body = new JsonObject { ["nonce"] = nonceHex }.ToJsonString();
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Post, answerRoute);
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", key);
            request.Content = new StringContent(body, Encoding.UTF8, "application/json");
            using var response = await http.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, ct).ConfigureAwait(false);
            var status = (int)response.StatusCode;
            if (status == 200)
                return "the hub took this world's answer to its challenge";
            string? reason = null;
            try
            {
                if (JsonNode.Parse(await ReadBody(response, ct).ConfigureAwait(false))?["error"]?["reason"] is JsonValue r
                    && r.GetValueKind() == JsonValueKind.String)
                    reason = new string(r.GetValue<string>().Where(ch => !char.IsControl(ch)).Take(40).ToArray());
            }
            catch (JsonException)
            {
            }
            return $"the hub refused this world's answer to its challenge ({status}{(reason is null ? "" : ", " + reason)})";
        }
        catch (Exception e) when (e is HttpRequestException or TaskCanceledException or IOException)
        {
            return $"the hub could not be reached with this world's answer to its challenge ({e.GetType().Name})";
        }
    }

    /// <summary>Whether the hub's message already ends as a sentence does.</summary>
    private static bool EndsSentence(string message) => message.Length > 0 && message[^1] is '.' or '?' or '!';

    /// <summary>Sends one `/dn verify` to `route` with `key`, and says how it went.</summary>
    public static async Task<string> SendAsync(HttpClient http, Uri route, string key, VerifyRequest r, CancellationToken ct)
    {
        var body = new JsonObject { ["token"] = r.Token, ["by"] = r.By }.ToJsonString();
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Post, route);
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", key);
            request.Content = new StringContent(body, Encoding.UTF8, "application/json");
            using var response = await http.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, ct).ConfigureAwait(false);
            return Line((int)response.StatusCode, await ReadBody(response, ct).ConfigureAwait(false));
        }
        catch (Exception e) when (e is HttpRequestException or TaskCanceledException or IOException)
        {
            return $"The Dereth Network's hub could not be reached ({e.GetType().Name}).";
        }
    }
}
