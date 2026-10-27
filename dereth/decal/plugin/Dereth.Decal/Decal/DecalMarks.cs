using System;
using Decal.Adapter.Wrappers;

namespace Dereth.Decal.Plugin
{
    /// <summary>The marks as Decal's 3-D objects.</summary>
    internal sealed class DecalMarks : IMarkBackend
    {
        private const string Font = "Arial";
        private readonly D3DService d3d;

        public DecalMarks(D3DService d3d) => this.d3d = d3d;

        public IDisposable Draw(Mark m)
        {
            // Decal takes a place as map coordinates north and east, and its height in meters.
            var (ns, ew, z) = ((float)m.Ns, (float)m.Ew, (float)m.Z);
            D3DObj o;
            switch (m.Look)
            {
                case MarkLook.Text:
                    o = m.Object is int t
                        ? d3d.MarkObjectWith3DText(t, m.Text, Font, 0)
                        : d3d.MarkCoordsWith3DText(ns, ew, z, m.Text, Font, 0);
                    o.Color = m.Color;
                    o.OrientToCamera(true);
                    break;
                case MarkLook.Arrow:
                    o = m.Object is int a
                        ? d3d.MarkObjectWithShape(a, D3DShape.VerticalArrow, m.Color)
                        : d3d.MarkCoordsWithShape(ns, ew, z, D3DShape.VerticalArrow, m.Color);
                    break;
                case MarkLook.Ring:
                    o = m.Object is int r
                        ? d3d.MarkObjectWithShape(r, D3DShape.Ring, m.Color)
                        : d3d.MarkCoordsWithShape(ns, ew, z, D3DShape.Ring, m.Color);
                    o.ScaleX = (float)m.Scale;
                    o.ScaleY = (float)m.Scale;
                    break;
                default:
                    o = m.Object is int p
                        ? d3d.PointToObject(p, m.Color)
                        : d3d.PointToCoords(ns, ew, z, m.Color);
                    break;
            }
            o.Visible = true;
            return new Drawn(o);
        }

        private sealed class Drawn : IDisposable
        {
            private D3DObj? o;

            public Drawn(D3DObj o) => this.o = o;

            public void Dispose()
            {
                if (o == null)
                    return;
                o.Visible = false;
                o.Dispose();
                o = null;
            }
        }
    }
}
