using System;
using System.Runtime.InteropServices;
using Decal.Adapter;

namespace Dereth.Decal.Plugin
{
    /// <summary>
    /// The game's window, as Decal names it: the game's keys are posted to it as its own key
    /// messages, and it is in front when it is Windows' foreground window.
    /// </summary>
    internal sealed class DecalKeys : IKeyWindow
    {
        private readonly CoreManager core;

        public DecalKeys(CoreManager core) => this.core = core;

        public void Post(int message, int wParam, int lParam) =>
            PostMessageW(core.Decal.Hwnd, unchecked((uint)message), new IntPtr(wParam), new IntPtr(lParam));

        /// <summary>Whether the game's window is in front.</summary>
        public bool InFront
        {
            get
            {
                try
                {
                    var game = core.Decal.Hwnd;
                    return game != IntPtr.Zero && GetForegroundWindow() == game;
                }
                catch (Exception)
                {
                    return false;
                }
            }
        }

        [DllImport("user32", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool PostMessageW(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);

        [DllImport("user32")]
        private static extern IntPtr GetForegroundWindow();
    }
}
