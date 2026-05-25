"use client";
import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import {
  fetchStations, fetchTotalSeries, fetchEventCounts,
  type Station, type TotalPoint, type EventCounts,
  formatMac, formatTs,
} from "@/lib/api";
import { usePoll } from "@/lib/usePoll";

// -----------------------------------------------------------------------------
// uPlot total packets chart
// -----------------------------------------------------------------------------

function TotalChart({ points }: { points: TotalPoint[] }) {
  const ref = useRef<HTMLDivElement>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    if (!ref.current) return;

    const ts    = points.map(p => p.timestamp_sec);
    const total = points.map(p => p.total);

    const opts: uPlot.Options = {
      width:  ref.current.clientWidth,
      height: 200,
      series: [
        {},
        {
          label:  "packets/s",
          stroke: "#7c6af7",
          fill:   "rgba(124,106,247,0.12)",
          width:  2,
        },
      ],
      axes: [
        {
          stroke:   "#6b6b8a",
          ticks:    { stroke: "#2a2a3a" },
          grid:     { stroke: "#2a2a3a" },
          space:	  80,
          values:   (_u, vals) => vals.map(v => new Date(v * 1000).toLocaleTimeString()),
        },
        {
          stroke: "#6b6b8a",
          ticks:  { stroke: "#2a2a3a" },
          grid:   { stroke: "#2a2a3a" },
        },
      ],
    };

    if (plotRef.current) {
      plotRef.current.setData([ts, total]);
    } else {
      plotRef.current = new uPlot(opts, [ts, total], ref.current);
    }
  }, [points]);

  useEffect(() => {
    const obs = new ResizeObserver(() => {
      if (ref.current && plotRef.current) {
        plotRef.current.setSize({ width: ref.current.clientWidth, height: 200 });
      }
    });
    if (ref.current) obs.observe(ref.current);
    return () => obs.disconnect();
  }, []);

  useEffect(() => {
    return () => {
      plotRef.current?.destroy();
      plotRef.current = null;
    };
  }, []);

  return <div ref={ref} style={{ width: "100%" }} />;
}

// -----------------------------------------------------------------------------
// Main page
// -----------------------------------------------------------------------------

const SECTION_STYLE = {
  background: "var(--surface)",
  border: "1px solid var(--border)",
  borderRadius: 8,
  padding: 16,
} as const;

const HEADER_STYLE = {
  color: "var(--muted)",
  fontSize: 11,
  letterSpacing: 2,
  marginBottom: 12,
  textTransform: "uppercase" as const,
};

const ROW_STYLE = {
  borderBottom: "1px solid var(--border)",
  padding: "8px 0",
  display: "flex",
  justifyContent: "space-between",
  alignItems: "center",
};

const LIST_SCROLL = { maxHeight: 400, overflowY: "auto" as const };

