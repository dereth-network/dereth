using System;
using Decal.Adapter;

namespace Dereth.Decal.Plugin
{
    /// <summary>
    /// The plugin host's half that runs while a character is in the world: Decal's plugin, which
    /// makes the plugins' windows as its views and reads the player's key bindings from the game's
    /// key map. Everything else is <see cref="DerethFilter"/>'s.
    /// </summary>
    [FriendlyName("Dereth Plugins")]
    public sealed class DerethPlugin : PluginBase
    {
        private DerethFilter? filter;

        protected override void Startup()
        {
            try
            {
                filter = Core.Filter<DerethFilter>();
                if (filter == null)
                {
                    Host.Actions.AddChatText("[Plugins] The Dereth Plugins network filter is not on in Decal, so no plugin runs.", 6);
                    return;
                }
                foreach (var n in filter.Notes)
                    Host.Actions.AddChatText("[Plugins] " + n, 6);
                filter.Notes.Clear();
                filter.Driver?.Views.Attach(new DecalViews(Host));
                filter.Driver?.Bindings(KeyOf);
            }
            catch (Exception e)
            {
                Host.Actions.AddChatText("[Plugins] The plugins' windows did not open: " + e.Message, 6);
            }
        }

        /// <summary>The game's key map's key for one of its actions, as Decal reads it: 0 for none.</summary>
        private int KeyOf(string action)
        {
            try
            {
                return Core.QueryKeyBoardMap(action);
            }
            catch (Exception)
            {
                return 0;
            }
        }

        protected override void Shutdown()
        {
            try
            {
                filter?.Driver?.Views.Detach();
            }
            catch (Exception)
            {
            }
            filter = null;
        }
    }
}
