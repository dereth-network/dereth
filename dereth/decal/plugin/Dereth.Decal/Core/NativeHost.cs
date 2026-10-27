using System;
using System.Collections.Generic;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace Dereth.Decal
{
    /// <summary>
    /// The host library (<c>dereth_decal_host.dll</c>, beside this plugin) through its C interface:
    /// the game's model and the plugins in it. Every call runs on the caller's thread; text crosses as
    /// UTF-8 with a length.
    /// </summary>
    public sealed class NativeHost : IDisposable
    {
        /// <summary>The library's file name.</summary>
        public const string Library = "dereth_decal_host.dll";

        /// <summary>The interface version this plugin speaks.</summary>
        public const uint Abi = 2;

        private IntPtr host;

        private NativeHost(IntPtr host) => this.host = host;

        /// <summary>
        /// Loads the library from <paramref name="folder"/>, once for the process, so that the calls
        /// below find it there rather than on the game's own search path.
        /// </summary>
        /// <exception cref="DllNotFoundException">The library is not there, or will not load.</exception>
        public static void LoadFrom(string folder)
        {
            var path = Path.Combine(folder, Library);
            if (LoadLibraryW(path) == IntPtr.Zero)
                throw new DllNotFoundException("The plugin host did not load from " + path + " (error " + Marshal.GetLastWin32Error() + ").");
        }

        /// <summary>A host made from its settings' JSON.</summary>
        /// <exception cref="InvalidOperationException">The library speaks another interface, or the settings do not read.</exception>
        public static NativeHost Create(string settings)
        {
            var abi = dereth_host_abi();
            if (abi != Abi)
                throw new InvalidOperationException("The plugin host speaks interface " + abi + ", not " + Abi + ": its library and this plugin are from different releases.");
            var bytes = Encoding.UTF8.GetBytes(settings);
            var h = dereth_host_new(bytes, (UIntPtr)bytes.Length);
            if (h == IntPtr.Zero)
                throw new InvalidOperationException("The plugin host's settings did not read.");
            return new NativeHost(h);
        }

        /// <summary>A message the game received, whole, its type first.</summary>
        public void ServerMessage(byte[] data, double now) =>
            dereth_host_server_message(Live(), data, (UIntPtr)data.Length, now);

        /// <summary>A message the game sent, whole, its type first.</summary>
        public void ClientMessage(byte[] data, double now) =>
            dereth_host_client_message(Live(), data, (UIntPtr)data.Length, now);

        /// <summary>The character's place: its cell, its place in the landblock in meters, its heading in degrees.</summary>
        public void Place(uint landcell, double x, double y, double z, double heading) =>
            dereth_host_place(Live(), landcell, x, y, z, heading);

        /// <summary>The object the player selected (0 for none).</summary>
        public void Selected(uint id) => dereth_host_selected(Live(), id);

        /// <summary>The player pressed a key in the game.</summary>
        public void PlayerMoved() => dereth_host_player_moved(Live());

        /// <summary>Whether the game's window is in front, and whether its chat line is open.</summary>
        public void GameState(bool inFront, bool chatOpen) =>
            dereth_host_game_state(Live(), inFront ? 1 : 0, chatOpen ? 1 : 0);

        /// <summary>The player's key bindings, as JSON: each of the game's action names with its virtual-key code.</summary>
        public void Bindings(string json)
        {
            var bytes = Encoding.UTF8.GetBytes(json);
            dereth_host_bindings(Live(), bytes, (UIntPtr)bytes.Length);
        }

        /// <summary>A line the player typed: whether a plugin claims it (the game must not send it).</summary>
        public bool Typed(string line)
        {
            var bytes = Encoding.UTF8.GetBytes(line);
            return dereth_host_typed(Live(), bytes, (UIntPtr)bytes.Length) == 1;
        }

        /// <summary>An event on a plugin window's view.</summary>
        public void ViewEvent(string key, string control, string ev, object? value)
        {
            var json = Json.Write(new Dictionary<string, object?>
            {
                ["key"] = key,
                ["control"] = control,
                ["event"] = ev,
                ["value"] = value,
            });
            var bytes = Encoding.UTF8.GetBytes(json);
            dereth_host_view_event(Live(), bytes, (UIntPtr)bytes.Length);
        }

        /// <summary>The messages handed over so far, applied as one batch.</summary>
        public void Flush() => dereth_host_flush(Live());

        /// <summary>One frame, at <paramref name="now"/> seconds on a steady clock.</summary>
        public void Frame(double now) => dereth_host_frame(Live(), now);

        /// <summary>What to do since the last call: each op a JSON object.</summary>
        public List<object?> Ops() => Json.List(Json.Parse(Read(dereth_host_ops)));

        /// <summary>The host's state: the session's phase, each plugin's status and console, the windows.</summary>
        public object? Report() => Json.Parse(Read(dereth_host_report));

        private delegate IntPtr Reader(IntPtr host, out UIntPtr len);

        private string Read(Reader read)
        {
            var p = read(Live(), out var len);
            if (p == IntPtr.Zero)
                return "null";
            var n = checked((int)len.ToUInt32());
            var bytes = new byte[n];
            Marshal.Copy(p, bytes, 0, n);
            return Encoding.UTF8.GetString(bytes);
        }

        private IntPtr Live()
        {
            if (host == IntPtr.Zero)
                throw new ObjectDisposedException(nameof(NativeHost));
            return host;
        }

        /// <summary>The host gone; its plugins' stores are written out first.</summary>
        public void Dispose()
        {
            if (host != IntPtr.Zero)
            {
                dereth_host_free(host);
                host = IntPtr.Zero;
            }
        }

        [DllImport("kernel32", CharSet = CharSet.Unicode, SetLastError = true)]
        private static extern IntPtr LoadLibraryW(string path);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint dereth_host_abi();

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr dereth_host_new(byte[] settings, UIntPtr len);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_free(IntPtr host);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_server_message(IntPtr host, byte[] data, UIntPtr len, double now);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_client_message(IntPtr host, byte[] data, UIntPtr len, double now);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_place(IntPtr host, uint landcell, double x, double y, double z, double heading);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_selected(IntPtr host, uint id);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_player_moved(IntPtr host);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_game_state(IntPtr host, int focused, int chatOpen);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_bindings(IntPtr host, byte[] json, UIntPtr len);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern int dereth_host_typed(IntPtr host, byte[] line, UIntPtr len);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_view_event(IntPtr host, byte[] json, UIntPtr len);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_flush(IntPtr host);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern void dereth_host_frame(IntPtr host, double now);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr dereth_host_ops(IntPtr host, out UIntPtr len);

        [DllImport(Library, CallingConvention = CallingConvention.Cdecl)]
        private static extern IntPtr dereth_host_report(IntPtr host, out UIntPtr len);
    }
}
