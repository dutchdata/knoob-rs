"use client";
import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import {
	fetchAps, fetchDevices, fetchTotalSeries, fetchEventCounts,
	type Ap, type Device, type TotalPoint, type EventCounts,
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

	// resize
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

export default function Overview() {
	const [aps,         setAps]         = useState<Ap[]>([]);
	const [devices,     setDevices]     = useState<Device[]>([]);
	const [series,      setSeries]      = useState<TotalPoint[]>([]);
	const [eventCounts, setEventCounts] = useState<EventCounts[]>([]);
	const [pollMs,      setPollMs]      = useState(3000);
	const [lastUpdate,  setLastUpdate]  = useState<string>("");

	async function refresh() {
		const [a, d, s, e] = await Promise.all([
			fetchAps(), fetchDevices(), fetchTotalSeries(), fetchEventCounts(),
		]);
		setAps(a);
		setDevices(d);
		setSeries(s);
		setEventCounts(e);
		setLastUpdate(new Date().toLocaleTimeString());
	}

	usePoll(refresh, pollMs);

	const ecMap = Object.fromEntries(eventCounts.map(e => [e.mac, e]));

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
			<section style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 8, padding: 16, marginBottom: 20 }}>
				<h2 style={{ color: "var(--muted)", fontSize: 11, letterSpacing: 2, marginBottom: 12, textTransform: "uppercase" }}>total packets / sec</h2>
		{series.length > 0
			? <TotalChart points={series} />
			: <div style={{ height: 200, display: "flex", alignItems: "center", justifyContent: "center", color: "var(--muted)" }}>waiting for data…</div>
		}
	</section>

	<div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20, marginBottom: 20 }}>

		{/* APs */}
		<section style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 8, padding: 16 }}>
			<h2 style={{ color: "var(--muted)", fontSize: 11, letterSpacing: 2, marginBottom: 12, textTransform: "uppercase" }}>
				access points ({aps.length})
			</h2>
		{aps.length === 0
			? <div style={{ color: "var(--muted)" }}>none seen</div>
			: aps.map(ap => (
				<div key={ap.bssid} style={{ borderBottom: "1px solid var(--border)", padding: "8px 0", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
					<div>
						<div style={{ color: "var(--text)" }}>{ap.ssid ?? <span style={{ color: "var(--muted)" }}>[hidden]</span>}</div>
						<div style={{ color: "var(--muted)", fontSize: 11 }}>{formatMac(ap.bssid)} · ch {ap.channel}</div>
					</div>
					<div style={{ textAlign: "right" }}>
						<div style={{ color: "var(--accent2)" }}>{ap.rssi} dBm</div>
						<div style={{ color: "var(--muted)", fontSize: 11 }}>
							{ap.device_count} dev · {ap.mfpr ? "MFP-req" : ap.mfpc ? "MFP-cap" : "no-MFP"}
						</div>
					</div>
				</div>
			))
		}
	</section>

	{/* devices */}
	<section style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: 8, padding: 16 }}>
		<h2 style={{ color: "var(--muted)", fontSize: 11, letterSpacing: 2, marginBottom: 12, textTransform: "uppercase" }}>
			devices ({devices.length})
		</h2>
		{devices.length === 0
			? <div style={{ color: "var(--muted)" }}>none seen</div>
			: devices.map(d => {
				const ec = ecMap[d.mac];
				return (
					<div key={d.mac} style={{ borderBottom: "1px solid var(--border)", padding: "8px 0", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
						<div>
							<Link href={`/device?mac=${encodeURIComponent(d.mac)}`} style={{ color: "var(--text)", fontFamily: "monospace" }}>
								{formatMac(d.mac)}
							</Link>
					{d.is_randomized && <span style={{ color: "var(--accent)", fontSize: 10, marginLeft: 6 }}>RAND</span>}
					<div style={{ color: "var(--muted)", fontSize: 11 }}>→ {formatMac(d.last_bssid)}</div>
				</div>
				<div style={{ textAlign: "right", color: "var(--muted)", fontSize: 11 }}>
					{ec && <><span style={{ color: "var(--success)" }}>↑{ec.connects}</span> <span style={{ color: "var(--danger)" }}>↓{ec.disconnects}</span></>}
					<div>last {formatTs(d.last_seen)}</div>
				</div>
			</div>
		);
			})
		}
	</section>
</div>
    </div>
  );
}