export default function Overview() {
  const [stations,    setStations]    = useState<Station[]>([]);
  const [series,      setSeries]      = useState<TotalPoint[]>([]);
  const [eventCounts, setEventCounts] = useState<EventCounts[]>([]);
  const [pollMs,      setPollMs]      = useState(3000);
  const [lastUpdate,  setLastUpdate]  = useState<string>("");

  async function refresh() {
    const [st, s, e] = await Promise.all([
      fetchStations(), fetchTotalSeries(), fetchEventCounts(),
    ]);
    setStations(st);
    setSeries(s);
    setEventCounts(e);
    setLastUpdate(new Date().toLocaleTimeString());
  }

  usePoll(refresh, pollMs);

  const ecMap = Object.fromEntries(eventCounts.map(e => [e.mac, e]));

  const aps     = stations.filter(s => s.is_ap);
  const awdl    = stations.filter(s => s.is_awdl);
  const probers = stations.filter(s => s.is_prober);
  const devices = stations.filter(s => !s.is_ap && !s.is_awdl && !s.is_prober);

  return (
    <div style={{ maxWidth: 1100, margin: "0 auto", padding: "24px 16px" }}>

      {/* header */}
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 24 }}>
        <h1 style={{ fontSize: 18, color: "var(--accent)", letterSpacing: 2 }}>knoob-rs</h1>
        <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
          <label style={{ color: "var(--muted)", display: "flex", alignItems: "center", gap: 8 }}>
            poll
            <input
              type="number"
              min={500}
              step={500}
              value={pollMs}
              onChange={e => setPollMs(Math.max(500, Number(e.target.value)))}
              style={{
                width: 70, background: "var(--surface)", border: "1px solid var(--border)",
                  color: "var(--text)", padding: "2px 6px", borderRadius: 4,
              }}
            />
            ms
          </label>
          <span style={{ color: "var(--muted)" }}>updated {lastUpdate}</span>
        </div>
      </div>

      {/* total time series */}
      <section style={{ ...SECTION_STYLE, marginBottom: 20 }}>
        <h2 style={HEADER_STYLE}>total packets / sec</h2>
        {series.length > 0
          ? <TotalChart points={series} />
          : <div style={{ height: 200, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--muted)" }}>waiting for data…</div>
        }
      </section>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20, marginBottom: 20 }}>

        {/* APs */}
        <section style={SECTION_STYLE}>
          <h2 style={HEADER_STYLE}>access points ({aps.length})</h2>
          {aps.length === 0
            ? <div style={{ color: "var(--muted)" }}>none seen</div>
            : <div style={LIST_SCROLL}>{aps.map(ap => (
              <div key={ap.mac} style={ROW_STYLE}>
                <div>
                  <div style={{ color: "var(--text)" }}>{ap.ssid ?? <span style={{ color: "var(--muted)" }}>[hidden]</span>}</div>
                  <Link href={`/device?mac=${encodeURIComponent(ap.mac)}`} style={{ color: "var(--muted)", fontFamily: "monospace", fontSize: 11 }}>
                    {formatMac(ap.mac)}
                  </Link>
                  <div style={{ color: "var(--muted)", fontSize: 11 }}>
                    ch {ap.channel}{ap.vendor && ` · ${ap.vendor}`}
                  </div>
                </div>
                <div style={{ textAlign: "right" }}>
                  <div style={{ color: "var(--accent2)" }}>{ap.rssi} dBm</div>
                  <div style={{ color: "var(--muted)", fontSize: 11 }}>last {formatTs(ap.last_seen)}</div>
                </div>
              </div>
            ))}</div>
          }
        </section>

        {/* devices */}
        <section style={SECTION_STYLE}>
          <h2 style={HEADER_STYLE}>devices ({devices.length})</h2>
          {devices.length === 0
            ? <div style={{ color: "var(--muted)" }}>none seen</div>
            : <div style={LIST_SCROLL}>{devices.map(d => {
              const ec = ecMap[d.mac];
              return (
                <div key={d.mac} style={ROW_STYLE}>
                  <div>
                    <Link href={`/device?mac=${encodeURIComponent(d.mac)}`} style={{ color: "var(--text)", fontFamily: "monospace" }}>
                      {formatMac(d.mac)}
                    </Link>
                    {d.is_randomized && <span style={{ color: "var(--accent)", fontSize: 10, marginLeft: 6 }}>RAND</span>}
                    {d.vendor && <div style={{ color: "var(--muted)", fontSize: 11 }}>{d.vendor}</div>}
                    <div style={{ color: "var(--muted)", fontSize: 11 }}>
                      tx: m{d.mgmt_tx} c{d.ctrl_tx} d{d.data_tx} · rx: d{d.data_rx}
                    </div>
                  </div>
                  <div style={{ textAlign: "right", color: "var(--muted)", fontSize: 11 }}>
                    {ec && <><span style={{ color: "var(--success)" }}>↑{ec.connects}</span> <span style={{ color: "var(--danger)" }}>↓{ec.disconnects}</span></>}
                    <div>last {formatTs(d.last_seen)}</div>
                  </div>
                </div>
              );
              })}</div>
          }
        </section>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20, marginBottom: 20 }}>

        {/* AWDL */}
        <section style={SECTION_STYLE}>
          <h2 style={HEADER_STYLE}>awdl peers ({awdl.length})</h2>
          {awdl.length === 0
            ? <div style={{ color: "var(--muted)" }}>none seen</div>
            : <div style={LIST_SCROLL}>{awdl.map(a => (
              <div key={a.mac} style={ROW_STYLE}>
                <Link href={`/device?mac=${encodeURIComponent(a.mac)}`} style={{ color: "var(--text)", fontFamily: "monospace" }}>
                  {formatMac(a.mac)}
                </Link>
                <div style={{ textAlign: "right", color: "var(--muted)", fontSize: 11 }}>
                  <div>{a.rssi} dBm · ch {a.channel}</div>
                  <div>last {formatTs(a.last_seen)}</div>
                </div>
              </div>
            ))}</div>
          }
        </section>

        {/* probers */}
        <section style={SECTION_STYLE}>
          <h2 style={HEADER_STYLE}>probers ({probers.length})</h2>
          {probers.length === 0
            ? <div style={{ color: "var(--muted)" }}>none seen</div>
            : <div style={LIST_SCROLL}>{probers.map(p => (
              <div key={p.mac} style={ROW_STYLE}>
                <Link href={`/device?mac=${encodeURIComponent(p.mac)}`} style={{ color: "var(--text)", fontFamily: "monospace" }}>
                  {formatMac(p.mac)}
                </Link>
                <div style={{ textAlign: "right", color: "var(--muted)", fontSize: 11 }}>
                  <div>{p.rssi} dBm · ch {p.channel}</div>
                  <div>last {formatTs(p.last_seen)}</div>
                </div>
              </div>
            ))}</div>
          }
        </section>
      </div>
    </div>
  );
}
