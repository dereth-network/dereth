using System;
using System.Collections.Generic;

namespace Dereth.Decal
{
    /// <summary>What a mark is drawn as.</summary>
    public enum MarkLook
    {
        /// <summary>Text floating over its object or place.</summary>
        Text,

        /// <summary>An arrow pointing down at its object or place.</summary>
        Arrow,

        /// <summary>A ring on the ground, <see cref="Mark.Scale"/> meters in radius.</summary>
        Ring,

        /// <summary>The arrow over the character's head, toward its object or place.</summary>
        Pointer,
    }

    /// <summary>One of Decal's 3-D objects, as the host's <c>mark</c> op describes it.</summary>
    public sealed class Mark
    {
        public string Key = "";
        public MarkLook Look;

        /// <summary>The object it is on, or else null and a place.</summary>
        public int? Object;

        /// <summary>Its place: map coordinates north and east, and meters up.</summary>
        public double Ns, Ew, Z;

        public string Text = "";

        /// <summary>Its color as Decal takes it: <c>0xAARRGGBB</c>, opaque.</summary>
        public int Color;

        public double Scale = 1.0;

        /// <summary>The mark an op describes; null when it is not one.</summary>
        public static Mark? Of(Dictionary<string, object?> op)
        {
            MarkLook look;
            switch (Json.Str(Json.Get(op, "look")))
            {
                case "text": look = MarkLook.Text; break;
                case "arrow": look = MarkLook.Arrow; break;
                case "ring": look = MarkLook.Ring; break;
                case "pointer": look = MarkLook.Pointer; break;
                default: return null;
            }
            var rgb = (uint)(Json.Num(Json.Get(op, "color")) ?? 0xFFFFFF) & 0xFFFFFF;
            return new Mark
            {
                Key = Json.Str(Json.Get(op, "key")),
                Look = look,
                Object = Json.Id(Json.Get(op, "object")),
                Ns = Json.Num(Json.Get(op, "ns")) ?? 0,
                Ew = Json.Num(Json.Get(op, "ew")) ?? 0,
                Z = Json.Num(Json.Get(op, "z")) ?? 0,
                Text = Json.Str(Json.Get(op, "text")),
                Color = unchecked((int)(0xFF000000u | rgb)),
                Scale = Json.Num(Json.Get(op, "scale")) ?? 1.0,
            };
        }
    }

    /// <summary>Decal's 3-D service, as the marks draw through it.</summary>
    public interface IMarkBackend
    {
        /// <summary>The mark drawn; disposing the answer takes it away.</summary>
        IDisposable Draw(Mark mark);
    }

    /// <summary>
    /// The plugins' markers as Decal's 3-D objects: each <c>mark</c> op draws one (in place of any
    /// with its key), each <c>unmark</c> takes one away.
    /// </summary>
    public sealed class Marks
    {
        private readonly IMarkBackend backend;
        private readonly Dictionary<string, IDisposable> drawn = new Dictionary<string, IDisposable>(StringComparer.Ordinal);

        public Marks(IMarkBackend backend) => this.backend = backend;

        /// <summary>The marks drawn, by key.</summary>
        public IEnumerable<string> Keys => drawn.Keys;

        /// <summary>One of the host's mark ops.</summary>
        public void Apply(Dictionary<string, object?> op)
        {
            var key = Json.Str(Json.Get(op, "key"));
            if (drawn.TryGetValue(key, out var old))
            {
                old.Dispose();
                drawn.Remove(key);
            }
            if (Json.Str(Json.Get(op, "op")) != "mark")
                return;
            var mark = Mark.Of(op);
            if (mark != null)
                drawn[key] = backend.Draw(mark);
        }

        /// <summary>Every mark taken away (the character left the world, or the plugins stopped).</summary>
        public void Clear()
        {
            foreach (var d in drawn.Values)
                d.Dispose();
            drawn.Clear();
        }
    }
}
